//! Tier-3 spikes (roast design r1): observations about Herdr 0.9.1 that hg-zmi.7/.8 design around. They run
//! only on request, in a private server, and print `SPIKE n: ...` lines; results live in docs/herdr-spikes.md.
//!
//!   cargo test --features private-herdr --test herdr_spikes -- --ignored --nocapture --test-threads=1
#![cfg(feature = "private-herdr")]

mod support;

use herdr_graph::herdr::HerdrClient;
use herdr_graph::model::HerdrPaneId;
use herdr_graph::ports::herdr::*;
use std::collections::BTreeSet;
use std::time::{Duration, Instant};
use support::private_herdr::PrivateHerdr;

fn start() -> Option<PrivateHerdr> {
    match PrivateHerdr::start() {
        Ok(h) => Some(h),
        Err(e) if support::herdr_binary().is_none() => {
            support::skip(&format!("{e}"));
            None
        }
        Err(e) => panic!("private herdr failed to start: {e:#}"),
    }
}

fn cwd(h: &PrivateHerdr) -> std::path::PathBuf {
    h.root.join("home")
}

/// A workspace whose first tab has `panes` panes and which has `tabs` tabs in total.
async fn build(
    c: &HerdrClient,
    h: &PrivateHerdr,
    label: &str,
    tabs: usize,
    panes: usize,
) -> Created {
    let ws = c
        .create_workspace(CreateWorkspace {
            label: label.into(),
            cwd: cwd(h),
            env: vec![],
        })
        .await
        .unwrap();
    let ws_id = ws.workspace.clone().unwrap();
    let mut all = vec![(ws.tab.clone().unwrap(), ws.pane.clone().unwrap())];
    for i in 1..tabs {
        let t = c
            .create_tab(CreateTab {
                workspace: ws_id.clone(),
                label: format!("t{i}"),
                cwd: cwd(h),
                env: vec![],
            })
            .await
            .unwrap();
        all.push((t.tab.unwrap(), t.pane.unwrap()));
    }
    for (_, root) in &all {
        for _ in 1..panes {
            c.split_pane(SplitPane {
                target: root.clone(),
                direction: SplitDirection::Right,
                cwd: cwd(h),
                env: vec![],
            })
            .await
            .unwrap();
        }
    }
    ws
}

/// Drain events for `window` after the action and render them as `name:id` strings, in arrival order.
async fn collect(rx: &mut HerdrEventStream, window: Duration) -> Vec<String> {
    let end = Instant::now() + window;
    let mut out = Vec::new();
    while let Ok(Some(ev)) =
        tokio::time::timeout(end.saturating_duration_since(Instant::now()), rx.recv()).await
    {
        let p = &ev.payload;
        let id = [
            "/pane/pane_id",
            "/pane_id",
            "/tab/tab_id",
            "/tab_id",
            "/workspace/workspace_id",
            "/workspace_id",
        ]
        .iter()
        .find_map(|ptr| p.pointer(ptr).and_then(|v| v.as_str()))
        .unwrap_or("?");
        out.push(format!("{}:{id}", ev.name));
    }
    out
}

fn ids(snap: &HerdrSnapshot) -> (BTreeSet<String>, BTreeSet<String>, BTreeSet<String>) {
    let ws = snap.workspaces.iter().map(|w| w.id.0.clone()).collect();
    let panes: Vec<&PaneInfo> = snap
        .workspaces
        .iter()
        .flat_map(|w| &w.tabs)
        .flat_map(|t| &t.panes)
        .collect();
    (
        ws,
        panes.iter().map(|p| p.id.0.clone()).collect(),
        panes
            .iter()
            .filter_map(|p| p.terminal_id.as_ref().map(|t| t.0.clone()))
            .collect(),
    )
}

