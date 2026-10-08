//! gray-clean — sweep ephemeral session junk at turn end.
//!
//! Port of hermes' `disk-cleanup` plugin, narrowed to gray's layout: the
//! only clearly-ephemeral roots are `~/.gray/spill/` and `~/.gray/tmp/`.
//! On `turn_end` (and on `/clean now`) every file under those roots older
//! than `GRAY_CLEAN_MAX_AGE_HOURS` (default 24) is deleted, then empty
//! directories left behind are removed — deepest first, roots kept.
//! Symlinks are never followed or deleted, so nothing outside the roots
//! can be touched. `sessions/`, `memory/`, `plugins/` and everything else
//! under ~/.gray is out of scope by construction.
//!
//! Every deletion and a per-sweep summary are appended to
//! ~/.gray/clean/log.jsonl (capped at ~1000 lines). `/clean now` sweeps on
//! demand, `/clean status` reports state, `/clean on|off` toggles
//! (persisted at ~/.gray/clean/disabled; default ON but conservative).

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

const LOG_CAP: usize = 1000;
const ROOTS: [&str; 2] = ["spill", "tmp"];

fn manifest() -> Value {
    json!({
        "name": "clean",
        "version": env!("CARGO_PKG_VERSION"),
        "protocol": "1.1",
        "tools": [],
        "commands": ["/clean"],
    })
}

fn gray_home() -> PathBuf {
    std::env::var_os("GRAY_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".gray")))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Everything the plugin needs to know about the outside world, gathered
/// once — parameterised so tests never touch the real ~/.gray.
struct Config {
    roots: Vec<PathBuf>,
    state_dir: PathBuf,
    log: PathBuf,
    max_age: Duration,
}

impl Config {
    fn from_env() -> Self {
        let home = gray_home();
        let hours = std::env::var("GRAY_CLEAN_MAX_AGE_HOURS")
            .ok()
            .and_then(|v| v.trim().parse::<f64>().ok())
            .filter(|h| *h >= 0.0 && h.is_finite())
            .unwrap_or(24.0);
        let state_dir = home.join("clean");
        Self {
            roots: ROOTS.iter().map(|r| home.join(r)).collect(),
            log: state_dir.join("log.jsonl"),
            state_dir,
            max_age: Duration::from_secs_f64(hours * 3600.0),
        }
    }

    fn enabled(&self) -> bool {
        !self.state_dir.join("disabled").exists()
    }
}

#[derive(Default)]
struct Sweep {
    files: u64,
    dirs: u64,
    freed: u64,
    errors: u64,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn log_line(log: &Path, line: &Value) {
    if let Some(parent) = log.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(log) {
        let _ = writeln!(f, "{line}");
    }
}

/// Keep the log at ~`cap` lines: rewrite only when it has grown past it.
fn cap_log(log: &Path, cap: usize) {
    let Ok(content) = std::fs::read_to_string(log) else {
        return;
    };
    let lines: Vec<&str> = content.lines().collect();
    if lines.len() > cap {
        let keep = &lines[lines.len() - cap..];
        let _ = std::fs::write(log, keep.join("\n") + "\n");
    }
}

/// Delete files under `roots` older than `max_age`, then empty dirs left
/// behind. Never follows symlinks, never removes the roots themselves.
fn sweep(roots: &[PathBuf], max_age: Duration, log: &Path, trigger: &str) -> Sweep {
    let cutoff = SystemTime::now()
        .checked_sub(max_age)
        .unwrap_or(UNIX_EPOCH);
    let mut s = Sweep::default();
    let mut dirs: Vec<PathBuf> = Vec::new();

    for root in roots {
        let Ok(root) = root.canonicalize() else { continue };
        if !root.is_dir() {
            continue;
        }
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            for entry in rd.flatten() {
                let path = entry.path();
                let Ok(ft) = entry.file_type() else { continue };
                if ft.is_symlink() {
                    continue;
                }
                if ft.is_dir() {
                    dirs.push(path.clone());
                    stack.push(path);
                    continue;
                }
                if !ft.is_file() {
                    continue;
                }
                let Ok(md) = entry.metadata() else { continue };
                let old = md.modified().map(|m| m < cutoff).unwrap_or(false);
                if !old {
                    continue;
                }
                match std::fs::remove_file(&path) {
                    Ok(()) => {
                        s.files += 1;
                        s.freed += md.len();
                        log_line(
                            log,
                            &json!({"ts": now_secs(), "event": "delete", "trigger": trigger,
                                    "kind": "file", "path": path, "bytes": md.len()}),
                        );
                    }
                    Err(_) => s.errors += 1,
                }
            }
        }
    }

    // Deepest first so parents go empty after their children are removed.
    dirs.sort_by_key(|d| std::cmp::Reverse(d.components().count()));
    for d in dirs {
        if std::fs::remove_dir(&d).is_ok() {
            s.dirs += 1;
            log_line(
                log,
                &json!({"ts": now_secs(), "event": "delete", "trigger": trigger,
                        "kind": "dir", "path": d, "bytes": 0}),
            );
        }
    }

    log_line(
        log,
        &json!({"ts": now_secs(), "event": "sweep", "trigger": trigger,
                "files": s.files, "dirs": s.dirs, "freed": s.freed, "errors": s.errors}),
    );
    cap_log(log, LOG_CAP);
    s
}

/// Live counts for `root`: (files, bytes, files older than cutoff).
fn root_stats(root: &Path, cutoff: SystemTime) -> (u64, u64, u64) {
    let mut out = (0u64, 0u64, 0u64);
    let Ok(root) = root.canonicalize() else { return out };
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for entry in rd.flatten() {
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                stack.push(entry.path());
            } else if ft.is_file() {
                if let Ok(md) = entry.metadata() {
                    out.0 += 1;
                    out.1 += md.len();
                    if md.modified().map(|m| m < cutoff).unwrap_or(false) {
                        out.2 += 1;
                    }
                }
            }
        }
    }
    out
}

