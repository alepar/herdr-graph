//! Test isolation (hg-zmi.77): every spawning test runs under its own temp root, a child that resolves a path
//! outside it is refused, and nothing a test started outlives it, even when it panics.
//!
//! The static scan at the bottom fails on any raw process spawn or real-HOME read in test code.
mod support;

use herdr_graph::config::InstancePaths;
use herdr_graph::daemon::lock;
use herdr_graph::herdr::isolation::{VIOLATION_EXIT, VIOLATIONS_LOG};
use herdr_graph::store::init::init_instance;
use serde_json::json;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use support::isolated::{TestRoot, processes_under};

const BIN: &str = env!("CARGO_BIN_EXE_herdr-graph");

fn stdout(o: &std::process::Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &std::process::Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

/// `kill(pid, 0)` fails: the process is gone.
fn dead(pid: u32) -> bool {
    // SAFETY: signal 0 only probes existence.
    unsafe { libc::kill(pid as i32, 0) != 0 }
}

fn wait_until(what: &str, secs: u64, mut cond: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(secs);
    while !cond() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// A fake Herdr socket at the root's `HERDR_SOCKET_PATH`: accepts and drops every connection.
fn fake_herdr(root: &TestRoot) -> UnixListener {
    let listener = UnixListener::bind(root.herdr_socket()).unwrap();
    let l2 = listener.try_clone().unwrap();
    std::thread::spawn(move || for _conn in l2.incoming() {});
    listener
}

/// `daemon --ensure` against the root's instance; returns the daemon pid from its lock.
fn start_daemon(root: &TestRoot) -> u32 {
    init_instance(&root.instance()).unwrap();
    let out = root
        .command(BIN)
        .args(["daemon", "--ensure"])
        .output()
        .unwrap();
    assert!(out.status.success(), "daemon --ensure: {}", stderr(&out));
    lock::read_info(&InstancePaths::new(&root.instance()).lock)
        .expect("lock info")
        .pid
}

// ------------------------------------------------------------------------------------------ init

#[test]
fn init_prints_where_it_wrote_config() {
    let root = TestRoot::new();
    let inst = root.path().join("inst");
    let out = root.command(BIN).arg("init").arg(&inst).output().unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    let cfg = root.home().join(".config/herdr-graph/config.toml");
    assert!(
        stdout(&out).contains(&format!("config:   wrote {}", cfg.display())),
        "{}",
        stdout(&out)
    );
    assert!(
        cfg.is_file(),
        "the user config was written inside the test HOME"
    );
}

#[test]
fn init_no_user_config_writes_nothing_under_home() {
    let root = TestRoot::new();
    let inst = root.path().join("inst");
    let out = root
        .command(BIN)
        .arg("init")
        .arg(&inst)
        .arg("--no-user-config")
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("not written (--no-user-config)"), "{text}");
    assert!(text.contains("HERDR_GRAPH_INSTANCE="), "{text}");
    assert!(
        !root.home().join(".config/herdr-graph").exists(),
        "nothing may be created under HOME"
    );
    assert!(
        inst.join("graph.toml").is_file(),
        "the instance itself is still created"
    );
}

// ------------------------------------------------------------------------------------------ tripwire

#[test]
fn child_outside_test_root_is_refused() {
    let root = TestRoot::new();
    // A HOME outside the root, with a user config that locate_instance would read.
    let other = tempfile::tempdir().unwrap();
    let cfg_dir = other.path().join(".config/herdr-graph");
    std::fs::create_dir_all(&cfg_dir).unwrap();
    std::fs::write(
        cfg_dir.join("config.toml"),
        "instance = \"/nonexistent/instance\"\n",
    )
    .unwrap();
    let out = root
        .command(BIN)
        .arg("status")
        .env_remove("HERDR_GRAPH_INSTANCE")
        .env("HOME", other.path()) // isolation-ok: deliberately outside the root to prove the tripwire refuses it
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(VIOLATION_EXIT),
        "stdout: {} stderr: {}",
        stdout(&out),
        stderr(&out)
    );
    let log = root.violations();
    let real_cfg = other
        .path()
        .canonicalize()
        .unwrap()
        .join(".config/herdr-graph/config.toml");
    assert!(
        log.contains(real_cfg.to_str().unwrap()),
        "violation log names the path: {log}"
    );
    assert!(
        stderr(&out).contains("outside the test root"),
        "{}",
        stderr(&out)
    );
    // The violation was deliberate: clear it, or TestRoot::drop would fail this test.
    std::fs::write(root.path().join(VIOLATIONS_LOG), "").unwrap();
}

