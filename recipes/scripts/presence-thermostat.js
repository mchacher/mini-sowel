// presence-thermostat: the port of Sowel's `sowel-recipe-presence-thermostat` to the script API
// (spec 055): Sowel's id, slots, info, texts and behaviour, adapted for Solite where its engine
// differs (the time of day and the weekday are the home's, `api.now().localTime` and
// `localWeekday`, where Sowel reads `new Date()`; while the clock is not set there is no time of
// day, so no night or preheat window is active until it is; a refused order's reason is logged;
// the `set_mode` action of the thermostat's card does not exist in Solite). The top level is
// shared by the instances, so it holds functions only: each takes the instance's data `c` (made
// in `start`). `api.now()` is read once per entry point and passed down (the engine's stack is
// small).
var GRACE_MS = 5000;

// ---- Time windows ----
function minutesOf(text) {
  var p = text.split(":").map(Number);
  return p[0] * 60 + p[1];
}
// The clock of an entry point: minutes of the day and weekday, or null while it is not set.
function clockOf(c) {
  var n = c.api.now();
  if (n.localTime === null) return null;
  return { min: minutesOf(n.localTime), day: n.localWeekday };
}
function inWindow(t, start, end) {
  var now = t.min, s = minutesOf(start), e = minutesOf(end);
  if (s <= e) return now >= s && now < e;
  return now >= s || now < e;
}
function isInNightWindow(c, t) {
  return c.hasNight && t !== null && inWindow(t, c.nightStart, c.nightEnd);
}
function isInPreheatWindow(c, t) {
  if (!c.hasPreheat || t === null) return false;
  var weekend = t.day === 0 || t.day === 6;
  if (weekend && c.weStart !== null && c.weEnd !== null) return inWindow(t, c.weStart, c.weEnd);
  if (!weekend && c.wdStart !== null && c.wdEnd !== null) return inWindow(t, c.wdStart, c.wdEnd);
  return false;
}
function hasMotion(c) {
  return c.api.get("zone", "motion") === true;
}
function formatDuration(ms) {
  if (ms >= 3600000) return Math.round(ms / 3600000) + "h";
  if (ms >= 60000) return Math.round(ms / 60000) + "min";
  return Math.round(ms / 1000) + "s";
}

// ---- State ----
function cancelEcoTimer(c) {
  if (c.ecoId !== null) c.api.cancel(c.ecoId);
  c.ecoId = null;
}
function clearTimerState(c) {
  c.api.state.delete("timerExpiresAt");
}
function clearOverrideMode(c) {
  if (!c.override) return;
  c.override = false;
  c.api.state.delete("overrideMode");
}

// ---- Setpoints ----
function dispatch(c, mode, target, reason) {
  c.mode = mode;
  c.api.state.set("currentMode", mode);
  c.graceUntil = Date.now() + GRACE_MS;
  c.lastSent = target;
  var refused = c.api.order("thermostat", "setpoint", target);
  for (var i = 0; i < refused.length; i++) {
    c.api.log.error("Setpoint → " + target + "°C FAILED: " + refused[i].reason);
  }
  c.api.log(reason + " — setpoint → " + target + "°C (" + mode + ")");
}
function setComfort(c, reason) {
  dispatch(c, "comfort", c.comfort, reason);
  c.api.state.delete("cocoonMode");
}
function setEco(c, reason) {
  dispatch(c, "eco", c.eco, reason);
  c.api.state.delete("cocoonMode");
  clearOverrideMode(c);
}
function setCocoon(c, reason) {
  c.mode = "cocoon";
  c.api.state.set("cocoonMode", true);
  cancelEcoTimer(c);
  clearTimerState(c);
  dispatch(c, "cocoon", c.cocoon, reason);
}
function setNight(c, reason) {
  cancelEcoTimer(c);
  clearTimerState(c);
  dispatch(c, "night", c.night, reason);
  c.api.state.delete("cocoonMode");
}

