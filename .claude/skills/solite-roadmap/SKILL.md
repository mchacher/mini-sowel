---
name: solite-roadmap
description: |
  Plans and reviews the roadmap of Solite: the next milestone toward the true
  north, cut into increments built one by one with /solite-feature, and the new
  needs placed as they come. Use when the user types /solite-roadmap, right after
  the setup, when a milestone is reached, when a new need comes up, or when they say "roadmap",
  "what next", "on fait quoi ensuite", "replanifier", "j'ai un nouveau besoin".
argument-hint: "[a new need, optional]"
---

# Solite Roadmap

New need: $ARGUMENTS

Models (table in `CLAUDE.md`): this skill runs on Sonnet; any agent you launch names its `model:`.

The roadmap says what gets built next and why, toward the **true north** in `CLAUDE.md`. It is
never frozen: every feature teaches something, and new needs come up at any time. It is planned
once at the start, reviewed briefly after every feature, and re-planned at every milestone.

The method is **agile in the first sense of the word**: the direction is set (the true north),
the tool moves toward it in small steps, and every step is a working increment the user can test.
Each step checks the tool is heading the right way, and adds value: after it, the tool does
something useful it could not do before, never a half-built part. If a step shows the direction
was wrong, the next steps change.

The user decides **what** comes next and in which order; you propose, with your reasons. Talk in
the user's language, plainly; files in English. Gates marked **STOP** wait for the user's explicit
yes, whatever the permission mode.

The file is `docs/roadmap.md`: the current milestone and its increments, the next milestones in
one line each, the **inbox** of new needs not yet placed, what is done, and a log of every change.
It is the place for the future and for how far the work has got: a planned block or capability
is written here, never in the docs that describe the tool (rule in `CLAUDE.md`, "Docs say what is
true today").

---

## Phase 1: Read and Choose the Mode

Read, in this order: `CLAUDE.md` (the true north), `docs/roadmap.md`, `docs/living-spec/`,
`docs/specs-index.md`, `docs/decisions.md`, `inputs/README.md`, and the "Steps" and "Risks and
unknowns" sections of `inputs/note-de-cadrage-mini-sowel.md`. Add the new need given with the
command, if any, to the inbox.

Then choose:

- **Plan** (Phase 2): no current milestone yet (first run after the setup), the current milestone
  is reached, a new need changes what the milestone should be, or the user asks for it.
- **Quick review** (Phase 3): otherwise, and always at the end of a feature.

> **GATE 1**: everything above read; mode chosen and told to the user in one sentence.

---

## Phase 2: Plan

### 2.1 The next milestone

Propose **one** milestone: a state of the tool where the user can do something useful they could
not do before. Write:

- **The user can then**: what they will do with the tool, in their words;
- **Why this one first**: how it brings the tool closer to the true north;
- **Done when**: what the user will see, with real or realistic data.

Keep it small: three to six increments. The first milestone of a new tool proves its **core** works
(the one thing without which the true north is out of reach), even roughly, before accounts,
settings, look or deployment. An existing project's first increment is always spec 001, the
safety net.

### 2.2 The increments

Cut the milestone into increments. Each one:

- is built in **one** `/solite-feature` conversation, and shown in a demo;
- leaves the tool working and more useful than before: value the user can test, not a layer of
  plumbing that only pays off later;
- has one line, and a **done when** the user can check;
- is not specified yet: the spec is written in `/solite-feature`, when its turn
  comes, with what the previous increments taught. The one exception is spec 001 of an existing
  project (the safety net), written at setup: the feature skill picks it up.

Order them: first what removes the biggest risk on the core (does it work at all, with real
data?), then what brings the most value, then the rest. Say why in one line for the first two.

For Solite, the risks of the framing note (`inputs/note-de-cadrage-mini-sowel.md`, "Risks and
unknowns") come first, each with a demo on real hardware: EZSP written in Rust (the radio spike),
the share of Zigbee2MQTT converters that export, the ESP32's memory. An increment that needs a
device or the box says which one, and whether the user has it. Work on the Sowel side (the
proposal prototype, `sowel-plugin-mini`, a Sowel issue) is not an increment of this repository: it
is listed as a dependency of the increment that needs it.

### 2.3 The inbox and the later milestones

Place every inbox item: in this milestone, in a later one, or dropped. Dropping needs the user's
yes and a reason in the log: nothing disappears silently. Later milestones get **one line each**,
no detail: they will be planned when their turn comes.

### 2.4 Present, in the user's language (translate the labels)

```
## Roadmap

**True north**: <one line, from CLAUDE.md>
**Next milestone**: <name>. The user can then: <...>
  Why first: <...>. Done when: <...>
**Increments**:
  1. <increment>: done when <...>  (first because <...>)
  2. ...
**Later**: <milestones, one line each>
**Inbox placed**: <item → where, or "dropped: reason">

Does this order suit you?
```

> **GATE 2 (STOP)**: the user approved the milestone, the increments and their order. If they
> change something, present again.

If the increments have ids (`B3`), set `PROJECT_FUTURE` at the top of `scripts/check-docs.sh` to
match them, with only the milestones' letters (for example `'[A-C][0-9]+'` for milestones A to
C), so that "available in B3" is refused outside the roadmap.

Write `docs/roadmap.md` and add a line to its log (date, what changed, why), on a branch:

```bash
git checkout main && git checkout -b docs/roadmap-<milestone>
git add docs/roadmap.md scripts/check-docs.sh && git commit -m "docs(roadmap): <milestone>"
```

Ask "Shall I merge the roadmap into main?" Only after an explicit yes:
`git checkout main && git merge --squash docs/roadmap-<milestone> && git commit -m "docs(roadmap):
<milestone>"`, then delete the branch. Every feature starts from `main`: the roadmap must be there.
Then Phase 4.

---

## Phase 3: Quick Review

Called at the end of every feature (before its commit, so the roadmap is updated in the same
change), or when the user brings a new need. Keep it short.

1. **Done**: mark the increment just built as done, with its spec number.
2. **Ask**, in one message, at most two questions:
   - "Did this feature give you a new need, or change one?" (the demo often does);
   - "Is the next increment still the right one, and are we still heading toward the true
     north?"
3. **New needs** go to the inbox, one line each, with the date and where they came from. Place one
   in the current milestone now only if the user wants it and it does not change the milestone.
4. **Re-plan due?** If the milestone is reached, or a new need changes what the milestone should
   be, or the inbox holds more than five items, say: "A full roadmap review is due:
   `/solite-roadmap` in a new conversation."
5. Add a line to the log if anything changed.
6. **`CLAUDE.md` still short?** Count its lines (`wc -l CLAUDE.md`). Over about 150, propose to the
   user what to move: a subject that grew goes to `docs/`, or becomes a check that enforces it.

> **GATE 3**: increment marked done; the user's answer about new needs recorded in the inbox (or
> "none"); a full review proposed if due; `CLAUDE.md` under about 150 lines, or a move proposed.

---

## Phase 4: Next Step

Tell the user the next increment, and how to start it: `/solite-feature
<increment>`, **in a new conversation**.
