// A recipe that logs what it hears: the events of its light, its zone, its motion sensors.
recipe({
  api: 1,
  id: "probe",
  name: "Probe",
  description: "Logs what it hears",
  version: "1.0.0",
  slots: {
    lights: { type: "equipment", name: "Lights", required: true, list: true, equipmentType: "light_onoff" },
    timeout: { type: "duration", defaultValue: "10m" },
  },
  start: function (api) {
    api.log("started " + JSON.stringify(api.params));
    api.on("lights", "state", function (value, id) { api.log("light " + id + " " + value); });
    api.onZone("motion", function (v) { api.log("zone motion " + v); });
    api.onZone("luminosity", function (v) { api.log("zone luminosity " + v); });
    api.onZone("motionSensors", function (v) { api.log("zone sensors " + v); });
    api.onMotionReport(function (v, id) { api.log("report " + v + " " + id); });
  },
});
