// schedule-on-off: the port of Sowel's `sowel-recipe-schedule-on-off` 2.0.2 to the script API
// (spec 055): Sowel's id, slots, info, texts and behaviour, adapted for Solite where its engine
// differs: the boundaries are `api.at("HH:MM")` timers (the home's time zone), a boundary that
// follows the sun is re-armed when it fires and at midnight (Solite has no `sunlight.changed`
// event), an equipment is named by the first 8 characters of its id in the log (a script does not
// read names), a refused order's reason is logged, and a stopped instance leaves no line (a script
// has no stop hook). The top level is shared by the instances, so it holds functions only: each
// takes the instance's data `c` (made in `start`).
var UNSET = "??:??";

// ---- Time ----
// "HH:MM" with two digits each, hours 00-23, minutes 00-59 (the engine has no regular expressions).
function isValidHHMM(s) {
  if (typeof s !== "string" || s.length !== 5 || s.charAt(2) !== ":") return false;
  var h = s.charCodeAt(0) * 10 + s.charCodeAt(1) - 528, m = s.charCodeAt(3) * 10 + s.charCodeAt(4) - 528;
  for (var i = 0; i < 5; i++) {
    var d = s.charCodeAt(i);
    if (i !== 2 && (d < 48 || d > 57)) return false;
  }
  return h <= 23 && m <= 59;
}
function minutesOf(text) {
  var p = text.split(":").map(Number);
  return p[0] * 60 + p[1];
}
// Shifts an "HH:MM" by a signed number of minutes, wrapping at midnight.
function applyOffset(hhmm, offsetMin) {
  var total = minutesOf(hhmm) + (isFinite(offsetMin) ? offsetMin : 0);
  total = ((total % 1440) + 1440) % 1440;
  var h = Math.floor(total / 60), m = total % 60;
  return (h < 10 ? "0" : "") + h + ":" + (m < 10 ? "0" : "") + m;
}
// Minutes from now to the next occurrence of an "HH:MM" (a time that is now is tomorrow's).
function minutesUntil(now, hhmm) {
  var local = now.localTime;
  var d = minutesOf(hhmm) - (local === null ? 0 : minutesOf(local));
  return d <= 0 ? d + 1440 : d;
}

// ---- The boundaries ----
function asKind(value) {
  return value === "sunrise" || value === "sunset" ? value : "time";
}
function asOffset(value) {
  var n = typeof value === "number" ? value : parseInt(String(value === undefined || value === null ? "" : value), 10);
  return isFinite(n) ? n : 0;
}
// A boundary is configured when it can yield a time: a fixed HH:MM, or a sun kind.
function configured(b) {
  return b.kind === "time" ? isValidHHMM(b.time) : true;
}
function readBoundary(params, prefix) {
  var time = params[prefix + "_time"];
  return { kind: asKind(params[prefix + "_kind"]), time: isValidHHMM(time) ? time : null, offset: asOffset(params[prefix + "_offset"]) };
}
function boundaryLabel(b) {
  if (b.kind === "time") return b.time === null ? UNSET : b.time;
  var sign = b.offset > 0 ? "+" + b.offset : b.offset < 0 ? String(b.offset) : "";
  return (b.kind === "sunrise" ? "lever" : "coucher") + (sign ? " " + sign + "min" : "");
}
function windowLabel(w) {
  return boundaryLabel(w.start) + " → " + boundaryLabel(w.end);
}
// `now` is the result of `api.now()`, read once by the caller (a call stack of a script is short).
// The "HH:MM" of a boundary today, or null when its sun time is unavailable ("--:--" at the poles).
function resolve(now, b) {
  if (b.kind === "time") return isValidHHMM(b.time) ? b.time : null;
  var base = b.kind === "sunrise" ? now.sunrise : now.sunset;
  return isValidHHMM(base) ? applyOffset(base, b.offset) : null;
}
function toIdList(value) {
  var out = [], i;
  if (Array.isArray(value)) {
    for (i = 0; i < value.length; i++) if (String(value[i]).length > 0) out.push(String(value[i]));
    return out;
  }
  if (value === undefined || value === null || value === "") return out;
  return [String(value)];
}

