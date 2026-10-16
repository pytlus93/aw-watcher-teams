# Validation: Teams module 0.4 — 2026-10-11

**Component checks pass; full installed-host acceptance remains incomplete.**
This run edits/builds only this checkout. It does not replace the installed 0.3.1
module, stop/restart production AW, reboot/lock Windows, read private Teams windows,
or open an interactive settings/test window.

Toolchain: rustc 1.99.0 (b940084d7, 2026-09-28), cargo 1.99.0
(5f94df478, 2026-08-27), Windows/MSVC, Windows PowerShell.
Both supplied AW server executables report version 0.13.2.

## Evidence and results

| Layer / scenario | Result | What was actually checked |
| --- | --- | --- |
| Build | PASS | Offline locked tests, Clippy with warnings denied, optimized release build |
| Normal tests | PASS: 56 | 34 core/title unit tests, 4 synthetic collector tests, 7 payload/settings/interop tests, 11 TCP transport tests |
| First bucket / existing 304 | PASS | TCP fixture and both isolated actual AW servers; creation 304 continues actual event updates |
| Genuine errors / endpoint scope | PASS | Creation 500, incompatible bucket type, event endpoint 304 all fail; no false acknowledgement |
| Producer restart | PASS | Reopen checkpoint, update same event ID/duration; TCP and actual server fixtures |
| Late AW / socket outage / restart | PASS | Collect/queue before host starts; stop only owned test server, queue update, restart and query resumed event |
| Missing/recreated database | PASS | Stale IDs cleared, retained update reinserted with original start; synthetic + Python memory-server recreation |
| Lost replies / restored-ID ownership | PASS | Synthetic lost insertion reply causes no duplicate; unrelated exposed session IDs are not overwritten |
| Strict identity collisions | PASS | Multiple timestamp matches fail closed; no arbitrary overwritten event |
| Original timestamps / gaps | PASS | Isolated servers preserve start at millisecond resolution, actual two-interval export; tracker-to-delivery outage test sums only observed segments |
| Extended legacy compatibility | PASS | Existing opt-in same-ID/hour-long/lost-ack/privacy/migration fixture run on an owned isolated Python server |
| Field/privacy contract | PASS | Every extra field individually selectable and absent when disabled; topic/org/account omitted from title and queued context; actual server API/export omissions on Python and Rust |
| Field order | REQUEST PASS; API DIFFERENCE | Wire title precedes extensions; Python API retains title before schema_version, Rust API reorders; no GUI-order promise |
| Lock / crash / relaunch | PASS (component probe) | Real isolated processes use the worker's shared OS-lock function; duplicate rejected, killed probe releases lock, intentional stop exits, invalid path fails, later launch succeeds |
| Caption collector output | PASS | Synthetic 500-window JSON does not deadlock a full pipe; 1 MiB limit, timeout/failure become unknown |
| Secure-desktop handling | SOURCE + INTEROP PASS | Guard before UI traversal rejects inaccessible/non-Default input desktop; C# interop compiles; no live lock/UAC/RDP transition test |
| Settings dialog | LOGIC/SYNTAX PASS; UI UNVERIFIED | Defaults match Rust; no form opened; visual layout, click/save and installed shortcut not exercised |
| Windows login | REGISTRATION ONLY | Read-only ActivityWatch.lnk resolves installed aw-qt.exe; Teams appears in existing autostart config from prior host check |
| Hard reboot / live module crash | NOT PERFORMED | No hard restart, login-after-reboot, installed-tray crash/relaunch or intentional-stop-across-login test |
| Bounded AW host recovery | NOT ACCEPTED | Component cannot prove installed tray's backoff, retry after launch exceptions, continued operation of other modules, or persisted intentional stop |
| Foreground / idle | SEPARATE SEMANTICS | User explicitly requires background/minimized call windows; evidence duration is not foreground work time. AFK/attention joining is not implemented |
| GUI bucket / tooltips | UNVERIFIED | Required title/custom metadata do not create ordinary current-window charts; direct tray settings action/custom timeline presentation are missing |

Python actual-server restart uses isolated **memory storage**, so restart recreates
its bucket. Rust uses an isolated **SQLite file**, so restart preserves its bucket.
TCP fixtures separately exercise preserved and missing bucket states. This is not
a Python persistent-database crash durability or hard-PC-restart test.

The initial timestamp test used submillisecond precision and failed against the
actual Python server. Its API returned .123Z for .123456Z. The accepted assertion
preserves the original instant at AW's millisecond precision; it does not pretend
the server preserves finer precision. Strict-mode acknowledgement recovery uses
that same observed resolution.

Checkpoint corruption is still handled by preserving the damaged file and starting
a fresh queue; missing unsent history cannot be recovered. Valid caption context is
scrubbed from the retained queue on privacy changes. Previously delivered earlier
history, old settings backups and damaged checkpoint files are not automatically
erased. New output/export privacy acceptance is separate from retrospective erasure.

The collector runs as a bounded child while the worker is alive. Abrupt worker
termination may leave its already-launched collector helper until that helper exits;
there is no worker-owned crash supervisor or validated kill-on-parent-exit job.
No fresh interval is inferred across a worker crash or observation failure.

## Reproduction (isolated, no install)

~~~powershell
scripts/build.ps1 -Test -Package

$env:AW_TEST_SERVER_KIND = 'python'
$env:AW_TEST_SERVER_EXE = '<AW directory>\aw-server\aw-server.exe'
cargo test --offline --locked --test server_lifecycle -- --ignored --nocapture

$env:AW_TEST_SERVER_KIND = 'rust'
$env:AW_TEST_SERVER_EXE = '<AW directory>\aw-server-rust\aw-server-rust.exe'
cargo test --offline --locked --test server_lifecycle -- --ignored --nocapture

