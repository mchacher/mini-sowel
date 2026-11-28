// state-watch: the port of Sowel's `sowel-recipe-state-watch` 3.1.0 to the script API (spec 055):
// Sowel's id, slots, info, texts and behaviour, adapted for Solite where its engine differs (the
// time of day is the home's, `api.now().localTime`, and while the clock is not set the check
// window counts as closed until a boundary or a change of the value; a deleted equipment leaves
// the settings by the generic settle; the summary's label does not use the binding's data
// category, which a script cannot read, so a value with no label of its own reads `<key> :
// <value>`). The top level is shared by the instances, so it holds functions only: each takes
// the instance's data `c` (made in `start`).

// ---- Labels (Sowel's ALIAS_VALUE_LABELS and fault codes) ----
function aliasLabel(dataKey, value) {
  if (dataKey === "stoveState") {
    if (value === "off") return "Éteint";
    if (value === "running") return "En marche";
    if (value === "standby") return "Veille";
    if (value === "error") return "Erreur";
  }
  // Stove fault codes (`error_A01`...), any case of the prefix.
  if (value.length > 6 && value.slice(0, 6).toLowerCase() === "error_") return "Erreur " + value.slice(6);
  return dataKey + " : " + value;
}
// « 30 s », « 10 min », « 2 h », « 1 h 30 ».
function formatDelay(ms) {
  if (ms < 60000) return Math.round(ms / 1000) + " s";
  var total = Math.round(ms / 60000);
  if (total < 60) return total + " min";
  var h = Math.floor(total / 60), m = total % 60;
  return m === 0 ? h + " h" : h + " h " + (m < 10 ? "0" : "") + m;
}
function formatDuration(ms) {
  if (ms >= 3600000) return Math.round(ms / 3600000) + "h";
  if (ms >= 60000) return Math.round(ms / 60000) + "min";
  return Math.round(ms / 1000) + "s";
}
function summaryOf(c, alarm) {
  if (alarm) return c.label + " en alarme";
  var delay = c.delay !== null && c.delay > 0 ? "après " + formatDelay(c.delay) : null;
  var window = c.hasWindow ? "de " + c.start + " à " + c.end : null;
  if (delay && window) return c.label + " " + delay + ", " + window;
  if (delay) return c.label + " " + delay;
  if (window) return c.label + " " + window;
  return c.label;
}
function publishSummary(c, alarm) {
  var summary = summaryOf(c, alarm);
  if (c.api.state.get("summary") !== summary) c.api.state.set("summary", summary);
}

// ---- The condition: the watched value AND (no window OR inside it) ----
function minutesOf(text) {
  var p = text.split(":").map(Number);
  return p[0] * 60 + p[1];
}
function currentValue(c) {
  try {
    return c.api.get("equipment", c.dataKey);
  } catch (e) {
    return undefined;
  }
}
function inWindow(c) {
  if (!c.hasWindow) return true;
  var local = c.api.now().localTime;
  if (local === null) return false;
  var now = minutesOf(local), s = minutesOf(c.start), e = minutesOf(c.end);
  if (s === e) return false;
  if (s < e) return now >= s && now < e;
  return now >= s || now < e;
}
function shouldAlarm(c) {
  return String(currentValue(c)) === c.watch && inWindow(c);
}

// ---- The alarm ----
function clearDelayTimer(c) {
  if (c.delayId !== null) c.api.cancel(c.delayId);
  c.delayId = null;
}
// Only ever called from a not-alarmed state (evaluate guards on it).
function raiseAlarm(c) {
  var st = c.api.state;
  st.set("alarm", true);
  st.set("alarmSince", new Date().toISOString());
  publishSummary(c, true);
  c.api.log.error("ALARM: " + c.dataKey + "=" + c.watch);
}
// Cancels a pending delay and clears an active alarm; logs `message` only when one was active.
function resetAlarm(c, message) {
  var st = c.api.state;
  clearDelayTimer(c);
  var was = st.get("alarm") === true;
  st.delete("watchStartedAt");
  if (was) {
    st.set("alarm", false);
    st.set("alarmSince", null);
    publishSummary(c, false);
    if (message) c.api.log(message);
  }
}
function armDelay(c, ms) {
  c.delayId = c.api.after(Math.max(100, ms), function () {
    c.delayId = null;
    if (shouldAlarm(c)) raiseAlarm(c);
  });
}

