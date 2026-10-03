//! Daemon/IPC integration: spawns the real binary against a temp instance and a fake Herdr socket.
//! Never touches a live Herdr session: every child runs with a cleared environment.
mod support;

use herdr_graph::config::{InstancePaths, socket_path};
use herdr_graph::daemon::client::Client;
use herdr_graph::daemon::lock;
use herdr_graph::ipc::IpcErrorCode;
use herdr_graph::store::init::init_instance;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant};
use support::isolated::TestRoot;

const BIN: &str = env!("CARGO_BIN_EXE_herdr-graph");

struct Fixture {
    /// Dropped first: shuts the daemon down and sweeps every process carrying its marker.
    root: TestRoot,
    instance: PathBuf,
    herdr_socket: PathBuf,
    _listener: UnixListener,
}

impl Fixture {
    fn new() -> Self {
        Self::with_instance_subdir(None)
    }

    fn with_instance_subdir(sub: Option<&str>) -> Self {
        let root = TestRoot::new();
        let instance = match sub {
            Some(s) => root.path().join(s).join("instance"),
            None => root.instance(),
        };
        init_instance(&instance).unwrap();
        let herdr_socket = root.herdr_socket();
        let listener = UnixListener::bind(&herdr_socket).unwrap();
        let l2 = listener.try_clone().unwrap();
        std::thread::spawn(move || for _conn in l2.incoming() {});
        Fixture {
            root,
            instance,
            herdr_socket,
            _listener: listener,
        }
    }

    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = self.root.command(BIN);
        c.args(args).env("HERDR_GRAPH_INSTANCE", &self.instance);
        c
    }

    fn run(&self, args: &[&str]) -> Output {
        self.cmd(args).output().unwrap()
    }

    fn sock(&self) -> PathBuf {
        socket_path(&self.instance)
    }

    fn lock_pid(&self) -> u32 {
        lock::read_info(&InstancePaths::new(&self.instance).lock)
            .expect("lock info")
            .pid
    }
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}
fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

#[test]
fn ensure_twice_yields_one_daemon() {
    let f = Fixture::new();
    let first = f.run(&["daemon", "--ensure"]);
    assert!(first.status.success(), "{}", stderr(&first));
    let pid = f.lock_pid();
    let second = f.run(&["daemon", "--ensure"]);
    assert!(second.status.success(), "{}", stderr(&second));
    assert_eq!(
        f.lock_pid(),
        pid,
        "second ensure must not replace the daemon"
    );
    assert!(
        stdout(&second).contains("already running"),
        "{}",
        stdout(&second)
    );
    assert!(stdout(&second).contains(&pid.to_string()));
    let ps = Command::new("ps")
        .args(["-o", "command=", "-p", &pid.to_string()])
        .output()
        .unwrap();
    assert!(
        stdout(&ps).contains("herdr-graph daemon"),
        "ps: {}",
        stdout(&ps)
    );
}

#[test]
fn cli_roundtrips_a_command() {
    let f = Fixture::new();
    assert!(f.run(&["daemon", "--ensure"]).status.success());
    let mut c = Client::connect(&f.sock(), Duration::from_secs(5)).unwrap();
    let hello = c.call("hello", serde_json::json!({})).unwrap();
    assert_eq!(hello["pid"].as_u64().unwrap() as u32, f.lock_pid());
    assert_eq!(
        hello["herdr_socket"].as_str().unwrap(),
        f.herdr_socket.to_str().unwrap()
    );
    let err = c.call("nope", serde_json::json!({})).unwrap_err();
    assert!(
        matches!(
            err,
            herdr_graph::daemon::client::ClientError::Remote {
                code: IpcErrorCode::UnknownCommand,
                ..
            }
        ),
        "{err:?}"
    );
    // The same connection keeps working after an error reply.
    assert!(c.call("hello", serde_json::json!({})).is_ok());
}