fn fmt_size(n: u64) -> String {
    let mut v = n as f64;
    for unit in ["B", "KB", "MB", "GB", "TB"] {
        if v < 1024.0 {
            return format!("{v:.1} {unit}");
        }
        v /= 1024.0;
    }
    format!("{v:.1} PB")
}

/// `/clean …` — `argv` excludes the command name.
fn run_command(argv: &[&str], cfg: &Config) -> String {
    match argv.first().copied() {
        Some("now") => {
            let s = sweep(&cfg.roots, cfg.max_age, &cfg.log, "command");
            format!(
                "swept: {} file(s) + {} empty dir(s), freed {}{}",
                s.files,
                s.dirs,
                fmt_size(s.freed),
                if s.errors > 0 {
                    format!(" — {} error(s), see {}", s.errors, cfg.log.display())
                } else {
                    String::new()
                }
            )
        }
        Some("off") => match std::fs::create_dir_all(&cfg.state_dir)
            .and_then(|_| std::fs::write(cfg.state_dir.join("disabled"), b""))
        {
            Ok(()) => "clean off — turn-end sweeps disabled (/clean now still works)".into(),
            Err(e) => format!("couldn't disable: {e}"),
        },
        Some("on") => match std::fs::remove_file(cfg.state_dir.join("disabled")).or_else(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Ok(())
            } else {
                Err(e)
            }
        }) {
            Ok(()) => "clean on".into(),
            Err(e) => format!("couldn't enable: {e}"),
        },
        _ => {
            let cutoff = SystemTime::now().checked_sub(cfg.max_age).unwrap_or(UNIX_EPOCH);
            let mut lines = vec![format!(
                "gray-clean {} — {} — max age {}h — sweeping: {}",
                env!("CARGO_PKG_VERSION"),
                if cfg.enabled() { "on" } else { "off" },
                cfg.max_age.as_secs_f64() / 3600.0,
                cfg.roots
                    .iter()
                    .map(|r| r.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
            )];
            for root in &cfg.roots {
                let (files, bytes, old) = root_stats(root, cutoff);
                lines.push(format!(
                    "  {}: {files} file(s), {}, {old} past max age",
                    root.display(),
                    fmt_size(bytes),
                ));
            }
            if let Ok(content) = std::fs::read_to_string(&cfg.log) {
                if let Some(last) = content
                    .lines()
                    .rev()
                    .find(|l| l.contains("\"sweep\""))
                {
                    lines.push(format!("last sweep: {last}"));
                }
            }
            lines.join("\n")
        }
    }
}

