# Changelog

## 1.0.0 — 2026-10-24

- Prepare the existing Teams call-evidence module as a versioned release.
- Add `--port <port>` for producer and management actions. Explicit port overrides
  the CLI/environment server URL's port; without a URL it selects 127.0.0.1.
  Testing/profile isolation remains unchanged and is never inferred from a port.
- Strengthen creation-only HTTP 304 and producer-restart tests with a bodyless
  response carrying Content-Length: 67 on a persistent-capable HTTP/1.1 fixture.
- Include the MIT license and release notes in cleanly staged ZIP packages, with
  SHA256 sidecars and locked local build/test commands.

Existing capture, privacy, interval/checkpoint recovery and tray-action behavior
is retained. Title capture remains off by default; call evidence includes background
and minimized windows and is not proof of attention or attendance. Real-call,
installed-host, UI and reboot acceptance remain separate from automated tests.
