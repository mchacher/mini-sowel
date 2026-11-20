---
name: add-remote
description: |
  Connects a local project to a remote repository (Bitbucket, GitHub or other), sets up the CI for
  that host and pushes, with the user's approval. Use when the user types /add-remote, or says
  "add a remote", "connect to Bitbucket", "mettre le projet en ligne", "ajouter le dépôt distant".
argument-hint: "[repository URL, optional]"
---

# Add a Remote Repository

Repository URL: $ARGUMENTS

The project already has local git (set up by `/start-software-project`). This skill gives it an
online copy, for backup, sharing and CI. Talk in the user's language, plainly; files in English.

## Phase 1: Check

1. `git status` and `git remote -v`. If a remote `origin` already exists, show it and ask whether
   to replace it; stop if not.
2. If no URL was given, ask for it. Explain where to find it: on Bitbucket or GitHub, create an
   **empty** repository, then copy the address from the Clone button.
3. `git ls-remote <url>`. If it fails, explain in one sentence what is missing (access rights,
   VPN, SSH key or token) and stop: nothing is changed.
4. If the remote is **not empty**, stop and ask: pushing over existing work is not this skill's
   job.

> **GATE 1**: a reachable, empty remote, and no conflicting `origin`.

## Phase 2: CI

Read `docs/decisions.md` for the stack and `CLAUDE.md` for the single validate command. Write the
CI file for the host, running that command on every pull request and on `main`:

| Host                                      | File                                |
| ----------------------------------------- | ----------------------------------- |
| `github.com`                              | `.github/workflows/ci.yml`          |
| `bitbucket.org` or a Bitbucket server     | `bitbucket-pipelines.yml` (Bitbucket server: ask whether pipelines or another CI such as Jenkins is used) |
| Other                                     | The host's equivalent; say what you could not test |

Record the remote as a decision in `docs/decisions.md` (host, URL, CI) and update the hosting line
of `CLAUDE.md` and of the project's feature skill (how pull requests are made).

> **GATE 2**: CI file written, decision recorded, the validate command passes locally.

## Phase 3: Push, on Approval

Present, in the user's language:

```
Remote: <url>
CI: <file>
Branches to push: main (and <others>)
Shall I push?
```

Only on an explicit yes: commit the CI and docs changes (`chore: connect remote and CI`, no AI
attribution line), `git remote add origin <url>`, `git push -u origin main`, then the other
branches if the user wants them. If the host runs CI, check the first pipeline and report its
result in plain words.

> **GATE 3**: pushed and CI result reported, or the user chose to wait.