/// One request → `Some(reply)`, or `None` for notifications. The bool asks
/// the loop to exit after writing the reply.
fn handle(req: &Value, cfg: &Config) -> (Option<Value>, bool) {
    let id = req.get("id").cloned();
    let method = req.get("method").and_then(Value::as_str).unwrap_or("");
    let params = req.get("params").cloned().unwrap_or(Value::Null);
    let Some(id) = id else {
        if method == "event/notify"
            && cfg.enabled()
            && params.get("type").and_then(Value::as_str) == Some("turn_end")
        {
            let _ = sweep(&cfg.roots, cfg.max_age, &cfg.log, "turn_end");
        }
        return (None, method == "plugin/shutdown");
    };
    let result = match method {
        "plugin/manifest" => manifest(),
        "command/run" => {
            let argv: Vec<&str> = params
                .get("argv")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            json!({ "text": run_command(&argv, cfg) })
        }
        "plugin/shutdown" => return (Some(json!({ "id": id, "result": {} })), true),
        _ => {
            let error = json!({ "code": -32601, "message": "method not found" });
            return (Some(json!({ "id": id, "error": error })), false);
        }
    };
    (Some(json!({ "id": id, "result": result })), false)
}

fn main() -> std::io::Result<()> {
    if std::env::args().nth(1).as_deref() == Some("manifest") {
        println!("{}", manifest());
        return Ok(());
    }
    let cfg = Config::from_env();
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        let Ok(req) = serde_json::from_str::<Value>(&line) else { continue };
        let (reply, exit) = handle(&req, &cfg);
        if let Some(reply) = reply {
            writeln!(stdout, "{reply}")?;
            stdout.flush()?;
        }
        if exit {
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(cfg: &Config, method: &str, params: Value) -> Value {
        handle(&json!({ "id": 1, "method": method, "params": params }), cfg)
            .0
            .unwrap()
    }

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "gray-clean-test-{}-{}",
            std::process::id(),
            tag
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A Config whose roots/state all live inside one temp dir.
    fn tmpcfg(tag: &str) -> (Config, PathBuf) {
        let home = tmpdir(tag);
        let cfg = Config {
            roots: vec![home.join("spill"), home.join("tmp")],
            state_dir: home.join("clean"),
            log: home.join("clean").join("log.jsonl"),
            max_age: Duration::ZERO, // everything is "old"
        };
        (cfg, home)
    }

    #[test]
    fn manifest_has_no_tools_and_one_command() {
        let (cfg, home) = tmpcfg("manifest");
        let m = call(&cfg, "plugin/manifest", Value::Null)["result"].clone();
        assert_eq!(m["name"], "clean");
        assert_eq!(m["protocol"], "1.1");
        assert_eq!(m["commands"], json!(["/clean"]));
        assert_eq!(m["tools"], json!([]));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn sweep_deletes_old_files_and_empty_dirs() {
        let (cfg, home) = tmpcfg("sweep");
        std::fs::create_dir_all(home.join("spill/nested/deep")).unwrap();
        std::fs::create_dir_all(home.join("tmp/nested")).unwrap();
        std::fs::write(home.join("spill/a.txt"), "junk").unwrap();
        std::fs::write(home.join("tmp/nested/b.txt"), "junk").unwrap();
        let s = sweep(&cfg.roots, cfg.max_age, &cfg.log, "test");
        assert_eq!(s.files, 2);
        assert!(!home.join("spill/a.txt").exists());
        // nested/ held b.txt → deleted; deep/ + nested/ now empty → removed.
        assert!(s.dirs >= 3);
        assert!(!home.join("tmp/nested").exists());
        // Roots themselves survive.
        assert!(home.join("spill").exists() && home.join("tmp").exists());
        // Log has a summary line.
        let log = std::fs::read_to_string(&cfg.log).unwrap();
        assert!(log.lines().any(|l| l.contains("\"sweep\"")));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn sweep_keeps_fresh_files_and_nonempty_dirs() {
        let (cfg, home) = tmpcfg("fresh");
        std::fs::create_dir_all(home.join("tmp/keep")).unwrap();
        std::fs::write(home.join("tmp/keep/f.txt"), "new").unwrap();
        // A century of max age → nothing is "old".
        let s = sweep(&cfg.roots, Duration::from_secs(3600 * 24 * 365 * 100), &cfg.log, "test");
        assert_eq!(s.files, 0);
        assert!(home.join("tmp/keep/f.txt").exists());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[cfg(unix)]
    #[test]
    fn sweep_never_touches_symlinks() {
        let (cfg, home) = tmpcfg("symlink");
        let secret = tmpdir("secret");
        std::fs::write(secret.join("keep.txt"), "precious").unwrap();
        std::fs::create_dir_all(home.join("spill")).unwrap();
        std::os::unix::fs::symlink(&secret, home.join("spill/link")).unwrap();
        std::os::unix::fs::symlink(secret.join("keep.txt"), home.join("spill/flink")).unwrap();
        let s = sweep(&cfg.roots, cfg.max_age, &cfg.log, "test");
        assert_eq!(s.files, 0);
        assert!(home.join("spill/link").exists());
        assert!(secret.join("keep.txt").exists());
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&secret);
    }

    #[test]
    fn turn_end_sweeps_only_when_enabled() {
        let (cfg, home) = tmpcfg("end");
        std::fs::create_dir_all(home.join("spill")).unwrap();
        std::fs::write(home.join("spill/x.txt"), "old").unwrap();
        // Disabled → turn_end is a no-op.
        std::fs::create_dir_all(&cfg.state_dir).unwrap();
        std::fs::write(cfg.state_dir.join("disabled"), b"").unwrap();
        let (reply, exit) = handle(
            &json!({"method":"event/notify","params":{"type":"turn_end","session":{}}}),
            &cfg,
        );
        assert!(reply.is_none() && !exit);
        assert!(home.join("spill/x.txt").exists());
        // Enabled → sweep happens.
        std::fs::remove_file(cfg.state_dir.join("disabled")).unwrap();
        handle(
            &json!({"method":"event/notify","params":{"type":"turn_end","session":{}}}),
            &cfg,
        );
        assert!(!home.join("spill/x.txt").exists());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn clean_now_sweeps_and_reports() {
        let (cfg, home) = tmpcfg("now");
        std::fs::create_dir_all(home.join("tmp")).unwrap();
        std::fs::write(home.join("tmp/y.txt"), "old").unwrap();
        let r = call(&cfg, "command/run", json!({"name":"/clean","argv":["now"]}));
        assert!(r["result"]["text"].as_str().unwrap().contains("1 file(s)"));
        assert!(!home.join("tmp/y.txt").exists());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn clean_status_reports_roots() {
        let (cfg, home) = tmpcfg("status");
        std::fs::create_dir_all(home.join("spill")).unwrap();
        let r = call(&cfg, "command/run", json!({"name":"/clean","argv":["status"]}));
        let text = r["result"]["text"].as_str().unwrap();
        assert!(text.contains("gray-clean") && text.contains("spill") && text.contains("tmp"));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn log_is_capped() {
        let (cfg, home) = tmpcfg("cap");
        let big: String = (0..1050).map(|i| format!("{{\"n\":{i}}}\n")).collect();
        std::fs::create_dir_all(cfg.log.parent().unwrap()).unwrap();
        std::fs::write(&cfg.log, &big).unwrap();
        cap_log(&cfg.log, LOG_CAP);
        let lines = std::fs::read_to_string(&cfg.log).unwrap().lines().count();
        assert_eq!(lines, LOG_CAP);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn shutdown_replies_then_exits() {
        let (cfg, home) = tmpcfg("shutdown");
        let (reply, exit) = handle(&json!({ "id": 2, "method": "plugin/shutdown" }), &cfg);
        assert!(reply.is_some() && exit);
        let _ = std::fs::remove_dir_all(&home);
    }
}
