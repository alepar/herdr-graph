//! Best-effort Herdr server incarnation probe (spec §4.3.1).
use crate::model::Incarnation;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const SUBPROCESS_TIMEOUT: Duration = Duration::from_secs(2);

/// Run `program args` with a deadline; stdout on success exit, `None` on any failure.
fn run(program: &str, args: &[&str]) -> Option<String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stdout.read_to_string(&mut s);
        s
    });
    let deadline = Instant::now() + SUBPROCESS_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    let out = reader.join().ok()?;
    // lsof exits 1 with empty output when nothing matches; treat any exit with output as usable.
    (status.success() || !out.is_empty()).then_some(out)
}

/// Server pid = the pid from `lsof -t <socket>` whose `ps -o command=` contains "herdr" and "server"
/// (excluding our own pid); start time = `ps -o lstart= -p <pid>` trimmed. Each subprocess has a 2 s
/// timeout; any failure leaves the field `None`.
pub fn probe(socket: &Path, generation: u64) -> Incarnation {
    let mut inc = Incarnation {
        generation,
        server_pid: None,
        server_started: None,
    };
    let Some(sock) = socket.to_str() else {
        return inc;
    };
    let Some(pids) = run("/usr/sbin/lsof", &["-t", sock]) else {
        return inc;
    };
    let me = std::process::id();
    for pid in pids
        .lines()
        .filter_map(|l| l.trim().parse::<u32>().ok())
        .filter(|p| *p != me)
    {
        let Some(cmd) = run("/bin/ps", &["-o", "command=", "-p", &pid.to_string()]) else {
            continue;
        };
        if cmd.contains("herdr") && cmd.contains("server") {
            inc.server_pid = Some(pid);
            inc.server_started = run("/bin/ps", &["-o", "lstart=", "-p", &pid.to_string()])
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty());
            break;
        }
    }
    inc
}