// ---- The eco timer ----
function startEcoTimer(c, suffix) {
  cancelEcoTimer(c);
  c.ecoId = c.api.after(c.timeout, function () {
    c.ecoId = null;
    clearTimerState(c);
    setEco(c, "No motion for " + formatDuration(c.timeout) + suffix);
  });
  c.api.state.set("timerExpiresAt", new Date(Date.now() + c.timeout).toISOString());
}

// ---- Events ----
function onZoneChanged(c, motion) {
  // Night mode: fixed setpoint, ignore presence changes.
  if (c.mode === "night") return;
  // Override mode: the recipe is suspended, only the room's vacancy is tracked.
  if (c.override) {
    if (motion) {
      cancelEcoTimer(c);
      clearTimerState(c);
    } else {
      startEcoTimer(c, " — override cleared");
    }
    return;
  }
  if (motion) {
    cancelEcoTimer(c);
    clearTimerState(c);
    if (c.mode === "eco") setComfort(c, "Motion detected");
    // Cocoon or comfort: stay, the eco timer is cancelled.
  } else {
    // Preheat protects comfort but not cocoon.
    if (c.mode === "comfort" && isInPreheatWindow(c, clockOf(c))) {
      cancelEcoTimer(c);
      clearTimerState(c);
      return;
    }
    if (c.mode === "comfort" || c.mode === "cocoon") startEcoTimer(c, "");
  }
}
function onSetpointChanged(c, value) {
  if (c.override) return;
  if (Date.now() < c.graceUntil) return;
  // Ignore the echo: the same value as what the recipe sent.
  if (c.lastSent !== null && Number(value) === c.lastSent) return;
  c.override = true;
  c.api.state.set("overrideMode", true);
  c.api.log("Manual setpoint change detected — entering override mode");
}
function onButtonAction(c) {
  if (c.override) return;
  if (c.mode === "night") return;
  if (c.mode === "cocoon") {
    // Second press: out of cocoon.
    cancelEcoTimer(c);
    clearTimerState(c);
    if (hasMotion(c)) setComfort(c, "Cocoon deactivated by button — motion present");
    else setEco(c, "Cocoon deactivated by button — no motion");
  } else {
    setCocoon(c, "Button pressed");
  }
}

// ---- The periodic check (every minute) ----
function checkPreheatTransition(c, t) {
  var inPreheat = isInPreheatWindow(c, t);
  if (c.override) {
    // Tracked, not acted on.
    c.wasPreheat = inPreheat;
    return;
  }
  if (inPreheat && !c.wasPreheat) {
    cancelEcoTimer(c);
    clearTimerState(c);
    if (c.mode === "eco") setComfort(c, "Preheat window started");
  }
  if (!inPreheat && c.wasPreheat) {
    if (c.mode === "comfort" && !hasMotion(c)) startEcoTimer(c, "");
  }
  c.wasPreheat = inPreheat;
}
function checkPeriodicTransitions(c) {
  var t = clockOf(c);
  var inNight = isInNightWindow(c, t);
  if (inNight && !c.wasNight) {
    cancelEcoTimer(c);
    clearTimerState(c);
    setNight(c, "Night window started");
    c.wasNight = true;
    c.wasPreheat = isInPreheatWindow(c, t);
    return;
  }
  if (!inNight && c.wasNight) {
    c.wasNight = false;
    if (c.mode === "night") {
      cancelEcoTimer(c);
      clearTimerState(c);
      if (isInPreheatWindow(c, t)) setComfort(c, "Night ended — preheat window active");
      else if (hasMotion(c)) setComfort(c, "Night ended — motion detected");
      else setEco(c, "Night ended — no motion");
    }
  }
  c.wasNight = inNight;
  if (c.mode !== "night") checkPreheatTransition(c, t);
}

// ---- The start sync ----
function syncOnStart(c, t) {
  if (isInNightWindow(c, t)) {
    setNight(c, "Recipe activated — night window active");
    return;
  }
  if (isInPreheatWindow(c, t)) setComfort(c, "Recipe activated — preheat window active");
  else if (hasMotion(c)) setComfort(c, "Recipe activated — motion detected");
  else setEco(c, "Recipe activated — no motion");
}

