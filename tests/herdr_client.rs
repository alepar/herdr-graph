//! Tier-3: `HerdrClient` against a private `herdr server` (spec §11). Skips when `herdr` is not installed.
#![cfg(feature = "private-herdr")]

mod support;

use herdr_graph::herdr::HerdrClient;
use herdr_graph::model::{HerdrPaneId, HerdrTabId, HerdrWorkspaceId};
use herdr_graph::ports::herdr::*;
use std::path::Path;
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

async fn eventually<T, F: std::future::Future<Output = Option<T>>>(
    what: &str,
    mut f: impl FnMut() -> F,
) -> T {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(v) = f().await {
            return v;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn read_trimmed(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

async fn new_workspace(c: &HerdrClient, h: &PrivateHerdr, label: &str) -> Created {
    c.create_workspace(CreateWorkspace {
        label: label.into(),
        cwd: h.root.join("home"),
        env: vec![],
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn private_create_tab_panes_with_env_snapshot_matches() {
    let Some(h) = start() else { return };
    let c = h.client();
    let cwd = h.root.join("home");
    let env = |clone: &str| {
        vec![
            ("HERDR_GRAPH".to_owned(), "1".to_owned()),
            ("HERDR_GRAPH_CLONE".to_owned(), clone.to_owned()),
        ]
    };

    let ws = c
        .create_workspace(CreateWorkspace {
            label: "graph-ws".into(),
            cwd: cwd.clone(),
            env: env("cl_ws"),
        })
        .await
        .unwrap();
    let ws_id = ws.workspace.clone().unwrap();
    let tab = c
        .create_tab(CreateTab {
            workspace: ws_id.clone(),
            label: "seat".into(),
            cwd: cwd.clone(),
            env: env("cl_tab"),
        })
        .await
        .unwrap();
    let tab_pane = tab.pane.clone().unwrap();
    let split = c
        .split_pane(SplitPane {
            target: tab_pane.clone(),
            direction: SplitDirection::Right,
            cwd: cwd.clone(),
            env: env("cl_split"),
        })
        .await
        .unwrap();
    let split_pane = split.pane.clone().unwrap();
    assert_eq!(split.tab, tab.tab, "split stays in the target's tab");

    c.report_pane_metadata(&tab_pane, "hg", "cl_01HXXXXXXXXXXXXXXXXXXXXXXX")
        .await
        .unwrap();
    c.report_workspace_metadata(&ws_id, "hg", "ts_01HXXXXXXXXXXXXXXXXXXXXXXX")
        .await
        .unwrap();
    c.rename_pane(&split_pane, "worker").await.unwrap();

    let snap = c.snapshot().await.unwrap();
    let w = snap
        .workspaces
        .iter()
        .find(|w| w.id == ws_id)
        .expect("workspace in snapshot");
    assert_eq!(w.label, "graph-ws");
    assert_eq!(
        w.metadata.get("hg").map(String::as_str),
        Some("ts_01HXXXXXXXXXXXXXXXXXXXXXXX")
    );
    assert_eq!(
        w.tabs.iter().map(|t| t.label.as_str()).collect::<Vec<_>>(),
        ["1", "seat"]
    );
    let seat = w
        .tabs
        .iter()
        .find(|t| Some(&t.id) == tab.tab.as_ref())
        .unwrap();
    assert_eq!(seat.panes.len(), 2);
    let stamped = seat.panes.iter().find(|p| p.id == tab_pane).unwrap();
    assert_eq!(
        stamped.metadata.get("hg").map(String::as_str),
        Some("cl_01HXXXXXXXXXXXXXXXXXXXXXXX")
    );
    assert_eq!(stamped.cwd.as_deref(), Some(cwd.as_path()));
    assert!(stamped.terminal_id.is_some());
    let worker = seat.panes.iter().find(|p| p.id == split_pane).unwrap();
    assert_eq!(worker.label.as_deref(), Some("worker"));
    assert!(worker.metadata.is_empty());
    assert_ne!(worker.terminal_id, stamped.terminal_id);
    assert!(
        snap.incarnation.server_pid.is_none_or(|p| p == h.pid()),
        "incarnation names our server, not another"
    );

    // The env really reached the shells: have each print its HERDR_GRAPH_CLONE.
    for (pane, clone, file) in [
        (&tab_pane, "cl_tab", "env-tab"),
        (&split_pane, "cl_split", "env-split"),
    ] {
        let out = h.root.join(file);
        let line = format!("echo $HERDR_GRAPH_CLONE > {}", out.display());
        c.send_keys(pane, &[KeyInput::Text(line), KeyInput::Key("Enter".into())])
            .await
            .unwrap();
        let got = eventually(file, || read_trimmed(&out)).await;
        assert_eq!(got, clone);
    }
}

#[tokio::test]
async fn private_events_created_renamed_closed() {
    let Some(h) = start() else { return };
    let c = h.client();
    let ws = new_workspace(&c, &h, "ev").await.workspace.unwrap();
    let mut rx = c.subscribe().await.unwrap();

    let tab = c
        .create_tab(CreateTab {
            workspace: ws,
            label: "t".into(),
            cwd: h.root.join("home"),
            env: vec![],
        })
        .await
        .unwrap();
    let tab_id = tab.tab.unwrap();
    c.rename_tab(&tab_id, "t2").await.unwrap();
    c.close_tab(&tab_id).await.unwrap();

    let mut seen: Vec<String> = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !["tab_created", "tab_renamed", "tab_closed"]
        .iter()
        .all(|n| seen.iter().any(|s| s == n))
    {
        let left = deadline.saturating_duration_since(Instant::now());
        let ev = tokio::time::timeout(left, rx.recv())
            .await
            .unwrap_or_else(|_| panic!("events so far: {seen:?}"))
            .expect("stream open");
        if ev.name.starts_with("tab_") {
            // `tab_created` nests the tab object; renamed/closed carry the id at the top level.
            let named = ev
                .payload
                .pointer("/tab/tab_id")
                .or_else(|| ev.payload.get("tab_id"));
            assert_eq!(
                named.and_then(|v| v.as_str()),
                Some(tab_id.0.as_str()),
                "{} names our tab",
                ev.name
            );
        }
        seen.push(ev.name);
    }
    let at = |n: &str| seen.iter().position(|s| s == n).unwrap();
    assert!(
        at("tab_created") < at("tab_closed"),
        "created before closed: {seen:?}"
    );
    let snap = c.snapshot().await.unwrap();
    assert!(
        snap.workspaces[0].tabs.iter().all(|t| t.id != tab_id),
        "closed tab is gone from the snapshot"
    );
}

#[tokio::test]
async fn private_snapshot_after_reconnect() {
    let Some(h) = start() else { return };
    let c = h.client();
    new_workspace(&c, &h, "rc").await;
    let first = c.subscribe().await.unwrap();
    let g1 = c.snapshot().await.unwrap().incarnation;
    drop(first);
    let _second = c.subscribe().await.unwrap();
    let g2 = c.snapshot().await.unwrap().incarnation;
    assert_eq!(
        g2.generation,
        g1.generation + 1,
        "each subscription bumps the generation"
    );
    if let Some(pid) = g2.server_pid {
        assert_eq!(pid, h.pid());
        assert!(g2.server_started.is_some_and(|s| !s.is_empty()));
    } else {
        eprintln!("NOTE: lsof/ps did not identify the server pid (incarnation is best effort)");
    }
}

#[tokio::test]
async fn private_process_info_is_shell() {
    let Some(h) = start() else { return };
    let c = h.client();
    let pane = new_workspace(&c, &h, "pi").await.pane.unwrap();
    let info = eventually("shell foreground", || async {
        c.process_info(&pane)
            .await
            .ok()
            .filter(|i| i.is_shell && i.foreground_pid.is_some())
    })
    .await;
    assert!(info.foreground_pid.is_some());
    c.send_keys(
        &pane,
        &[
            KeyInput::Text("sleep 60".into()),
            KeyInput::Key("Enter".into()),
        ],
    )
    .await
    .unwrap();
    let busy = eventually("sleep in foreground", || async {
        c.process_info(&pane).await.ok().filter(|i| !i.is_shell)
    })
    .await;
    assert!(
        busy.foreground_argv.iter().any(|a| a == "sleep"),
        "foreground argv: {:?}",
        busy.foreground_argv
    );
}

#[tokio::test]
async fn private_agent_absent_and_rejections() {
    let Some(h) = start() else { return };
    let c = h.client();
    let pane = new_workspace(&c, &h, "ag").await.pane.unwrap();
    assert_eq!(c.agent(&pane).await.unwrap(), None);
    // Unknown ids are rejections, not transport failures.
    let missing = HerdrPaneId("w99:p99".into());
    assert!(matches!(
        c.close_pane(&missing).await,
        Err(HerdrError::Rejected { .. })
    ));
    assert!(matches!(
        c.rename_tab(&HerdrTabId("w99:t99".into()), "x").await,
        Err(HerdrError::Rejected { .. })
    ));
    assert!(matches!(
        c.close_workspace(&HerdrWorkspaceId("w99".into())).await,
        Err(HerdrError::Rejected { .. })
    ));
    // An unsupported agent kind is a precondition problem for the caller, not an error.
    let out = c
        .start_agent(StartAgent {
            pane: pane.clone(),
            kind: "no-such-agent".into(),
            args: vec![],
        })
        .await
        .unwrap();
    assert!(
        matches!(
            out,
            herdr_graph::model::harness::StartOutcome::NeedsRevision { .. }
        ),
        "got {out:?}"
    );
}