cargo build --offline --locked --example module_lock
$env:AW_TEAMS_LOCK_PROBE = (Join-Path (Get-Location) 'target\debug\examples\module_lock.exe')
cargo test --offline --locked --test module_lifecycle -- --ignored --nocapture
~~~

The server harness chooses an ephemeral loopback port, uses only synthetic events,
sets isolated app-data directories and no legacy import, and terminates only the
child PID it created. Python also runs the earlier opt-in protocol fixture in a
separate owned synthetic bucket on that same isolated server. Helpers and private
test state under ignored local/ are not part of the installed module.

## Remaining acceptance / ownership

AW host work must implement/validate bounded restart backoff, retry failed relaunches
without abandoning other modules, persistent explicit stops and clean startup after
hard reboot. Do not add a second module-owned service/scheduled task to bypass AW.
Installed AW manager/tray source and version behavior were not conclusively verified:
current upstream source has crash-restart logic, but that is not evidence that the
bundled installed host contains it or meets this workspace policy.

Settings need a deliberate visual/save test and optional direct AW tray-menu action.
Real lock/suspend/reconnect and Teams minimized/pop-out/sharing/locale/title variants
need their independent ground-truth timelines. AW GUI display/tooltip order needs
presentation-layer validation; Rust API order demonstrates why request insertion
order alone cannot satisfy it.

The installed generic schema at /api/swagger.json was read without accessing any
private events. Its Event object describes timestamp, duration and object data,
with no official call-evidence properties. The route /api/0/swagger.json returned
404; this was a schema-location probe, not a failed transport acceptance test.

References: [AW REST API](https://docs.activitywatch.net/en/latest/api/rest.html),
[data model](https://docs.activitywatch.net/en/latest/buckets-and-events.html),
[watchers](https://docs.activitywatch.net/en/latest/watchers.html),
[writing a watcher](https://docs.activitywatch.net/en/latest/examples/writing-watchers.html),
[upstream manager](https://github.com/ActivityWatch/aw-qt/blob/master/aw_qt/manager.py),
[upstream tray](https://github.com/ActivityWatch/aw-qt/blob/master/aw_qt/trayicon.py).
See DATA-CONTRACT.md for every data field and settings migration.

All results above came from local commands; no GitHub Actions workflow was added,
enabled or used. GitHub Actions are unavailable for this setup. The reproducible
entry point is scripts/build.ps1 -Test -Package; isolated opt-in commands are listed
above. Preserve inherited upstream workflows if any are introduced by a later import.

## Local module actions validation (0.5, 2026-10-11)

scripts/build.ps1 -Test -Package passed: 64 tests, Clippy with warnings denied,
and the release ZIP. Four opt-in tests remain excluded from the normal suite.
The separately built owned_action example and action_children opt-in test also passed:
killing the synthetic owner terminated its PowerShell child. This is isolated job
ownership evidence, not installed AW tray shutdown or live producer crash evidence.

Eight action tests cover shared profile/testing resolution, manual overrides,
malformed context and unknown commands, every manifest action dispatched while the
producer lock is held and the server is offline, synthetic info/testing mismatch,
privacy-safe logs, manual settings alias, and isolated installer sidecar copy/removal.
The rollback fixture uses a current binary with its sidecar absent, not an older
binary compatibility run. No settings dialog, live Teams collection, installed tray,
production process restart, deployment or reboot was exercised.

The 0.4 real-server results above remain historical evidence; those opt-in server
runs were not repeated for 0.5. Transport regression tests passed. The 0.5 collector
now shares the owned-helper job guard; older helper-lifetime limitations above apply
to the older version. Settings visual/save behavior and the upgraded host submenu
still need separate validation. See TRAY-ACTIONS.md for protocol and rollback details.

## 2026-10-12 call-gap diagnosis and 0.5.1 diagnostics

The user reported one continuous call with screen sharing and camera changes. Privacy-safe logs from the real desktop namespace show joined_candidate → unknown at 11:02:42 UTC, joined_candidate at 11:03:39, unknown at 11:06:07 and joined_candidate at 11:06:13. Stored call intervals end at the last positive observations, hence the visible gaps. This is loss of collection evidence, not evidence of hang-up. The old unknown code does not reveal whether collection failed or controls became unavailable, so media changes are a plausible correlation, not a proven cause.

0.5.1 adds collector_failure and controls_unavailable diagnostic codes and logs reason transitions even when the state stays unknown. Codes contain no titles, accounts, organizations, button captions or window IDs. Existing old unknown records remain readable. Payload/event contracts and privacy defaults are unchanged. New regression confirms camera/sharing toggles on the same positively observed call window do not split its local session. Unknown evidence still breaks duration; no historical gaps are filled by assumption.

The initial offline suite passed. A repeated combined transport run subsequently failed the existing database-restore case with Windows socket error 10053; the cause is unresolved. All 11 transport cases passed separately in fresh test processes, and the library suite passed (36 tests, one opt-in ignored). Clippy and release packaging passed; opt-in real-server harnesses remain unrun. No live Teams capture/config changes or deployment performed for this follow-up. The new diagnostics must be installed in the desktop-owned instance before they can distinguish a future interruption; historical unknown records cannot supply information that was not logged.


## Fresh validation — 2026-10-16

Fresh scripts/build.ps1 -Test -Package PASS: 66 tests passed, four opt-in tests ignored; Clippy with warnings denied and release ZIP passed. The initial sandbox run failed a local status HTTP test; approved loopback-capable rerun passed, including all 11 transport cases. Production AW contains positive-duration call-evidence events dated today with title-only data. Installed executable SHA256 differs from this freshly tested executable; receipt does not certify this exact build. No real call scenario, UI, production restart or reboot was exercised.