// ---- Validation helpers ----
function given(v) {
  return v !== undefined && v !== null && v !== "";
}
function str(v) {
  return typeof v === "string" && v !== "" ? v : null;
}
// Two digits, a colon, two digits (the engine has no regular expressions).
function hhmm(v) {
  if (typeof v !== "string" || v.length !== 5 || v.charAt(2) !== ":") return false;
  for (var i = 0; i < 5; i++) {
    var d = v.charCodeAt(i);
    if (i !== 2 && (d < 48 || d > 57)) return false;
  }
  return true;
}
function durationText(v) {
  return "Invalid duration format: " + String(v) + '. Use e.g. "10m", "30s", "1h"';
}
function idList(value) {
  var out = [], list = value, i;
  if (typeof value === "string" && value.length > 0) list = value.split(",");
  if (!Array.isArray(list)) return out;
  for (i = 0; i < list.length; i++) if (typeof list[i] === "string" && list[i] !== "") out.push(list[i]);
  return out;
}
function pairText(params, startKey, endKey) {
  var start = params[startKey], end = params[endKey];
  var hasStart = given(start), hasEnd = given(end);
  if (hasStart !== hasEnd) return startKey + " and " + endKey + " must both be provided or both omitted";
  if (hasStart && typeof start === "string" && !hhmm(start)) return startKey + " must be in HH:MM format";
  if (hasEnd && typeof end === "string" && !hhmm(end)) return endKey + " must be in HH:MM format";
  return "";
}

// ---- The slots and their texts ----
function tables() {
var SLOTS = {
  zone: { type: "zone", name: "Zone", description: "Zone to monitor for presence", required: true },
  thermostat: {
    type: "equipment",
    name: "Thermostat",
    description: "Thermostat equipment (must have a 'setpoint' order binding)",
    required: true,
    constraints: { equipmentType: "thermostat" }
  },
  comfortTemp: {
    type: "number",
    name: "Comfort Temperature",
    description: "Setpoint when presence is detected (°C)",
    required: true
  },
  ecoTemp: {
    type: "number",
    name: "Eco Temperature",
    description: "Setpoint after absence timeout (°C)",
    required: true
  },
  timeout: {
    type: "duration",
    name: "Timeout",
    description: "Delay with no motion before switching to eco",
    required: true,
    defaultValue: "30m"
  },
  nightTemp: {
    type: "number",
    name: "Night Temperature",
    description: "Fixed setpoint during the night window, regardless of presence (°C, optional)",
    required: false,
    group: "night"
  },
  nightStart: {
    type: "time",
    name: "Night Start",
    description: "Start of night window (HH:MM)",
    required: false,
    group: "night"
  },
  nightEnd: {
    type: "time",
    name: "Night End",
    description: "End of night window (HH:MM)",
    required: false,
    group: "night"
  },
  preheatStart: {
    type: "time",
    name: "Preheat Start (Weekday)",
    description: "Start of weekday preheat window (HH:MM, Mon-Fri)",
    required: false,
    group: "preheat"
  },
  preheatEnd: {
    type: "time",
    name: "Preheat End (Weekday)",
    description: "End of weekday preheat window (HH:MM, Mon-Fri)",
    required: false,
    group: "preheat"
  },
  weekendPreheatStart: {
    type: "time",
    name: "Preheat Start (Weekend)",
    description: "Start of weekend preheat window (HH:MM, Sat-Sun)",
    required: false,
    group: "preheat"
  },
  weekendPreheatEnd: {
    type: "time",
    name: "Preheat End (Weekend)",
    description: "End of weekend preheat window (HH:MM, Sat-Sun)",
    required: false,
    group: "preheat"
  },
  buttons: {
    type: "equipment",
    name: "Cocoon Buttons",
    description: "Button equipments that toggle cocoon mode (optional)",
    required: false,
    list: true,
    constraints: { equipmentType: "button" },
    group: "cocoon"
  },
  cocoonTemp: {
    type: "number",
    name: "Cocoon Temperature",
    description: "Boosted setpoint when cocoon is activated by button (°C)",
    required: false,
    group: "cocoon"
  }
};
var FR = {
  zone: { name: "Zone", description: "Zone à surveiller" },
  thermostat: { name: "Thermostat", description: "Équipement thermostat (doit avoir un binding d'ordre 'setpoint')" },
  comfortTemp: { name: "Température confort", description: "Consigne quand il y a de la présence (°C)" },
  ecoTemp: { name: "Température éco", description: "Consigne après le délai d'absence (°C)" },
  timeout: { name: "Délai", description: "Délai sans mouvement avant passage en éco" },
  nightTemp: {
    name: "Température nuit",
    description: "Consigne fixe appliquée pendant la plage nocturne, indépendamment de la présence (°C, optionnel)"
  },
  nightStart: { name: "Début nuit", description: "Début de la plage nocturne (HH:MM)" },
  nightEnd: { name: "Fin nuit", description: "Fin de la plage nocturne (HH:MM)" },
  preheatStart: { name: "Début préchauffe (semaine)", description: "Début de la préchauffe en semaine (HH:MM, lun-ven)" },
  preheatEnd: { name: "Fin préchauffe (semaine)", description: "Fin de la préchauffe en semaine (HH:MM, lun-ven)" },
  weekendPreheatStart: {
    name: "Début préchauffe (week-end)",
    description: "Début de la préchauffe le week-end (HH:MM, sam-dim)"
  },
  weekendPreheatEnd: {
    name: "Fin préchauffe (week-end)",
    description: "Fin de la préchauffe le week-end (HH:MM, sam-dim)"
  },
  buttons: { name: "Boutons cocoon", description: "Boutons qui activent le mode cocoon (optionnel)" },
  cocoonTemp: {
    name: "Température cocoon",
    description: "Consigne boostée quand le cocoon est activé par bouton (°C)"
  }
};
return { slots: SLOTS, fr: FR };
}

