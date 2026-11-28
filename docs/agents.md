# How agents are run

The rules for models, command output, agent briefs and the project's agents (moved out of `CLAUDE.md`, still strict).

- **Models, to save tokens**: the session runs on Sonnet (`/model sonnet`); Opus is the exception.
  Whoever launches an agent writes `model:` explicitly (an agent without it inherits the session's).
  **Opus**: a hard spec (a new block, a choice of architecture or data model, several options
  to weigh), arbitrating, `code-reviewer` (set in its file), an agent that loops (e.g. EZSP).
  **Sonnet**: everything else: a spec that follows an existing pattern, reading (`Explore`), code
  and tests (`implementer`, following the approved `plan.md`, set in its file), `spec-reviewer`,
  `validate.sh`, roadmap, docs, small fixes. For a spec, the user chooses by its difficulty
  (`/model opus`, then back to `/model sonnet` after the approval); remind them when a session
  drifts to Opus for routine work.
- **Output out of the context (strict, you and every agent)**: a command that can print more than
  a few lines (tests, builds, QEMU, `npm`, `gh`, `validate.sh`) runs as `sh scripts/quiet.sh <command>`,
  never bare; read the full log only when its verdict lines are not enough. Every agent prompt
  repeats this (Sowel issue 1291).
- **No hang (strict, you and every agent)**: a test that waits on a socket, a child process or a
  thread has its own timeout and fails instead of waiting forever (a read timeout, a kill after
  30 s). Never leave a test run or a host process behind in the background: one run at a time,
  killed before the next. While an agent builds, the session watches for a test binary of
  `target/` running more than 10 minutes and steps in at once.
- **Agent briefs (strict)**: one agent, one subject (split a build into several agents, each
  with its own brief, rather than one that piles up context). The prompt names the files and
  lines to start from (`serve.rs:329`), so the agent does not search, and says to read a large
  file by range. The agent writes its report to a file whose path the prompt gives (the session's
  scratchpad) and answers in one line; read the file only when you need it.
- **Project agents**: `.claude/agents/` (`spec-reviewer`, `implementer`, `code-reviewer`), launched
  by the feature and fix skills; each sets its model in its file.
  If one is not available as an agent type (a conversation started outside the project folder),
  launch a general-purpose agent and tell it to follow that file word for word.
