// big-recipe: a test script of about 32 KB (spec 053), for the memory checks of a script at its
// size bound. It is `motion-light` (the port of Sowel's recipe) with a second part of real code, a
// family of schedule and dimming helpers that the instance's `start` runs once, so that the
// bytecode of all of it is compiled and kept. Not shipped: tests only.
// motion-light: the port of Sowel's `sowel-recipe-motion-light` 1.1.2 to the script API (spec 046):
// Sowel's id, slots, info, texts and behaviour, adapted for Solite where its engine differs (a
// deleted equipment leaves the settings by the generic settle, a button that loses its `action`
// binding disables the instance, the legacy `light` key is not read, a refused order's reason is
// logged, the 2 s echo window is counted on the wall clock). The top level is shared by the
// instances, so it holds functions only: each takes the instance's data `c` (made in `start`),
// which keeps an instance down to a few small closures.
var GRACE_MS = 2000;
var HYSTERESIS = 0.1;

// ---- What the home says ----
function isOn(value) {
  return value === true || (typeof value === "string" && value.toUpperCase() === "ON");
}
function anyLightOn(c) {
  var values = c.api.get("lights", "state");
  for (var i = 0; i < values.length; i++) if (isOn(values[i])) return true;
  return false;
}
function isTooBright(c, l) {
  return c.lux_ !== null && l !== null && l > c.lux_;
}
function isBrightEnoughToTurnOff(c, l) {
  return c.lux_ !== null && l !== null && l > c.lux_ * (1 + HYSTERESIS);
}
function isDaytime(c) {
  return c.day && c.api.now().isDaylight === true;
}
function isMotionActive(value) {
  if (typeof value === "boolean") return value;
  if (typeof value === "number") return value !== 0;
  if (typeof value === "string") {
    var t = value.trim().toLowerCase();
    return t === "true" || t === "on" || t === "1" || t === "occupied" || t === "detected";
  }
  return false;
}
function formatDuration(ms) {
  if (ms >= 3600000) return Math.round(ms / 3600000) + "h";
  if (ms >= 60000) return Math.round(ms / 60000) + "min";
  return Math.round(ms / 1000) + "s";
}
function iso(ms) {
  return new Date(ms).toISOString();
}

// ---- State keys, rebuilt in Sowel's order at each change ----
function syncState(c) {
  var st = c.api.state;
  st.delete("overrideMode");
  st.delete("timerExpiresAt");
  st.delete("failsafeExpiresAt");
  if (c.override) st.set("overrideMode", true);
  if (c.timerAt !== null) st.set("timerExpiresAt", c.timerAt);
  if (c.failsafeAt !== null) st.set("failsafeExpiresAt", c.failsafeAt);
}
// A handler of the instance, then the state.
function bind(c, fn) {
  return function (a) {
    fn(c, a);
    syncState(c);
  };
}

// ---- Timers ----
function deleteTimerKey(c) {
  c.timerAt = null;
}
function cancelOffTimer(c) {
  if (c.offId !== null) c.api.cancel(c.offId);
  c.offId = null;
}
function cancelFailsafe(c) {
  if (c.failsafeId !== null) c.api.cancel(c.failsafeId);
  c.failsafeId = null;
  c.failsafeAt = null;
}
function startOffTimer(c) {
  cancelOffTimer(c);
  c.offId = c.api.after(c.timeout, bind(c, fireOff));
  c.timerAt = iso(Date.now() + c.timeout);
}
function startOffTimerForOverrideClear(c) {
  cancelOffTimer(c);
  c.offId = c.api.after(c.timeout, bind(c, fireOverrideClear));
  c.timerAt = iso(Date.now() + c.timeout);
}
function armFailsafe(c) {
  c.failsafeId = c.api.after(c.maxOn, bind(c, fireFailsafe));
  c.failsafeAt = iso(Date.now() + c.maxOn);
}
// Armed once; an armed failsafe is left as it is.
function startFailsafeTimer(c) {
  if (c.maxOn === null || c.failsafeId !== null) return;
  armFailsafe(c);
}
// Restarts an armed failsafe with its full duration; it never arms one.
function resetFailsafeTimer(c) {
  if (c.maxOn === null || c.failsafeId === null) return;
  c.api.cancel(c.failsafeId);
  armFailsafe(c);
}