// ------------------------------------------------------------------------------------------ reaping

#[test]
fn test_root_drop_reaps_detached_daemon() {
    let root = TestRoot::new();
    let path = root.path().to_path_buf();
    let _herdr = fake_herdr(&root);
    let pid = start_daemon(&root);
    assert!(!dead(pid));
    assert!(
        processes_under(&path).contains(&pid),
        "the setsid daemon carries the root marker"
    );
    // A stopped daemon cannot answer `shutdown` and ignores SIGTERM until continued: only the SIGKILL
    // escalation of the sweep can remove it.
    // SAFETY: the pid was read from this root's daemon lock.
    unsafe { libc::kill(pid as i32, libc::SIGSTOP) };
    drop(root);
    wait_until("the daemon to be gone", 10, || {
        dead(pid) && processes_under(&path).is_empty()
    });
    assert!(!path.exists(), "the root directory is removed");
}

/// Re-executed by `panicking_test_leaves_no_process_behind`: starts a detached daemon and a process-group child,
/// reports their pids, then panics. Returns at once in a normal run.
#[test]
fn helper_panics_with_live_children() {
    let Some(report) = std::env::var_os("HG_ISOLATION_PANIC_CHILD") else {
        return;
    };
    let root = TestRoot::new();
    let _herdr = fake_herdr(&root);
    let daemon_pid = start_daemon(&root);
    let child = root.spawn(
        root.command("sh")
            .args(["-c", "sleep 600 & exec sleep 600"]),
    );
    let info = json!({ "root": root.path(), "daemon_pid": daemon_pid, "child_pid": child.id() });
    std::fs::write(report, info.to_string()).unwrap();
    panic!("intentional panic with live children");
}

/// Everything a re-executed helper reported, and what must be gone once it panicked.
fn assert_helper_left_nothing(out: &std::process::Output, report: &Path, pid_keys: &[&str]) {
    assert!(
        !out.status.success(),
        "the helper must fail: {}",
        stdout(out)
    );
    let all = format!("{}{}", stdout(out), stderr(out));
    assert!(
        all.contains("intentional panic"),
        "the helper did not reach its panic:\n{all}"
    );
    let info: serde_json::Value =
        serde_json::from_slice(&std::fs::read(report).expect("helper wrote its report")).unwrap();
    let child_root = PathBuf::from(info["root"].as_str().unwrap());
    let pids: Vec<u32> = pid_keys
        .iter()
        .map(|k| info[*k].as_u64().unwrap() as u32)
        .collect();
    wait_until("every recorded process to be gone", 10, || {
        pids.iter().all(|p| dead(*p)) && processes_under(&child_root).is_empty()
    });
    assert!(!child_root.exists(), "the helper's root was removed");
}

