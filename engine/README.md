# Engine

The Engine is castLabs Electron (ADR 0006), run by the Shell as a child process.

- `electron.pin.json`: the pinned release tag and the sha256 of each platform's zip. Bump it deliberately and re-check the Apps.
- `fetch-electron.mjs`: downloads the pinned zip for this machine, checks the sha256 and unpacks it into `.m4tv-dev/electron/`. `just engine` runs it, and `just run` runs `just engine` first. The image build uses the same pin.
- `host/`: the script Electron runs. One hidden off-screen window per App, in its own storage partition; commands and live state travel over a Unix socket (a named pipe on Windows) as one JSON message per line, and each paint bitmap is copied into a file in `/dev/shm` (a temp directory on Windows). The protocol is described at the top of `host/main.js`; the Shell's side is `crates/shell/src/electron_engine.rs`.

Tests: `cargo test` covers the Shell's side with a stand-in host. The real Electron is checked by
`cargo test -p m4tv-shell --test electron_host -- --ignored` (the second test also needs the internet).
