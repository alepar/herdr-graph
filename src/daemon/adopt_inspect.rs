//! `adopt.inspect` (onboarding design, phase 2): read-only facts about one unmanaged Herdr tab or pane, for the
//! adopt skill. One Herdr snapshot, `process_info` per pane, the pane-seat map and the seat view; nothing is
//! committed and no plan is created, so the deps hold no writer. Threads memberships are not read here: the
//! graph has no service operation for them, so the CLI adds them from the user's threads CLI.
use crate::daemon::registry::{CommandCtx, CommandError, Registry};
use crate::model::IdKind;
use crate::model::harness::for_agent_kind;
use crate::observe::sessions::capture_session;
use crate::ports::herdr::{HerdrApi, HerdrSnapshot, PaneInfo, ProcessInfo, TabInfo, WorkspaceInfo};
use crate::ports::threads::{ThreadsSeatRef, ThreadsSeatView};
use crate::reconcile::planner::token_of;
use crate::threads::mapping::PaneSeatMap;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct AdoptInspectDeps {
    pub herdr: Option<Arc<dyn HerdrApi>>,
    pub seats: Arc<dyn PaneSeatMap>,
    pub seat_view: Arc<dyn ThreadsSeatView>,
    pub claude_root: PathBuf,
}

/// Registers `adopt.inspect`. Request `{target}`; reply is the schema-1 document of the design.
pub fn register(reg: &mut Registry, deps: AdoptInspectDeps) {
    let deps = Arc::new(deps);
    reg.command("adopt.inspect", move |_cx: CommandCtx, args: Value| {
        let d = deps.clone();
        async move { handle(&d, &args).await }
    });
}

type Found<'a> = (&'a WorkspaceInfo, &'a TabInfo, Vec<&'a PaneInfo>);

