# Teams module tray actions (0.5)

An AW host supporting module actions protocol 1 reads the UTF-8
aw-watcher-teams.exe.actions.json beside aw-watcher-teams.exe. Older trays retain
their ordinary module toggle; they do not acquire a submenu from this file.

| Action | Literal arguments | Behavior |
| --- | --- | --- |
| Configuration and privacy | configure | Opens the existing settings form; works while AW is offline |
| Operational log | logs-ui | Displays only typed timestamp/state records |
| Server reachability | status-ui | Probes /api/0/info with a two-second timeout |
| About | about | Displays version and inference/ownership information |

Actions dispatch before the producer lock or collector. Unknown commands, unsupported
protocols and mismatched action IDs fail. No action starts another producer.
Manual --settings remains an alias for configure. --no-ui on these actions runs the
same handler without opening a dialog, for isolated validation.

The shared resolver consumes AW_MODULE_ACTION_PROTOCOL=1, AW_MODULE_ACTION,
AW_PROFILE, AW_TESTING=0|1 and AW_SERVER_URL. Explicit --profile, --testing,
--server and --config options provide manual overrides; conflicting explicit
profile/testing options fail. Profiles use lowercase letters, digits, hyphens and
underscores, up to 32 characters, starting with a letter or digit.

Default configuration, delivery checkpoint, lock and logs retain their existing
%LOCALAPPDATA%/activitywatch/aw-watcher-teams location. Named profiles use
%LOCALAPPDATA%/activitywatch-<profile>/aw-watcher-teams; --testing selects testing
and defaults to port 5666. Other profiles default to port 5600. Legacy shared
testing configuration is not imported. Server overrides must be HTTP loopback.
Producer and actions use this same resolver.

Operational logs contain UTC timestamps and enumerated state/delivery codes, never
captions, account, organization, arbitrary exception strings or configuration values.
The log viewer skips malformed/unrecognized records and displays at most 50 records.
A 32 KiB rotation retains one backup. Server reachability does not assert that a
producer is running, events are delivered or someone attended a call; a testing
context mismatch is reported separately.

AW owns the direct action process. Settings and collector PowerShell helpers belong
to a Windows job with kill-on-close, including descendants. Helpers run synchronously
and are not detached. Native information dialogs do not spawn an external viewer.
The host remains responsible for action termination and producer supervision.

Build with scripts/build.ps1 -Test -Package. The release executable and ZIP contain
the matching full-executable-name sidecar. The installer copies both and backs up
existing files with .before-teams. Installing a binary without a source sidecar removes
the stale installed sidecar and Settings.lnk. For rollback, restore a matching
executable/sidecar pair; remove the sidecar when the older executable lacks actions.
Restart modules only through AW. No live installation is part of these validation runs.
