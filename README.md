# MS Teams ActivityWatch module — initial implementation

AW-managed local Windows collector and reusable Rust evidence core. This is an initial
UI evidence detector, not validated attendance tracking. No Graph, Teams cloud API,
audio/video recording, injection, message contents, credentials or participant data.

## Behavior
Enumerates all top-level windows owned by ms-teams.exe, including background windows.
Reads button accessibility names transiently and emits only allowlisted boolean evidence.
A leave/hang-up button plus microphone and camera controls, with no join button,
produces joined_candidate. Camera/microphone controls can exist while disabled.
Prejoin, multiple qualifying windows and failures remain separate states.
No control set is unknown, never proof that a call ended.
All candidates remain ui_inferred_unvalidated until actual Teams testing.

data.title is always present, with Call, Presenting, Prejoin, Multiple call windows
or Call state unknown. The bucket display name is MS Teams, so titles omit Teams.
Meeting/call classification and meeting title extraction are not implemented yet.
Neither titles nor roster/message surfaces are read. English/Czech button rules are
provisional: unsupported language/version yields unknown, not compatibility claims.
A single window may contain stale/hidden controls; the rule needs real-host validation.

Adjacent identical joined candidates produce bounded inferred intervals. Transitions,
large wall-clock gaps (sleep), failure or failed delivery break continuity.
These are inferred call intervals, not attention/work time. No AFK override is applied.
Foreground/AFK, lock events, stable session identities, reconnect reconciliation,
capability registry/WASAPI collectors and cross-app JSONL export are future work.
Unknown samples are zero-duration observations. Duration is not backfilled after outages.

## Host integration
The executable is an AW-managed worker, not an independent tray/service product.
AW owns its lifecycle through Modules and configuration through its standard local
component directory; bucket data is stored by AW. No independent scheduled task.
Initial settings are JSON (server, interval_seconds, collector_timeout_seconds).
No dedicated settings/review GUI yet. Custom call-evidence buckets require query/UI
integration: data.title and name do not make them ordinary window-watcher events.

Build: cargo test --locked; cargo clippy --all-targets -- -D warnings;
cargo build --release --locked.
Initialize configuration: aw-watcher-teams.exe --init-config.
The worker expects that initialization before AW starts it.

Use scripts/install.ps1 -ActivityWatchDirectory <AW-directory> after building. It copies the worker, initializes config and preserves/backups the tray autostart list. The installed worker is under ActivityWatch/aw-watcher-teams/aw-watcher-teams.exe.
The installer adds aw-watcher-teams to
[aw-qt].autostart_modules in aw-qt.toml, preserving other modules. Validate discovery
against the installed AW release before deployment. No installation has been performed.
Config: %LOCALAPPDATA%/activitywatch/aw-watcher-teams/config.json.
Only HTTP loopback is accepted, redirects disabled; defaults to 127.0.0.1:5600.
Testing should use a separate AW test server at 5666.
AW_PROFILE is not implemented in this initial version; do not deploy into custom
profiles until corresponding directory handling is added.
Collection subprocesses are bounded by timeout; errors log no private UI content.
Single-instance lock prevents duplicate collection. HTTP delivery is bounded and
failed samples are dropped; there is no durable retry queue in this version.
Worker has no auto-restart policy of its own; AW controls stop/start.

## Validation
Automated tests cover incomplete controls, prejoin, muted/camera-off control evidence,
ambiguous windows, unknown/failure, readable titles, bounded gaps, transitions and
loopback URL restrictions. Synthetic subprocess tests cover valid collector output and termination on timeout. PowerShell syntax is checked without executing real collection.
No private/live meeting data has been inspected. Synthetic tests do not verify
Teams accessibility, minimized controls, policy restrictions or actual AW integration.

Before enabling: test synthetic 1:1/group/scheduled calls, ringing/prejoin/lobby,
background/minimized/pop-outs, silent listening, sharing, device switching,
multiple accounts, reconnects, lock/suspend, Teams restarts and version/localization.
Compare independently noted join/leave times with evidence, report unknown duration,
false positives and boundary error. Explicitly test AW bucket creation/name, duration,
outage recovery and Modules stop/start. Add loopback protocol tests before release.

## Sources and licensing
Original code; MIT. No prior-art code copied.
- https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-enumwindows
- https://learn.microsoft.com/en-ie/windows/win32/winauto/uiauto-eventsoverview
- https://docs.activitywatch.net/en/latest/watchers.html
- https://help.elgato.com/hc/en-us/articles/26249796361613-How-to-pair-Stream-Deck-plugin-with-Microsoft-Teams
The discontinued device API is deliberately not used.
