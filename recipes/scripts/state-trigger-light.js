// state-trigger-light: the port of Sowel's `sowel-recipe-state-trigger-light` 0.2.3 to the script
// API (spec 055): Sowel's id, slots, info, texts and behaviour, adapted for Solite where its
// engine differs (a deleted equipment leaves the settings by the generic settle, a refused
// order's reason is logged, the lights' type is also checked by the slots' constraint). The top
// level is shared by the instances, so it holds functions only: each takes the instance's data
// `c` (made in `start`).
var TURN_OFF_GRACE_MS = 2000;

// ---- What the home says ----
function isOnValue(value) {
  return value === true || String(value).toUpperCase() === "ON";
}
function asString(value) {
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  return "";
}
function anyLightOn(c) {
  var values = c.api.get("lights", "state");
  for (var i = 0; i < values.length; i++) if (isOnValue(values[i])) return true;
  return false;
}
function isNight(c) {
  // No coordinates (isDaylight null): night, as Sowel.
  return c.api.now().isDaylight !== true;
}
function formatDuration(ms) {
  if (ms >= 3600000) return Math.round(ms / 3600000) + "h";
  if (ms >= 60000) return Math.round(ms / 60000) + "min";
  return Math.round(ms / 1000) + "s";
}

// ---- Orders ----
function orderLights(c, on) {
  var refused = c.api.order("lights", "state", on ? "ON" : "OFF");
  var errors = [];
  for (var i = 0; i < refused.length; i++) errors.push(refused[i].id + ": " + refused[i].reason);
  if (errors.length > 0) {
    c.api.log.error("Error turning " + (on ? "on" : "off") + " some lights: " + errors.join("; "));
  }
}

// ---- The off timer ----
function cancelOffTimer(c) {
  if (c.offId !== null) c.api.cancel(c.offId);
  c.offId = null;
}
function turnOff(c, reason) {
  orderLights(c, false);
  c.api.log(reason);
  c.graceUntil = Date.now() + TURN_OFF_GRACE_MS;
  cancelOffTimer(c);
  c.api.state.delete("expiresAt");
}
function armOffTimer(c, ms) {
  cancelOffTimer(c);
  ms = Math.max(100, ms);
  c.offId = c.api.after(ms, function () {
    c.offId = null;
    turnOff(c, "State trigger off after " + formatDuration(c.duration));
  });
  c.api.state.set("expiresAt", new Date(Date.now() + ms).toISOString());
}

// ---- The trigger ----
function fire(c) {
  if (c.nightOnly && !isNight(c)) {
    c.api.log("State trigger ignored — daytime");
    return;
  }
  if (anyLightOn(c)) {
    c.api.log("State trigger ignored — at least one light already on");
    return;
  }
  orderLights(c, true);
  c.api.log("State trigger fired — " + c.count + " light(s) on for " + formatDuration(c.duration));
  armOffTimer(c, c.duration);
}
// Sowel tracks the last state itself: a trigger that keeps re-reporting the target does not fire.
function onTriggerState(c, raw) {
  var value = asString(raw);
  var wasTarget = c.lastTrigger === c.target;
  c.lastTrigger = value;
  if (value !== c.target) return;
  if (wasTarget) return;
  fire(c);
}
// Only a real ON -> OFF transition of a light counts (and not our own echo).
function onLightState(c, raw, id) {
  var on = isOnValue(raw);
  var wasOn = c.seenOn[id] === true;
  c.seenOn[id] = on;
  if (on || !wasOn) return;
  if (Date.now() < c.graceUntil) return;
  if (c.offId !== null && !anyLightOn(c)) {
    cancelOffTimer(c);
    c.api.state.delete("expiresAt");
    c.api.log("Lights turned off externally — timer cancelled");
  }
}

// ---- Validation helpers ----
function given(v) {
  return v !== undefined && v !== null && v !== "";
}
function lightIds(value) {
  var out = [], list = value, i;
  if (typeof value === "string" && value.length > 0) list = value.split(",");
  if (!Array.isArray(list)) return out;
  for (i = 0; i < list.length; i++) if (typeof list[i] === "string" && list[i] !== "") out.push(list[i]);
  return out;
}