#[test]
fn status_reports_instance_and_pid() {
    let f = Fixture::new();
    assert!(f.run(&["daemon", "--ensure"]).status.success());
    let pid = f.lock_pid();
    let out = f.run(&["status"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains(f.instance.to_str().unwrap()), "{text}");
    assert!(text.contains(&format!("pid {pid}")), "{text}");
    assert!(text.contains("running"), "{text}");
}

#[test]
fn status_without_daemon_reads_directly() {
    let f = Fixture::new();
    let out = f.run(&["status"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("not running"), "{text}");
    assert!(text.contains(f.instance.to_str().unwrap()), "{text}");
    let head = git_head(&f.instance);
    assert!(
        text.contains(&format!("head: {head}")),
        "expected head {head} in {text}"
    );
    assert!(!f.sock().exists(), "status must not start a daemon");
}

fn git_head(root: &Path) -> String {
    let repo = git2::Repository::open(root).unwrap();
    repo.refname_to_id("refs/heads/main").unwrap().to_string()
}

#[test]
fn daemon_refuses_without_herdr_socket() {
    let f = Fixture::new();
    let out = f
        .cmd(&["daemon"])
        .env_remove("HERDR_SOCKET_PATH")
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("not inside Herdr"),
        "{}",
        stderr(&out)
    );
    let out = f
        .cmd(&["daemon", "--ensure"])
        .env_remove("HERDR_SOCKET_PATH")
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("not inside Herdr"),
        "{}",
        stderr(&out)
    );
    assert!(!f.sock().exists());
}

#[test]
fn daemon_refuses_unreachable_herdr_socket() {
    let f = Fixture::new();
    let dead = f.root.home().join("dead.sock");
    let out = f
        .cmd(&["daemon"])
        .env("HERDR_SOCKET_PATH", &dead)
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(stderr(&out).contains("not reachable"), "{}", stderr(&out));
}

#[test]
fn ensure_without_instance_exits_zero() {
    let f = Fixture::new();
    let out = f
        .cmd(&["daemon", "--ensure"])
        .env_remove("HERDR_GRAPH_INSTANCE")
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(!f.sock().exists());
    assert!(
        !InstancePaths::new(&f.instance).lock.exists(),
        "no daemon may have started"
    );
}

