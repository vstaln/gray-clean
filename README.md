# gray-clean

Sweep ephemeral session junk (spill/tmp) older than a max age at turn end. Port of hermes disk-cleanup.

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
