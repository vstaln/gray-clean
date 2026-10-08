<p align="center">
  <img src="assets/gray-logo.svg" alt="gray" width="96">
</p>
<h1 align="center">gray-clean</h1>
<p align="center">Sweep ephemeral session files (spill/tmp) at turn end.</p>
<p align="center">
  <a href="https://github.com/vstaln/gray-clean/blob/main/LICENSE"><img alt="MIT License" src="https://img.shields.io/badge/license-MIT-blue.svg"></a>
  <img alt="gray plugin" src="https://img.shields.io/badge/gray-plugin-7aa2f7.svg">
  <img alt="rust" src="https://img.shields.io/badge/built%20with-rust-orange.svg">
</p>

Sweep ephemeral session junk (spill/tmp) older than a max age at turn end.

On `turn_end`, deletes files under `~/.gray/spill/` and `~/.gray/tmp/`
older than `GRAY_CLEAN_MAX_AGE_HOURS` (default 24), then removes empty
directories left behind (deepest first, roots kept). Symlinks are never
followed or deleted; `sessions/`, `memory/`, `plugins/` and everything
else under `~/.gray` is out of scope by construction. Deletions are
appended to `~/.gray/clean/log.jsonl` (~1000-line cap). Default ON but
conservative.

A sidecar plugin for [gray](https://github.com/vstaln/gray), scaffolded by
[gray-account](https://github.com/vstaln/gray-account).

## Wire methods used

- `plugin/manifest`, `plugin/shutdown`
- `event/notify` — `turn_end` triggers the sweep
- `command/run` — `/clean now` (sweep on demand), `/clean status`,
  `/clean on|off` (persisted at `~/.gray/clean/disabled`)

No capabilities required.

## Install

```sh
gray plugin install clean
```

## Develop

```sh
cargo test
gray account check      # entry point + manifest handshake
gray account publish    # check → build → release → publish to the gray registry
```

Bump `version` in `Cargo.toml` before each `publish`; the registry refuses to
republish a version.

---
Part of the [gray](https://github.com/vstaln/gray) plugin ecosystem —
the open-source AI agent harness. <https://gray.alignment.id>
