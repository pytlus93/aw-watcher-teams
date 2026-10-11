# MS Teams ActivityWatch module (0.4)

Local AW-managed Windows worker with a reusable Rust session tracker. No Graph,
Teams cloud API, audio/video recording, injection, message bodies, credentials or roster.

## Capture and titles
Enumerates every top-level window owned by each ms-teams process, irrespective of
foreground, visibility or minimized state. It does not select only MainWindowHandle.
Button accessibility names are used transiently for a narrow allowlist:
leave/hang up plus microphone, camera or sharing controls, with no join control.
Window captions are read ONLY for qualifying call windows when collect_meeting_title
is enabled. The collector falls back to that qualified window's UIA root name when
its Win32 caption is empty/generic. No arbitrary text nodes/chat headings are searched.
Root name/caption may still be generic or absent; then context remains unavailable.

data.title contains the topic and configured context labels with the Teams suffix removed.
No app/watcher prefix is added. In extended mode, selected meeting_title/title_source properties carry topic/provenance. See title_format below.
Fallbacks are Call (title unavailable) and Meeting (title unavailable).
call_kind is meeting only when a recognized meeting-info control is exposed; otherwise
unknown. A topic alone does not prove meeting vs 1:1/group call.
Title collection defaults off for new installations, and must be enabled explicitly.
The user requested title capture for the installed local module; it is enabled there.
Privacy-disabled collection avoids reading captions and redacts undelivered titles.

English/Czech control rules remain provisional; locales and Teams versions vary.
A title may contain personal/topic metadata. It stays in the local AW bucket and
local delivery checkpoint, never console/error logs. No private fixture is committed.

## One interval per observed call
Two matching observations create the first positive-duration event, using the first
observation as its UTC start. Nothing is written for the initial instantaneous sample,
prejoin, unknown or ambiguous state. No fabricated positive duration is assigned.
Further observations update that SAME AW event ID; they do not append heartbeat
fragments. Sharing/mute/camera/title changes do not split the event.
The event duration grows through continuous evidence and stops at the last positive
observation. The end boundary has polling uncertainty; an unobserved tail is not added.

The locally generated session_id is not a Teams meeting ID. Process start time plus
HWND is volatile correlation only. Same-title meetings on different windows are not
automatically joined. Multiple qualifying windows remain ambiguous; a duplicated
meeting toolbar may require future validated reconciliation.
Failure, loss of qualifying controls, different window/process or a large clock gap
starts a fresh observed interval. Thus an actual call can still have separated
evidence intervals when observation fails. It never claims those gaps were attended.
Collector sleep uses bounded wall-clock spacing. The collector now rejects lock/secure desktop before inspecting UI. Suspend is
bounded by sampling-gap checks rather than explicit power notifications. Attention/AFK
integration remains separate work; these intervals are call evidence, not work time.

## Delivery
POST /events inserts a positive session event; subsequent POSTs use its returned ID
to replace only that event. A local atomic session-delivery.json checkpoint persists
queued updates and IDs. A lost insertion reply is recovered using a selected local session_id or the original
start timestamp at AW millisecond precision; ambiguous matches fail closed. See
[the contract](docs/DATA-CONTRACT.md) for strict-mode reconciliation. Completed failed sessions remain queued;
backlog is capped at 1000. The latest acknowledged session is retained for updates.
Restarts preserve delivery but start new detection identity; they do not infer that a
call survived the worker's downtime. No old history is deleted or reconstructed.

Strict mode (the new/missing-config default) sends only the workspace-required title;
extended mode adds individually selected fields. This is a custom bucket with no
official data schema. [Every field, defaults and migration](docs/DATA-CONTRACT.md)
and [current evidence/limitations](docs/VALIDATION.md) are documented separately.

The custom call-evidence bucket ID is aw-watcher-teams_<hostname>. The installed
Python AW server ignores a creation name field and its public update route does not
support renaming this bucket; do not claim its label is MS Teams in the GUI.
data.title is supported metadata but does not enable ordinary window-watcher charts.
Dedicated query/timeline/review UI integration remains pending.

## Lifecycle and settings
AW owns discovery, startup and storage; no independent tray app, service or task.
Build:
  cargo test --offline --locked
  cargo clippy --offline --all-targets -- -D warnings
  cargo build --offline --release --locked
Install:
  scripts/install.ps1 -ActivityWatchDirectory <existing AW directory>
Installer initializes settings and preserves/backups the AW tray module list.
Restart AW to discover/start the module; Modules -> aw-watcher-teams controls it.
Only HTTP loopback is accepted and redirects are disabled. Default AW server is
127.0.0.1:5600; use a separate 5666 test server where available.
Config: %LOCALAPPDATA%/activitywatch/aw-watcher-teams/config.json
Fields: server, interval_seconds (2..60), collector_timeout_seconds (1..15),
collect_meeting_title (bool, default false), title_format (optional object). Existing config files remain compatible.
The collector runs in a hidden bounded subprocess. Single-instance lock prevents
duplicate workers. Custom AW_PROFILE directories are explicitly unsupported.
Run the module with --settings (or Settings.lnk in the installed module directory)
for mode/field/privacy choices. Use AW Modules to restart it after saving. Mapping,
separator, server and timing still use JSON. The dialog has no direct tray-menu entry;
visual behavior has not been verified in this validation run.

