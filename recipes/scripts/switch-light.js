// switch-light: the port of Sowel's `sowel-recipe-switch-light` 1.0.0 to the script API (spec 052):
// Sowel's id, slots, info, texts and behaviour, adapted for Solite where its engine differs (a
// deleted equipment leaves the settings by the generic settle, a refused order's reason is
// logged, the lights' types are checked by the slots' constraint). The top level is shared by the
// instances, so it holds functions only: each takes the instance's data `c` (made in `start`),
// which keeps an instance down to a few small closures.

// ---- What the home says ----
function isOn(value) {
  return value === true || (typeof value === "string" && value.toUpperCase() === "ON");
}
function anyLightOn(c) {
  var values = c.api.get("lights", "state");
  for (var i = 0; i < values.length; i++) if (isOn(values[i])) return true;
  return false;
}
function formatDuration(ms) {
  if (ms >= 3600000) return Math.round(ms / 3600000) + "h";
  if (ms >= 60000) return Math.round(ms / 60000) + "min";
  return Math.round(ms / 1000) + "s";
}

// ---- The failsafe timer ----
function startFailsafeTimer(c) {
  if (c.maxOn === null || c.failsafeId !== null) return;
  c.failsafeId = c.api.after(c.maxOn, function () {
    fireFailsafe(c);
  });
  c.api.state.set("failsafeExpiresAt", new Date(Date.now() + c.maxOn).toISOString());
}
function cancelFailsafeTimer(c) {
  if (c.failsafeId !== null) c.api.cancel(c.failsafeId);
  c.failsafeId = null;
}
function fireFailsafe(c) {
  c.failsafeId = null;
  c.api.state.delete("failsafeExpiresAt");
  sent(c, false, "off");
  c.api.log.warn("Failsafe: lights forced off after " + formatDuration(c.maxOn) + " max on duration");
}

// ---- Orders ----
function sent(c, on, what) {
  var refused = c.api.order("lights", "state", on ? "ON" : "OFF");
  var errors = [];
  for (var i = 0; i < refused.length; i++) errors.push(refused[i].id + ": " + refused[i].reason);
  if (errors.length > 0) c.api.log.error("Error turning " + what + " some lights: " + errors.join("; "));
}

// ---- Handlers ----
function onButtonAction(c) {
  if (anyLightOn(c)) {
    sent(c, false, "off");
    c.api.log("Button pressed — lights toggled off");
    cancelFailsafeTimer(c);
    c.api.state.delete("failsafeExpiresAt");
  } else {
    sent(c, true, "on");
    c.api.log("Button pressed — lights toggled on");
    startFailsafeTimer(c);
  }
}
function onLightChanged(c, value) {
  if (!isOn(value)) {
    if (!anyLightOn(c)) {
      cancelFailsafeTimer(c);
      c.api.state.delete("failsafeExpiresAt");
    }
  } else if (c.maxOn !== null) {
    startFailsafeTimer(c);
  }
}

recipe({
  api: 1,
  id: "switch-light",
  version: "1.0.0",
  name: "Switch Light",
  description:
    "Lights follow manual commands — any button press toggles lights on/off. For rooms without motion sensors, controlled via wall switches, remotes, or smart buttons.",
  icon: "ToggleRight",
  i18n: {
    fr: {
      name: "Lumière sur interrupteur",
      description:
        "Allume ou éteint les lumières d'une pièce via un interrupteur ou une télécommande Zigbee. Chaque appui bascule l'état des lumières.",
      slots: {
        zone: { name: "Zone", description: "Pièce contenant les lumières" },
        lights: { name: "Lumières", description: "Lumières à contrôler" },
        buttons: {
          name: "Interrupteurs",
          description: "Boutons ou télécommandes qui déclenchent l'allumage/extinction"
        },
        maxOnDuration: {
          name: "Extinction automatique",
          description: "Éteint automatiquement après cette durée (protection anti-oubli)"
        }
      }
    }
  },
  slots: {
    zone: { type: "zone", name: "Zone", description: "Zone containing the lights", required: true },
    lights: {
      type: "equipment",
      name: "Lights",
      description: "Lights to control (must belong to the selected zone)",
      required: true,
      list: true,
      constraints: { equipmentType: ["light_onoff", "light_dimmable", "light_color"] }
    },
    buttons: {
      type: "equipment",
      name: "Buttons",
      description: "Button/switch equipments that trigger toggle",
      required: true,
      list: true,
      constraints: { equipmentType: "button" }
    },
    maxOnDuration: {
      type: "duration",
      name: "Max On Duration",
      description: "Force lights off after this duration (optional failsafe)",
      required: false
    }
  },

  // Sowel's 1.0.0 `validate`, with its texts and in its order (the host's own checks of the slots
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
      if (item.zoneId !== params.zone) return 'Light "' + item.name + '" does not belong to the selected zone';
      if (item.orders.indexOf("state") < 0) return 'Light "' + item.name + '" has no "state" order binding';
    }
    ids = Array.isArray(params.buttons) ? params.buttons : [];
    if (ids.length === 0) return "At least one button is required";
    for (i = 0; i < ids.length; i++) {
      item = home.equipment(ids[i]);
      if (!item) return "Button equipment not found: " + ids[i];
      if (item.data.indexOf("action") < 0) return 'Button "' + item.name + '" has no "action" data binding';
    }
    value = params.maxOnDuration;
    if (value !== undefined && value !== null && typeof value !== "number") {
      return "Invalid duration format: " + String(value) + '. Use e.g. "10m", "30s", "1h"';
    }
    return "";
  },

  start: function (api) {
    var p = api.params;
    // The instance's data: what the functions above read and write.
    var c = {
      api: api,
      maxOn: typeof p.maxOnDuration === "number" && p.maxOnDuration > 0 ? Math.max(100, p.maxOnDuration) : null,
      failsafeId: null
    };
    api.on("buttons", "action", function () {
      onButtonAction(c);
    });
    api.on("lights", "state", function (value) {
      onLightChanged(c, value);
    });
  }
});
