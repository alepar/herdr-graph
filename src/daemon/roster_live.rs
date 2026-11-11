//! `roster.live` (spec §2.2 `--live`): the committed roster at one pinned HEAD plus a live overlay from one
//! Herdr snapshot and the threads seat read the daemon already holds. The roster itself is built by
//! `inspect::roster` (the daemon never builds a second one); this handler only gathers the live inputs.
use crate::daemon::compose::SeatReadState;
use crate::daemon::registry::{CommandCtx, CommandError, Registry};
use crate::inspect::pin_head;
use crate::inspect::roster::{RosterQuery, ThreadsRead, build_with_bindings, overlay};
use crate::inspect::scope::{AllowClone, Scope, ScopeArg};
use crate::observe::step::SeatLost;
use crate::ports::herdr::HerdrApi;
use crate::store::GitStore;
use crate::store::instruction_source::InstructionError;
use serde_json::{Value, json};
use std::sync::Arc;

pub struct RosterLiveDeps {
    pub git: Arc<GitStore>,
    pub herdr: Option<Arc<dyn HerdrApi>>,
    pub seat_read: SeatReadState,
    pub lost: SeatLost,
}

/// Registers `roster.live`. Request `{scope: [words], all: bool, unmanaged: bool}`; reply
/// `{head, roster}` where `roster` is the `Roster` body with its live slots filled.
pub fn register(reg: &mut Registry, deps: RosterLiveDeps) {
    let deps = Arc::new(deps);
    reg.command("roster.live", move |_cx: CommandCtx, args: Value| {
        let d = deps.clone();
        async move { handle(&d, &args).await }
    });
}

/// The one `InstructionError` mapping of this handler: a store failure goes through `from_store` (an integrity
/// failure is exit 6, as in the daemon-free reader); every other variant is the caller's bad request.
fn instruction_error(e: InstructionError) -> CommandError {
    match e {
        InstructionError::Store(s) => CommandError::from_store(s),
        other => CommandError::bad_request(other.to_string()),
    }
}

