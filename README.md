<div align="center">
  <img alt="gray-clean" src="assets/icon.svg" width="120" height="120" />
  <h1>gray-clean</h1>
  <p><strong>Sweep ephemeral session junk older than a max age at turn end.</strong></p>
  <p>
    <a href="https://gray.alignment.id">Website</a> ·
    <a href="https://gray.alignment.id/plugins/gray-clean">Store</a> ·
    <a href="https://github.com/vstaln/gray-clean">Source</a> ·
    <a href="https://github.com/vstaln/gray">gray</a>
  </p>
  <p>
    <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-1c1c20?style=flat-square&labelColor=0a0a0b" /></a>
    <a href="https://www.rust-lang.org"><img alt="Built with Rust" src="https://img.shields.io/badge/built%20with-rust-1c1c20?style=flat-square&labelColor=0a0a0b&logo=rust&logoColor=d4a373" /></a>
    <a href="https://gray.alignment.id/plugins/gray-clean"><img alt="gray plugin" src="https://img.shields.io/badge/gray-plugin-1c1c20?style=flat-square&labelColor=0a0a0b&color=7aa2f7" /></a>
  </p>
</div>

<br/>

```bash
gray plugin install gray-clean
```

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

## Tags

`gray` `plugin` `clean` `rust`

---
Part of the [gray](https://github.com/vstaln/gray) plugin ecosystem —
the open-source AI agent harness. <https://gray.alignment.id>