#[tokio::test]
#[ignore = "spike: run with --ignored"]
async fn spike1_close_order_multi_pane_tab_and_multi_tab_workspace() {
    let Some(h) = start() else { return };
    let c = h.client();
    // A keeper workspace so closing the others never empties the server.
    build(&c, &h, "keeper", 1, 1).await;
    let ws = build(&c, &h, "multi-pane", 2, 3).await;
    let snap = c.snapshot().await.unwrap();
    let w = snap
        .workspaces
        .iter()
        .find(|w| Some(&w.id) == ws.workspace.as_ref())
        .unwrap();
    let target_tab = w.tabs[1].clone();
    println!(
        "SPIKE 1: tab {} holds panes {:?}",
        target_tab.id,
        target_tab
            .panes
            .iter()
            .map(|p| p.id.0.as_str())
            .collect::<Vec<_>>()
    );

    let mut rx = c.subscribe().await.unwrap();
    // (a) one pane of a multi-pane tab
    c.close_pane(&target_tab.panes[1].id).await.unwrap();
    println!(
        "SPIKE 1: events closing one pane of a 3-pane tab: {:?}",
        collect(&mut rx, Duration::from_millis(1500)).await
    );
    // (b) the whole tab, two panes still inside
    c.close_tab(&target_tab.id).await.unwrap();
    println!(
        "SPIKE 1: events closing the multi-pane tab (2 panes left): {:?}",
        collect(&mut rx, Duration::from_millis(1500)).await
    );
    // (c) the only pane of a one-pane tab in a workspace that has other tabs
    let solo = c
        .create_tab(CreateTab {
            workspace: ws.workspace.clone().unwrap(),
            label: "solo".into(),
            cwd: cwd(&h),
            env: vec![],
        })
        .await
        .unwrap();
    let _ = collect(&mut rx, Duration::from_millis(500)).await;
    c.close_pane(solo.pane.as_ref().unwrap()).await.unwrap();
    println!(
        "SPIKE 1: events closing the only pane of a one-pane tab: {:?}",
        collect(&mut rx, Duration::from_millis(1500)).await
    );
    let snap = c.snapshot().await.unwrap();
    let w = snap
        .workspaces
        .iter()
        .find(|x| Some(&x.id) == ws.workspace.as_ref())
        .unwrap();
    println!(
        "SPIKE 1: that tab still exists: {}",
        w.tabs.iter().any(|t| Some(&t.id) == solo.tab.as_ref())
    );
    // (d) the workspace with its remaining multi-pane tab
    let remaining: Vec<String> = w
        .tabs
        .iter()
        .flat_map(|t| t.panes.iter().map(|p| p.id.0.clone()))
        .collect();
    println!("SPIKE 1: panes of the workspace before closing it: {remaining:?}");
    c.close_workspace(ws.workspace.as_ref().unwrap())
        .await
        .unwrap();
    println!(
        "SPIKE 1: events closing the workspace: {:?}",
        collect(&mut rx, Duration::from_millis(1500)).await
    );
}

#[tokio::test]
#[ignore = "spike: run with --ignored"]
async fn spike2_last_tab_close_closes_workspace() {
    let Some(h) = start() else { return };
    let c = h.client();
    let keeper = build(&c, &h, "keeper", 1, 1).await;
    let ws = build(&c, &h, "two-tabs", 2, 1).await;
    let w_id = ws.workspace.clone().unwrap();
    let tabs: Vec<_> = c
        .snapshot()
        .await
        .unwrap()
        .workspaces
        .iter()
        .find(|w| w.id == w_id)
        .unwrap()
        .tabs
        .iter()
        .map(|t| t.id.clone())
        .collect();
    c.close_tab(&tabs[1]).await.unwrap();
    let present = |snap: &HerdrSnapshot| snap.workspaces.iter().any(|w| w.id == w_id);
    println!(
        "SPIKE 2: after closing 1 of 2 tabs the workspace exists: {}",
        present(&c.snapshot().await.unwrap())
    );

    let mut rx = c.subscribe().await.unwrap();
    let res = c.close_tab(&tabs[0]).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let snap = c.snapshot().await.unwrap();
    println!("SPIKE 2: close_tab on the last tab returned {res:?}");
    println!(
        "SPIKE 2: after closing the last tab the workspace exists: {}",
        present(&snap)
    );
    if let Some(w) = snap.workspaces.iter().find(|w| w.id == w_id) {
        println!(
            "SPIKE 2: remaining tabs of that workspace: {}",
            w.tabs.len()
        );
    }
    println!(
        "SPIKE 2: events: {:?}",
        collect(&mut rx, Duration::from_millis(1000)).await
    );
    println!(
        "SPIKE 2: keeper workspace still exists: {}",
        snap.workspaces
            .iter()
            .any(|w| Some(&w.id) == keeper.workspace.as_ref())
    );
}

