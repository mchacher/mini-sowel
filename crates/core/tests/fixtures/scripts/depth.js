// How deep a recursion goes before the stack limit stops it, logged.
recipe({
  api: 1,
  id: "depth",
  name: "Depth",
  description: "recursion",
  slots: {},
  start: function (api) {
    api.onZone("motion", function () {
      var depth = 0;
      function f() { depth++; f(); }
      try { f(); } catch (e) { api.log("depth " + depth); }
    });
  },
});
