# Plan: Spec 043

## Steps

- [ ] 1. Probation `Health` with the PSRAM, its reason logged once; the firmware's `healthy()` and the
      OTA thread without its buffer
- [ ] 2. The restart route, `Reply::Restart`, the safe mode's exception; host 501; firmware 202 then
      `restart_after_update`, once
- [ ] 3. The Restart card
- [ ] 4. Docs: living spec, sowel-subset, D79, roadmap

## Test plan

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| probation | Healthy with the PSRAM for 60 s | Confirm |
| probation | Engine and API up, no PSRAM, until 300 s | Roll back |
| probation | PSRAM present, engine down | Not healthy |
| probation | Healthy 30 s, then the PSRAM missing, then back | The 60 s count starts again |
| probation | The reason over 300 ticks | Logged once per change |
| api | `POST /system/restart` with a session | `Reply::Restart` |
| api | Without a session | 401 |
| api | During an update received; an update check; a backup | 409 with each text of R1.2 |
| api | During a probation | 409 `The update is being checked, restart in a minute` |
| api | An update installed waiting for its restart; a restore waiting | `Reply::Restart` |
| api | In safe mode: the restart; another `POST` | `Reply::Restart`; 503 |
| host | The host's answer to `Reply::Restart` | 501 with its text |
| ui | Cancel; confirm and the box back; confirm and a 401; 90 s without answer; a 409; the host's 501 | No call; reload; reload; the message; the text and the button back; the text |
| ui | The card in French and in English | Its texts |
| box | The button (with the user) | Restarted, cause `asked`, the page back at the login |

## Validation

- [ ] Validate command green
- [ ] Agent review done, blocking findings fixed
- [ ] Demo seen and approved by the user (box: with the user)
