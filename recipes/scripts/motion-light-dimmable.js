// motion-light-dimmable: the port of Sowel's `sowel-recipe-motion-light-dimmable` 1.0.2 to the
// script API (spec 054): Sowel's id, slots, info, texts and behaviour, adapted for Solite where
// its engine differs (a deleted equipment leaves the settings by the generic settle, a button that
// loses its `action` binding disables the instance, the legacy `light` key is not read, a refused
// order's reason is logged, the echo windows are counted on the wall clock, the time of day of the
// brightness slots is `api.now().localTime`, the home's time, where Sowel reads `new Date()`). The
// top level is shared by the instances, so it holds functions only: each takes the instance's data
// `c` (made in `start`).
var GRACE_MS = 5000;
var HYSTERESIS = 0.1;

// ---- What the home says ----
function isOn(value) {
  return value === true || String(value).toUpperCase() === "ON";
}
function anyLightOn(c) {
  var values = c.api.get("lights", "state");
  for (var i = 0; i < values.length; i++) if (isOn(values[i])) return true;
  return false;
}
function isTooBright(c, l) {
  return c.lux_ !== null && l !== null && l > c.lux_;
}
function isBrightEnoughToTurnOff(c, l) {
  return c.lux_ !== null && l !== null && l > c.lux_ * (1 + HYSTERESIS);
}
function isDaytime(c) {
  return c.day && c.api.now().isDaylight === true;
}
function formatDuration(ms) {
  if (ms >= 3600000) return Math.round(ms / 3600000) + "h";
  if (ms >= 60000) return Math.round(ms / 60000) + "min";
  return Math.round(ms / 1000) + "s";
}
function iso(ms) {
  return new Date(ms).toISOString();
}
function minutesOf(text) {
  var p = text.split(":").map(Number);
  return p[0] * 60 + p[1];
}

// ---- The brightness: the first slot that matches wins ----
function targetBrightness(c) {
  if (c.bright === null) return null;
  var local = c.api.now().localTime;
  // The clock is not set (the box before its first sync): no time of day, so no slot.
  if (local === null) return c.bright;
  var now = minutesOf(local);
  for (var i = 0; i < c.slots.length; i++) {
    var s = c.slots[i], from = minutesOf(s.start), to = minutesOf(s.end);
    if (from <= to) {
      if (now >= from && now < to) return s.brightness;
    } else if (now >= from || now < to) {
      return s.brightness;
    }
  }
  return c.bright;
}

// ---- State keys, rebuilt in Sowel's order at each change ----
function syncState(c) {
  var st = c.api.state;
  st.delete("overrideMode");
  st.delete("timerExpiresAt");
  st.delete("failsafeExpiresAt");
  if (c.override) st.set("overrideMode", true);
  if (c.timerAt !== null) st.set("timerExpiresAt", c.timerAt);
  if (c.failsafeAt !== null) st.set("failsafeExpiresAt", c.failsafeAt);
}
// A handler of the instance, then the state.
function bind(c, fn) {
  return function (a) {
    fn(c, a);
    syncState(c);
  };
}

// ---- Timers ----
function cancelOffTimer(c) {
  if (c.offId !== null) c.api.cancel(c.offId);
  c.offId = null;
}
function cancelFailsafe(c) {
  if (c.failsafeId !== null) c.api.cancel(c.failsafeId);
  c.failsafeId = null;
  c.failsafeAt = null;
}
function startOffTimer(c) {
  cancelOffTimer(c);
  c.offId = c.api.after(c.timeout, bind(c, fireOff));
  c.timerAt = iso(Date.now() + c.timeout);
}
function startOffTimerForOverrideClear(c) {
  cancelOffTimer(c);
  c.offId = c.api.after(c.timeout, bind(c, fireOverrideClear));
  c.timerAt = iso(Date.now() + c.timeout);
}
function armFailsafe(c) {
  c.failsafeId = c.api.after(c.maxOn, bind(c, fireFailsafe));
  c.failsafeAt = iso(Date.now() + c.maxOn);
}
// Armed once; an armed failsafe is left as it is.
function startFailsafeTimer(c) {
  if (c.maxOn === null || c.failsafeId !== null) return;
  armFailsafe(c);
}
// Restarts an armed failsafe with its full duration; it never arms one.
function resetFailsafeTimer(c) {
  if (c.maxOn === null || c.failsafeId === null) return;
  c.api.cancel(c.failsafeId);
  armFailsafe(c);
}

