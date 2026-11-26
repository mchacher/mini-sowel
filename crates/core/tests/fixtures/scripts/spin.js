// A small recipe whose zone handler works for a while, then logs.
recipe({
  api: 1,
  id: "spin",
  name: "Spin",
  description: "Works on each motion change",
  slots: {},
  start: function (api) {
    api.onZone("motion", function () {
      var n = 0;
      for (var i = 0; i < 300000; i++) { n += i; }
      api.log("spun");
    });
  },
});
