// Logs the home's time of day, as `now().localTime` reads it, at the start.
recipe({
  api: 1,
  id: "localtime",
  name: "Local time",
  description: "now().localTime",
  slots: {},
  start: function (api) {
    api.log("local " + api.now().localTime);
  },
});
