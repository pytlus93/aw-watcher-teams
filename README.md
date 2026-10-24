# ActivityWatch call evidence for Microsoft Teams

Version **1.0.0**. ActivityWatch module/client identity: `aw-watcher-teams`.

A Windows module managed by ActivityWatch that infers ongoing Teams calls from qualifying window controls. It observes background and minimized call windows as well as foreground windows. These intervals describe call evidence, not measured attention or proof of attendance.

## Requirements

- Windows with Windows PowerShell and UI Automation available.
- Microsoft Teams exposing recognizable call controls.
- A local ActivityWatch server and `aw-qt` module launcher.

English/Czech control matching is provisional. Teams versions, locales, pop-outs and reconnect behavior need real-call acceptance checks; see [validation](docs/VALIDATION.md).

## Install or upgrade

Build or obtain the `aw-watcher-teams-<version>.zip` package and extract it. Close ActivityWatch normally before replacing a running module. From the extracted folder:

```powershell
.\install.ps1 -ActivityWatchDirectory 'C:\path\to\activitywatch'
```

From source, use `./scripts/install.ps1` with the same argument. The directory must contain `aw-qt.exe`. The installer preserves existing settings, backs up replaced module files/tray configuration and adds the module to autostart. Use `-NoAutostart` to leave the tray startup list unchanged.

Start ActivityWatch normally. **Modules → aw-watcher-teams** controls the worker. Hosts supporting module actions protocol 1 also show **Settings…**, **Status…**, **Logs…** and **About…**. Older trays retain the module toggle; use `Settings.lnk` or `aw-watcher-teams.exe --settings` for settings. Keep the executable and its matching `.exe.actions.json` sidecar together on upgrades/rollback.

## Configuration and everyday use

The default config is `%LOCALAPPDATA%\activitywatch\aw-watcher-teams\config.json`. Named profiles and testing use separate roots; see [tray actions](docs/TRAY-ACTIONS.md). Restart the module through AW after saving settings.

`--testing` selects the isolated testing profile and defaults to port `5666`.
`--port <port>` accepts 1–65535 and overrides the server port for both the producer
and management actions. It takes precedence over `--server` and `AW_SERVER_URL`;
without either URL, it uses `127.0.0.1` and overrides the configured endpoint.
Port selection does not change the profile or imply testing mode. For example:

```powershell
.\aw-watcher-teams.exe --testing --port 7123 --init-config
```

| Setting | New configuration default |
| --- | --- |
| `server` | `http://127.0.0.1:5600/`; HTTP loopback only |
| `interval_seconds` | `5` (range 2–60) |
| `collector_timeout_seconds` | `8` (range 1–15) |
| `collect_meeting_title` | `false` |
| Payload mode | Strict: readable title only |

Use the settings dialog to choose strict/extended payloads, individual fields and caption permission. Mapping, separators, server and timing use JSON. Title capture is optional: it transiently reads a qualifying window's caption/root name, which can contain personal or customer context. Account context is off by default. See [title formatting](docs/TITLES.md) and [the field/privacy contract](docs/DATA-CONTRACT.md).

Events appear in `aw-watcher-teams_<hostname>` with type `call-evidence`. Continuous observations update one positive-duration interval; unknown/ambiguous observations and collection gaps end continuity. A worker restart starts a fresh detection interval. No Graph API, cloud delivery, message capture or audio/video recording is used. A local delivery checkpoint retains pending events and permitted metadata.

## Troubleshooting

- **No events:** check AW reachability, module enablement and supported call controls. Prejoin, unknown and ambiguous states deliberately produce no events.
- **Gaps during one call:** inspect privacy-safe logs for collector failure or unavailable controls. Do not fill gaps by assuming attendance.
- **No meeting title:** title capture defaults off; captions may remain generic even when enabled.
- **Status succeeds but nothing is captured:** server reachability does not prove producer health or delivery. Inspect recent bucket events separately.

## Local development

Requires Rust/MSVC on Windows. The build uses cached Cargo dependencies; fetch them with `cargo fetch --locked` when setting up a development environment.

```powershell
./scripts/build.ps1 -Test -Package
```

The Windows x64 package is `dist/aw-watcher-teams-1.0.0.zip`, with a `.sha256`
checksum sidecar. Tests use synthetic collection/protocol fixtures; opt-in
real-server and helper-lifecycle checks are documented in [validation](docs/VALIDATION.md).
They do not establish real-call or reboot compatibility.
[Tray protocol and rollback](docs/TRAY-ACTIONS.md) documents integration with AW.

## License

[MIT](LICENSE). Original implementation and provenance are described in
[the data contract](docs/DATA-CONTRACT.md). [Release notes](CHANGELOG.md) describe 1.0.0.