// ---- Orders ----
function shortId(id) {
  return id.slice(0, 8);
}
function dispatch(c, value) {
  var refused = c.api.order("equipments", "state", value);
  for (var i = 0; i < refused.length; i++) {
    c.api.log.error("Erreur " + value + " " + shortId(refused[i].id) + " : " + refused[i].reason);
  }
}
function names(c) {
  return c.ids.map(shortId).join(", ");
}
function fireOn(c, w) {
  dispatch(c, "ON");
  c.api.state.set("status", "running");
  c.api.state.set("currentSlot", windowLabel(w));
  c.api.log("Marche créneau " + windowLabel(w) + " (" + names(c) + ")");
}
function fireOff(c, w) {
  dispatch(c, "OFF");
  c.api.state.set("status", "idle");
  c.api.state.set("currentSlot", null);
  c.api.log("Arrêt créneau " + windowLabel(w) + " (" + names(c) + ")");
}

// ---- The timers ----
// A boundary `b` holds its window `w`, whether it starts the window, the id of its `at` timer and
// whether its missing sun time was reported. A sun boundary is armed again when it fires (the sun
// moves a few minutes a day) and at midnight. The timer's function holds `c` and `b` only: each
// timer costs the instance memory.
function schedule(c, now, b) {
  if (b.timer !== null) c.api.cancel(b.timer);
  b.timer = null;
  var hhmm = resolve(now, b);
  if (hhmm === null) {
    // Sun time unavailable today: armed again at midnight.
    if (!b.warned) {
      c.api.log.warn("Créneau " + b.w.index + " : " + boundaryLabel(b) + " indisponible aujourd'hui, borne ignorée");
      b.warned = true;
    }
    return;
  }
  b.warned = false;
  b.timer = c.api.at(hhmm, function () {
    (b.starts ? fireOn : fireOff)(c, b.w);
    var later = c.api.now();
    if (b.kind !== "time") schedule(c, later, b);
    updateNextLabels(c, later);
  });
}
function armAll(c) {
  var now = c.api.now();
  for (var i = 0; i < c.windows.length; i++) {
    schedule(c, now, c.windows[i].start);
    schedule(c, now, c.windows[i].end);
  }
  updateNextLabels(c, now);
}
// Boundaries that follow the sun are resolved again; fixed times stay as they are.
function rearmSun(c) {
  var now = c.api.now();
  for (var i = 0; i < c.windows.length; i++) {
    var w = c.windows[i];
    if (w.start.kind !== "time") schedule(c, now, w.start);
    if (w.end.kind !== "time") schedule(c, now, w.end);
  }
  updateNextLabels(c, now);
}
function nextHHMM(now, boundaries) {
  var list = [];
  for (var i = 0; i < boundaries.length; i++) {
    var t = resolve(now, boundaries[i]);
    if (t !== null) list.push(t);
  }
  if (list.length === 0) return null;
  var best = list[0];
  for (i = 1; i < list.length; i++) if (minutesUntil(now, list[i]) < minutesUntil(now, best)) best = list[i];
  return best;
}
function updateNextLabels(c, now) {
  var starts = [], ends = [];
  for (var i = 0; i < c.windows.length; i++) {
    starts.push(c.windows[i].start);
    ends.push(c.windows[i].end);
  }
  c.api.state.set("nextStart", starts.length ? nextHHMM(now, starts) : null);
  c.api.state.set("nextEnd", ends.length ? nextHHMM(now, ends) : null);
}