recipe({
  api: 1,
  id: "state-trigger-light",
  version: "0.2.3",
  name: "State-Triggered Light",
  description:
    "Turn lights on for a fixed duration when a watched equipment's state changes to a configured target value (e.g. gate → open at night).",
  icon: "Bell",
  i18n: {
    fr: {
      name: "Lumière sur changement d'état",
      description:
        "Allume des lumières pour une durée fixe quand l'état d'un équipement passe à une valeur cible (ex: portail ouvert la nuit).",
      slots: {
        zone: { name: "Zone", description: "Zone des lumières" },
        trigger: {
          name: "Équipement déclencheur",
          description: "Équipement dont l'état est surveillé (alias 'state')"
        },
        stateValue: {
          name: "Valeur cible",
          description: "La recette se déclenche quand l'état devient exactement cette valeur (ex: 'open', 'ON')"
        },
        lights: { name: "Lumières", description: "Lumières à allumer" },
        duration: { name: "Durée", description: "Durée d'allumage après le déclenchement" },
        nightOnly: { name: "Seulement la nuit", description: "Ne se déclenche que si le soleil est couché" }
      }
    }
  },
  slots: {
    zone: { type: "zone", name: "Zone", description: "Zone where the lights live", required: true },
    trigger: {
      type: "equipment",
      name: "Trigger equipment",
      description: "Equipment whose `state` alias is watched",
      required: true,
      constraints: { crossZone: true }
    },
    stateValue: {
      type: "text",
      name: "Target state value",
      description: "Recipe fires when the equipment's state changes to this exact value (e.g. 'open', 'ON', 'true')",
      required: true
    },
    lights: {
      type: "equipment",
      name: "Lights",
      description: "Lights to turn on",
      required: true,
      list: true,
      constraints: { equipmentType: "light_onoff", includeDescendants: true }
    },
    duration: {
      type: "duration",
      name: "Duration",
      description: "How long the lights stay on after a trigger",
      required: true,
      defaultValue: "5m"
    },
    nightOnly: {
      type: "boolean",
      name: "Night only",
      description: "Only fire when it's dark outside (uses sunrise/sunset)",
      required: false,
      defaultValue: true
    }
  },

  // Sowel's 0.2.3 `validate`, with its texts and in its order (the host's own checks of the slots
  // follow, and can only refuse more).
  validate: function (params, home) {
    var i, item, ids, value;
    if (typeof params.zone !== "string" || params.zone === "") return "Zone is required";
    if (!home.zone(params.zone)) return "Zone not found";
    if (typeof params.trigger !== "string" || params.trigger === "") return "Trigger equipment is required";
    item = home.equipment(params.trigger);
    if (!item) return "Trigger equipment not found";
    if (item.data.indexOf("state") < 0) return 'Trigger "' + item.name + '" has no "state" data binding';
    value = params.stateValue;
    if (typeof value !== "string" || value.trim() === "") return "Target state value is required";
    ids = lightIds(params.lights);
    if (ids.length === 0) return "At least one light is required";
    for (i = 0; i < ids.length; i++) {
      item = home.equipment(ids[i]);
      if (!item) return "Light " + ids[i] + " not found";
      if (item.type !== "light_onoff") return '"' + item.name + '" is not a light_onoff equipment';
    }
    if (ids.indexOf(params.trigger) >= 0) return "Trigger equipment cannot also be a light controlled by this recipe";
    value = params.duration;
    if (given(value)) {
      if (typeof value !== "number") return "Invalid duration format: " + String(value) + '. Use e.g. "10m", "30s", "1h"';
      if (!isFinite(value) || value <= 0) return "Duration must be > 0";
    }
    return "";
  },

  start: function (api) {
    var p = api.params, i, ids = lightIds(p.lights);
    var c = {
      api: api,
      target: String(p.stateValue).trim(),
      duration: given(p.duration) ? p.duration : 300000,
      nightOnly: p.nightOnly !== false,
      count: ids.length,
      offId: null,
      graceUntil: 0, // wall clock
      lastTrigger: "",
      seenOn: {}
    };
    try {
      c.lastTrigger = asString(api.get("trigger", "state"));
    } catch (e) {
      c.lastTrigger = "";
    }
    try {
      var now = api.get("lights", "state");
      for (i = 0; i < ids.length; i++) c.seenOn[ids[i]] = isOnValue(now[i]);
    } catch (e) {
      for (i = 0; i < ids.length; i++) c.seenOn[ids[i]] = false;
    }

    // Restore from the state kept across a restart.
    var kept = api.state.get("expiresAt");
    if (typeof kept === "string") {
      var expires = Date.parse(kept);
      if (isFinite(expires)) {
        var remaining = expires - Date.now();
        // A clock that was not set when the deadline was written makes it decades away: never
        // wait longer than the configured duration.
        if (remaining > c.duration) remaining = c.duration;
        if (remaining > 0) {
          armOffTimer(c, remaining);
          api.log("Resumed off-timer with " + formatDuration(remaining) + " remaining");
        } else if (anyLightOn(c)) {
          turnOff(c, "Resumed state trigger past deadline — lights off");
        } else {
          api.state.delete("expiresAt");
        }
      } else {
        api.state.delete("expiresAt");
      }
    }

    api.on("trigger", "state", function (value) {
      onTriggerState(c, value);
    });
    api.on("lights", "state", function (value, id) {
      onLightState(c, value, id);
    });
  }
});