// ---- Primitive actions ----
function send(c, on) {
  var refused = c.api.order("lights", "state", on ? "ON" : "OFF");
  var errors = [];
  for (var i = 0; i < refused.length; i++) errors.push(refused[i].id + ": " + refused[i].reason);
  return errors;
}
function sent(c, on, what) {
  var errors = send(c, on);
  if (errors.length > 0) c.api.log.error("Error turning " + what + " some lights: " + errors.join("; "));
}
function startGrace(c) {
  c.graceUntil = Date.now() + GRACE_MS;
}
function inGrace(c) {
  return Date.now() < c.graceUntil;
}
function turnOn(c) {
  c.byRecipe = true;
  c.lastOn = true;
  sent(c, true, "on");
  c.api.log("Motion detected — " + c.count + " light(s) turned on");
  startFailsafeTimer(c);
}
function turnOff(c, reason) {
  c.byRecipe = false;
  c.lastOn = false;
  startGrace(c);
  sent(c, false, "off");
  c.api.log(reason);
  cancelFailsafe(c);
  c.override = false;
}

// ---- Timers firing ----
function fireOff(c) {
  c.offId = null;
  deleteTimerKey(c);
  turnOff(c, "No motion for " + formatDuration(c.timeout) + " — lights turned off");
}
function fireOverrideClear(c) {
  c.offId = null;
  deleteTimerKey(c);
  if (anyLightOn(c)) {
    c.byRecipe = false;
    c.lastOn = false;
    startGrace(c);
    send(c, false); // the refusals are ignored on this path
  }
  c.override = false;
  cancelFailsafe(c);
  c.api.log("No motion for " + formatDuration(c.timeout) + " — override cleared, lights off");
}
function fireFailsafe(c) {
  c.failsafeId = null;
  cancelFailsafe(c);
  cancelOffTimer(c);
  deleteTimerKey(c);
  // Like turning off, but the override mode is left as it is.
  c.byRecipe = false;
  c.lastOn = false;
  startGrace(c);
  sent(c, false, "off");
  c.api.log.warn("Failsafe: lights forced off after " + formatDuration(c.maxOn) + " max on duration");
}

// ---- Handlers ----
function onZoneChanged(c, motion) {
  var api = c.api, l = c.zoneLux;
  if (c.override) {
    if (motion) {
      cancelOffTimer(c);
      deleteTimerKey(c);
      resetFailsafeTimer(c);
      api.log("Motion detected but override mode active — ignoring");
    } else if (c.offId === null) {
      startOffTimerForOverrideClear(c);
    }
    return;
  }
  var lightsOn = anyLightOn(c);
  if (lightsOn && isBrightEnoughToTurnOff(c, l)) {
    if (!c.byRecipe) {
      if (inGrace(c)) return;
      c.override = true;
      startFailsafeTimer(c);
      api.log("Light turned on manually above lux threshold — entering override mode");
    } else {
      cancelOffTimer(c);
      deleteTimerKey(c);
      turnOff(c, "Luminosity above threshold — lights turned off");
    }
    return;
  }
  if (motion && !lightsOn) {
    if (inGrace(c)) return;
    if (c.byRecipe) {
      c.byRecipe = false;
      c.override = true;
      startOffTimerForOverrideClear(c);
      api.log("Light turned off manually while motion active — entering override mode");
      return;
    }
    if (isTooBright(c, l)) {
      api.log("Motion detected but luminosity " + l + " exceeds threshold " + c.lux_ + " — not turning on");
      return;
    }
    if (isDaytime(c)) {
      api.log("Motion detected but daytime — not turning on");
      return;
    }
    turnOn(c);
  } else if (motion && lightsOn) {
    cancelOffTimer(c);
    deleteTimerKey(c);
    resetFailsafeTimer(c);
  } else if (!motion && lightsOn && c.offId === null) {
    startOffTimer(c);
  }
}