#[test]
fn panicking_test_leaves_no_process_behind() {
    let root = TestRoot::new();
    let report = root.path().join("report.json");
    let out = root
        .command(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "helper_panics_with_live_children",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("HG_ISOLATION_PANIC_CHILD", &report)
        .output()
        .unwrap();
    assert_helper_left_nothing(&out, &report, &["daemon_pid", "child_pid"]);
}

/// Re-executed by `panicking_test_leaves_no_private_herdr_behind`.
#[cfg(feature = "private-herdr")]
#[test]
fn helper_panics_with_private_herdr() {
    let Some(report) = std::env::var_os("HG_ISOLATION_PANIC_CHILD") else {
        return;
    };
    let herdr = support::private_herdr::PrivateHerdr::start().expect("private herdr starts");
    let info = json!({ "root": herdr.root, "herdr_pid": herdr.pid() });
    std::fs::write(report, info.to_string()).unwrap();
    panic!("intentional panic with a private herdr");
}

#[cfg(feature = "private-herdr")]
#[test]
fn panicking_test_leaves_no_private_herdr_behind() {
    if support::herdr_binary().is_none() {
        support::skip("herdr is not installed");
        return;
    }
    let root = TestRoot::new();
    let report = root.path().join("report.json");
    let out = root
        .command(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "helper_panics_with_private_herdr",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("HG_ISOLATION_PANIC_CHILD", &report)
        .output()
        .unwrap();
    assert_helper_left_nothing(&out, &report, &["herdr_pid"]);
}

// ------------------------------------------------------------------------------------------ static scan

/// `(1-based line, rule)` for every line of test code that spawns outside the isolated helpers or reads the
/// real HOME. `// isolation-ok: <why>` on the line waives a hit; comment-only lines are skipped.
///
/// * R1: `Command::new(` other than `ps`, `kill` and `git`.
/// * R2: a read of the `HOME` variable from the process environment.
/// * R3: an explicit `.env("HOME"` / `.env("XDG_*"` / `.env("CLAUDE_CONFIG_DIR"` / `.env("HERDR_PLUGIN_*"`: the
///   environment must come from `scrubbed_env`.
fn scan(text: &str) -> Vec<(usize, &'static str)> {
    const R1_ALLOWED: &[&str] = &[
        "\"/bin/ps\"",
        "\"ps\"",
        "\"/bin/kill\"",
        "\"kill\"",
        "\"git\"",
    ];
    const R2: &[&str] = &["env::var(\"HOME\")", "var_os(\"HOME\")"];
    const R3: &[&str] = &[
        ".env(\"HOME\"",
        ".env(\"XDG_",
        ".env(\"CLAUDE_CONFIG_DIR\"",
        ".env(\"HERDR_PLUGIN_",
    ];
    let mut hits = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.trim_start().starts_with("//") || line.contains("isolation-ok:") {
            continue;
        }
        if line.contains("Command::new(") && !R1_ALLOWED.iter().any(|a| line.contains(a)) {
            hits.push((i + 1, "R1"));
        }
        if R2.iter().any(|p| line.contains(p)) {
            hits.push((i + 1, "R2"));
        }
        if R3.iter().any(|p| line.contains(p)) {
            hits.push((i + 1, "R3"));
        }
    }
    hits
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

#[test]
fn scanner_flags_raw_cli_spawn_and_real_home_read() {
    // Built at runtime so this file never contains a literal match itself.
    let spawn = concat!("let c = std::process::Command::", "new(\"herdr-graph\");");
    let home = concat!("let h = std::env::var_os(\"HO", "ME\");");
    let env = concat!("c.env(\"XDG_", "CONFIG_HOME\", p);");
    assert_eq!(scan(spawn), vec![(1, "R1")]);
    assert_eq!(scan(home), vec![(1, "R2")]);
    assert_eq!(scan(env), vec![(1, "R3")]);
    // Line numbers and several hits in one text.
    assert_eq!(
        scan(&format!("fn a() {{}}\n{spawn}\n{home}\n")),
        vec![(2, "R1"), (3, "R2")]
    );
    // A waiver, a comment, and the allowed helpers are not flagged.
    assert!(scan(&format!("{spawn} // isolation-ok: cargo build")).is_empty());
    assert!(scan(&format!("// {spawn}")).is_empty());
    for prog in ["/bin/ps", "ps", "/bin/kill", "kill", "git"] {
        assert!(
            scan(&format!("{}{prog}\");", concat!("Command::", "new(\""))).is_empty(),
            "{prog}"
        );
    }
    assert!(
        scan(&format!(
            "{}{}",
            concat!("let x = Command::", "new("),
            "bin);"
        ))
        .len()
            == 1
    );
}

#[test]
fn test_sources_spawn_only_through_isolated_helpers() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rust_files(&root.join("tests"), &mut files);
    rust_files(&root.join("src"), &mut files);
    files.sort();
    let mut violations = Vec::new();
    let mut scanned = 0;
    for f in files {
        let rel = f.strip_prefix(root).unwrap().to_string_lossy().into_owned();
        if [
            "tests/support/isolated.rs",
            "tests/support/private_herdr.rs",
            "tests/test_isolation.rs",
        ]
        .contains(&rel.as_str())
        {
            continue;
        }
        let text = std::fs::read_to_string(&f).unwrap();
        let (code, first_line) = if rel.starts_with("tests/")
            || rel.ends_with("/tests.rs")
            || rel.ends_with("_tests.rs")
        {
            (text.as_str(), 1)
        } else {
            // Production file: only what follows its first `#[cfg(test)]` is test code.
            let Some(at) = text.find("#[cfg(test)]") else {
                continue;
            };
            (&text[at..], text[..at].lines().count() + 1)
        };
        scanned += 1;
        let lines: Vec<&str> = code.lines().collect();
        for (n, rule) in scan(code) {
            violations.push(format!(
                "{rel}:{}: {rule}: {}",
                first_line + n - 1,
                lines[n - 1].trim()
            ));
        }
    }
    assert!(scanned > 20, "the scan looked at only {scanned} files");
    assert!(
        violations.is_empty(),
        "raw spawns / real-HOME reads in test code:\n{}",
        violations.join("\n")
    );
}