// ---- The reconciliation: on a value change and at the window's edges ----
function evaluate(c) {
  var st = c.api.state;
  if (shouldAlarm(c)) {
    if (st.get("alarm") === true || c.delayId !== null) return; // already alarmed / pending
    if (c.delay !== null && c.delay > 0) {
      st.set("watchStartedAt", new Date().toISOString());
      armDelay(c, c.delay);
      c.api.log(c.dataKey + "=" + c.watch + " — alarm in " + formatDuration(c.delay));
    } else {
      raiseAlarm(c);
    }
  } else if (st.get("alarm") === true) {
    resetAlarm(c, c.dataKey + " — alarm cleared");
  } else if (c.delayId !== null) {
    clearDelayTimer(c);
    st.delete("watchStartedAt");
  }
}

// ---- After a restart, from the state kept ----
function restoreState(c) {
  var st = c.api.state;
  c.api.log("Current: " + c.dataKey + "=" + String(currentValue(c)));
  var alarmed = st.get("alarm") === true;
  if (!shouldAlarm(c)) {
    if (alarmed) resetAlarm(c, "Alarm cleared on restart — condition no longer met");
    return;
  }
  if (alarmed) {
    c.api.log("Alarm still active after restart");
    return;
  }
  var since = st.get("watchStartedAt");
  if (c.delay !== null && c.delay > 0 && typeof since === "string") {
    var remaining = c.delay - (Date.now() - Date.parse(since));
    // A start written while the clock was not set can be decades off: never wait longer than the delay.
    if (remaining > c.delay) remaining = c.delay;
    if (remaining <= 0) raiseAlarm(c);
    else armDelay(c, remaining);
  } else {
    evaluate(c);
  }
}

// ---- Validation helpers ----
function given(v) {
  return v !== undefined && v !== null && v !== "";
}
// One or two digits, a colon, two digits (the engine has no regular expressions).
function hhmm(v) {
  var t = String(v), i = t.indexOf(":");
  if (i < 1 || i > 2 || t.length !== i + 3) return false;
  for (var k = 0; k < t.length; k++) {
    var d = t.charCodeAt(k);
    if (k !== i && (d < 48 || d > 57)) return false;
  }
  return true;
}
function durationText(v) {
  return "Invalid duration format: " + String(v) + '. Use e.g. "10m", "30s", "1h"';
}

