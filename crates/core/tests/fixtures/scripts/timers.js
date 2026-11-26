// The timers, logged when they fire.
var count = 0;
recipe({
  api: 1,
  id: "timers",
  name: "Timers",
  description: "after, every, at, cancel",
  slots: {},
  start: function (api) {
    api.after("2s", function () { api.log("after 2s"); });
    var counter = api.every("1s", function () { count++; api.log("every " + count); if (count === 3) api.cancel(counter); });
    var cancelled = api.after(1500, function () { api.log("cancelled ran"); });
    api.cancel(cancelled);
    api.at("07:00", function () { api.log("at 07:00"); });
  },
});
