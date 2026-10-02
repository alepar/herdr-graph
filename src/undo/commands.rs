//! IPC handlers `undo.list` and `undo.apply` (spec §6, §10). Selecting an action and previewing its undo
//! reuse `plan.create` with the words `["undo", <act>]`; applying goes through `undo.apply`, which admits the
//! op and answers at once so the CLI can print the op id before the caller's own pane may close.
use super::adopt::admit_adopted_binding;
use super::candidates::{DEFAULT_LIMIT, list_candidates, render_list};
use crate::daemon::registry::{CommandCtx, CommandError, Registry};
use crate::plan::commands::{PlanDeps, admit_apply, wait_result};
use crate::plan::kind::KindRegistry;
use crate::plan::store::PlanStore;
use crate::ports::clock::Clock;
use crate::ports::store::Store;
use crate::ports::writer::Writer;
use crate::store::tree::CommitView;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;

pub struct UndoDeps {
    pub store: Arc<dyn Store>,
    pub kinds: Arc<KindRegistry>,
    pub plans: Arc<PlanStore>,
    pub writer: Arc<dyn Writer>,
    pub clock: Arc<dyn Clock>,
    pub instance: PathBuf,
}

impl UndoDeps {
    fn plan_deps(&self) -> PlanDeps {
        PlanDeps {
            kinds: self.kinds.clone(),
            plans: self.plans.clone(),
            store: self.store.clone(),
            writer: self.writer.clone(),
            clock: self.clock.clone(),
            instance: self.instance.clone(),
        }
    }
}

/// `undo.list` payload: `{candidates: [...], rendered: "<numbered text>"}`.
pub fn list_json(store: &dyn Store, limit: usize) -> Result<Value, CommandError> {
    let head = store.head().map_err(|e| CommandError::internal(e.to_string()))?;
    let view = CommitView { store, at: head };
    let list = list_candidates(&view, limit).map_err(|e| CommandError::internal(e.to_string()))?;
    Ok(json!({ "candidates": list, "rendered": render_list(&list) }))
}

/// Registers `undo.list` and `undo.apply`.
pub fn register_commands(reg: &mut Registry, deps: UndoDeps) {
    let deps = Arc::new(deps);
    let d = deps.clone();
    reg.command("undo.list", move |_cx: CommandCtx, args: Value| {
        let d = d.clone();
        async move {
            let limit = args.get("limit").and_then(Value::as_u64).map_or(DEFAULT_LIMIT, |n| n as usize);
            list_json(&*d.store, limit)
        }
    });
    let d = deps;
    reg.command("undo.apply", move |cx: CommandCtx, args: Value| {
        let d = d.clone();
        async move {
            let plan = args
                .get("plan")
                .and_then(Value::as_str)
                .ok_or_else(|| CommandError::bad_request("undo.apply needs {plan}"))?
                .to_owned();
            let confirm = args.get("confirm").and_then(Value::as_str).map(str::to_owned);
            let mode = args.get("mode").and_then(Value::as_str).unwrap_or("relay").to_owned();
            let op = {
                let d = d.clone();
                let caller = cx.caller.clone();
                tokio::task::spawn_blocking(move || {
                    admit_apply(&d.plan_deps(), &caller, &plan, confirm.as_deref(), &mode)
                })
                .await
                .map_err(|e| CommandError::internal(e.to_string()))??
            };
            // After the op commits, the adopted pane's binding is written (token stamped by the reconciler).
            let watcher = d.clone();
            let watched = op.clone();
            tokio::spawn(async move {
                let Ok(result) = wait_result(&watcher.plan_deps(), &watched).await else { return };
                if result["state"] == "committed"
                    && let Some(action) = result["action"].as_str().and_then(|a| a.parse().ok())
                {
                    let _ = admit_adopted_binding(&*watcher.store, &*watcher.writer, &action);
                }
            });
            Ok(json!({ "op": op, "state": "admitted" }))
        }
    });
}