// ---- Primitive actions ----
function send(c, slot, value) {
  var refused = c.api.order("lights", slot, value);
  var errors = [];
  for (var i = 0; i < refused.length; i++) errors.push(refused[i].id + ": " + refused[i].reason);
  return errors;
}
function sent(c, on, what) {
  var errors = send(c, "state", on ? "ON" : "OFF");
  if (errors.length > 0) c.api.log.error("Error turning " + what + " some lights: " + errors.join("; "));
}
function inGrace(c) {
  return Date.now() < c.graceUntil;
}
function turnOn(c) {
  c.byRecipe = true;
  c.lastOn = true;
  sent(c, true, "on");
  var b = targetBrightness(c);
  if (b !== null) {
    c.brightGraceUntil = Date.now() + GRACE_MS;
    c.lastSent = b;
    var errors = send(c, "brightness", b);
    if (errors.length > 0) c.api.log.error("Error setting brightness: " + errors.join("; "));
    c.api.log("Motion detected — " + c.count + " light(s) turned on at brightness " + b);
  } else {
    c.api.log("Motion detected — " + c.count + " light(s) turned on");
  }
  startFailsafeTimer(c);
}
function turnOff(c, reason) {
  c.byRecipe = false;
  c.lastOn = false;
  c.graceUntil = Date.now() + GRACE_MS;
  sent(c, false, "off");
  c.api.log(reason);
  cancelFailsafe(c);
  c.override = false;
}

// ---- Timers firing ----
function fireOff(c) {
  c.offId = null;
  c.timerAt = null;
  turnOff(c, "No motion for " + formatDuration(c.timeout) + " — lights turned off");
}
function fireOverrideClear(c) {
  c.offId = null;
  c.timerAt = null;
  if (anyLightOn(c)) {
    c.byRecipe = false;
    c.lastOn = false;
    c.graceUntil = Date.now() + GRACE_MS;
    send(c, "state", "OFF"); // the refusals are ignored on this path
  }
  c.override = false;
  cancelFailsafe(c);
  c.api.log("No motion for " + formatDuration(c.timeout) + " — override cleared, lights off");
}
function fireFailsafe(c) {
  c.failsafeId = null;
  cancelFailsafe(c);
  cancelOffTimer(c);
  c.timerAt = null;
  // Like turning off, but the override mode is left as it is.
  c.byRecipe = false;
  c.lastOn = false;
  c.graceUntil = Date.now() + GRACE_MS;
  sent(c, false, "off");
  c.api.log.warn("Failsafe: lights forced off after " + formatDuration(c.maxOn) + " max on duration");
}

// ---- Handlers ----
function onZoneChanged(c, motion) {
  var api = c.api, l = c.zoneLux;
  if (c.override) {
    if (motion) {
      cancelOffTimer(c);
      c.timerAt = null;
      resetFailsafeTimer(c);
      api.log("Motion detected but override mode active — ignoring");
    } else if (c.offId === null) {
      startOffTimerForOverrideClear(c);
    }
    return;
  }
  var lightsOn = anyLightOn(c);
  if (lightsOn && isBrightEnoughToTurnOff(c, l)) {
    if (!c.byRecipe) {
      if (inGrace(c)) return;
      c.override = true;
      startFailsafeTimer(c);
      api.log("Light turned on manually above lux threshold — entering override mode");
      return;
    }
    cancelOffTimer(c);
    c.timerAt = null;
    turnOff(c, "Luminosity above threshold — lights turned off");
    return;
  }
  if (motion && !lightsOn) {
    if (c.byRecipe) {
      c.byRecipe = false;
      if (inGrace(c)) return;
      c.override = true;
      startOffTimerForOverrideClear(c);
      api.log("Light turned off manually while motion active — entering override mode");
      return;
    }
    if (isTooBright(c, l)) {
      api.log("Motion detected but luminosity " + l + " exceeds threshold " + c.lux_ + " — not turning on");
      return;
    }
    if (isDaytime(c)) {
      api.log("Motion detected but daytime — not turning on");
      return;
    }
    turnOn(c);
  } else if (motion && lightsOn) {
    cancelOffTimer(c);
    c.timerAt = null;
    resetFailsafeTimer(c);
  } else if (!motion && lightsOn) {
    if (c.offId === null) startOffTimer(c);
  }
}