function onLightChanged(c, value) {
  var api = c.api, lightOn = isOn(value);
  if (lightOn === c.lastOn) return;
  c.lastOn = lightOn;
  if (c.override) return;
  var motion = api.get("zone", "motion");
  if (lightOn && !motion) {
    startOffTimer(c);
    startFailsafeTimer(c);
    api.log("Light turned on externally — turning off in " + formatDuration(c.timeout));
  } else if (lightOn) {
    cancelOffTimer(c);
    deleteTimerKey(c);
  } else {
    if (inGrace(c)) return;
    if (c.byRecipe && motion) {
      c.byRecipe = false;
      c.override = true;
      cancelOffTimer(c);
      cancelFailsafe(c);
      deleteTimerKey(c);
      startOffTimerForOverrideClear(c);
      api.log("Light turned off manually while motion active — entering override mode");
      return;
    }
    c.byRecipe = false;
    if (c.offId !== null || c.failsafeId !== null) {
      cancelOffTimer(c);
      cancelFailsafe(c);
      deleteTimerKey(c);
      api.log("Light turned off externally — timers cancelled");
    }
  }
}

function onButtonAction(c) {
  if (anyLightOn(c)) {
    c.byRecipe = false;
    c.lastOn = false;
    startGrace(c);
    sent(c, false, "off");
    cancelOffTimer(c);
    cancelFailsafe(c);
    deleteTimerKey(c);
    c.override = true;
    c.api.log("Button pressed — lights off, entering override mode");
    startOffTimerForOverrideClear(c);
  } else {
    c.override = false;
    turnOn(c);
  }
}

// ---- The start sync: the lights set to what the zone calls for ----
function syncLightsOnStart(c, motion, l) {
  if (motion && !isTooBright(c, l) && !isDaytime(c)) {
    turnOn(c);
  } else if (isDaytime(c)) {
    turnOff(c, "Recipe activated — daytime, lights off");
  } else if (motion) {
    turnOff(c, "Recipe activated — luminosity above threshold, lights off");
  } else {
    turnOff(c, "Recipe activated — no motion, lights off");
  }
}

