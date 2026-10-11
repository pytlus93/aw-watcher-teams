# Teams component guidance

This directory is an independent Git checkout. Follow ../AGENTS.md; do not edit
shared root docs, other components, deployed AW binaries/settings or private history
as a side effect of component validation. Use repository-local Filip Vrba
<pytlus93@seznam.cz> identity and commit coherent source/tests/docs changes.

The custom call-evidence bucket has no official data property schema. Preserve
strict=workspace title only and individually selected extended properties. Update
docs/DATA-CONTRACT.md for any field/setting change. Disabled caption fields must be
absent from new payloads/exports and not leak through titles or queued context.
Never commit private fixtures; synthetic development state belongs under local/.

All Teams windows, including background/minimized windows, are intentionally
examined for call evidence. This user requirement is not foreground work-duration
tracking: label the evidence as inferred and keep attention/AFK separate. Do not
change it to foreground-only based on generic watcher guidance. Lock/secure desktop,
failed collection and large sampling gaps must break evidence intervals. Do not
claim actual attendance, provider IDs, or precise join/leave boundaries.

AW owns startup, recovery and explicit stop. The worker must not add its own
service/scheduled task/background supervisor. Bounded crash/failed-launch supervision
and persistent intentional-stop policy are host responsibilities. Component lock
harness results are not evidence that the installed tray implements those policies.
No production process termination/restart, install, reboot or interactive UI in a
validation run without explicit user authorization.

Use scripts/build.ps1 -Test -Package for offline tests, Clippy and packaging.
The opt-in real-server harness uses explicit AW_TEST_SERVER_EXE and
AW_TEST_SERVER_KIND=python|rust, only owned processes, a free port, memory storage
(Python) or an isolated local SQLite path/no import (Rust). It must never kill a
process by name or target production history. The opt-in lock probe is a development
example only; do not include it in installed packages. Settings/collector script
tests must use ValidateOnly/ValidateInterop to avoid UI and private collection.
Keep build, synthetic, isolated real-server, UI and installed-host evidence separate.
