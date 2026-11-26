// What a script may not reach: ids, other slots, the engine's functions, what Solite lacks.
recipe({
  api: 1,
  id: "sandbox",
  name: "Sandbox",
  description: "Tries what it may not",
  slots: {
    lights: { type: "equipment", name: "Lights", required: true, list: true, constraints: { equipmentType: "light_onoff" } },
  },
  start: function (api) {
    var id = api.params.lights[0];
    var tries = {
      "on-slot": function () { api.on("nope", "state", function () {}); },
      "on-id": function () { api.on(id, "state", function () {}); },
      "get-slot": function () { api.get("nope", "state"); },
      "get-id": function () { api.get(id, "state"); },
      "get-recipe": function () { api.get("buttons", "action"); },
      "order-slot": function () { api.order("nope", "state", "ON"); },
      "order-id": function () { api.order(id, "state", "ON"); },
      "get-proto": function () { api.get("constructor", "state"); },
    };
    for (var k in tries) {
      try { tries[k](); api.log(k + ": no error"); } catch (e) { api.log(k + ": " + e.message); }
    }
    api.log("host " + typeof globalThis.__host + " " + typeof __host);
    api.log("globals " + Object.keys(globalThis).filter(function (n) { return n.indexOf("host") >= 0 || n.indexOf("shim") >= 0; }).join(","));
    // The entry points are reachable by name (the shim keeps them there) but frozen, and every call
    // of the engine goes by handle and slot name, checked in Rust.
    api.log("shim " + typeof __solite_shim + " " + Object.isFrozen(__solite_shim));
    api.log("params " + typeof api.params.lights[0]);
  },
});