function define(T) {
return {
  api: 1,
  id: "presence-thermostat",
  version: "1.2.0",
  name: "Presence Thermostat",
  description:
    "Adjusts thermostat setpoint based on zone presence. Sends comfort temperature when motion is detected, switches to eco after a timeout with no motion. Supports night mode (fixed setpoint during night window, ignoring presence), weekday/weekend preheat windows, cocoon mode (button-triggered boost), and manual override detection.",
  icon: "Thermometer",
  i18n: {
    fr: {
      name: "Thermostat présence",
      description:
        "Ajuste la consigne du thermostat en fonction de la présence dans la zone. Envoie la température confort quand un mouvement est détecté, passe en éco après un délai sans mouvement. Supporte un mode nuit (consigne fixe pendant la plage nocturne, indépendamment de la présence), des plages de préchauffe semaine/week-end, un mode cocoon (boost par bouton), et la détection de changement manuel.",
      slots: T.fr,
      groups: { night: "Nuit", preheat: "Préchauffe", cocoon: "Cocoon" }
    },
    en: {
      name: "Presence Thermostat",
      description:
        "Adjusts thermostat setpoint based on zone presence. Sends comfort temperature when motion is detected, switches to eco after a timeout with no motion.",
      groups: { night: "Night", preheat: "Preheat", cocoon: "Cocoon" }
    }
  },
  slots: T.slots,

  // Sowel's `validate`, with its texts and in its order (the host's own checks of the slots
  // follow, and can only refuse more).
  validate: function (params, home) {
    var i, item, ids, text;
    if (typeof params.zone !== "string" || params.zone === "") return "Zone parameter is required";
    if (!home.zone(params.zone)) return "Zone not found: " + params.zone;
    if (typeof params.thermostat !== "string" || params.thermostat === "") return "Thermostat parameter is required";
    item = home.equipment(params.thermostat);
    if (!item) return "Thermostat equipment not found: " + params.thermostat;
    if (item.zoneId !== params.zone) return 'Thermostat "' + item.name + '" does not belong to the selected zone';
    if (item.orders.indexOf("setpoint") < 0) return 'Thermostat "' + item.name + '" has no "setpoint" order binding';
    if (params.comfortTemp === undefined || params.comfortTemp === null) return "comfortTemp is required";
    if (isNaN(Number(params.comfortTemp))) return "comfortTemp must be a number";
    if (params.ecoTemp === undefined || params.ecoTemp === null) return "ecoTemp is required";
    if (isNaN(Number(params.ecoTemp))) return "ecoTemp must be a number";
    if (params.timeout && typeof params.timeout !== "number") return durationText(params.timeout);
    var nt = params.nightTemp, ns = params.nightStart, ne = params.nightEnd;
    var hasNt = given(nt), hasNs = given(ns), hasNe = given(ne);
    if (hasNt && (!hasNs || !hasNe)) return "nightTemp requires nightStart and nightEnd";
    if ((hasNs || hasNe) && !hasNt) return "nightStart/nightEnd require nightTemp";
    if (hasNs && hasNe) {
      if (typeof ns === "string" && !hhmm(ns)) return "nightStart must be in HH:MM format";
      if (typeof ne === "string" && !hhmm(ne)) return "nightEnd must be in HH:MM format";
    }
    if (hasNt && isNaN(Number(nt))) return "nightTemp must be a number";
    text = pairText(params, "preheatStart", "preheatEnd");
    if (text !== "") return text;
    text = pairText(params, "weekendPreheatStart", "weekendPreheatEnd");
    if (text !== "") return text;
    ids = idList(params.buttons);
    var hasCocoon = given(params.cocoonTemp);
    if (ids.length > 0 && !hasCocoon) return "cocoonTemp is required when buttons are configured";
    if (hasCocoon && ids.length === 0) return "buttons are required when cocoonTemp is configured";
    if (hasCocoon && isNaN(Number(params.cocoonTemp))) return "cocoonTemp must be a number";
    for (i = 0; i < ids.length; i++) {
      item = home.equipment(ids[i]);
      if (!item) return "Button equipment not found: " + ids[i];
      if (item.data.indexOf("action") < 0) return 'Button "' + item.name + '" has no "action" data binding';
    }
    return "";
  },

  start: function (api) {
    var p = api.params;
    var c = {
      api: api,
      comfort: Number(p.comfortTemp),
      eco: Number(p.ecoTemp),
      timeout: Math.max(100, p.timeout || 1800000),
      night: given(p.nightTemp) ? Number(p.nightTemp) : null,
      nightStart: str(p.nightStart),
      nightEnd: str(p.nightEnd),
      wdStart: str(p.preheatStart),
      wdEnd: str(p.preheatEnd),
      weStart: str(p.weekendPreheatStart),
      weEnd: str(p.weekendPreheatEnd),
      cocoon: given(p.cocoonTemp) ? Number(p.cocoonTemp) : null,
      hasNight: false,
      hasPreheat: false,
      mode: "eco",
      override: false,
      lastSent: null,
      graceUntil: 0, // wall clock
      ecoId: null,
      wasPreheat: false,
      wasNight: false
    };
    c.hasNight = c.nightStart !== null && c.nightEnd !== null && c.night !== null;
    c.hasPreheat = (c.wdStart !== null && c.wdEnd !== null) || (c.weStart !== null && c.weEnd !== null);

    api.state.delete("overrideMode");
    api.state.delete("cocoonMode");
    api.state.set("currentMode", "eco");
    if (api.get("zone", "motionSensors") === 0) {
      api.log.warn("Zone has no motion sensors — recipe will only work with preheat windows");
    }

    api.onZone("motion", function (motion) {
      onZoneChanged(c, motion === true);
    });
    api.on("thermostat", "setpoint", function (value) {
      onSetpointChanged(c, value);
    });
    if ((p.buttons || []).length > 0) {
      api.on("buttons", "action", function () {
        onButtonAction(c);
      });
    }
    var t = clockOf(c);
    if (c.hasPreheat || c.hasNight) {
      c.wasPreheat = isInPreheatWindow(c, t);
      c.wasNight = isInNightWindow(c, t);
      api.every(60000, function () {
        checkPeriodicTransitions(c);
      });
    }
    syncOnStart(c, t);
  }
};
}
recipe(define(tables()));
// The definition is made: its builders are not needed again, and their code and texts are freed.
tables = define = undefined;
