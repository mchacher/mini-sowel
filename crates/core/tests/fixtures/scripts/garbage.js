// A small recipe that makes cycles on each motion change: garbage that only the collector frees.
recipe({
  api: 1,
  id: "garbage",
  name: "Garbage",
  description: "Makes cycles",
  slots: {},
  start: function (api) {
    api.onZone("motion", function () {
      for (var i = 0; i < 700; i++) { var o = { a: new Array(8).fill(i) }; o.self = o; }
      api.log("garbage made");
    });
  },
});