#[test]
fn long_socket_path_fallback_serves() {
    let f = Fixture::with_instance_subdir(Some(&format!("{}/{}", "d".repeat(40), "e".repeat(40))));
    let direct = f.instance.join(".graph-local/daemon.sock");
    assert!(
        direct.as_os_str().len() >= 100,
        "fixture path too short: {}",
        direct.as_os_str().len()
    );
    let out = f.run(&["daemon", "--ensure"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let sock = f.sock();
    assert!(
        sock.starts_with(format!("/private/tmp/herdr-graph-{}", unsafe {
            libc::getuid()
        })),
        "{sock:?}"
    );
    assert!(sock.exists());
    let mut c = Client::connect(&sock, Duration::from_secs(5)).unwrap();
    assert_eq!(
        c.call("hello", serde_json::json!({})).unwrap()["pid"]
            .as_u64()
            .unwrap() as u32,
        f.lock_pid()
    );
}

#[test]
fn read_command_without_instance_exits_2() {
    let f = Fixture::new();
    let out = f
        .cmd(&["status"])
        .env_remove("HERDR_GRAPH_INSTANCE")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("run herdr-graph init"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn doctor_reports_checks_and_fails_without_daemon() {
    let f = Fixture::new();
    let out = f.run(&["doctor"]);
    let text = stdout(&out);
    assert!(text.contains("[ok] instance repo"), "{text}");
    assert!(text.contains("[ok] herdr socket"), "{text}");
    assert!(text.contains("[FAIL] daemon:"), "{text}");
    // No herdr-threads state dir in the fixture HOME: a degraded install, reported as WARN, never FAIL.
    assert!(text.contains("[WARN] threads:"), "{text}");
    assert!(!text.contains("[FAIL] threads"), "{text}");
    assert_eq!(out.status.code(), Some(1));
    assert!(f.run(&["daemon", "--ensure"]).status.success());
    let out = f.run(&["doctor"]);
    assert!(out.status.success(), "{}", stdout(&out));
    let text = stdout(&out);
    assert!(text.contains("[ok] daemon:"), "{text}");
    assert!(text.contains("[WARN] threads:"), "{text}");
}

#[test]
fn shutdown_command_stops_daemon_and_removes_socket() {
    let f = Fixture::new();
    assert!(f.run(&["daemon", "--ensure"]).status.success());
    let sock = f.sock();
    let mut c = Client::connect(&sock, Duration::from_secs(5)).unwrap();
    c.call("shutdown", serde_json::json!({})).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while sock.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        !sock.exists(),
        "socket file must be removed on graceful shutdown"
    );
    // The daemon removes the socket before it drops its lock; wait for the lock so a fresh ensure does not
    // find a holder that is about to exit (flaky under a parallel run).
    let lock_path = InstancePaths::new(&f.instance).lock;
    let deadline = Instant::now() + Duration::from_secs(10);
    while lock::is_held(&lock_path) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    // A fresh ensure after shutdown starts a new daemon.
    let out = f.run(&["daemon", "--ensure"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(stdout(&out).contains("started"), "{}", stdout(&out));
}

#[cfg(feature = "test-support")]
#[test]
fn cli_command_during_slow_compose_succeeds_with_one_daemon() {
    let f = Fixture::new();
    let spawn_resume = |f: &Fixture| {
        let mut c = f.cmd(&["writer", "resume"]);
        c.env("HG_TEST_COMPOSE_DELAY_MS", "12000");
        std::thread::spawn(move || {
            let began = Instant::now();
            let out = c.output().unwrap();
            (out, began.elapsed())
        })
    };
    let a = spawn_resume(&f);
    std::thread::sleep(Duration::from_millis(500));
    let b = spawn_resume(&f);
    let (out_a, took_a) = a.join().unwrap();
    let (out_b, _) = b.join().unwrap();
    for out in [&out_a, &out_b] {
        assert!(
            out.status.success(),
            "stderr: {}\nstdout: {}",
            stderr(out),
            stdout(out)
        );
        assert!(
            stdout(out).contains("writer was not halted"),
            "{}",
            stdout(out)
        );
    }
    assert!(
        took_a >= Duration::from_secs(11),
        "the command must have waited for compose, took {took_a:?}"
    );
    let mut c = Client::connect(&f.sock(), Duration::from_secs(5)).unwrap();
    let hello = c.call("hello", serde_json::json!({})).unwrap();
    assert_eq!(
        hello["pid"].as_u64().unwrap() as u32,
        f.lock_pid(),
        "one daemon owns the instance"
    );
    let log = std::fs::read_to_string(InstancePaths::new(&f.instance).log).unwrap_or_default();
    assert!(
        !log.contains("already running"),
        "a second daemon was spawned:\n{log}"
    );
}

#[cfg(feature = "test-support")]
#[test]
fn session_start_hook_is_fast_without_daemon_and_spool_is_ingested() {
    use std::io::Write;
    use std::process::Stdio;
    let f = Fixture::new();
    let command = herdr_graph::bootstrap::setup_claude::hook_command(Path::new(env!(
        "CARGO_BIN_EXE_herdr-graph"
    )));
    let spool = herdr_graph::transcripts::capture::spool_dir(&f.instance);
    let spooled = || -> usize {
        std::fs::read_dir(&spool)
            .map(|rd| {
                rd.flatten()
                    .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
                    .count()
            })
            .unwrap_or(0)
    };
    let run_hook = || {
        let mut c = f.root.command("sh");
        c.args(["-c", &command])
            .env("HERDR_GRAPH_INSTANCE", &f.instance)
            .env("HERDR_GRAPH", "1")
            .env("HERDR_GRAPH_CLONE", "cl_01ARZ3NDEKTSV4RRFFQ69G5FAV")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let began = Instant::now();
        let mut child = c.spawn().unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(br#"{"session_id":"s-1","transcript_path":null,"source":"startup"}"#)
            .unwrap();
        let out = child.wait_with_output().unwrap();
        (out, began.elapsed())
    };

    // No daemon at all.
    let (out, took) = run_hook();
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(took < Duration::from_secs(2), "hook took {took:?}");
    assert!(stdout(&out).contains("run /seat"), "{}", stdout(&out));
    assert_eq!(
        spooled(),
        1,
        "the report was spooled; stderr: {}",
        stderr(&out)
    );

    // A daemon that is still composing.
    let log = std::fs::File::create(f.root.path().join("daemon.out")).unwrap();
    let mut daemon = f.root.spawn(
        f.cmd(&["daemon"])
            .env("HG_TEST_FAKE_SERVICES", "1")
            .env("HG_TEST_COMPOSE_DELAY_MS", "4000")
            .stdin(Stdio::null())
            .stdout(log.try_clone().unwrap())
            .stderr(log),
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    while herdr_graph::daemon::client::hello(&f.sock()).is_none() {
        assert!(Instant::now() < deadline, "daemon did not answer hello");
        std::thread::sleep(Duration::from_millis(50));
    }
    let (out, took) = run_hook();
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        took < Duration::from_secs(2),
        "hook took {took:?} while the daemon was starting"
    );
    assert!(stdout(&out).contains("run /seat"), "{}", stdout(&out));
    assert_eq!(spooled(), 2, "stderr: {}", stderr(&out));

    // Once the daemon is up it ingests the spool.
    let deadline = Instant::now() + Duration::from_secs(45);
    while spooled() > 0 {
        assert!(
            Instant::now() < deadline,
            "spool was never ingested: {} left",
            spooled()
        );
        std::thread::sleep(Duration::from_millis(200));
    }
    if let Ok(mut c) = Client::connect(&f.sock(), Duration::from_secs(5)) {
        let _ = c.call("shutdown", serde_json::json!({}));
    }
    assert!(
        daemon.wait_timeout(Duration::from_secs(10)).is_some(),
        "daemon did not exit after shutdown"
    );
}

#[cfg(feature = "test-support")]
#[test]
fn hello_reports_starting_during_compose() {
    let f = Fixture::new();
    let began = Instant::now();
    let out = f
        .cmd(&["daemon", "--ensure"])
        .env("HG_TEST_COMPOSE_DELAY_MS", "3000")
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        began.elapsed() < Duration::from_millis(2500),
        "ensure must return before compose ends"
    );
    assert!(stdout(&out).contains("daemon started"), "{}", stdout(&out));
    let mut c = Client::connect(&f.sock(), Duration::from_secs(5)).unwrap();
    let hello = c.call("hello", serde_json::json!({})).unwrap();
    assert_eq!(hello["state"], "starting", "{hello}");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let hello = c.call("hello", serde_json::json!({})).unwrap();
        if hello["state"] == "ready" {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "daemon never became ready: {hello}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn ensure_does_not_spawn_while_lock_held() {
    let f = Fixture::new();
    let paths = InstancePaths::new(&f.instance);
    std::fs::create_dir_all(&paths.local).unwrap();
    let info = lock::LockInfo {
        pid: 424242,
        herdr_socket: f.herdr_socket.clone(),
        socket: paths.socket.clone(),
        started_at: chrono::Utc::now(),
    };
    let _held = lock::DaemonLock::try_acquire(&paths.lock, &info)
        .unwrap()
        .expect("lock");
    let env = herdr_graph::config::Env {
        instance: Some(f.instance.to_string_lossy().into_owned()),
        herdr_socket: Some(f.herdr_socket.clone()),
        ..Default::default()
    };
    let err = herdr_graph::daemon::ensure::ensure(&env, Duration::from_secs(1)).unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("424242") && msg.contains("did not answer"),
        "{msg}"
    );
    assert!(
        !paths.log.exists(),
        "no daemon may have been spawned (it would have opened the log)"
    );
}