## Validation on 2026-10-08
20 automated tests passed: hour-long accumulation, positive-only records, privacy,
prejoin/ambiguity, changing controls/titles, identity changes, failure, clock gaps,
loopback restrictions, synthetic collector JSON/switch handling and timeout.
An explicit live AW protocol test passed against the installed server in an isolated
synthetic bucket: repeated updates produce ONE event at 3600 seconds; lost insertion
acknowledgement/restart recovery keeps the same ID; privacy redaction also updates it.
Test bucket is deleted by its ownership guard, without touching real history.
Run that opt-in test:
  $env:AW_PROTOCOL_SERVER='http://127.0.0.1:5600/'
  cargo test --offline --locked installed_aw_updates_one_session_and_recovers_lost_ack -- --ignored
PowerShell syntax, Clippy and release build passed.
Live read-only collector execution enumerated 16 Teams windows with zero qualifying
call windows. This verifies enumeration/execution, not real meeting title extraction.
User-supplied legacy export had 291 call fragments (~3312 seconds) and 355 zero
unknown observations. It is defect evidence, not a complete ground-truth test.

Before claiming complete compatibility, test synthetic ongoing calls in background,
minimized windows, sharing toolbar/pop-outs, listen-only mode, 1:1/group/scheduled
calls, lobby/ringing/device tests, accounts, reconnects, restart/suspend and locales.
Compare an independent join/leave timeline with captured title and duration.

## Sources / license
Original code, MIT. No prior-art code copied.
https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-enumwindows
https://learn.microsoft.com/en-ie/windows/win32/winauto/uiauto-eventsoverview
https://docs.activitywatch.net/en/latest/watchers.html
Discontinued device API deliberately excluded.

## Configurable title formatting (0.3)

Add title_format to config.json (other existing settings stay as they are):

```json
"title_format": {
  "include_organization": true,
  "include_account": false,
  "organization_map": {
    "Example Company Ltd.": "Example",
    "Example Subsidiary": "Subsidiary"
  },
  "separator": " | "
}
```

For a validated caption like
Echo | Example Company Ltd. | self@example.invalid | Microsoft Teams
the displayed title becomes Echo | Example.
Set include_organization to false for Echo alone. Set include_account to true only
if you deliberately want the account suffix. separator may be changed to " — ".
Mapping keys match full organization names without case sensitivity. Unmapped
organizations retain their name. An empty mapped value omits that organization.
Mappings are literal names, not regular expressions.

The topic is preserved (including pipes inside the topic). The trailing email is
classified as the current account only in a caption with a preceding topic. An
organization without an email suffix is recognized only when it explicitly matches
a mapping key; unknown pipe-separated topic text is not guessed to be an organization.
These rules are based on the observed desktop caption format; new Teams formats
may need another parser profile.

When selected in extended mode, data.meeting_title stores only the topic; data.organization retains the permitted
canonical organization so replay does not chain aliases. The shortened label is used
in data.title. Disabled account/organization fields are omitted from emitted metadata
as well as titles. Account collection is off by default; email in an actual meeting
topic is not indiscriminately removed. collect_meeting_title=false still disables all
caption metadata. The collector reads the permitted whole caption transiently to parse
its parts, then discards the unused fields; no raw caption field is persisted.
These events use schema_version 3.

After editing settings, stop/start this module through AW's Modules menu (or restart AW).
Formatting of the latest session retained in the delivery checkpoint is reapplied in
place on startup, preserving its AW event ID, session ID and duration. Earlier history
is not bulk rewritten. Turning a field back on cannot recover previously discarded
historical metadata; later observations can supply it again.

0.3 validation: 37 automated tests passed, plus the explicit installed-AW synthetic
protocol test. New checks cover mappings, privacy options, custom separators, long
topics, topic pipes, legacy caption migration and replay without repeated aliasing.
The historical real AW test confirms formatting changes leave one event and its duration intact.

0.3.1 startup/delivery recovery: invalid delivery checkpoints are preserved in a
uniquely named .corrupt-* file and startup continues with an empty queue. Unsent
records in a damaged file cannot be recovered. Checkpoint writes sync file contents
before replacement. Collection begins while AW is unavailable; bucket initialization
and missing-bucket recovery belong to delivery retries. Events use the API list form.
Validation: 46 automated tests passed, including zero-filled checkpoint recovery,
existing-bucket 304, producer restart, late AW startup, socket outage/server restart,
lost replies and stale event IDs. Transport recovery tests use synthetic loopback
fixtures; real-server restart and Rust-server acceptance remain unverified.


## Current source validation (0.4)
Use scripts/build.ps1 -Test -Package. This run does not deploy its package.
See docs/VALIDATION.md for exact current results, isolated server commands, lifecycle
boundaries and remaining host/UI requirements. Historical counts above describe the
older versions rather than current acceptance. No hard reboot or live host crash was
performed. A login shortcut alone does not prove hard-restart recovery.
