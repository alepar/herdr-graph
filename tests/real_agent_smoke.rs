//! Opt-in tier-4 smoke test with REAL Claude agents in a private Herdr server (spec §11). Runs only with
//! `HG_REAL_AGENTS=1`, `claude` on PATH and credentials (explicit `ANTHROPIC_API_KEY`, or a keychain login that
//! `claude auth status` confirms); otherwise it prints `REAL-AGENT: SKIPPED (<reason>)` and passes.
//!
//!   HG_REAL_AGENTS=1 cargo test --features private-herdr --test real_agent_smoke -- --nocapture
//!
//! Flow: init --with-examples -> setup claude (private CLAUDE_CONFIG_DIR) -> daemon -> apply project-team and
//! system-summarizer -> foreman (claude) is prompted `/seat` by the SessionStart hook and resolves -> the foreman
//! session ends -> an rq_ exists for its transcript -> the summarizer runs `request ack` (recorded separately from
//! completion) and `request complete` with coverage. The user's live Herdr, `~/.claude`, threads state and memory
//! observer are never touched: every child gets only the scrubbed private env (see `herdr::isolation`).
//! Delivery uses the Notify fallback; an isolated real threads daemon (HG_REAL_THREADS) is not started here.
#![cfg(feature = "private-herdr")]

mod support;

use herdr_graph::ports::herdr::{HerdrApi, KeyInput, PaneInfo};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use support::private_herdr::PrivateHerdr;

const BIN: &str = env!("CARGO_BIN_EXE_herdr-graph");

fn skipped(reason: &str) {
    println!("REAL-AGENT: SKIPPED ({reason})");
    support::skip(reason);
}

fn executable_on_path(name: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|d| d.join(name))
        .find(|p| p.metadata().is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0))
}

/// Everything the test runs against.
struct Rig {
    herdr: PrivateHerdr,
    instance: PathBuf,
}

impl Rig {
    /// `herdr-graph <args>` with exactly the private env (HERDR_GRAPH_INSTANCE = the instance) plus `extra`.
    fn cmd(&self, args: &[&str], extra: &[(&str, &str)]) -> Command {
        let mut c = self.herdr.command(Path::new(BIN));
        c.args(args).env("HERDR_GRAPH_INSTANCE", &self.instance);
        for (k, v) in extra {
            c.env(k, v);
        }
        c.stdin(Stdio::null());
        c
    }

    /// Run and return stdout; panics with both streams when the command fails.
    fn run(&self, args: &[&str], extra: &[(&str, &str)]) -> String {
        let out = self.cmd(args, extra).output().expect("spawn herdr-graph");
        assert!(
            out.status.success(),
            "herdr-graph {args:?} failed ({}):\nstdout: {}\nstderr: {}",
            out.status,
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// `plan --json <words>` then `apply <pl> --confirm <hash> --confirmed-by user-relay`.
    fn plan_apply(&self, words: &[&str]) {
        let mut args = vec!["plan", "--json"];
        args.extend_from_slice(words);
        let plan: Value = serde_json::from_str(&self.run(&args, &[])).unwrap_or_else(|e| panic!("plan {words:?}: {e}"));
        let (id, hash) = (plan["plan_id"].as_str().expect("plan_id"), plan["hash"].as_str().expect("hash"));
        let out = self.run(&["apply", id, "--confirm", hash, "--confirmed-by", "user-relay"], &[]);
        assert!(out.contains("committed"), "apply {words:?} did not commit: {out}");
    }

    fn requests(&self, flag: &str) -> Vec<String> {
        self.run(&["request", "list", flag], &[]).lines().map(str::to_owned).collect()
    }
}

fn wait_until<T>(what: &str, limit: Duration, mut f: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + limit;
    loop {
        if let Some(v) = f() {
            return v;
        }
        assert!(Instant::now() < deadline, "timed out after {limit:?} waiting for {what}");
        std::thread::sleep(Duration::from_secs(2));
    }
}

fn panes(rt: &tokio::runtime::Runtime, herdr: &PrivateHerdr) -> Vec<PaneInfo> {
    let snap = rt.block_on(herdr.client().snapshot()).expect("snapshot");
    snap.workspaces.into_iter().flat_map(|w| w.tabs).flat_map(|t| t.panes).collect()
}

fn is_claude(p: &PaneInfo) -> bool {
    p.agent.as_ref().is_some_and(|a| a.kind == "claude")
}

/// The Claude pane whose `who` output (or label) names `seat`.
fn claude_pane_of(rig: &Rig, rt: &tokio::runtime::Runtime, seat: &str) -> Option<PaneInfo> {
    panes(rt, &rig.herdr).into_iter().filter(is_claude).find(|p| {
        p.label.as_deref().is_some_and(|l| l.to_lowercase().contains(seat))
            || rig
                .cmd(&["who", &p.id.to_string()], &[])
                .output()
                .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).to_lowercase().contains(seat))
    })
}

