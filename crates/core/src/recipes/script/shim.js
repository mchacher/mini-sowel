// The JavaScript side of Solite's script-recipe API, version 1 (spec 041, R2). It runs in the
// recipe's own context before the recipe's source and returns the entry points the Rust side
// calls. The engine's functions (`__host`) are captured here and removed from the globals: the
// script cannot reach them. Every call into the engine goes by slot name and instance handle;
// Rust refuses a handle that is not one of this recipe's.
// The placeholders `@NAME@` are replaced by Rust with the constants of the engine.
// The entry points are the value this script evaluates to; Rust keeps them as the global
// `__solite_shim` (not enumerable), since a saved value cannot move between threads. A script that
// reached them could only call into its own recipe's instances: every call of the engine is by
// handle and slot name, checked against the table Rust holds.
(function () {
  "use strict";
  var H = globalThis.__host;
  delete globalThis.__host;
  var CLOCK_SET = @CLOCK_SET@, MAX_TIMERS = @MAX_TIMERS@, MAX_LISTEN = @MAX_LISTEN@, MAX_STATE = @MAX_STATE@, MAX_LINES = @MAX_LINES@, MAX_ALIAS = @MAX_ALIAS@;
  var hasOwn = Object.prototype.hasOwnProperty;
  var RealDate = Date;
  // The two functions are captured here: a script that rewrites `JSON.stringify` changes nothing
  // of what the engine is told.
  var stringify = JSON.stringify, parse = JSON.parse;
  var RT = { mono: 0, wall: 0, set: false };
  var defs = 0, defn = null;
  var insts = Object.create(null);
  var cur = null, out = null;
  // Promises rejected with no handler yet (at most 8 kept) during the call: a throw inside a
  // `then` is one, and the engine would not otherwise hear of it.
  var rej = [];
  var ZONE_ALIASES = ["motion", "luminosity", "motionSensors"];

  // ---- Time: Date is pinned to the wall clock of the call ----
  function PinnedDate() {
    if (!new.target) return new RealDate(RT.wall).toString();
    return arguments.length ? new RealDate(...arguments) : new RealDate(RT.wall);
  }
  PinnedDate.prototype = RealDate.prototype;
  PinnedDate.now = function () { return RT.wall; };
  PinnedDate.UTC = RealDate.UTC;
  PinnedDate.parse = RealDate.parse;
  globalThis.Date = PinnedDate;

  // ---- Log ----
  function addLog(level, text) {
    if (out === null) return;
    if (out.logs.length >= MAX_LINES) { out.dropped = true; return; }
    text = String(text);
    if (text.length > 400) {
      // Never cut a surrogate pair in two.
      var c = text.charCodeAt(399);
      text = text.slice(0, c >= 0xd800 && c < 0xdc00 ? 399 : 400);
    }
    out.logs.push([level, wf(text)]);
  }
  function log(text) { addLog(0, text); }
  log.warn = function (text) { addLog(1, text); };
  log.error = function (text) { addLog(2, text); };
  globalThis.console = { log: log, info: log, warn: log.warn, error: log.error };

  // ---- The definition: `recipe({...})`, once, at the top level of the script ----
  globalThis.recipe = function (definition) {
    defs++;
    if (defs === 1) defn = definition;
  };

  // ---- Errors ----
  // Text the engine will parse as JSON cannot hold half of a surrogate pair (serde refuses it): a
  // lone half becomes U+FFFD. `wf` cleans a string; `wfJson` cleans serialized JSON, where
  // `stringify` writes a lone half as an escape (a valid pair is never escaped).
  // (No regular expressions in the engine: both are loops.)
  function wf(text) {
    var out = "", from = 0;
    for (var i = 0; i < text.length; i++) {
      var c = text.charCodeAt(i);
      if (c < 0xd800 || c > 0xdfff) continue;
      var d = i + 1 < text.length ? text.charCodeAt(i + 1) : 0;
      if (c < 0xdc00 && d >= 0xdc00 && d <= 0xdfff) { i++; continue; }
      out += text.slice(from, i) + "\ufffd";
      from = i + 1;
    }
    return from === 0 ? text : out + text.slice(from);
  }
  function wfJson(text) {
    var out = "", from = 0;
    for (var i = text.indexOf("\\"); i >= 0 && i < text.length; i = text.indexOf("\\", i)) {
      var n = text.charAt(i + 1);
      if (n === "u" && text.charAt(i + 2) === "d" && "89abcdef".indexOf(text.charAt(i + 3)) >= 0) {
        out += text.slice(from, i) + "\\ufffd";
        i += 6; from = i;
      } else i += 2;
    }
    return from === 0 ? text : out + text.slice(from);
  }

  function msg(e) { return e !== null && e !== undefined && e.message !== undefined ? String(e.message) : String(e); }
  // A failure of the recipe as a whole, not of the instance.
  function failureOf(e) {
    if (e === null || e === undefined) return null;
    var m = msg(e);
    if (e.name === "RangeError" && m.indexOf("stack") >= 0) return "stack overflow";
    if (e.name === "InternalError" && m.indexOf("out of memory") >= 0) return "out of memory";
    return null;
  }
  // Runs a handler of the script: its error is a log line, the call goes on; a failure of the
  // recipe is kept for the end of the call.
  function guard(fn, a, b) {
    if (out.fail !== null) return;
    try { fn(a, b); } catch (e) {
      var f = failureOf(e);
      if (f !== null) out.fail = f; else addLog(2, "Script error: " + msg(e));
    }
  }
  function unknownSlot(name) { return new Error("Unknown slot: " + name); }
  function needFn(fn, what) { if (typeof fn !== "function") throw new TypeError(what + ": a function is expected"); }

  function slotOf(inst, name) { return hasOwn.call(inst.slots, name) ? inst.slots[name] : undefined; }
  function equipmentSlot(inst, name) {
    var s = slotOf(inst, name);
    if (s === undefined || s.t !== "equipment") throw unknownSlot(name);
    return s;
  }

  // ---- State: keys in the order first set, at most MAX_STATE bytes serialized ----
  function u8len(s) {
    var n = 0;
    for (var i = 0; i < s.length; i++) {
      var c = s.charCodeAt(i);
      if (c < 0x80) n += 1; else if (c < 0x800) n += 2;
      else if (c >= 0xd800 && c < 0xdc00) { n += 4; i++; } else n += 3;
    }
    return n;
  }
  function stateText(inst) {
    var parts = [];
    for (var i = 0; i < inst.st.length; i++) parts.push(stringify(inst.st[i][0]) + ":" + inst.st[i][1]);
    return wfJson("{" + parts.join(",") + "}");
  }
  function stateAt(inst, key) {
    for (var i = 0; i < inst.st.length; i++) if (inst.st[i][0] === key) return i;
    return -1;
  }

  function na(name) { return function () { throw new Error("Not available in Solite: " + name); }; }
  // What does not depend on the instance is made once for the recipe, not once per instance.
  var NO_MODE = na("mode"), NO_HISTORY = na("history"), NO_ENERGY = { claim: na("energy"), release: na("energy") };
  function now() {
    var sun = H.sun();
    // The home's time of day, "HH:MM" in its time zone (UTC when it has none).
    var sec = (Math.floor(RT.wall / 1000) + H.offset(RT.wall)) % 86400;
    if (sec < 0) sec += 86400;
    var hh = Math.floor(sec / 3600), mm = Math.floor(sec % 3600 / 60);
    return { time: new RealDate(RT.wall).toISOString(), localTime: (hh < 10 ? "0" : "") + hh + ":" + (mm < 10 ? "0" : "") + mm, isDaylight: sun.isDaylight, sunrise: sun.sunrise, sunset: sun.sunset };
  }

  // ---- Timers ----
  function addTimer(inst, kind, fn) {
    if (inst.timers.length >= MAX_TIMERS) throw new Error("Too many timers (at most " + MAX_TIMERS + ")");
    var t = { id: ++inst.tid, k: kind, fn: fn, due: 0, p: 0, h: 0, m: 0, last: null };
    inst.timers.push(t);
    return t;
  }
  function dropTimer(inst, t) {
    var i = inst.timers.indexOf(t);
    if (i >= 0) inst.timers.splice(i, 1);
  }

  function makeApi(inst) {
    var h = inst.h;
    return {
      params: inst.params,
      on: function (slot, alias, fn) {
        var s = equipmentSlot(inst, slot);
        needFn(fn, "on");
        alias = String(alias);
        if (wf(alias) !== alias) throw new Error("on: the alias is not valid text");
        if (u8len(alias) > MAX_ALIAS) throw new Error("on: the alias is over " + MAX_ALIAS + " bytes");
        var fresh = 0;
        for (var i = 0; i < s.ids.length; i++) {
          if (!hasOwn.call(inst.pairs, s.ids[i] + "\n" + alias)) fresh++;
        }
        if (inst.npairs + fresh > MAX_LISTEN) throw new Error("Too many listened equipments (at most " + MAX_LISTEN + ")");
        for (var j = 0; j < s.ids.length; j++) {
          var key = s.ids[j] + "\n" + alias;
          if (!hasOwn.call(inst.pairs, key)) { inst.pairs[key] = true; inst.npairs++; }
        }
        inst.handlers.push({ slot: slot, alias: alias, fn: fn });
        inst.dirty = true;
      },
      onZone: function (alias, fn) {
        alias = String(alias);
        if (ZONE_ALIASES.indexOf(alias) < 0) throw new Error("Unknown zone data: " + alias);
        needFn(fn, "onZone");
        inst.zoneHandlers.push({ alias: alias, fn: fn });
        inst.dirty = true;
      },
      onMotionReport: function (fn) {
        needFn(fn, "onMotionReport");
        inst.motionHandlers.push(fn);
        inst.dirty = true;
      },
      get: function (slot, alias) {
        var s = slotOf(inst, slot);
        if (s === undefined) throw unknownSlot(slot);
        if (s.t === "zone") return H.zone(h, slot, String(alias));
        if (s.t !== "equipment") throw unknownSlot(slot);
        var values = H.get(h, slot, String(alias));
        return s.l ? values : values[0];
      },
      order: function (slot, alias, value) {
        equipmentSlot(inst, slot);
        var json = stringify(value);
        if (json === undefined) throw new TypeError("order: the value cannot be sent");
        var refused = parse(H.order(h, slot, String(alias), json));
        var list = [];
        for (var i = 0; i < refused.length; i++) list.push({ id: refused[i][0], reason: refused[i][1] });
        return list;
      },
      after: function (delay, fn) {
        needFn(fn, "after");
        var ms = H.dur(delay);
        if (ms < 100) throw new Error("after: the delay must be at least 100 ms");
        var t = addTimer(inst, "after", fn);
        t.due = RT.mono + ms;
        return t.id;
      },
      every: function (period, fn) {
        needFn(fn, "every");
        var ms = H.dur(period);
        if (ms < 1000) throw new Error("every: the period must be at least 1 s");
        var t = addTimer(inst, "every", fn);
        t.p = ms;
        t.due = RT.mono + ms;
        return t.id;
      },
      at: function (time, fn) {
        needFn(fn, "at");
        var parts = String(time).split(":");
        var hh = parts.length === 2 ? Number(parts[0]) : NaN, mm = parts.length === 2 ? Number(parts[1]) : NaN;
        if (!(hh >= 0 && hh <= 23 && mm >= 0 && mm <= 59) || hh % 1 !== 0 || mm % 1 !== 0 || parts[1].length !== 2) {
          throw new Error('at: expected a time like "07:30"');
        }
        var t = addTimer(inst, "at", fn);
        t.h = hh;
        t.m = mm;
        t.last = RT.set ? RT.wall : null;
        return t.id;
      },
      cancel: function (id) {
        for (var i = 0; i < inst.timers.length; i++) {
          if (inst.timers[i].id === id) { inst.timers.splice(i, 1); return true; }
        }
        return false;
      },
      now: now,
      state: {
        set: function (key, value) {
          key = String(key);
          var json = stringify(value);
          if (json === undefined) throw new TypeError("state.set: the value cannot be kept");
          var at = stateAt(inst, key), old = at >= 0 ? inst.st[at][1] : undefined;
          if (at >= 0) inst.st[at][1] = json; else inst.st.push([key, json]);
          if (u8len(stateText(inst)) > MAX_STATE) {
            if (at >= 0) inst.st[at][1] = old; else inst.st.pop();
            throw new Error("state is over " + MAX_STATE + " bytes");
          }
        },
        get: function (key) {
          var at = stateAt(inst, String(key));
          return at < 0 ? undefined : parse(inst.st[at][1]);
        },
        delete: function (key) {
          var at = stateAt(inst, String(key));
          if (at >= 0) inst.st.splice(at, 1);
          return at >= 0;
        }
      },
      log: log,
      mode: NO_MODE,
      energy: NO_ENERGY,
      history: NO_HISTORY
    };
  }

  // ---- A call into an instance: begin, the call, (the pending jobs run in Rust), end ----
  function begin(inst, mono, wall) {
    var set = wall >= CLOCK_SET;
    RT.mono = mono; RT.wall = wall; RT.set = set;
    cur = inst;
    rej = [];
    out = { logs: [], dropped: false, fail: null };
    if (set) {
      for (var i = 0; i < inst.timers.length; i++) {
        var t = inst.timers[i];
        if (t.k === "at" && t.last === null) t.last = wall;
      }
    }
  }
  function result() {
    if (out.fail !== null) return "f:" + out.fail;
    return "";
  }
  function inSlot(inst, slot, id) {
    var s = slotOf(inst, slot);
    return s !== undefined && s.ids.indexOf(id) >= 0;
  }
  function listenOf(inst) {
    var eq = [];
    for (var key in inst.pairs) {
      var cut = key.indexOf("\n");
      eq.push([key.slice(0, cut), key.slice(cut + 1)]);
    }
    return { e: eq, z: inst.zoneHandlers.length > 0, m: inst.motionHandlers.length > 0 };
  }
  function rebuildPairs(inst) {
    inst.pairs = Object.create(null);
    inst.npairs = 0;
    for (var i = 0; i < inst.handlers.length; i++) {
      var hd = inst.handlers[i], s = slotOf(inst, hd.slot);
      if (s === undefined) continue;
      for (var j = 0; j < s.ids.length; j++) {
        var key = s.ids[j] + "\n" + hd.alias;
        if (!hasOwn.call(inst.pairs, key)) { inst.pairs[key] = true; inst.npairs++; }
      }
    }
    inst.dirty = true;
  }

  var entry = {
    // What the script declared: a JSON text for Rust to check.
    def: function () {
      var d = defn;
      if (defs !== 1 || d === null || typeof d !== "object") return stringify({ count: defs });
      // The slots as [id, definition] pairs: an object's order is lost once it is parsed in Rust.
      var sl = d.slots, pairs = null;
      if (sl === undefined) pairs = [];
      else if (sl !== null && typeof sl === "object" && !Array.isArray(sl)) {
        pairs = Object.keys(sl).map(function (k) { return [k, sl[k]]; });
      }
      var text = stringify({
        count: defs, api: d.api, id: d.id, version: d.version, name: d.name, description: d.description,
        icon: d.icon, i18n: d.i18n, slots: pairs,
        hasStart: typeof d.start === "function", hasValidate: typeof d.validate === "function"
      });
      // Rust has what it needs: only the two functions stay in memory.
      defn = { start: d.start, validate: d.validate };
      return text;
    },
    validate: function (paramsJson, mono, wall) {
      RT.mono = mono; RT.wall = wall; RT.set = wall >= CLOCK_SET;
      rej = [];
      out = { logs: [], dropped: false, fail: null };
      try {
        var params = parse(paramsJson), known = Object.create(null);
        // `validate` may look at the equipments its own params name, and no others.
        for (var k in params) {
          var v = params[k];
          if (typeof v === "string") known[v] = true;
          else if (Array.isArray(v)) for (var j = 0; j < v.length; j++) if (typeof v[j] === "string") known[v[j]] = true;
        }
        var home = {
          equipment: function (id) {
            id = String(id);
            if (known[id] !== true) return null;
            var f = H.facts(id);
            return f === null || f === undefined ? null : parse(f);
          },
          zone: function (id) {
            id = String(id);
            if (known[id] !== true) return null;
            var f = H.zoneFacts(id);
            return f === null || f === undefined ? null : parse(f);
          }
        };
        var r = defn.validate(params, home);
        out = null;
        return typeof r === "string" && r !== "" ? "v:" + r : "";
      } catch (e) {
        out = null;
        var f = failureOf(e);
        return f !== null ? "f:" + f : "e:" + msg(e);
      }
    },
    start: function (h, paramsJson, slotsJson, mono, wall) {
      var inst = {
        h: h, params: parse(paramsJson), slots: parse(slotsJson),
        handlers: [], zoneHandlers: [], motionHandlers: [], timers: [], tid: 0,
        st: [], pairs: Object.create(null), npairs: 0, dirty: true, lastState: ""
      };
      insts[h] = inst;
      begin(inst, mono, wall);
      try { defn.start(makeApi(inst)); } catch (e) {
        var f = failureOf(e);
        return f !== null ? "f:" + f : "e:" + msg(e);
      }
      return result();
    },
    // kind 1: equipment (a = equipment id, b = alias, c = value); 2: zone (a = zone id,
    // b = motion, c = luminosity); 3: motion report (a = equipment id, b = value)
    event: function (h, kind, mono, wall, a, b, c) {
      var inst = insts[h];
      begin(inst, mono, wall);
      var i, hd;
      if (kind === 1) {
        var list = inst.handlers.slice();
        for (i = 0; i < list.length; i++) {
          hd = list[i];
          if (hd.alias === b && inSlot(inst, hd.slot, a)) guard(hd.fn, c, a);
        }
      } else if (kind === 2) {
        var zl = inst.zoneHandlers.slice();
        for (i = 0; i < zl.length; i++) {
          hd = zl[i];
          var v;
          try {
            v = hd.alias === "motion" ? b : hd.alias === "luminosity" ? c : H.zone(inst.h, "zone", hd.alias);
          } catch (e) { addLog(2, "Script error: " + msg(e)); continue; }
          guard(hd.fn, v);
        }
      } else if (kind === 3) {
        var ml = inst.motionHandlers.slice();
        for (i = 0; i < ml.length; i++) guard(ml[i], b, a);
      }
      return result();
    },
    tick: function (h, mono, wall) {
      var inst = insts[h];
      begin(inst, mono, wall);
      var list = inst.timers.slice();
      for (var i = 0; i < list.length; i++) {
        var t = list[i];
        if (inst.timers.indexOf(t) < 0) continue;
        if (t.k === "at") {
          if (t.last === null) continue;
          var w = H.atNext(t.h, t.m, t.last);
          if (w === null || wall < w) continue;
          t.last = wall;
        } else {
          if (mono < t.due) continue;
          if (t.k === "after") dropTimer(inst, t);
          else { t.due += t.p; if (t.due <= mono) t.due = mono + t.p; }
        }
        guard(t.fn);
      }
      return result();
    },
    // The engine's hook on promise rejections (set by Rust once the shim is loaded).
    rejected: function (p, reason, handled) {
      if (!handled) { if (rej.length < 8) rej.push([p, reason]); return; }
      for (var i = 0; i < rej.length; i++) if (rej[i][0] === p) { rej.splice(i, 1); return; }
    },
    refresh: function (h, paramsJson, slotsJson) {
      var inst = insts[h];
      inst.params = parse(paramsJson);
      inst.slots = parse(slotsJson);
      rebuildPairs(inst);
      return "";
    },
    stop: function (h) { delete insts[h]; },
    // The end of a call: what Rust keeps of the instance (state, listen set, deadline) and the log.
    end: function (h) {
      var inst = insts[h], o = out;
      // What the pending jobs left: a promise rejected and never handled is an error of the script.
      if (o !== null) {
        for (var r = 0; r < rej.length; r++) {
          var why = failureOf(rej[r][1]);
          if (why !== null) { if (o.fail === null) o.fail = why; } else addLog(2, "Script error: " + msg(rej[r][1]));
        }
      }
      rej = [];
      out = null; cur = null;
      var res = {};
      if (o !== null && o.fail !== null) res.f = o.fail;
      if (o !== null && o.logs.length > 0 || (o !== null && o.dropped)) {
        if (o.dropped) o.logs.push([1, "Log lines dropped"]);
        res.g = o.logs;
      }
      var text = stateText(inst);
      if (text !== inst.lastState) { res.s = text; inst.lastState = text; }
      if (inst.dirty) { res.l = listenOf(inst); inst.dirty = false; }
      var m = null, a = [];
      for (var i = 0; i < inst.timers.length; i++) {
        var t = inst.timers[i];
        if (t.k === "at") a.push([t.h, t.m, t.last]);
        else if (m === null || t.due < m) m = t.due;
      }
      res.d = { m: m, a: a };
      return stringify(res);
    }
  };
  Object.freeze(entry);
  Object.defineProperty(globalThis, "__solite_shim", { value: entry, enumerable: false, configurable: false, writable: false });
})();