// ---- The slots and their French texts, the six boundaries made alike ----
function tables() {
  var KIND_OPTIONS = [
    { value: "time", label: "Fixed time" },
    { value: "sunrise", label: "Sunrise" },
    { value: "sunset", label: "Sunset" }
  ];
  var SLOTS = {
    zone: { type: "zone", name: "Zone", description: "Zone the equipments belong to", required: true },
    equipments: {
      type: "equipment",
      name: "Equipments",
      description: "On/off equipments to drive on the schedule",
      required: true,
      list: true,
      constraints: {
        equipmentType: ["switch", "light_onoff", "light_dimmable", "light_color", "water_valve", "pool_pump", "water_heater"]
      }
    }
  };
  // Sowel's order: the two slots, the starts, then the ends.
  var FR = {
    zone: { name: "Zone", description: "Zone des équipements" },
    equipments: { name: "Équipements", description: "Équipements on/off à piloter sur les créneaux" }
  };
  var which = ["start", "end"], heads = ["Start", "End"], frHeads = ["Début", "Fin"];
  var n, w, prefix;
  for (w = 0; w < 2; w++) {
    for (n = 1; n <= 3; n++) {
      prefix = "slot" + n + "_" + which[w];
      FR[prefix + "_kind"] = {
        name: frHeads[w],
        description: frHeads[w] + " : heure fixe, lever ou coucher du soleil",
        options: { time: "Heure fixe", sunrise: "Lever du soleil", sunset: "Coucher du soleil" }
      };
      FR[prefix + "_time"] = { name: "Heure " + frHeads[w].toLowerCase(), description: "Utilisée si le type est Heure fixe" };
      FR[prefix + "_offset"] = { name: "Décalage +/- (min)", description: "Minutes avant (-) ou après (+) le lever/coucher" };
    }
  }
  // Sowel's order: window 1 start, window 1 end, window 2 start...
  for (n = 1; n <= 3; n++) {
    for (w = 0; w < 2; w++) {
      prefix = "slot" + n + "_" + which[w];
      SLOTS[prefix + "_kind"] = {
        type: "select",
        name: heads[w],
        description: heads[w] + ": fixed time, sunrise or sunset",
        required: n === 1,
        defaultValue: "time",
        options: KIND_OPTIONS,
        group: "slot" + n
      };
      SLOTS[prefix + "_time"] = {
        type: "time",
        name: heads[w] + " time",
        description: "Used when the type is Fixed time",
        required: false,
        hiddenWhen: { slot: prefix + "_kind", equals: ["sunrise", "sunset"] },
        group: "slot" + n
      };
      SLOTS[prefix + "_offset"] = {
        type: "number",
        name: "Offset +/- (min)",
        description: "Minutes before (-) or after (+) sunrise/sunset",
        required: false,
        defaultValue: 0,
        hiddenWhen: { slot: prefix + "_kind", equals: "time" },
        constraints: { min: -180, max: 180 },
        group: "slot" + n
      };
    }
  }
  return { slots: SLOTS, fr: FR };
}
function define(T) {
  return {
    api: 1,
    id: "schedule-on-off",
    version: "2.0.2",
    name: "Schedule On/Off",
    description:
      "Scheduled on/off for any on/off equipment: fixed time, sunrise or sunset, up to 3 daily windows",
    icon: "CalendarClock",
    i18n: {
      fr: {
        name: "Programmation horaire",
        description:
          "Plages horaires marche/arrêt pour des équipements on/off : heure fixe, lever ou coucher du soleil, jusqu'à 3 créneaux par jour",
        slots: T.fr,
        groups: { slot1: "Créneau 1", slot2: "Créneau 2", slot3: "Créneau 3" }
      }
    },
    slots: T.slots,

    // Sowel's 2.0.2 `validate`, with its texts and in its order (the host's own checks of the
    // slots follow, and can only refuse more).
    validate: function (params) {
      if (!params.zone) return "Zone is required";
      if (toIdList(params.equipments).length === 0) return "At least one equipment is required";
      if (!configured(readBoundary(params, "slot1_start")) || !configured(readBoundary(params, "slot1_end"))) {
        return "Slot 1 start and end are required (a fixed time, or sunrise/sunset)";
      }
      // Slots 2 and 3: start and end come as a pair.
      for (var n = 2; n <= 3; n++) {
        var startSet = configured(readBoundary(params, "slot" + n + "_start"));
        var endSet = configured(readBoundary(params, "slot" + n + "_end"));
        if (startSet !== endSet) return "Slot " + n + " start and end must both be set";
      }
      return "";
    },

    start: function (api) {
      var p = api.params, n, hasSun = false;
      var c = {
        api: api,
        ids: toIdList(p.equipments),
        windows: []
      };
      for (n = 1; n <= 3; n++) {
        var start = readBoundary(p, "slot" + n + "_start"), end = readBoundary(p, "slot" + n + "_end");
        if (configured(start) && configured(end)) {
          var w = { index: n, start: start, end: end };
          start.w = end.w = w;
          start.starts = true;
          start.timer = end.timer = null;
          start.warned = end.warned = false;
          c.windows.push(w);
          if (start.kind !== "time" || end.kind !== "time") hasSun = true;
        }
      }
      api.state.set("status", "idle");
      api.state.set("currentSlot", null);
      armAll(c);
      if (hasSun) {
        api.at("00:00", function () {
          rearmSun(c);
        });
      }
      var labels = [];
      for (n = 0; n < c.windows.length; n++) labels.push(windowLabel(c.windows[n]));
      api.log(
        "Recette démarrée : " + c.ids.length + " équipement(s), " + c.windows.length + " créneau(x) [" + labels.join(", ") + "]"
      );
    }
  };
}
recipe(define(tables()));
// The definition is made: its builders are not needed again, and their code and texts are freed.
tables = define = undefined;