/// An exact pane id, else an exact tab id, else a unique tab label across the snapshot.
fn resolve<'a>(snap: &'a HerdrSnapshot, target: &str) -> Result<Found<'a>, CommandError> {
    let tabs = || {
        snap.workspaces
            .iter()
            .flat_map(|w| w.tabs.iter().map(move |t| (w, t)))
    };
    if let Some((w, t)) = tabs().find(|(_, t)| t.panes.iter().any(|p| p.id.0 == target)) {
        let pane = t.panes.iter().find(|p| p.id.0 == target).expect("found");
        return Ok((w, t, vec![pane]));
    }
    if let Some((w, t)) = tabs().find(|(_, t)| t.id.0 == target) {
        return Ok((w, t, t.panes.iter().collect()));
    }
    let by_label: Vec<_> = tabs().filter(|(_, t)| t.label == target).collect();
    match by_label.as_slice() {
        [(w, t)] => Ok((w, t, t.panes.iter().collect())),
        [] => Err(CommandError::bad_request(format!(
            "no pane, tab id or tab label matches {target:?}"
        ))),
        many => Err(CommandError::bad_request(format!(
            "tab label {target:?} is ambiguous: {}",
            many.iter()
                .map(|(w, t)| format!("{} ({})", t.id, w.label))
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

/// The clone a pane's graph token names, if any (a workspace token does not make a tab managed).
fn managed_clone(md: &std::collections::BTreeMap<String, String>) -> Option<String> {
    token_of(md)
        .filter(|id| id.kind() == IdKind::Clone)
        .map(|id| id.as_str().to_owned())
}

fn git_toplevel(cwd: &Path) -> Option<String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(["rev-parse", "--show-toplevel"])
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?;
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_owned())
}

async fn threads_facts(d: &AdoptInspectDeps, pane: &PaneInfo) -> Value {
    if pane.agent.is_none() {
        return json!({ "seat": null, "seat_note": "no agent in pane; not resolved" });
    }
    let seat = match d.seats.seat_for(&pane.id).await {
        Ok(Some(s)) => s,
        Ok(None) => {
            return json!({ "seat": null, "seat_note": "threads maps no seat to this pane" });
        }
        Err(e) => return json!({ "seat": null, "seat_error": e.to_string() }),
    };
    let mut out = json!({ "seat": seat.0 });
    match d.seat_view.inspect(std::slice::from_ref(&seat)).await {
        Ok(map) => match map.get(&ThreadsSeatRef(seat.0.clone())) {
            Some(st) => out["status"] = json!(st),
            None => out["status_error"] = json!("seat view refused this seat"),
        },
        Err(e) => out["status_error"] = json!(e.to_string()),
    }
    out
}

async fn pane_facts(d: &AdoptInspectDeps, pane: &PaneInfo) -> Value {
    let cwd = pane.cwd.clone();
    let git = match cwd.clone() {
        Some(c) => tokio::task::spawn_blocking(move || git_toplevel(&c))
            .await
            .ok()
            .flatten(),
        None => None,
    };
    let harness = pane.agent.as_ref().and_then(|a| for_agent_kind(&a.kind));
    let (process, process_error): (Option<ProcessInfo>, Option<String>) = match &d.herdr {
        Some(h) => match h.process_info(&pane.id).await {
            Ok(p) => (Some(p), None),
            Err(e) => (None, Some(e.to_string())),
        },
        None => (None, Some("no herdr client".into())),
    };
    let session = match (harness, pane.agent.as_ref()) {
        (Some(h), Some(_)) => {
            let recorded = cwd.clone().unwrap_or_default();
            capture_session(h, pane, process.as_ref(), &d.claude_root, &recorded)
                .map(|c| json!({ "native_session_id": c.native_session_id, "transcript_path": c.transcript_path }))
        }
        _ => None,
    };
    let mut out = json!({
        "id": pane.id,
        "terminal_id": pane.terminal_id,
        "label": pane.label,
        "cwd": cwd,
        "git_toplevel": git,
        "agent": pane.agent.as_ref().map(|a| json!({ "kind": a.kind, "status": a.status, "harness": harness })),
        "process": process.as_ref().map(|p| json!({
            "foreground_pid": p.foreground_pid,
            "argv": p.foreground_argv,
            "is_shell": p.is_shell,
        })),
        "session": session,
        "threads": threads_facts(d, pane).await,
    });
    if let Some(e) = process_error {
        out["process_error"] = json!(e);
    }
    out
}

async fn handle(d: &Arc<AdoptInspectDeps>, args: &Value) -> Result<Value, CommandError> {
    let target = args
        .get("target")
        .and_then(Value::as_str)
        .filter(|t| !t.is_empty())
        .ok_or_else(|| CommandError::bad_request("adopt.inspect needs a non-empty `target`"))?;
    let herdr = d
        .herdr
        .as_ref()
        .ok_or_else(|| CommandError::unavailable("no herdr client"))?;
    let snap = herdr
        .snapshot()
        .await
        .map_err(|e| CommandError::unavailable(e.to_string()))?;
    let (ws, tab, panes) = resolve(&snap, target)?;
    if let Some((p, cl)) = panes
        .iter()
        .find_map(|p| managed_clone(&p.metadata).map(|c| (p, c)))
    {
        return Err(CommandError::bad_request(format!(
            "pane {} is managed by graph (clone {cl})",
            p.id
        )));
    }
    let mut out = Vec::with_capacity(panes.len());
    for p in panes {
        out.push(pane_facts(d, p).await);
    }
    Ok(json!({
        "schema": 1,
        "target": target,
        "workspace": { "id": ws.id, "label": ws.label },
        "tab": { "id": tab.id, "label": tab.label },
        "panes": out,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::registry::CallerInfo;
    use crate::herdr::fake::FakeHerdr;
    use crate::model::{HerdrPaneId, HerdrTabId};
    use crate::ports::herdr::{
        AgentInfo, AgentSession, AgentStatus, CreateTab, CreateWorkspace, ProcessInfo,
    };
    use crate::ports::threads::{SeatStatus, ThreadsError};
    use crate::threads::mapping::SeatResolution;
    use crate::threads::{FakePaneSeatMap, FakeThreadsSeatView};
    use std::sync::Mutex;

    /// Counts `seat_for` calls per pane over a fake map.
    struct CountingMap {
        inner: Arc<FakePaneSeatMap>,
        calls: Mutex<Vec<String>>,
    }

    #[async_trait::async_trait]
    impl PaneSeatMap for CountingMap {
        async fn seat_for(
            &self,
            pane: &HerdrPaneId,
        ) -> Result<Option<ThreadsSeatRef>, ThreadsError> {
            self.calls.lock().unwrap().push(pane.0.clone());
            self.inner.seat_for(pane).await
        }
        async fn resolve_seat(
            &self,
            pane: &HerdrPaneId,
            op: &crate::ports::threads::OpKey,
        ) -> Result<SeatResolution, ThreadsError> {
            self.inner.resolve_seat(pane, op).await
        }
    }

    struct Rig {
        reg: Registry,
        fake: Arc<FakeHerdr>,
        map: Arc<CountingMap>,
        seats: Arc<FakePaneSeatMap>,
        tab: HerdrTabId,
        claude: HerdrPaneId,
        shell: HerdrPaneId,
        managed: HerdrPaneId,
        managed_tab: HerdrTabId,
        repo: tempfile::TempDir,
    }

    async fn rig() -> Rig {
        let repo = tempfile::tempdir().unwrap();
        let ok = std::process::Command::new("git")
            .arg("init")
            .arg("-q")
            .arg(repo.path())
            .status()
            .unwrap()
            .success();
        assert!(ok, "git init");
        let fake = FakeHerdr::new();
        let ws = fake
            .create_workspace(CreateWorkspace {
                label: "legacy-ws".into(),
                cwd: repo.path().to_owned(),
                env: vec![],
            })
            .await
            .unwrap();
        let ws_id = ws.workspace.clone().unwrap();
        let tab = fake
            .create_tab(CreateTab {
                workspace: ws_id.clone(),
                label: "legacy".into(),
                cwd: repo.path().to_owned(),
                env: vec![],
            })
            .await
            .unwrap();
        let claude = tab.pane.clone().unwrap();
        let tab_id = tab.tab.clone().unwrap();
        let shell = fake.add_user_pane(&tab_id, "sh");
        fake.set_agent(
            &claude,
            Some(AgentInfo {
                kind: "claude".into(),
                status: AgentStatus::Idle,
                session: Some(AgentSession::Id("abc".into())),
            }),
        );
        fake.set_process(
            &claude,
            ProcessInfo {
                foreground_pid: Some(42),
                foreground_argv: vec!["claude".into(), "--model".into(), "x".into()],
                is_shell: false,
            },
        );
        let m = fake
            .create_tab(CreateTab {
                workspace: ws_id,
                label: "mine".into(),
                cwd: "/".into(),
                env: vec![],
            })
            .await
            .unwrap();
        let managed = m.pane.clone().unwrap();
        fake.report_pane_metadata(&managed, "hg", "hg=cl_01HYYYYYYYYYYYYYYYYYYYYYYY")
            .await
            .unwrap();
        let seats = Arc::new(FakePaneSeatMap::new());
        seats.set(&claude, "s1");
        let map = Arc::new(CountingMap {
            inner: seats.clone(),
            calls: Mutex::new(vec![]),
        });
        let view = Arc::new(FakeThreadsSeatView::new());
        view.set(
            &ThreadsSeatRef("s1".into()),
            SeatStatus::Resolved {
                pane: claude.clone(),
            },
        );
        let mut reg = Registry::default();
        register(
            &mut reg,
            AdoptInspectDeps {
                herdr: Some(fake.clone() as Arc<dyn HerdrApi>),
                seats: map.clone(),
                seat_view: view,
                claude_root: repo.path().join("claude-root"),
            },
        );
        Rig {
            reg,
            fake,
            map,
            seats,
            tab: tab_id,
            claude,
            shell,
            managed,
            managed_tab: m.tab.unwrap(),
            repo,
        }
    }

    async fn call(r: &Rig, target: &str) -> Result<Value, CommandError> {
        let cx = CommandCtx {
            request_id: "r".into(),
            caller: CallerInfo::default(),
        };
        r.reg
            .handler("adopt.inspect")
            .unwrap()
            .call(cx, json!({ "target": target }))
            .await
    }

    #[tokio::test]
    async fn inspect_pane_reports_ids_cwd_git_agent_process_session_and_seat() {
        let r = rig().await;
        let v = call(&r, &r.claude.0).await.unwrap();
        assert_eq!(v["schema"], json!(1));
        assert_eq!(v["target"], json!(r.claude.0));
        assert_eq!(v["workspace"]["label"], json!("legacy-ws"));
        assert_eq!(v["tab"]["id"], json!(r.tab.0));
        assert_eq!(v["tab"]["label"], json!("legacy"));
        let panes = v["panes"].as_array().unwrap();
        assert_eq!(panes.len(), 1, "a pane target yields only that pane");
        let p = &panes[0];
        assert_eq!(p["id"], json!(r.claude.0));
        let top = std::fs::canonicalize(r.repo.path()).unwrap();
        assert_eq!(p["cwd"], json!(r.repo.path()));
        assert_eq!(
            std::fs::canonicalize(p["git_toplevel"].as_str().unwrap()).unwrap(),
            top
        );
        assert_eq!(
            p["agent"],
            json!({"kind": "claude", "status": "idle", "harness": "claude"})
        );
        assert_eq!(
            p["process"],
            json!({"foreground_pid": 42, "argv": ["claude", "--model", "x"], "is_shell": false})
        );
        assert_eq!(p["session"]["native_session_id"], json!("abc"));
        assert_eq!(p["session"]["transcript_path"], json!(null));
        assert_eq!(p["threads"]["seat"], json!("s1"));
        assert_eq!(p["threads"]["status"]["state"], json!("resolved"));
        assert_eq!(p["threads"]["status"]["pane"], json!(r.claude.0));
        assert_eq!(r.fake.snapshot_calls(), 1);
    }

    #[tokio::test]
    async fn inspect_tab_reports_every_pane_and_skips_seat_resolution_without_an_agent() {
        let r = rig().await;
        let v = call(&r, &r.tab.0).await.unwrap();
        let panes = v["panes"].as_array().unwrap();
        assert_eq!(panes.len(), 2, "claude pane and shell pane of the tab");
        let sh = panes.iter().find(|p| p["id"] == json!(r.shell.0)).unwrap();
        assert_eq!(sh["threads"]["seat"], json!(null));
        assert!(
            sh["threads"]["seat_note"]
                .as_str()
                .unwrap()
                .contains("no agent")
        );
        assert_eq!(sh["agent"], json!(null));
        assert_eq!(sh["session"], json!(null));
        assert_eq!(sh["process"]["is_shell"], json!(true));
        assert_eq!(*r.map.calls.lock().unwrap(), vec![r.claude.0.clone()]);
    }

    #[tokio::test]
    async fn inspect_resolves_a_unique_tab_label_and_refuses_an_ambiguous_one() {
        let r = rig().await;
        let v = call(&r, "legacy").await.unwrap();
        assert_eq!(v["tab"]["id"], json!(r.tab.0));
        let e = call(&r, "nope").await.unwrap_err();
        assert_eq!(e.code, crate::ipc::IpcErrorCode::BadRequest);
        // a second tab with the same label, in another workspace
        let other = r
            .fake
            .create_workspace(CreateWorkspace {
                label: "second-ws".into(),
                cwd: "/".into(),
                env: vec![],
            })
            .await
            .unwrap();
        let t2 = r
            .fake
            .create_tab(CreateTab {
                workspace: other.workspace.unwrap(),
                label: "legacy".into(),
                cwd: "/".into(),
                env: vec![],
            })
            .await
            .unwrap()
            .tab
            .unwrap();
        let e = call(&r, "legacy").await.unwrap_err();
        assert_eq!(e.code, crate::ipc::IpcErrorCode::BadRequest);
        assert!(e.message.contains(&r.tab.0), "{}", e.message);
        assert!(
            e.message.contains(&format!("{} (second-ws)", t2.0)),
            "{}",
            e.message
        );
    }

    #[tokio::test]
    async fn inspect_refuses_a_managed_pane_and_a_tab_holding_one() {
        let r = rig().await;
        let e = call(&r, &r.managed.0).await.unwrap_err();
        assert_eq!(e.code, crate::ipc::IpcErrorCode::BadRequest);
        assert_eq!(
            e.message,
            format!(
                "pane {} is managed by graph (clone cl_01HYYYYYYYYYYYYYYYYYYYYYYY)",
                r.managed.0
            )
        );
        let e = call(&r, &r.managed_tab.0).await.unwrap_err();
        assert!(e.message.contains("managed by graph"), "{}", e.message);
        // a managed pane added to the unmanaged tab makes that tab refused too
        let extra = r.fake.add_user_pane(&r.tab, "extra");
        r.fake
            .report_pane_metadata(&extra, "hg", "hg=cl_01HYYYYYYYYYYYYYYYYYYYYYYY")
            .await
            .unwrap();
        let e = call(&r, &r.tab.0).await.unwrap_err();
        assert!(e.message.contains(&extra.0), "{}", e.message);
        // the unmanaged pane next to it is still inspectable on its own
        call(&r, &r.claude.0).await.unwrap();
    }

    #[tokio::test]
    async fn inspect_reports_threads_failure_instead_of_no_seat() {
        let r = rig().await;
        r.seats.set_failing(true);
        let v = call(&r, &r.claude.0).await.unwrap();
        let t = &v["panes"][0]["threads"];
        assert_eq!(t["seat"], json!(null));
        assert!(
            t["seat_error"].as_str().unwrap().contains("unavailable"),
            "{t}"
        );
        assert!(t.get("seat_note").is_none(), "an error is not a note: {t}");
    }

    #[tokio::test]
    async fn inspect_commits_nothing() {
        // The deps carry no writer and no store, so the handler cannot commit. What remains observable is that
        // it never calls a mutating Herdr method.
        let r = rig().await;
        r.fake.clear_calls();
        call(&r, &r.tab.0).await.unwrap();
        assert!(r.fake.calls().is_empty(), "{:?}", r.fake.calls());
    }
}