// Helper 0: the level for a window 0 of the day, from the hour and a base level.
function level0(hour, base) {
  var window = (hour + 0) % 24;
  var level = base;
  if (window < 6) level = Math.round(base * 0.2);
  else if (window < 9) level = Math.min(100, base + 0);
  else if (window < 18) level = Math.max(0, base - 0);
  else level = Math.round(base * 1.0);
  if (level > 100) level = 100;
  return level;
}
function label0(hour, base) {
  var l = level0(hour, base);
  var kind = l >= 80 ? "bright" : l >= 40 ? "medium" : "dim";
  return "w0:" + kind + ":" + l + "%";
}
// Helper 1: the level for a window 1 of the day, from the hour and a base level.
function level1(hour, base) {
  var window = (hour + 1) % 24;
  var level = base;
  if (window < 6) level = Math.round(base * 0.3);
  else if (window < 9) level = Math.min(100, base + 1);
  else if (window < 18) level = Math.max(0, base - 1);
  else level = Math.round(base * 1.1);
  if (level > 100) level = 100;
  return level;
}
function label1(hour, base) {
  var l = level1(hour, base);
  var kind = l >= 80 ? "bright" : l >= 40 ? "medium" : "dim";
  return "w1:" + kind + ":" + l + "%";
}
// Helper 2: the level for a window 2 of the day, from the hour and a base level.
function level2(hour, base) {
  var window = (hour + 2) % 24;
  var level = base;
  if (window < 6) level = Math.round(base * 0.4);
  else if (window < 9) level = Math.min(100, base + 2);
  else if (window < 18) level = Math.max(0, base - 2);
  else level = Math.round(base * 1.2);
  if (level > 100) level = 100;
  return level;
}
function label2(hour, base) {
  var l = level2(hour, base);
  var kind = l >= 80 ? "bright" : l >= 40 ? "medium" : "dim";
  return "w2:" + kind + ":" + l + "%";
}
// Helper 3: the level for a window 3 of the day, from the hour and a base level.
function level3(hour, base) {
  var window = (hour + 3) % 24;
  var level = base;
  if (window < 6) level = Math.round(base * 0.5);
  else if (window < 9) level = Math.min(100, base + 3);
  else if (window < 18) level = Math.max(0, base - 3);
  else level = Math.round(base * 1.3);
  if (level > 100) level = 100;
  return level;
}
function label3(hour, base) {
  var l = level3(hour, base);
  var kind = l >= 80 ? "bright" : l >= 40 ? "medium" : "dim";
  return "w3:" + kind + ":" + l + "%";
}
// Helper 4: the level for a window 4 of the day, from the hour and a base level.
function level4(hour, base) {
  var window = (hour + 4) % 24;
  var level = base;
  if (window < 6) level = Math.round(base * 0.6);
  else if (window < 9) level = Math.min(100, base + 4);
  else if (window < 18) level = Math.max(0, base - 4);
  else level = Math.round(base * 1.4);
  if (level > 100) level = 100;
  return level;
}
function label4(hour, base) {
  var l = level4(hour, base);
  var kind = l >= 80 ? "bright" : l >= 40 ? "medium" : "dim";
  return "w4:" + kind + ":" + l + "%";
}
// Helper 5: the level for a window 5 of the day, from the hour and a base level.
function level5(hour, base) {
  var window = (hour + 5) % 24;
  var level = base;
  if (window < 6) level = Math.round(base * 0.7);
  else if (window < 9) level = Math.min(100, base + 5);
  else if (window < 18) level = Math.max(0, base - 5);
  else level = Math.round(base * 1.0);
  if (level > 100) level = 100;
  return level;
}
function label5(hour, base) {
  var l = level5(hour, base);
  var kind = l >= 80 ? "bright" : l >= 40 ? "medium" : "dim";
  return "w5:" + kind + ":" + l + "%";
}
// Helper 6: the level for a window 6 of the day, from the hour and a base level.
function level6(hour, base) {
  var window = (hour + 6) % 24;
  var level = base;
  if (window < 6) level = Math.round(base * 0.8);
  else if (window < 9) level = Math.min(100, base + 6);
  else if (window < 18) level = Math.max(0, base - 6);
  else level = Math.round(base * 1.1);
  if (level > 100) level = 100;
  return level;
}
function label6(hour, base) {
  var l = level6(hour, base);
  var kind = l >= 80 ? "bright" : l >= 40 ? "medium" : "dim";
  return "w6:" + kind + ":" + l + "%";
}
// Helper 7: the level for a window 7 of the day, from the hour and a base level.
function level7(hour, base) {
  var window = (hour + 7) % 24;
  var level = base;
  if (window < 6) level = Math.round(base * 0.2);
  else if (window < 9) level = Math.min(100, base + 7);
  else if (window < 18) level = Math.max(0, base - 7);
  else level = Math.round(base * 1.2);
  if (level > 100) level = 100;
  return level;
}
function label7(hour, base) {
  var l = level7(hour, base);
  var kind = l >= 80 ? "bright" : l >= 40 ? "medium" : "dim";
  return "w7:" + kind + ":" + l + "%";
}
// Helper 8: the level for a window 8 of the day, from the hour and a base level.
function level8(hour, base) {
  var window = (hour + 8) % 24;
  var level = base;
  if (window < 6) level = Math.round(base * 0.3);
  else if (window < 9) level = Math.min(100, base + 8);
  else if (window < 18) level = Math.max(0, base - 8);
  else level = Math.round(base * 1.3);
  if (level > 100) level = 100;
  return level;
}
function label8(hour, base) {
  var l = level8(hour, base);
  var kind = l >= 80 ? "bright" : l >= 40 ? "medium" : "dim";
  return "w8:" + kind + ":" + l + "%";
}
// Helper 9: the level for a window 9 of the day, from the hour and a base level.
function level9(hour, base) {
  var window = (hour + 9) % 24;
  var level = base;
  if (window < 6) level = Math.round(base * 0.4);
  else if (window < 9) level = Math.min(100, base + 9);
  else if (window < 18) level = Math.max(0, base - 9);
  else level = Math.round(base * 1.4);
  if (level > 100) level = 100;
  return level;
}
function label9(hour, base) {
  var l = level9(hour, base);
  var kind = l >= 80 ? "bright" : l >= 40 ? "medium" : "dim";
  return "w9:" + kind + ":" + l + "%";
}
// Helper 10: the level for a window 10 of the day, from the hour and a base level.
function level10(hour, base) {
  var window = (hour + 10) % 24;
  var level = base;
  if (window < 6) level = Math.round(base * 0.5);
  else if (window < 9) level = Math.min(100, base + 10);
  else if (window < 18) level = Math.max(0, base - 10);
  else level = Math.round(base * 1.0);
  if (level > 100) level = 100;
  return level;
}
function label10(hour, base) {
  var l = level10(hour, base);
  var kind = l >= 80 ? "bright" : l >= 40 ? "medium" : "dim";
  return "w10:" + kind + ":" + l + "%";
}
// Helper 11: the level for a window 11 of the day, from the hour and a base level.
function level11(hour, base) {
  var window = (hour + 11) % 24;
  var level = base;
  if (window < 6) level = Math.round(base * 0.6);
  else if (window < 9) level = Math.min(100, base + 11);
  else if (window < 18) level = Math.max(0, base - 0);
  else level = Math.round(base * 1.1);
  if (level > 100) level = 100;
  return level;
}
function label11(hour, base) {
  var l = level11(hour, base);
  var kind = l >= 80 ? "bright" : l >= 40 ? "medium" : "dim";
  return "w11:" + kind + ":" + l + "%";
}
// Helper 12: the level for a window 12 of the day, from the hour and a base level.
function level12(hour, base) {
  var window = (hour + 12) % 24;
  var level = base;
  if (window < 6) level = Math.round(base * 0.7);
  else if (window < 9) level = Math.min(100, base + 12);
  else if (window < 18) level = Math.max(0, base - 1);
  else level = Math.round(base * 1.2);
  if (level > 100) level = 100;
  return level;
}
function label12(hour, base) {
  var l = level12(hour, base);
  var kind = l >= 80 ? "bright" : l >= 40 ? "medium" : "dim";
  return "w12:" + kind + ":" + l + "%";
}
// Helper 13: the level for a window 13 of the day, from the hour and a base level.
function level13(hour, base) {
  var window = (hour + 13) % 24;
  var level = base;
  if (window < 6) level = Math.round(base * 0.8);
  else if (window < 9) level = Math.min(100, base + 0);
  else if (window < 18) level = Math.max(0, base - 2);
  else level = Math.round(base * 1.3);
  if (level > 100) level = 100;
  return level;
}
function label13(hour, base) {
  var l = level13(hour, base);
  var kind = l >= 80 ? "bright" : l >= 40 ? "medium" : "dim";
  return "w13:" + kind + ":" + l + "%";
}
// Helper 14: the level for a window 14 of the day, from the hour and a base level.
function level14(hour, base) {
  var window = (hour + 14) % 24;
  var level = base;
  if (window < 6) level = Math.round(base * 0.2);
  else if (window < 9) level = Math.min(100, base + 1);
  else if (window < 18) level = Math.max(0, base - 3);
  else level = Math.round(base * 1.4);
  if (level > 100) level = 100;
  return level;
}
function label14(hour, base) {
  var l = level14(hour, base);
  var kind = l >= 80 ? "bright" : l >= 40 ? "medium" : "dim";
  return "w14:" + kind + ":" + l + "%";
}
// Helper 15: the level for a window 15 of the day, from the hour and a base level.
function level15(hour, base) {
  var window = (hour + 15) % 24;
  var level = base;
  if (window < 6) level = Math.round(base * 0.3);
  else if (window < 9) level = Math.min(100, base + 2);
  else if (window < 18) level = Math.max(0, base - 4);
  else level = Math.round(base * 1.0);
  if (level > 100) level = 100;
  return level;
}
function label15(hour, base) {
  var l = level15(hour, base);
  var kind = l >= 80 ? "bright" : l >= 40 ? "medium" : "dim";
  return "w15:" + kind + ":" + l + "%";
}
// Helper 16: the level for a window 16 of the day, from the hour and a base level.
function level16(hour, base) {
  var window = (hour + 16) % 24;
  var level = base;
  if (window < 6) level = Math.round(base * 0.4);
  else if (window < 9) level = Math.min(100, base + 3);
  else if (window < 18) level = Math.max(0, base - 5);
  else level = Math.round(base * 1.1);
  if (level > 100) level = 100;
  return level;
}
function label16(hour, base) {
  var l = level16(hour, base);
  var kind = l >= 80 ? "bright" : l >= 40 ? "medium" : "dim";
  return "w16:" + kind + ":" + l + "%";
}
// Helper 17: the level for a window 17 of the day, from the hour and a base level.
function level17(hour, base) {
  var window = (hour + 17) % 24;
  var level = base;
  if (window < 6) level = Math.round(base * 0.5);
  else if (window < 9) level = Math.min(100, base + 4);
  else if (window < 18) level = Math.max(0, base - 6);
  else level = Math.round(base * 1.2);
  if (level > 100) level = 100;
  return level;
}
function label17(hour, base) {
  var l = level17(hour, base);
  var kind = l >= 80 ? "bright" : l >= 40 ? "medium" : "dim";
  return "w17:" + kind + ":" + l + "%";
}
// Helper 18: the level for a window 18 of the day, from the hour and a base level.
function level18(hour, base) {
  var window = (hour + 18) % 24;
  var level = base;
  if (window < 6) level = Math.round(base * 0.6);
  else if (window < 9) level = Math.min(100, base + 5);
  else if (window < 18) level = Math.max(0, base - 7);
  else level = Math.round(base * 1.3);
  if (level > 100) level = 100;
  return level;
}
function label18(hour, base) {
  var l = level18(hour, base);
  var kind = l >= 80 ? "bright" : l >= 40 ? "medium" : "dim";
  return "w18:" + kind + ":" + l + "%";
}
// Helper 19: the level for a window 19 of the day, from the hour and a base level.
function level19(hour, base) {
  var window = (hour + 19) % 24;
  var level = base;
  if (window < 6) level = Math.round(base * 0.7);
  else if (window < 9) level = Math.min(100, base + 6);
  else if (window < 18) level = Math.max(0, base - 8);
  else level = Math.round(base * 1.4);
  if (level > 100) level = 100;
  return level;
}
function label19(hour, base) {
  var l = level19(hour, base);
  var kind = l >= 80 ? "bright" : l >= 40 ? "medium" : "dim";
  return "w19:" + kind + ":" + l + "%";
}
// Helper 20: the level for a window 20 of the day, from the hour and a base level.
function level20(hour, base) {
  var window = (hour + 20) % 24;
  var level = base;
  if (window < 6) level = Math.round(base * 0.8);
  else if (window < 9) level = Math.min(100, base + 7);
  else if (window < 18) level = Math.max(0, base - 9);
  else level = Math.round(base * 1.0);
  if (level > 100) level = 100;
  return level;
}
function label20(hour, base) {
  var l = level20(hour, base);
  var kind = l >= 80 ? "bright" : l >= 40 ? "medium" : "dim";
  return "w20:" + kind + ":" + l + "%";
}
// Helper 21: the level for a window 21 of the day, from the hour and a base level.
function level21(hour, base) {
  var window = (hour + 21) % 24;
  var level = base;
  if (window < 6) level = Math.round(base * 0.2);
  else if (window < 9) level = Math.min(100, base + 8);
  else if (window < 18) level = Math.max(0, base - 10);
  else level = Math.round(base * 1.1);
  if (level > 100) level = 100;
  return level;
}
function label21(hour, base) {
  var l = level21(hour, base);
  var kind = l >= 80 ? "bright" : l >= 40 ? "medium" : "dim";
  return "w21:" + kind + ":" + l + "%";
}
// Helper 22: the level for a window 22 of the day, from the hour and a base level.
function level22(hour, base) {
  var window = (hour + 22) % 24;
  var level = base;
  if (window < 6) level = Math.round(base * 0.3);
  else if (window < 9) level = Math.min(100, base + 9);
  else if (window < 18) level = Math.max(0, base - 0);
  else level = Math.round(base * 1.2);
  if (level > 100) level = 100;
  return level;
}
function label22(hour, base) {
  var l = level22(hour, base);
  var kind = l >= 80 ? "bright" : l >= 40 ? "medium" : "dim";
  return "w22:" + kind + ":" + l + "%";
}
// Helper 23: the level for a window 23 of the day, from the hour and a base level.
function level23(hour, base) {
  var window = (hour + 23) % 24;
  var level = base;
  if (window < 6) level = Math.round(base * 0.4);
  else if (window < 9) level = Math.min(100, base + 10);
  else if (window < 18) level = Math.max(0, base - 1);
  else level = Math.round(base * 1.3);
  if (level > 100) level = 100;
  return level;
}
function label23(hour, base) {
  var l = level23(hour, base);
  var kind = l >= 80 ? "bright" : l >= 40 ? "medium" : "dim";
  return "w23:" + kind + ":" + l + "%";
}
// Helper 24: the level for a window 24 of the day, from the hour and a base level.
function level24(hour, base) {
  var window = (hour + 24) % 24;
  var level = base;
  if (window < 6) level = Math.round(base * 0.5);
  else if (window < 9) level = Math.min(100, base + 11);
  else if (window < 18) level = Math.max(0, base - 2);
  else level = Math.round(base * 1.4);
  if (level > 100) level = 100;
  return level;
}
function label24(hour, base) {
  var l = level24(hour, base);
  var kind = l >= 80 ? "bright" : l >= 40 ? "medium" : "dim";
  return "w24:" + kind + ":" + l + "%";
}
// Helper 25: the level for a window 25 of the day, from the hour and a base level.
function level25(hour, base) {
  var window = (hour + 25) % 24;
  var level = base;
  if (window < 6) level = Math.round(base * 0.6);
  else if (window < 9) level = Math.min(100, base + 12);
  else if (window < 18) level = Math.max(0, base - 3);
  else level = Math.round(base * 1.0);
  if (level > 100) level = 100;
  return level;
}
function label25(hour, base) {
  var l = level25(hour, base);
  var kind = l >= 80 ? "bright" : l >= 40 ? "medium" : "dim";
  return "w25:" + kind + ":" + l + "%";
}
// Helper 26: the level for a window 26 of the day, from the hour and a base level.
function level26(hour, base) {
  var window = (hour + 26) % 24;
  var level = base;
  if (window < 6) level = Math.round(base * 0.7);
  else if (window < 9) level = Math.min(100, base + 0);
  else if (window < 18) level = Math.max(0, base - 4);
  else level = Math.round(base * 1.1);
  if (level > 100) level = 100;
  return level;
}
function label26(hour, base) {
  var l = level26(hour, base);
  var kind = l >= 80 ? "bright" : l >= 40 ? "medium" : "dim";
  return "w26:" + kind + ":" + l + "%";
}
var HELPERS = [label0, label1, label2, label3, label4, label5, label6, label7, label8, label9, label10, label11, label12, label13, label14, label15, label16, label17, label18, label19, label20, label21, label22, label23, label24, label25, label26];
function runHelpers(hour, base) {
  var out = [];
  for (var i = 0; i < HELPERS.length; i++) out.push(HELPERS[i](hour, base));
  return out.length;
}