fn file_hash(p: &Path) -> Option<(std::time::SystemTime, Vec<u8>)> {
    let m = std::fs::metadata(p).ok()?;
    Some((m.modified().ok()?, std::fs::read(p).ok()?))
}

#[test]
fn real_agent_seat_bootstrap_and_summarizer_flow() {
    // 1. Gate.
    if std::env::var("HG_REAL_AGENTS").as_deref() != Ok("1") {
        return skipped("HG_REAL_AGENTS is not 1");
    }
    let Some(claude) = executable_on_path("claude") else {
        return skipped("no `claude` executable on PATH");
    };
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    // The user's real Claude config, observed before and after to prove it is untouched.
    let real_settings = std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".claude/settings.json"));
    let real_before = real_settings.as_deref().and_then(file_hash);

    let herdr = match PrivateHerdr::start() {
        Ok(h) => h,
        Err(e) => return skipped(&format!("private herdr unavailable: {e}")),
    };
    let has_key = herdr.env.iter().any(|(k, v)| k == "ANTHROPIC_API_KEY" && !v.is_empty());
    if !has_key {
        let ok = herdr.command(&claude).args(["auth", "status"]).stdin(Stdio::null()).output().is_ok_and(|o| o.status.success());
        if !ok {
            return skipped("no ANTHROPIC_API_KEY and `claude auth status` reports no login");
        }
    }

    // 2. Instance, private Claude config, hook and skills. The instance is the private env's own
    //    HERDR_GRAPH_INSTANCE so that panes the daemon launches inside the private server resolve it too.
    let instance = herdr.root.join("graph");
    let rig = Rig { herdr, instance };
    rig.run(&["init", rig.instance.to_str().unwrap(), "--with-examples"], &[]);
    let claude_dir = rig.herdr.root.join("claude");
    let settings = claude_dir.join("settings.json");
    let before_setup = std::fs::read(&settings).ok();
    rig.run(&["setup", "claude"], &[]);
    let after_setup = std::fs::read(&settings).expect("setup claude wrote the private settings.json");
    assert_ne!(before_setup.as_deref(), Some(after_setup.as_slice()), "setup claude changed nothing in the private settings.json");
    assert!(String::from_utf8_lossy(&after_setup).contains("seat"), "private settings.json lacks the SessionStart seat hook");

    // 3. Daemon inside the private env (Notify fallback delivery; no real threads daemon here).
    rig.run(&["daemon", "--ensure"], &[]);
    let status = rig.run(&["status"], &[]);
    assert!(status.contains("daemon: running"), "daemon not running: {status}");
    if std::env::var("HG_REAL_THREADS").as_deref() == Ok("1") {
        println!("REAL-AGENT: note HG_REAL_THREADS=1 is not wired in this test; NotifyFallback delivery is used");
    }

    // From here on the daemon is shut down on every exit path, including a failed assertion.
    struct Shutdown<'a>(&'a Rig);
    impl Drop for Shutdown<'_> {
        fn drop(&mut self) {
            let sock = herdr_graph::config::InstancePaths::new(&self.0.instance).socket;
            if let Ok(mut c) = herdr_graph::daemon::client::Client::connect(&sock, Duration::from_secs(3)) {
                let _ = c.call("shutdown", serde_json::json!({}));
            }
        }
    }
    let _shutdown = Shutdown(&rig);

    // 4. Teamspace and applications through the CLI relay path.
    rig.plan_apply(&["teamspace", "create", "demo", "--active"]);
    rig.plan_apply(&["application", "apply", "project-team", "--teamspace", "demo", "--name", "proj"]);
    rig.plan_apply(&["application", "apply", "system-summarizer", "--teamspace", "demo", "--name", "sum"]);

    // 5. The foreman agent appears and the SessionStart hook leads to /seat resolving it.
    let foreman = wait_until("foreman pane running claude", Duration::from_secs(180), || claude_pane_of(&rig, &rt, "foreman"));
    let pane = foreman.id.to_string();
    let seat: Value = wait_until("seat --json bound for the foreman pane", Duration::from_secs(60), || {
        let out = rig.cmd(&["seat", "--json"], &[("HERDR_PANE_ID", &pane)]).output().ok()?;
        let v: Value = serde_json::from_slice(&out.stdout).ok()?;
        let r = v.get("resolution").unwrap_or(&v);
        (r["status"] == "bound" && r["native_session"].is_string()).then_some(v)
    });
    let resolution = seat.get("resolution").unwrap_or(&seat);
    assert!(
        resolution["seat"].as_str().is_some_and(|s| s.starts_with("st_")),
        "foreman seat not resolved: {resolution}"
    );
    assert!(
        rig.run(&["who", &pane], &[]).to_lowercase().contains("foreman"),
        "`who` does not name the foreman for its pane"
    );

    // 6. Give the foreman something to say, then end its session (ctrl+c twice).
    rt.block_on(rig.herdr.client().send_keys(&foreman.id, &[KeyInput::Text("say hello".into()), KeyInput::Key("Enter".into())]))
        .expect("send prompt");
    std::thread::sleep(Duration::from_secs(20));
    wait_until("foreman idle", Duration::from_secs(120), || {
        let p = panes(&rt, &rig.herdr).into_iter().find(|p| p.id == foreman.id)?;
        matches!(p.agent?.status, herdr_graph::ports::herdr::AgentStatus::Idle | herdr_graph::ports::herdr::AgentStatus::Done).then_some(())
    });
    for _ in 0..2 {
        rt.block_on(rig.herdr.client().send_keys(&foreman.id, &[KeyInput::Key("ctrl+c".into())])).expect("ctrl+c");
        std::thread::sleep(Duration::from_secs(1));
    }
    let rq_line = wait_until("a pending/delivered request for the foreman transcript", Duration::from_secs(120), || {
        let all = rig.requests("--pending");
        all.into_iter().find(|l| l.starts_with("rq_"))
    });
    let mut cols = rq_line.split_whitespace();
    let (rq, _status0, transcript, range) = (cols.next().unwrap().to_owned(), cols.next(), cols.next().unwrap().to_owned(), cols.next().unwrap().to_owned());
    assert!(transcript.starts_with("tr_"), "request names a transcript: {rq_line}");
    let (start, end) = range.split_once('-').expect("range start-end");
    assert_eq!(start, "0", "request range starts at 0: {rq_line}");
    let end: u64 = end.parse().expect("range end");
    assert!(end > 0, "request range is empty: {rq_line}");

    // 7. Summarizer (relaunched if absent) is delivered the request and acks it; ACK is not completion.
    let summarizer = wait_until("summarizer pane running claude", Duration::from_secs(300), || claude_pane_of(&rig, &rt, "summarizer"));
    let record = rig.instance.join(format!("requests/{rq}.toml"));
    let acked = wait_until("delivery dispatched (ACK) before completion", Duration::from_secs(300), || {
        let text = std::fs::read_to_string(&record).ok()?;
        (text.contains("dispatched_at") && !text.contains("status = \"completed\"")).then_some(text)
    });
    assert!(acked.contains("dispatched_at"), "ACK not recorded");
    assert!(!acked.contains("covered_range"), "request already carried a result when the ACK was observed");
    let _ = summarizer;
    let done = wait_until("request completed", Duration::from_secs(600), || {
        let text = std::fs::read_to_string(&record).ok()?;
        text.contains("status = \"completed\"").then_some(text)
    });
    assert!(done.contains("covered_range"), "completed request has no covered_range: {done}");

    // Coverage: the transcript record's coverage contains the request range.
    let tr_record = std::fs::read_dir(rig.instance.join("transcripts"))
        .expect("transcripts dir")
        .flatten()
        .map(|d| d.path().join(format!("{transcript}.toml")))
        .find(|p| p.exists())
        .expect("transcript record");
    let tr_text = std::fs::read_to_string(&tr_record).unwrap();
    assert!(tr_text.contains("coverage"), "transcript has no coverage after completion: {tr_text}");
    assert!(tr_text.contains(&format!("end = {end}")), "transcript coverage does not reach {end}: {tr_text}");

    // The summary file exists under the foreman seat folder.
    let summary = find_summary(&rig.instance).expect("a summary file under <seat folder>/summaries/");

    // 8. Evidence, then the real settings must be untouched.
    println!("REAL-AGENT: VERIFIED transcript={} summary={}", tr_record.display(), summary.display());
    let real_after = real_settings.as_deref().and_then(file_hash);
    assert_eq!(real_before, real_after, "the user's real ~/.claude/settings.json changed during the test");
    // Teardown: the daemon is shut down by the guard; PrivateHerdr removes the root unless HG_KEEP_PRIVATE_ROOT=1.
}

/// First regular file under any `*/summaries/` directory of the instance.
fn find_summary(root: &Path) -> Option<PathBuf> {
    fn walk(dir: &Path, in_summaries: bool) -> Option<PathBuf> {
        for e in std::fs::read_dir(dir).ok()?.flatten() {
            let p = e.path();
            let name = e.file_name();
            if name == ".git" {
                continue;
            }
            if p.is_dir() {
                if let Some(f) = walk(&p, in_summaries || name == "summaries") {
                    return Some(f);
                }
            } else if in_summaries {
                return Some(p);
            }
        }
        None
    }
    walk(root, false)
}