recipe({
  api: 1,
  id: "state-watch",
  version: "3.1.0",
  name: "State Watch",
  description:
    "Monitor an equipment data key and raise an alarm while the value stays in a watched state. Permanent by default, with an optional delay and time window. Re-notification is configured on the notification itself.",
  icon: "Eye",
  i18n: {
    fr: {
      name: "Surveillance d'état",
      description:
        "Surveille une donnée d'équipement et déclenche une alarme tant que la valeur reste dans un état donné. Surveillance permanente par défaut, avec délai et créneau horaire optionnels. La re-notification se règle sur la notification.",
      slots: {
        zone: { name: "Zone", description: "Zone de l'équipement à surveiller" },
        equipment: { name: "Équipement", description: "Équipement à surveiller" },
        dataKey: { name: "Clé de donnée", description: "Alias de la donnée à surveiller (ex: contact, state)" },
        watchValue: {
          name: "Valeur surveillée",
          description: "Valeur qui déclenche la surveillance (ex: open, true)"
        },
        delay: { name: "Délai", description: "Durée avant la première alarme (ex: 10m)" },
        checkStart: {
          name: "Début du créneau",
          description: "Optionnel. N'alarme que si l'heure est dans ce créneau (début). Vide = surveillance permanente."
        },
        checkEnd: {
          name: "Fin du créneau",
          description:
            "Optionnel. Fin du créneau de surveillance. Le créneau peut passer minuit (ex: 21:00 à 06:00)."
        }
      }
    }
  },
  slots: {
    zone: { type: "zone", name: "Zone", description: "Zone of the equipment to monitor", required: true },
    equipment: { type: "equipment", name: "Equipment", description: "Equipment to monitor", required: true },
    dataKey: {
      type: "data-key",
      name: "Data Key",
      description: "Data binding alias to watch (e.g., contact, state)",
      required: true
    },
    watchValue: {
      type: "text",
      name: "Watch Value",
      description: "Value that triggers the alarm (e.g., open, true)",
      required: true
    },
    delay: {
      type: "duration",
      name: "Delay",
      description: "Time in watched state before first alarm (e.g., 10m)",
      required: false
    },
    checkStart: {
      type: "time",
      name: "Check Window Start",
      description:
        "Optional. Only alarm while the time is inside this window (start). Leave empty for permanent monitoring.",
      required: false
    },
    checkEnd: {
      type: "time",
      name: "Check Window End",
      description: "Optional. End of the check window. The window may cross midnight (e.g. 21:00 to 06:00).",
      required: false
    }
  },

  // Sowel's `validate`, with its texts and in its order (the host's own checks of the slots
  // follow, and can only refuse more).
  validate: function (params, home) {
    var item, hasStart, hasEnd;
    if (typeof params.zone !== "string" || params.zone === "") return "Zone parameter is required";
    if (!home.zone(params.zone)) return "Zone not found: " + params.zone;
    if (typeof params.equipment !== "string" || params.equipment === "") return "Equipment parameter is required";
    item = home.equipment(params.equipment);
    if (!item) return "Equipment not found: " + params.equipment;
    if (item.zoneId !== params.zone) return 'Equipment "' + item.name + '" does not belong to the selected zone';
    if (typeof params.dataKey !== "string" || params.dataKey === "") return "Data key parameter is required";
    if (item.data.indexOf(params.dataKey) < 0) {
      return 'Equipment "' + item.name + '" has no data binding with alias "' + params.dataKey + '"';
    }
    if (!given(params.watchValue)) return "Watch value parameter is required";
    if (given(params.delay) && typeof params.delay !== "number") return durationText(params.delay);
    hasStart = given(params.checkStart);
    hasEnd = given(params.checkEnd);
    if (hasStart !== hasEnd) return "The check window needs both a start and an end time";
    if (hasStart && !hhmm(params.checkStart)) {
      return "Invalid start time: " + String(params.checkStart) + '. Use HH:MM (e.g., "21:00")';
    }
    if (hasEnd && !hhmm(params.checkEnd)) {
      return "Invalid end time: " + String(params.checkEnd) + '. Use HH:MM (e.g., "06:00")';
    }
    return "";
  },

  start: function (api) {
    var p = api.params;
    var c = {
      api: api,
      dataKey: p.dataKey,
      watch: String(p.watchValue),
      delay: given(p.delay) ? p.delay : null,
      start: given(p.checkStart) ? String(p.checkStart) : null,
      end: given(p.checkEnd) ? String(p.checkEnd) : null,
      hasWindow: false,
      label: "",
      delayId: null
    };
    c.hasWindow = c.start !== null && c.end !== null;
    c.label = aliasLabel(c.dataKey, c.watch);

    // The state keys exist from the start (so a notification can pick them).
    var st = api.state;
    if (st.get("alarm") !== true && st.get("alarm") !== false) st.set("alarm", false);
    if (st.get("alarmSince") === undefined || st.get("alarmSince") === null) st.set("alarmSince", null);

    api.on("equipment", c.dataKey, function () {
      evaluate(c);
    });
    if (c.hasWindow) {
      api.at(c.start, function () {
        evaluate(c);
      });
      api.at(c.end, function () {
        evaluate(c);
      });
    }
    restoreState(c);
    publishSummary(c, st.get("alarm") === true);
  }
});