function onLightChanged(c, value) {
  var api = c.api, lightOn = isOn(value);
  if (lightOn === c.lastOn) return;
  c.lastOn = lightOn;
  if (c.override) return;
  var motion = api.get("zone", "motion") === true;
  if (lightOn && !motion) {
    startOffTimer(c);
    startFailsafeTimer(c);
    api.log("Light turned on externally — turning off in " + formatDuration(c.timeout));
  } else if (lightOn) {
    cancelOffTimer(c);
    c.timerAt = null;
  } else {
    if (c.byRecipe && motion && Date.now() >= c.graceUntil) {
      c.byRecipe = false;
      c.override = true;
      cancelOffTimer(c);
      cancelFailsafe(c);
      c.timerAt = null;
      startOffTimerForOverrideClear(c);
      api.log("Light turned off manually while motion active — entering override mode");
      return;
    }
    c.byRecipe = false;
    if (c.offId !== null || c.failsafeId !== null) {
      cancelOffTimer(c);
      cancelFailsafe(c);
      c.timerAt = null;
      api.log("Light turned off externally — timers cancelled");
    }
  }
}

// A brightness that is not the one sent, with the lights on: a manual change.
function onBrightnessChanged(c, value) {
  if (c.override || !anyLightOn(c) || c.lastSent === null) return;
  if (Date.now() < c.brightGraceUntil || Number(value) === c.lastSent) return;
  c.override = true;
  c.api.log("Manual brightness change detected — entering override mode");
}

function onButtonAction(c) {
  if (anyLightOn(c)) {
    c.byRecipe = false;
    c.lastOn = false;
    c.graceUntil = Date.now() + GRACE_MS;
    sent(c, false, "off");
    cancelOffTimer(c);
    c.timerAt = null;
    cancelFailsafe(c);
    c.override = true;
    c.api.log("Button pressed — lights off, entering override mode");
    startOffTimerForOverrideClear(c);
  } else {
    c.override = false;
    turnOn(c);
  }
}

// ---- The start sync: the lights set to what the zone calls for ----
function syncLightsOnStart(c, motion, l) {
  if (motion && !isTooBright(c, l) && !isDaytime(c)) {
    turnOn(c);
  } else if (isDaytime(c)) {
    turnOff(c, "Recipe activated — daytime, lights off");
  } else if (motion) {
    turnOff(c, "Recipe activated — luminosity above threshold, lights off");
  } else {
    turnOff(c, "Recipe activated — no motion, lights off");
  }
}