#[tokio::test]
#[ignore = "spike: run with --ignored"]
async fn spike3_restart_preserves_metadata_labels_terminal_ids() {
    let Some(mut h) = start() else { return };
    let c = h.client();
    let ws = build(&c, &h, "persist", 2, 2).await;
    let w_id = ws.workspace.clone().unwrap();
    c.report_workspace_metadata(&w_id, "hg", "ts_SPIKE")
        .await
        .unwrap();
    c.report_pane_metadata(ws.pane.as_ref().unwrap(), "hg", "cl_SPIKE")
        .await
        .unwrap();
    c.rename_pane(ws.pane.as_ref().unwrap(), "spike-label")
        .await
        .unwrap();
    let before = c.snapshot().await.unwrap();
    let (bw, bp, bt) = ids(&before);
    println!("SPIKE 3: before restart: workspaces {bw:?} panes {bp:?} terminals {bt:?}");

    h.restart().unwrap();
    let c = h.client();
    let after = c.snapshot().await.unwrap();
    let (aw, ap, at) = ids(&after);
    println!("SPIKE 3: after restart: workspaces {aw:?} panes {ap:?} terminals {at:?}");
    println!(
        "SPIKE 3: workspace ids reused: {}; pane ids reused: {}; terminal ids reused: {}",
        bw == aw && !aw.is_empty(),
        bp == ap && !ap.is_empty(),
        bt == at && !at.is_empty()
    );
    println!(
        "SPIKE 3: terminal ids shared with before: {:?}",
        bt.intersection(&at).collect::<Vec<_>>()
    );
    for w in &after.workspaces {
        println!(
            "SPIKE 3: workspace {} label {:?} metadata {:?}",
            w.id, w.label, w.metadata
        );
        for t in &w.tabs {
            for p in &t.panes {
                println!(
                    "SPIKE 3:   tab {:?} pane {} label {:?} metadata {:?}",
                    t.label, p.id, p.label, p.metadata
                );
            }
        }
    }
    println!(
        "SPIKE 3: incarnation before {:?} after {:?}",
        before.incarnation, after.incarnation
    );
}