async fn handle(d: &Arc<RosterLiveDeps>, args: &Value) -> Result<Value, CommandError> {
    let words: Vec<String> = args
        .get("scope")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|w| w.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let flag = |k: &str| args.get(k).and_then(Value::as_bool).unwrap_or(false);
    let (all, unmanaged) = (flag("all"), flag("unmanaged"));

    let git = d.git.clone();
    let (head, mut roster, bindings) = tokio::task::spawn_blocking(move || {
        let head = pin_head(&*git).map_err(CommandError::from_store)?;
        let arg = ScopeArg::parse(&words, AllowClone::No)
            .map_err(|e| CommandError::bad_request(e.to_string()))?;
        let scope = Scope::resolve(&arg, &*git, &head).map_err(instruction_error)?;
        let (roster, bindings) =
            build_with_bindings(&git, &head, &RosterQuery { scope, all }, unmanaged)
                .map_err(CommandError::from_store)?;
        Ok::<_, CommandError>((head, roster, bindings))
    })
    .await
    .map_err(|e| CommandError::internal(e.to_string()))??;

    // Exactly one snapshot; a failure is reported, never turned into "no agent".
    let snap = match &d.herdr {
        None => Err("no herdr client".to_owned()),
        Some(h) => h.snapshot().await.map_err(|e| e.to_string()),
    };
    // One snapshot of the last seat read; `Ok` only when that read succeeded, a read never made counts as
    // unavailable. `lost` is read while the snapshot lock is held.
    let threads = {
        let snap = d.seat_read.lock().unwrap_or_else(|e| e.into_inner());
        match &snap.last {
            None => ThreadsRead::Unavailable("no threads seat read yet".to_owned()),
            Some(Err(msg)) => ThreadsRead::Unavailable(msg.clone()),
            Some(Ok(())) => ThreadsRead::Ok {
                refusals: snap.refusals.clone(),
                lost: d.lost.lock().unwrap_or_else(|e| e.into_inner()).clone(),
            },
        }
    };
    overlay(
        &mut roster,
        &bindings,
        snap.as_ref().map_err(String::clone),
        threads,
        unmanaged,
    );
    Ok(json!({ "head": head.0, "roster": roster }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::registry::{CallerInfo, CommandCtx};
    use crate::herdr::fake::{FakeHerdr, Fault};
    use crate::inspect::fixture::Fx;
    use crate::model::{CloneLifecycle, Lifecycle};
    use crate::ports::herdr::CreateWorkspace;
    use crate::ports::store::{Store, StoreError, unresolved};

    struct Rig {
        fx: Fx,
        fake: Arc<FakeHerdr>,
        reg: Registry,
        read: SeatReadState,
    }

    async fn rig(with_herdr: bool) -> Rig {
        let mut fx = Fx::new();
        let t = fx.teamspace("proj", Lifecycle::Active);
        let s = fx.seat(&t, "foreman", Lifecycle::Active);
        fx.clone_rec(&s, "main", CloneLifecycle::Active, vec![], None);
        fx.commit("c");
        let fake = FakeHerdr::new();
        fake.create_workspace(CreateWorkspace {
            label: "other".into(),
            cwd: "/".into(),
            env: vec![],
        })
        .await
        .unwrap();
        let git = Arc::new(GitStore::open(fx.store.root()).unwrap());
        // Threads answered the last read, so the only degradation in a test is the one it makes.
        let read = SeatReadState::default();
        read.lock().unwrap().last = Some(Ok(()));
        let mut reg = Registry::default();
        register(
            &mut reg,
            RosterLiveDeps {
                git,
                herdr: with_herdr.then(|| fake.clone() as Arc<dyn HerdrApi>),
                seat_read: read.clone(),
                lost: SeatLost::default(),
            },
        );
        Rig {
            fx,
            fake,
            reg,
            read,
        }
    }

    async fn call(r: &Rig, args: Value) -> Result<Value, CommandError> {
        let cx = CommandCtx {
            request_id: "r".into(),
            caller: CallerInfo::default(),
        };
        r.reg.handler("roster.live").unwrap().call(cx, args).await
    }

    #[tokio::test]
    async fn one_call_takes_one_snapshot_and_adds_no_commit() {
        let r = rig(true).await;
        let before = r.fx.store.head().unwrap();
        let v = call(&r, json!({"scope": [], "all": false, "unmanaged": true}))
            .await
            .unwrap();
        assert_eq!(r.fake.snapshot_calls(), 1);
        assert_eq!(v["head"], json!(before.0));
        assert_eq!(r.fx.store.head().unwrap(), before, "no commit");
        let roster = &v["roster"];
        assert_eq!(roster["live_unavailable"], json!(null));
        let clone = &roster["teamspaces"][0]["seats"][0]["clones"][0];
        assert_eq!(clone["live"]["live_pane_matches"], json!(false));
        assert_eq!(roster["threads_live"]["seat_lost"], json!([]));
        assert_eq!(
            roster["unmanaged"]["workspaces"][0]["label"],
            json!("other")
        );
    }

    #[tokio::test]
    async fn snapshot_failure_and_missing_client_are_reported() {
        let r = rig(true).await;
        r.fake.fail_next("session.snapshot", Fault::Unavailable);
        let v = call(&r, json!({"scope": [], "unmanaged": true}))
            .await
            .unwrap();
        assert_eq!(r.fake.snapshot_calls(), 1);
        assert!(
            v["roster"]["live_unavailable"]["herdr"]
                .as_str()
                .unwrap()
                .contains("unavailable"),
            "{v}"
        );
        assert_eq!(
            v["roster"]["teamspaces"][0]["seats"][0]["clones"][0]["live"],
            json!(null)
        );
        assert_eq!(v["roster"]["unmanaged"], json!(null));

        let none = rig(false).await;
        let v = call(&none, json!({})).await.unwrap();
        assert_eq!(
            v["roster"]["live_unavailable"]["herdr"],
            json!("no herdr client")
        );
        assert_eq!(none.fake.snapshot_calls(), 0);
    }

    #[tokio::test]
    async fn failed_or_never_made_seat_read_names_threads_and_bad_scope_is_a_bad_request() {
        let r = rig(true).await;
        r.read.lock().unwrap().last = Some(Err("seat view rejected".into()));
        let v = call(&r, json!({})).await.unwrap();
        assert_eq!(
            v["roster"]["live_unavailable"],
            json!({"threads": "seat view rejected"})
        );
        assert_eq!(v["roster"]["threads_live"]["seat_refusals"], json!(null));
        // a transport failure (Disconnected) degrades the same way: only the last read counts
        r.read.lock().unwrap().last = Some(Err("disconnected: socket closed".into()));
        let v = call(&r, json!({})).await.unwrap();
        assert_eq!(
            v["roster"]["live_unavailable"],
            json!({"threads": "disconnected: socket closed"})
        );
        // never read: unavailable, not an empty-but-healthy report
        *r.read.lock().unwrap() = Default::default();
        let v = call(&r, json!({})).await.unwrap();
        assert_eq!(
            v["roster"]["live_unavailable"],
            json!({"threads": "no threads seat read yet"})
        );
        assert_eq!(v["roster"]["threads_live"]["seat_lost"], json!(null));
        // a later success clears it and reports that read's refusals
        {
            let mut snap = r.read.lock().unwrap();
            snap.last = Some(Ok(()));
            snap.refusals.insert("sn9".into(), "refused".into());
        }
        let v = call(&r, json!({})).await.unwrap();
        assert_eq!(v["roster"]["live_unavailable"], json!(null));
        assert_eq!(
            v["roster"]["threads_live"]["seat_refusals"],
            json!({"sn9": "refused"})
        );
        r.read.lock().unwrap().last = Some(Err("seat view rejected".into()));
        let e = call(&r, json!({"scope": ["seat", "nope"]}))
            .await
            .unwrap_err();
        assert_eq!(e.code, crate::ipc::IpcErrorCode::BadRequest);
        let e = call(&r, json!({"scope": ["clone", "x"]}))
            .await
            .unwrap_err();
        assert_eq!(e.code, crate::ipc::IpcErrorCode::BadRequest);
    }

    #[test]
    fn instruction_store_errors_route_through_from_store() {
        use crate::ipc::IpcErrorCode;
        let e = instruction_error(InstructionError::Store(unresolved(
            "st_a",
            "teamspace",
            "ts_b",
        )));
        assert_eq!(e.code, IpcErrorCode::Integrity);
        let e = instruction_error(InstructionError::Store(StoreError::Git("boom".into())));
        assert_eq!(e.code, IpcErrorCode::Internal);
        let e = instruction_error(InstructionError::Usage("x".into()));
        assert_eq!(e.code, IpcErrorCode::BadRequest);
    }

    #[tokio::test]
    async fn dangling_clone_seat_is_integrity() {
        let mut r = rig(true).await;
        let t = r.fx.teamspace("p2", Lifecycle::Active);
        let seat = r.fx.seat(&t, "other", Lifecycle::Active);
        let c =
            r.fx.clone_rec(&seat, "c2", CloneLifecycle::Active, vec![], None);
        r.fx.clone_mut(&c).seat = crate::model::SeatId::new();
        r.fx.commit("bad");
        let e = call(&r, json!({})).await.unwrap_err();
        assert_eq!(e.code, crate::ipc::IpcErrorCode::Integrity, "{}", e.message);
    }

    async fn two_seat_rig() -> Rig {
        let mut r = rig(false).await;
        let t = r.fx.teamspace("proj2", Lifecycle::Active);
        let s = r.fx.seat(&t, "aide", Lifecycle::Active);
        r.fx.clone_rec(&s, "side", CloneLifecycle::Active, vec![], None);
        let gone = r.fx.seat(&t, "gone", Lifecycle::Retired);
        r.fx.clone_rec(&gone, "old", CloneLifecycle::Retired, vec![], None);
        r.fx.commit("more");
        r
    }

    fn seat_names(v: &Value) -> Vec<String> {
        v["roster"]["teamspaces"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|t| t["seats"].as_array().unwrap())
            .map(|s| s["name"].as_str().unwrap().to_owned())
            .collect()
    }

    #[tokio::test]
    async fn scoped_roster_lists_only_that_seat() {
        let r = two_seat_rig().await;
        let v = call(&r, json!({"scope": ["seat", "foreman"]}))
            .await
            .unwrap();
        assert_eq!(seat_names(&v), vec!["foreman"]);
    }

    #[tokio::test]
    async fn all_includes_retired() {
        let r = two_seat_rig().await;
        let v = call(&r, json!({"all": false})).await.unwrap();
        let mut names = seat_names(&v);
        names.sort();
        assert_eq!(names, vec!["aide", "foreman"]);
        let v = call(&r, json!({"all": true})).await.unwrap();
        let mut names = seat_names(&v);
        names.sort();
        assert_eq!(names, vec!["aide", "foreman", "gone"]);
    }

    #[tokio::test]
    async fn loaded_record_status_reaches_the_reply() {
        use crate::bootstrap::instructions::{HOOK_RENDER_CAP, fit_hook, render_instructions};
        use crate::inspect::fixture::{session, ts};
        use crate::inspect::loaded::{self, InjectionSource};
        let mut r = rig(false).await;
        let t = r.fx.teamspace("proj3", Lifecycle::Active);
        let seat = r.fx.seat(&t, "occ", Lifecycle::Active);
        let sess = session(
            crate::model::harness::Harness::Claude,
            "native-1",
            "2026-11-01T00:00:00Z",
            None,
            None,
        );
        let ns = sess.id.clone();
        let cid =
            r.fx.clone_rec(&seat, "main", CloneLifecycle::Active, vec![sess], Some(ns));
        r.fx.commit("occupied");
        let git = GitStore::open(r.fx.store.root()).unwrap();
        let head = r.fx.store.head().unwrap();
        let status_of = |v: &Value| {
            v["roster"]["teamspaces"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|t| t["seats"].as_array().unwrap())
                .flat_map(|s| s["clones"].as_array().unwrap())
                .find(|c| c["id"] == json!(cid.to_string()))
                .unwrap()["instructions"]["status"]
                .clone()
        };
        let v = call(&r, json!({})).await.unwrap();
        assert_eq!(status_of(&v), json!("unknown"), "no record yet");
        let rendered = render_instructions(
            &crate::store::tree::CommitView {
                store: &git,
                at: head.clone(),
            },
            &head,
            &seat,
        )
        .unwrap();
        loaded::record(
            r.fx.store.root(),
            &cid,
            &rendered,
            &fit_hook(&rendered, HOOK_RENDER_CAP),
            InjectionSource::Hook,
            Some("native-1".into()),
            true,
            ts("2026-11-08T10:30:00Z"),
        );
        let v = call(&r, json!({})).await.unwrap();
        assert_eq!(status_of(&v), json!("current"), "{v}");
    }
}
