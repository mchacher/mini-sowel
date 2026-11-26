// One slot of each type, for the host's validation of the settings.
recipe({
  api: 1,
  id: "slots",
  name: "Slots",
  description: "Every slot type",
  icon: "Settings",
  i18n: { fr: { name: "Emplacements", description: "Tous les types" } },
  slots: {
    zone: { type: "zone", name: "Zone", description: "Where", required: true },
    lights: { type: "equipment", name: "Lights", required: true, list: true, constraints: { equipmentType: "light_onoff" } },
    button: { type: "equipment", name: "Button", constraints: { equipmentType: ["button", "sensor"], crossZone: true } },
    level: { type: "number", name: "Level", constraints: { min: 0, max: 100 } },
    delay: { type: "duration", name: "Delay", default: "5m" },
    mode: { type: "select", name: "Mode", options: [{ value: "eco", label: "Eco" }, { value: "boost", label: "Boost" }] },
    flag: { type: "boolean", name: "Flag" },
    wake: { type: "time", name: "Wake" },
  },
  validate: function (params) {
    if (params.level === 42) return "forty-two is taken";
    if (params.level === 43) throw new Error("validate threw");
  },
  start: function (api) {
    api.log(JSON.stringify(api.params));
  },
});
