//! IPC handlers `undo.list` and `undo.apply` (spec §6, §10). Selecting an action and previewing its undo
//! reuse `plan.create` with the words `["undo", <act>]`; applying goes through `undo.apply`, which admits the
//! op and answers at once so the CLI can print the op id before the caller's own pane may close.
use super::adopt::{ADOPT_BINDING_KEY, caller_binding};
use super::candidates::{DEFAULT_LIMIT, list_candidates, render_list};
use crate::daemon::registry::{CommandCtx, CommandError, Registry};
use crate::plan::commands::{PlanDeps, admit_apply_with};
use crate::plan::kind::KindRegistry;
use crate::plan::store::PlanStore;
use crate::ports::clock::Clock;
use crate::ports::herdr::HerdrApi;
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
    /// Where the caller's pane is looked up when the undo is applied from a pane.
    pub herdr: Arc<dyn HerdrApi>,
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
    let head = store
        .head()
        .map_err(|e| CommandError::internal(e.to_string()))?;
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
            let limit = args
                .get("limit")
                .and_then(Value::as_u64)
                .map_or(DEFAULT_LIMIT, |n| n as usize);
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
            let confirm = args
                .get("confirm")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let mode = args
                .get("mode")
                .and_then(Value::as_str)
                .unwrap_or("relay")
                .to_owned();
            // The caller's pane as Herdr shows it now (workspace, tab, terminal, incarnation): the undo commits
            // the adopted clone's binding itself, so the reconciler never sees a restored clone without a pane.
            let mut extra = serde_json::Map::new();
            if let Some(pane) = cx.caller.pane_id.as_deref()
                && let Ok(snap) = d.herdr.snapshot().await
                && let Some(b) = caller_binding(&snap, pane)
            {
                extra.insert(
                    ADOPT_BINDING_KEY.into(),
                    serde_json::to_value(b).map_err(|e| CommandError::internal(e.to_string()))?,
                );
            }
            let op = {
                let d = d.clone();
                let caller = cx.caller.clone();
                tokio::task::spawn_blocking(move || {
                    admit_apply_with(
                        &d.plan_deps(),
                        &caller,
                        &plan,
                        confirm.as_deref(),
                        &mode,
                        extra,
                    )
                })
                .await
                .map_err(|e| CommandError::internal(e.to_string()))??
            };
            Ok(json!({ "op": op, "state": "admitted" }))
        }
    });
}
