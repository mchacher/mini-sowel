// A recipe that does one named thing, in `start` (`case`) or when the zone's motion changes
// (`on-case`): the bad scripts of the tests, in one file.
var keep = [];
var CASES = {
  ok: function (api) {},
  throws: function (api) { throw new Error("boom"); },
  loop: function (api) { for (;;) {} },
  bomb: function (api) { for (;;) keep.push(new Array(1000).fill(1)); },
  "bomb-caught": function (api) {
    try { for (;;) keep.push(new Array(1000).fill(1)); } catch (e) { api.log("caught " + e.name); }
  },
  recursion: function (api) { function f() { return 1 + f(); } f(); },
  depth: function (api) {
    var depth = 0;
    function f() { depth++; f(); }
    try { f(); } catch (e) { api.log("depth " + depth + " " + e.name); }
  },
  garbage: function (api) {
    // Cycles that reference counting cannot free: only the collector does.
    for (var i = 0; i < 1500; i++) { var o = { a: new Array(8).fill(i) }; o.self = o; }
    api.log("garbage made");
  },
  timers17: function (api) {
    var made = 0;
    try { for (var i = 0; i < 17; i++) { api.after(60000, function () {}); made++; } } catch (e) { api.log(made + " timers, then: " + e.message); }
  },
  "timers-short": function (api) {
    try { api.after(10, function () {}); } catch (e) { api.log(e.message); }
    try { api.every(500, function () {}); } catch (e) { api.log(e.message); }
    try { api.at("25:00", function () {}); } catch (e) { api.log(e.message); }
  },
  "timer-throws": function (api) { api.after(1000, function () { throw new Error("late"); }); },
  state: function (api) {
    // 4 keys of 5 + 117 characters: exactly 512 bytes; one more byte is refused.
    var big = "x".repeat(119);
    api.state.set("a", big); api.state.set("b", big); api.state.set("c", big);
    // Fill the fourth key to the byte.
    var left = 512 - JSON.stringify({ a: big, b: big, c: big, d: "" }).length;
    api.state.set("d", "y".repeat(left));
    api.log("512 accepted");
    try { api.state.set("d", "y".repeat(left + 1)); } catch (e) { api.log("513: " + e.message); }
    api.log("kept " + api.state.get("d").length);
  },
  "state-order": function (api) {
    api.state.set("overrideMode", true); api.state.set("timerExpiresAt", "t"); api.state.set("failsafeExpiresAt", "f");
    api.state.delete("overrideMode"); api.state.set("overrideMode", true);
  },
  log21: function (api) { for (var i = 1; i <= 21; i++) api.log("line " + i); },
  log300: function (api) { api.log("z".repeat(300)); },
  na: function (api) {
    var tries = { mode: function () { api.mode(); }, energy: function () { api.energy.claim(); }, history: function () { api.history(); } };
    for (var k in tries) { try { tries[k](); } catch (e) { api.log(e.message); } }
  },
  then: function (api) {
    api.log("a");
    Promise.resolve().then(function () { api.log("then"); api.state.set("job", "ran"); });
    api.log("b");
  },
  date: function (api) {
    api.log("now " + Date.now() + " " + new Date().toISOString() + " " + api.now().time);
  },
  console: function (api) { console.log("via console"); },
  listen33: function (api) {
    api.on("lights", "state", function () {});
    api.on("lights", "state", function () {});
    api.on("lights", "other", function () {});
    try { api.on("button", "action", function () {}); api.log("33 ok"); } catch (e) { api.log(e.message); }
  },
  badzone: function (api) {
    try { api.onZone("temperature", function () {}); } catch (e) { api.log(e.message); }
  },
  orders: function (api) {
    var refused = api.order("lights", "state", "ON");
    api.log("refused " + refused.length + " " + JSON.stringify(refused.map(function (r) { return r.reason; })));
  },
  spin: function (api) {
    // Work that makes the interrupt handler look at the clock a number of times.
    var n = 0;
    for (var i = 0; i < 300000; i++) { n += i; }
    api.log("spun");
  },
  "then-throws": function (api) { Promise.resolve().then(function () { throw new Error("late"); }); },
  "reject-long": function (api) { Promise.reject(new Error("x".repeat(2000))); },
  "timer-async-long": function (api) {
    api.after(1000, async function () { throw new Error("bad " + "y".repeat(1700)); });
  },
  "lone-log": function (api) { api.log("on: " + "Lampe \u{1F4A1}".slice(0, 7)); },
  "cut-pair": function (api) { api.log("a".repeat(399) + "\u{1F4A1} light"); },
  "lone-state": function (api) {
    var half = "\u{1F4A1}".slice(0, 1);
    api.state.set("k" + half, "v" + half);
    api.state.set("n", { ["m" + half]: [half] });
  },
  "lone-alias": function (api) { api.on("lights", "x" + "\u{1F4A1}".slice(1), function () {}); },
  "then-handled": function (api) {
    Promise.resolve().then(function () { throw new Error("handled"); }).catch(function (e) { api.log("caught " + e.message); });
  },
  "then-recursion": function (api) {
    Promise.resolve().then(function () { function f() { return 1 + f(); } f(); });
  },
  "listen-motion": function (api) {
    api.onZone("motion", function (v) { api.log("motion " + v); });
  },
};
var NAMES = Object.keys(CASES);
var ALL = [];
NAMES.forEach(function (n) { ALL.push(n); ALL.push("on-" + n); });
recipe({
  api: 1,
  id: "cases",
  name: "Cases",
  description: "One named thing",
  slots: {
    lights: { type: "equipment", name: "Lights", list: true, equipmentType: "light_onoff" },
    button: { type: "equipment", name: "Button" },
    case: { type: "select", name: "Case", required: true, options: ALL },
  },
  start: function (api) {
    var c = api.params.case;
    if (c.indexOf("on-") === 0) {
      var name = c.slice(3);
      api.onZone("motion", function () { CASES[name](api); });
    } else {
      CASES[c](api);
    }
  },
});
