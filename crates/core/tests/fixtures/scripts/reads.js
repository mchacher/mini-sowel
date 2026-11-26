// What `get` reads, logged when the zone's motion changes.
recipe({
  api: 1,
  id: "reads",
  name: "Reads",
  description: "get",
  slots: {
    lights: { type: "equipment", name: "Lights", required: true, list: true, constraints: { equipmentType: "light_onoff" } },
    first: { type: "equipment", name: "First" },
  },
  start: function (api) {
    api.onZone("motion", function () {
      api.log("first " + JSON.stringify(api.get("first", "state")));
      api.log("lights " + JSON.stringify(api.get("lights", "state")));
      api.log("zone " + JSON.stringify([api.get("zone", "motion"), api.get("zone", "luminosity"), api.get("zone", "motionSensors")]));
      api.log("power " + JSON.stringify(api.get("lights", "power")));
    });
  },
});
