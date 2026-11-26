# Plan: Spec 042

## Steps

- [ ] 0. The user's review of the spec and answers to its open questions (catalogue repository,
      reserve, `motion-light-script` built in or not)
- [ ] 1. The signed file: header, checks, the recipe key built in; `--recipe-sign`, `--recipe-verify`
- [ ] 2. `Storage::remove` and `list`; the engine's `replace_script` and `remove_script`
- [ ] 3. One guard for the update, the backup and the install (from `Updates::claim` and the
      backup's state); install, update, uninstall, boot load before `start_all`, factory reset; the
      memory rule and `MemoryProbe`
- [ ] 4. The catalogue and the store routes; the download
- [ ] 5. `GET /system/resources`; the firmware's probe
- [ ] 6. Interface: the Resources and Recipes cards
- [ ] 7. Docs: living spec, sowel-subset, D78, memory budget, roadmap
- [ ] 8. Box run with the user

## Test plan

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| file | A file signed with the recipe key | Accepted, the source and version out |
| file | Signed with the firmware's key; with another key; one byte of the source changed; of the header; the magic `SOLITEUP`; format 2; reserved bytes set; length field wrong; 127 bytes; 16 KB + 129 bytes | Refused, each with its reason |
| file | The script declares 1.0.1 behind a header of 1.0.0 | 400 `version differs from the signed one` |
| file | Format 2; an id of 33 characters; an id with `/` | Refused with its reason |
| install | The catalogue's id or version differs from the file's | 400, nothing written |
| catalogue | A valid list; 33 entries; 32 KB + 1; bad JSON; a URL refused by D66 | Kept; refused as a whole, the last good list kept |
| install | From the catalogue; from an upload | 201, `RecipeInfo` and version; the file written once |
| install | Size or SHA-256 differs from the catalogue | 400, nothing written |
| install | A built-in id; an installed id; a third script recipe; an unknown id; `minVersion` above the firmware | 409, 409, 409, 404, 409 |
| install | Kept stopped instances of that id, enabled and disabled | The enabled ones start |
| guard | An install during a backup; an update during an install; a backup during an install | 409 with the running job's name |
| memory | Probe with `free − limit` under the reserve; just over | Refused with the figures; admitted |
| update | Newer version that loads | Instances restarted with their settings, file replaced |
| update | Newer version that does not load | Old one back, instances restarted, 400 with the reason |
| update | Same or older version (catalogue or upload) | 409 |
| update | The write fails after the new version loaded | Old version back, 500 |
| update | The old file does not load again | Recipe failed, instances stopped with the reason, 500 |
| uninstall | Installed recipe | Instances stopped and kept, `Recipe not found: <id>` logged, file removed |
| uninstall | A built-in script | 409 |
| boot | Two installed files; one changed on the flash | The good one loaded; the other refused, logged, left in place |
| boot | Safe mode | No installed script loaded |
| boot | An enabled instance of an installed recipe | Running after the restart |
| boot | An installed id that a built-in script also has | Built-in loaded, the file left and logged |
| reset | Factory reset with an installed recipe | Its file removed |
| resources | The route with a fake probe | Sowel-style camelCase JSON, the scripts' rows |
| api | The routes without a session; in safe mode | 401; refused |
| host | `--recipe-sign` then `--recipe-verify`; verify of a changed file | Exit 0; exit 1 with the reason |
| ui | The Resources card with a fake answer; no PSRAM | Bars and rows; the PSRAM row hidden |
| ui | The Recipes card: install, update, remove, upload; an error | The calls made; the message shown |
| flash | Install, update, uninstall on the box (`box.sh saves`) | One write each, none at boot |

## Validation

- [ ] Validate command green
- [ ] Agent review done, blocking findings fixed
- [ ] Demo seen and approved by the user