#[tokio::test]
#[ignore = "spike: run with --ignored"]
async fn spike4_detached_child_survives_startup_hook() {
    use std::os::unix::fs::PermissionsExt;
    let Some(mut h) = start() else { return };
    let dir = h.root.join("spike4-plugin");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("herdr-plugin.toml"),
        "id = \"hg-spike4\"\nname = \"Spike 4\"\nversion = \"0.1.0\"\nmin_herdr_version = \"0.9.1\"\nplatforms = [\"macos\"]\n\n[[startup]]\nid = \"daemon\"\ncommand = [\"./start.sh\"]\n",
    )
    .unwrap();
    let out = h.root.display();
    // Variant A: plain `nohup ... &`. Variant B: a new session via perl's setsid (macOS ships no setsid(1)).
    let script = format!(
        "#!/bin/sh\necho \"hook ran pid=$$\" >> {out}/hook.log\n\
         nohup sh -c 'echo $$ > {out}/child-a.pid; exec sleep 300' >/dev/null 2>&1 &\n\
         perl -MPOSIX -e 'POSIX::setsid(); exec @ARGV' sh -c 'echo $$ > {out}/child-b.pid; exec sleep 300' >/dev/null 2>&1 &\n\
         echo \"hook exiting\" >> {out}/hook.log\n"
    );
    std::fs::write(dir.join("start.sh"), script).unwrap();
    std::fs::set_permissions(dir.join("start.sh"), std::fs::Permissions::from_mode(0o755)).unwrap();

    let link = h
        .command(h.herdr_path())
        .args(["plugin", "link"])
        .arg(&dir)
        .output()
        .unwrap();
    println!(
        "SPIKE 4: plugin link status {:?}: {} {}",
        link.status.code(),
        String::from_utf8_lossy(&link.stdout).trim(),
        String::from_utf8_lossy(&link.stderr).trim()
    );
    if !link.status.success() {
        println!("SPIKE 4: not verified: plugin link failed in the private server");
        return;
    }
    let ran = |root: &std::path::Path| root.join("hook.log").exists();
    // Startup hooks run when a server starts: restart the private server to trigger it.
    let mut triggered = ran(&h.root);
    println!("SPIKE 4: hook ran right after link: {triggered}");
    if !triggered {
        h.restart().unwrap();
        for _ in 0..50 {
            if ran(&h.root) {
                triggered = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        println!("SPIKE 4: hook ran after server restart: {triggered}");
    }
    if !triggered {
        println!("SPIKE 4: not verified: startup hook never ran");
        return;
    }
    tokio::time::sleep(Duration::from_secs(3)).await;
    println!(
        "SPIKE 4: hook.log: {:?}",
        std::fs::read_to_string(h.root.join("hook.log")).unwrap_or_default()
    );
    for variant in ["a", "b"] {
        match std::fs::read_to_string(h.root.join(format!("child-{variant}.pid")))
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
        {
            None => println!("SPIKE 4: variant {variant}: no pid file written"),
            Some(pid) => {
                let alive = std::process::Command::new("/bin/kill")
                    .args(["-0", &pid.to_string()])
                    .stderr(std::process::Stdio::null())
                    .status()
                    .unwrap()
                    .success();
                let ps = std::process::Command::new("/bin/ps")
                    .args(["-o", "pid=,ppid=,pgid=,command=", "-p", &pid.to_string()])
                    .output()
                    .unwrap();
                println!(
                    "SPIKE 4: variant {variant}: child pid {pid} alive 3 s after the hook exited: {alive}; ps: {}",
                    String::from_utf8_lossy(&ps.stdout).trim()
                );
                if alive {
                    let _ = std::process::Command::new("/bin/kill")
                        .args(["-TERM", &pid.to_string()])
                        .status();
                }
            }
        }
    }
}

#[tokio::test]
#[ignore = "spike: run with --ignored"]
async fn spike5_agent_session_for_agent_started_claude() {
    if std::env::var("HG_REAL_AGENTS").as_deref() != Ok("1")
        || std::env::var("ANTHROPIC_API_KEY").is_err()
    {
        support::skip(
            "spike 5 needs HG_REAL_AGENTS=1 and an explicit ANTHROPIC_API_KEY (no user credentials are copied)",
        );
        println!(
            "SPIKE 5: not verified: HG_REAL_AGENTS=1 and ANTHROPIC_API_KEY not set; no real agent was launched"
        );
        return;
    }
    let Some(h) = start() else { return };
    let c = h.client();
    let made = build(&c, &h, "agent", 1, 1).await;
    let pane: HerdrPaneId = made.pane.unwrap();
    c.rename_pane(&pane, "spike-claude").await.unwrap();
    let outcome = c
        .start_agent(StartAgent {
            pane: pane.clone(),
            kind: "claude".into(),
            args: vec![],
        })
        .await
        .unwrap();
    println!("SPIKE 5: start_agent outcome {outcome:?}");
    let deadline = Instant::now() + Duration::from_secs(90);
    let mut last = None;
    while Instant::now() < deadline {
        last = c.agent(&pane).await.unwrap();
        if last.as_ref().is_some_and(|a| a.session.is_some()) {
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    println!("SPIKE 5: agent after wait: {last:?}");
    println!(
        "SPIKE 5: agent_session reported without Herdr's Claude integration: {}",
        last.as_ref().is_some_and(|a| a.session.is_some())
    );
}