// ---- Validation helpers ----
function given(v) {
  return v !== undefined && v !== null && v !== "";
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
function outOfRange(v) {
  var b = Number(v);
  return isNaN(b) || b < 1 || b > 254;
}

// ---- The slots and their French texts, the three brightness slots made alike ----
function tables() {
var SLOTS = {
  zone: { type: "zone", name: "Zone", description: "Zone to monitor", required: true },
  lights: {
    type: "equipment",
    name: "Lights",
    description: "Dimmable lights to control (must belong to the selected zone)",
    required: true,
    list: true,
    constraints: { equipmentType: ["light_dimmable", "light_color"] }
  },
  timeout: {
    type: "duration",
    name: "Timeout",
    description: "Delay with no motion before turning off",
    required: true,
    defaultValue: "10m"
  },
  brightness: {
    type: "number",
    name: "Brightness",
    description: "Default brightness level when no time slot matches (1-254)",
    required: false,
    constraints: { min: 1, max: 254 }
  },
  luxThreshold: {
    type: "number",
    name: "Lux Threshold",
    description:
      "Lights won't turn on when ambient brightness exceeds this value; turns off if it rises above threshold + 10% hysteresis",
    required: false,
    constraints: { min: 0 }
  },
  maxOnDuration: {
    type: "duration",
    name: "Safety Auto-off",
    description: "Force lights off after this duration even with continued motion (failsafe)",
    required: false
  },
  buttons: {
    type: "equipment",
    name: "Manual Switches",
    description: "Physical switches for manual on/off toggle",
    required: false,
    list: true,
    constraints: { equipmentType: "button" }
  },
  disableWhenDaylight: {
    type: "boolean",
    name: "Inactive During Day",
    description: "Do not turn on lights during daytime (based on sunrise/sunset and offsets from settings)",
    required: false
  }
};
var FR = {
  zone: { name: "Zone", description: "Zone à surveiller" },
  lights: { name: "Lumières", description: "Lumières dimmables à contrôler (doivent appartenir à la zone)" },
  timeout: { name: "Délai", description: "Délai sans mouvement avant extinction" },
  brightness: { name: "Luminosité", description: "Luminosité par défaut hors plages horaires (1-254)" }
};
var GROUPS = {};
for (var n = 1; n <= 3; n++) {
  SLOTS["slot" + n + "Start"] = {
    type: "time",
    name: "Slot " + n + " Start",
    description: "Start time for brightness slot " + n + " (HH:MM)",
    required: false,
    group: "slot" + n
  };
  SLOTS["slot" + n + "End"] = {
    type: "time",
    name: "Slot " + n + " End",
    description: "End time for brightness slot " + n + " (HH:MM)",
    required: false,
    group: "slot" + n
  };
  SLOTS["slot" + n + "Brightness"] = {
    type: "number",
    name: "Slot " + n + " Brightness",
    description: "Brightness level during slot " + n + " (1-254)",
    required: false,
    constraints: { min: 1, max: 254 },
    group: "slot" + n
  };
  FR["slot" + n + "Start"] = { name: "Début", description: "Heure de début de la plage " + n };
  FR["slot" + n + "End"] = { name: "Fin", description: "Heure de fin de la plage " + n };
  FR["slot" + n + "Brightness"] = { name: "Luminosité", description: "Luminosité pendant la plage " + n + " (1-254)" };
  GROUPS["slot" + n] = "Plage " + n;
}
FR.luxThreshold = { name: "Seuil lux (max)", description: "Au-dessus de ce seuil, les lumières ne s'allument pas" };
FR.maxOnDuration = {
  name: "Extinction auto (sécurité)",
  description: "Coupe les lumières après cette durée même avec mouvement — anti-oubli"
};
FR.buttons = { name: "Interrupteurs", description: "Interrupteurs physiques pour allumer/éteindre manuellement" };
FR.disableWhenDaylight = {
  name: "Inactif le jour",
  description: "Ne pas allumer pendant la journée (basé sur lever/coucher du soleil)"
};
return { slots: SLOTS, fr: FR, groups: GROUPS };
}

function define(T) {
return {
  api: 1,
  id: "motion-light-dimmable",
  version: "1.0.2",
  name: "Motion Light (Dimmable)",
  description:
    "Turns on dimmable lights when motion is detected with brightness control. Supports up to 3 brightness time slots, manual brightness override detection, lux threshold, and button toggle.",
  icon: "SunDim",
  i18n: {
    fr: {
      name: "Lumière dimmable sur mouvement",
      description:
        "Allume les lumières dimmables quand un mouvement est détecté avec contrôle de luminosité. Supporte jusqu'à 3 plages horaires de luminosité, détection de changement manuel, seuil de luminosité et boutons.",
      slots: T.fr,
      groups: T.groups
    }
  },
  slots: T.slots,

  // Sowel's 1.0.2 `validate`, with its texts and in its order (the host's own checks of the slots
  // follow, and can only refuse more).
  validate: function (params, home) {
    var i, item, ids, value;
    if (typeof params.zone !== "string" || params.zone === "") return "Zone parameter is required";
    if (!home.zone(params.zone)) return "Zone not found: " + params.zone;
    ids = Array.isArray(params.lights) ? params.lights : [];
    if (ids.length === 0) return "At least one light is required";
    for (i = 0; i < ids.length; i++) {
      item = home.equipment(ids[i]);
      if (!item) return "Light equipment not found: " + ids[i];
      if (item.type !== "light_dimmable" && item.type !== "light_color") {
        return 'Light "' + item.name + '" is type "' + item.type + '" but this recipe requires light_dimmable or light_color';
      }
      if (item.zoneId !== params.zone) return 'Light "' + item.name + '" does not belong to the selected zone';
      if (item.orders.indexOf("state") < 0) return 'Light "' + item.name + '" has no "state" order binding';
    }
    if (params.timeout && typeof params.timeout !== "number") return durationText(params.timeout);
    value = params.luxThreshold;
    if (given(value) && (isNaN(Number(value)) || Number(value) < 0)) return "luxThreshold must be a non-negative number";
    value = params.maxOnDuration;
    if (given(value) && typeof value !== "number") return durationText(value);
    ids = Array.isArray(params.buttons) ? params.buttons : [];
    for (i = 0; i < ids.length; i++) {
      item = home.equipment(ids[i]);
      if (!item) return "Button equipment not found: " + ids[i];
      if (item.data.indexOf("action") < 0) return 'Button "' + item.name + '" has no "action" data binding';
    }
    if (given(params.brightness) && outOfRange(params.brightness)) return "brightness must be between 1 and 254";
    for (i = 1; i <= 3; i++) {
      var s = params["slot" + i + "Start"], e = params["slot" + i + "End"], b = params["slot" + i + "Brightness"];
      var count = (given(s) ? 1 : 0) + (given(e) ? 1 : 0) + (given(b) ? 1 : 0);
      if (count > 0 && count < 3) return "Slot " + i + ": start, end, and brightness must all be provided or all omitted";
      if (given(s) && typeof s === "string" && !hhmm(s)) return "slot" + i + "Start must be in HH:MM format";
      if (given(e) && typeof e === "string" && !hhmm(e)) return "slot" + i + "End must be in HH:MM format";
      if (given(b) && outOfRange(b)) return "slot" + i + "Brightness must be between 1 and 254";
    }
    return "";
  },

  start: function (api) {
    var p = api.params, i;
    var maxOn = given(p.maxOnDuration) ? Math.max(100, p.maxOnDuration) : null;
    // The instance's data: what the functions above read and write.
    var c = {
      api: api,
      timeout: Math.max(100, p.timeout || 600000),
      lux_: given(p.luxThreshold) ? Number(p.luxThreshold) : null,
      maxOn: maxOn,
      day: p.disableWhenDaylight === true,
      count: (p.lights || []).length,
      bright: given(p.brightness) ? Number(p.brightness) : null,
      slots: [],
      lastSent: null,
      brightGraceUntil: 0,
      offId: null,
      failsafeId: null,
      override: false,
      byRecipe: false,
      lastOn: false,
      graceUntil: 0, // wall clock
      timerAt: null, // the state keys, as ISO texts
      failsafeAt: null,
      zoneLux: null // the zone's luminosity, as the last zone event said
    };
    for (i = 1; i <= 3; i++) {
      var s = p["slot" + i + "Start"], e = p["slot" + i + "End"], b = p["slot" + i + "Brightness"];
      if (typeof s === "string" && s && typeof e === "string" && e && given(b)) {
        c.slots.push({ start: s, end: e, brightness: Number(b) });
      }
    }
    if (api.get("zone", "motionSensors") === 0) {
      api.log.warn("Zone has no motion sensors — recipe will never trigger");
    }
    c.zoneLux = api.get("zone", "luminosity");
    c.byRecipe = c.lastOn = anyLightOn(c);
    syncLightsOnStart(c, api.get("zone", "motion") === true, c.zoneLux);
    c.graceUntil = 0;
    syncState(c);

    // The luminosity handler comes first: both get the same event, in this order.
    api.onZone("luminosity", function (value) {
      c.zoneLux = value;
    });
    api.onZone("motion", bind(c, onZoneChanged));
    api.on("lights", "state", bind(c, onLightChanged));
    if ((p.buttons || []).length > 0) api.on("buttons", "action", bind(c, onButtonAction));
    if (c.bright !== null) api.on("lights", "brightness", bind(c, onBrightnessChanged));
  }
};
}
recipe(define(tables()));
// The definition is made: its builders are not needed again, and their code and texts are freed.
tables = define = undefined;
