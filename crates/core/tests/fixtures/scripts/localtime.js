// Logs the home's time of day and weekday, as `now().localTime` and `localWeekday` read them, at the start.
recipe({
  api: 1,
  id: "localtime",
  name: "Local time",
  description: "now().localTime",
  slots: {},
  start: function (api) {
    var n = api.now();
    api.log("local " + n.localTime);
    api.log("weekday " + n.localWeekday);
  },
});
