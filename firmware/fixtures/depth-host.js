// QEMU's stack check (spec 041 R6.2): recurses as deep as the stack limit allows, then recurses
// again, calling the engine (`get` and `order`) at the deepest level the limit leaves room for.
// Logs what it reached. Its instance's `start` runs the recursion, on whichever thread creates it.
recipe({
  api: 1,
  id: "depth-host",
  name: "Depth host",
  description: "Recursion with calls into the engine at its deepest, for the firmware's stack check.",
  slots: {
    lights: {
      type: "equipment",
      name: "Lights",
      list: true,
      required: true,
      constraints: { equipmentType: "light_onoff" },
    },
  },
  start: function (api) {
    var depth = 0;
    var target = 0;
    var reached = 0;
    function f() {
      depth++;
      if (target !== 0 && depth === target) {
        api.get("lights", "state");
        api.order("lights", "state", "OFF");
        reached = depth;
        return;
      }
      f();
    }
    function run(at) {
      depth = 0;
      target = at;
      try {
        f();
      } catch (e) {
        // the stack limit (or the call at the deepest level, which did not fit)
      }
    }
    run(0);
    var max = depth;
    for (var at = max - 1; at > max - 9 && reached === 0; at--) {
      run(at);
    }
    api.log("depth " + max + ", engine called at " + reached);
  },
});