recipe({
  api: 1,
  id: "big-recipe",
  version: "1.1.2",
  name: "Big Recipe",
  description:
    "Turns on lights when motion is detected, turns off after a timeout with no motion. Supports lux threshold, failsafe max-on duration, and button override.",
  icon: "Lightbulb",
  i18n: {
    fr: {
      name: "Lumière sur mouvement",
      description:
        "Allume les lumières quand un mouvement est détecté, éteint après un délai sans mouvement. Supporte un seuil de luminosité optionnel et une durée max d'allumage.",
      slots: {
        zone: { name: "Zone", description: "Zone à surveiller" },
        lights: { name: "Lumières", description: "Lumières à contrôler (doivent appartenir à la zone)" },
        timeout: { name: "Délai", description: "Délai sans mouvement avant extinction" },
        luxThreshold: { name: "Seuil lux (max)", description: "Au-dessus de ce seuil, les lumières ne s'allument pas" },
        maxOnDuration: {
          name: "Extinction auto (sécurité)",
          description:
            "Anti-oubli pour capteur bloqué : coupe les lumières après cette durée sans nouveau mouvement. Une présence active la réarme en continu, donc elle ne coupe pas tant que tu es dans la pièce.",
        },
        buttons: { name: "Interrupteurs", description: "Interrupteurs physiques pour allumer/éteindre manuellement" },
        disableWhenDaylight: {
          name: "Inactif le jour",
          description: "Ne pas allumer pendant la journée (basé sur lever/coucher du soleil)",
        },
      },
    },
  },
  slots: {
    zone: { type: "zone", name: "Zone", description: "Zone to monitor", required: true },
    lights: {
      type: "equipment",
      name: "Lights",
      description: "Lights to control (must belong to the selected zone)",
      required: true,
      list: true,
      constraints: { equipmentType: "light_onoff" },
    },
    timeout: {
      type: "duration",
      name: "Timeout",
      description: "Delay with no motion before turning off",
      required: true,
      defaultValue: "10m",
    },
    luxThreshold: {
      type: "number",
      name: "Lux Threshold",
      description:
        "Lights won't turn on when ambient brightness exceeds this value; turns off if it rises above threshold + 10% hysteresis",
      required: false,
      constraints: { min: 0 },
    },
    maxOnDuration: {
      type: "duration",
      name: "Safety Auto-off",
      description:
        "Failsafe for a stuck sensor: force lights off after this duration with no fresh motion. Active presence keeps resetting it, so it won't cut the light while you're in the room.",
      required: false,
    },
    buttons: {
      type: "equipment",
      name: "Manual Switches",
      description: "Physical switches for manual on/off toggle",
      required: false,
      list: true,
      constraints: { equipmentType: "button" },
    },
    disableWhenDaylight: {
      type: "boolean",
      name: "Inactive During Day",
      description: "Do not turn on lights during daytime (based on sunrise/sunset and offsets from settings)",
      required: false,
    },
  },

  // Sowel's 1.1.2 `validate`, with its texts and in its order (the host's own checks of the slots
  // follow, and can only refuse more).
  validate: function (params, home) {
    var i, item, ids, value;
    var durationText = function (v) {
      return "Invalid duration format: " + String(v) + '. Use e.g. "10m", "30s", "1h"';
    };
    if (typeof params.zone !== "string" || params.zone === "") return "Zone parameter is required";
    if (!home.zone(params.zone)) return "Zone not found: " + params.zone;
    ids = Array.isArray(params.lights) ? params.lights : [];
    if (ids.length === 0) return "At least one light is required";
    if (ids.length > 16) return "At most 16 lights";
    for (i = 0; i < ids.length; i++) {
      item = home.equipment(ids[i]);
      if (!item) return "Light equipment not found: " + ids[i];
      if (item.type !== "light_onoff") {
        return 'Light "' + item.name + '" is type "' + item.type + '" but this recipe requires light_onoff';
      }
      if (item.zoneId !== params.zone) return 'Light "' + item.name + '" does not belong to the selected zone';
      if (item.orders.indexOf("state") < 0) return 'Light "' + item.name + '" has no "state" order binding';
    }
    if (typeof params.timeout !== "number") return durationText(params.timeout);
    value = params.luxThreshold;
    if (value !== undefined && value !== null && (typeof value !== "number" || !(value >= 0))) {
      return "luxThreshold must be a non-negative number";
    }
    value = params.maxOnDuration;
    if (value !== undefined && value !== null && typeof value !== "number") return durationText(value);
    ids = Array.isArray(params.buttons) ? params.buttons : [];
    if (ids.length > 16) return "At most 16 buttons";
    for (i = 0; i < ids.length; i++) {
      item = home.equipment(ids[i]);
      if (!item) return "Button equipment not found: " + ids[i];
      if (item.data.indexOf("action") < 0) return 'Button "' + item.name + '" has no "action" data binding';
    }
    return "";
  },

  start: function (api) {
    var p = api.params;
    var maxOn = typeof p.maxOnDuration === "number" && p.maxOnDuration > 0 ? Math.max(100, p.maxOnDuration) : null;
    // The instance's data: what the functions above read and write.
    var c = {
      api: api,
      timeout: Math.max(100, p.timeout || 600000),
      lux_: typeof p.luxThreshold === "number" ? p.luxThreshold : null,
      maxOn: maxOn,
      day: p.disableWhenDaylight === true,
      count: (p.lights || []).length,
      offId: null,
      failsafeId: null,
      override: false,
      byRecipe: false,
      lastOn: false,
      graceUntil: 0, // wall clock
      timerAt: null, // the state keys, as ISO texts
      failsafeAt: null,
      zoneLux: null // the zone's luminosity, as the last zone event said
    };
    if (api.get("zone", "motionSensors") === 0) {
      api.log.warn("Zone has no motion sensors — recipe will never trigger");
    }
    c.zoneLux = api.get("zone", "luminosity");
    c.helpers = runHelpers(12, 50);
    c.byRecipe = c.lastOn = anyLightOn(c);
    syncLightsOnStart(c, api.get("zone", "motion") === true, c.zoneLux);
    c.graceUntil = 0;
    syncState(c);

    // The luminosity handler comes first: both get the same event, in this order.
    api.onZone("luminosity", function (value) {
      c.zoneLux = value;
    });
    api.onZone("motion", bind(c, onZoneChanged));
    api.on("lights", "state", bind(c, onLightChanged));
    if ((p.buttons || []).length > 0) api.on("buttons", "action", bind(c, onButtonAction));
    if (maxOn !== null) {
      api.onMotionReport(function (value) {
        if (isMotionActive(value)) {
          resetFailsafeTimer(c);
          syncState(c);
        }
      });
    }
  },
});
