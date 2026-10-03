//! IPC handlers `plan.create`, `plan.show`, `plan.apply` (spec §3.3, §10). There is no bypass: apply needs the
//! plan's own hash, and `mode: "tty"` is accepted only from a caller that is on a terminal.
use super::hash::plan_hash;
use super::kind::{KindRegistry, PlanCx, PlanError, make_plan};
use super::store::PlanStore;
use super::types::{Plan, Reserved, StoredPlan};
use crate::daemon::budget;
use crate::daemon::registry::{CallerInfo, CommandCtx, CommandError, Registry};
use crate::journal::Journal;
use crate::model::change::{ChangeRequest, ConfirmedPlan, Requester};
use crate::model::operation::{ConfirmMode, Confirmation, OpState};
use crate::model::seat::SeatRecord;
use crate::model::{CloneId, OpId, PlanId, SeatId};
use crate::ports::clock::Clock;
use crate::ports::store::Store;
use crate::ports::writer::{Writer, WriterError};
use crate::store::layout;
use crate::store::record::read_toml;
use crate::store::tree::CommitView;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

pub struct PlanDeps {
    pub kinds: Arc<KindRegistry>,
    pub plans: Arc<PlanStore>,
    pub store: Arc<dyn Store>,
    pub writer: Arc<dyn Writer>,
    pub clock: Arc<dyn Clock>,
    pub instance: PathBuf,
}

const APPLY_POLL: Duration = Duration::from_millis(50);

fn plan_err(e: PlanError) -> CommandError {
    match e {
        PlanError::Usage(m) | PlanError::Invalid(m) => CommandError::bad_request(m),
        PlanError::Store(s) => CommandError::internal(s.to_string()),
    }
}

fn io_err(e: std::io::Error) -> CommandError {
    CommandError::internal(format!("plan store: {e}"))
}

fn writer_err(e: WriterError) -> CommandError {
    match e {
        WriterError::Invalid(m) => CommandError::bad_request(m),
        other => CommandError::unavailable(other.to_string()),
    }
}

/// Text rendering used by `plan.create` and the CLI.
pub fn render(p: &Plan, hash: &str) -> String {
    let mut out = format!(
        "plan {} ({})\nhash {hash}\ncommitted rev {}\n",
        p.id,
        crate::writer::kind_name(p.request.kind),
        p.committed_rev.0
    );
    out.push_str("effects:\n");
    if p.effects.is_empty() {
        out.push_str("  (none)\n");
    }
    for (i, e) in p.effects.iter().enumerate() {
        out.push_str(&format!("  {}. {}\n", i + 1, e.describe()));
    }
    if !p.warnings.is_empty() {
        out.push_str("warnings:\n");
        for w in &p.warnings {
            out.push_str(&format!("  - {w}\n"));
        }
    }
    if let Some(r) = &p.repair_required {
        out.push_str(&format!(
            "repair required (this plan cannot be applied): {r}\n"
        ));
    }
    out
}

fn plan_json(s: &StoredPlan) -> Value {
    json!({
        "plan_id": s.plan.id,
        "hash": s.hash,
        "plan": s.plan,
        "rendered": render(&s.plan, &s.hash),
    })
}

/// Requester from the caller's environment: the seat/clone named by `HERDR_GRAPH_*` (when parseable) and
/// whether a human is at a terminal.
fn requester_for(deps: &PlanDeps, caller: &CallerInfo, human: bool) -> Requester {
    let seat = caller
        .graph_seat
        .as_deref()
        .and_then(|s| s.parse::<SeatId>().ok());
    let clone = caller
        .graph_clone
        .as_deref()
        .and_then(|s| s.parse::<CloneId>().ok());
    let mut teamspace = None;
    if let (Some(seat), Ok(head)) = (&seat, deps.store.head()) {
        let view = CommitView {
            store: &*deps.store,
            at: head,
        };
        if let Ok(Some(loc)) = layout::locate(&view, &seat.to_any())
            && let Ok(Some(rec)) = read_toml::<SeatRecord>(&view, &loc.record_path)
        {
            teamspace = Some(rec.teamspace);
        }
    }
    Requester {
        teamspace,
        seat,
        clone,
        native_session: None,
        human,
    }
}

/// Remove a `--supersedes <op>` (or `--supersedes=<op>`) pair from the change words.
fn take_supersedes(words: Vec<String>) -> Result<(Vec<String>, Option<OpId>), CommandError> {
    let mut out = Vec::new();
    let mut op = None;
    let mut it = words.into_iter();
    while let Some(w) = it.next() {
        let value =
            if w == "--supersedes" {
                Some(it.next().ok_or_else(|| {
                    CommandError::bad_request("--supersedes needs an operation id")
                })?)
            } else {
                w.strip_prefix("--supersedes=").map(str::to_owned)
            };
        match value {
            Some(v) => {
                op = Some(
                    v.parse::<OpId>()
                        .map_err(|e| CommandError::bad_request(format!("--supersedes: {e}")))?,
                );
            }
            None => out.push(w),
        }
    }
    Ok((out, op))
}

/// `plan.create`: parse the words with the registered kinds, plan against the head revision, store the plan.
pub fn create_plan(
    deps: &PlanDeps,
    caller: &CallerInfo,
    words: Vec<String>,
) -> Result<Value, CommandError> {
    let (words, supersedes) = take_supersedes(words)?;
    let (kind, rest) = deps.kinds.resolve(&words).map_err(plan_err)?;
    let args = kind.parse(&rest, caller).map_err(plan_err)?;
    let at = deps
        .store
        .head()
        .map_err(|e| CommandError::internal(e.to_string()))?;
    let view = CommitView {
        store: &*deps.store,
        at: at.clone(),
    };
    let pcx = PlanCx {
        tree: &view,
        at,
        caller,
        now: deps.clock.now(),
        instance: &deps.instance,
    };
    let plan = make_plan(&*kind, &pcx, args, Reserved::default()).map_err(plan_err)?;
    let stored = StoredPlan {
        hash: plan_hash(&plan),
        plan,
        created_at: deps.clock.now(),
        caller: caller.clone(),
        requester: requester_for(deps, caller, caller.tty),
        supersedes,
    };
    deps.plans.put(&stored).map_err(io_err)?;
    Ok(plan_json(&stored))
}

fn load_plan(deps: &PlanDeps, id: &str) -> Result<StoredPlan, CommandError> {
    let id: PlanId = id
        .parse()
        .map_err(|e| CommandError::bad_request(format!("plan id: {e}")))?;
    deps.plans
        .get(&id)
        .map_err(io_err)?
        .ok_or_else(|| CommandError::bad_request(format!("unknown plan {id}")))
}

/// `plan.show`.
pub fn show_plan(deps: &PlanDeps, plan: &str) -> Result<Value, CommandError> {
    Ok(plan_json(&load_plan(deps, plan)?))
}

/// `plan.apply`, admission half: validate the confirmation and admit the change request.
/// `relied_on` is left empty on purpose: the writer's apply recomputes the plan and rejects `stale_plan`
/// when the effects differ, so unrelated revision bumps of a relied-on object must not reject earlier.
pub fn admit_apply(
    deps: &PlanDeps,
    caller: &CallerInfo,
    plan: &str,
    confirm: Option<&str>,
    mode: &str,
) -> Result<OpId, CommandError> {
    admit_apply_with(deps, caller, plan, confirm, mode, serde_json::Map::new())
}

/// `admit_apply` with `extra` request arguments: observed inputs the handler gathered at admission (they are
/// facts, not confirmed effects, so they stay out of the plan hash).
pub fn admit_apply_with(
    deps: &PlanDeps,
    caller: &CallerInfo,
    plan: &str,
    confirm: Option<&str>,
    mode: &str,
    extra: serde_json::Map<String, Value>,
) -> Result<OpId, CommandError> {
    let stored = load_plan(deps, plan)?;
    let Some(confirm) = confirm else {
        return Err(CommandError::bad_request(
            "confirmation required; there is no bypass",
        ));
    };
    if confirm != stored.hash {
        return Err(CommandError::bad_request(format!(
            "confirmation required: {confirm:?} is not the hash of plan {}",
            stored.plan.id
        )));
    }
    let mode = match mode {
        "tty" if caller.tty => ConfirmMode::Tty,
        "tty" => {
            return Err(CommandError::bad_request(
                "tty confirmation requires a terminal; use the relay flow",
            ));
        }
        "relay" => ConfirmMode::Relay,
        other => {
            return Err(CommandError::bad_request(format!(
                "unknown confirmation mode {other:?}"
            )));
        }
    };
    if let Some(why) = &stored.plan.repair_required {
        return Err(CommandError::rejected(format!(
            "plan {} requires repair and cannot be applied: {why}",
            stored.plan.id
        )));
    }
    let confirmation = Confirmation {
        mode,
        plan_hash: stored.hash.clone(),
        at: deps.clock.now(),
    };
    let req = ChangeRequest {
        kind: stored.plan.request.kind,
        args: stored.plan.request.args.clone(),
        relied_on: vec![],
        requester: requester_for(deps, caller, mode == ConfirmMode::Tty),
        supersedes: stored.supersedes.clone(),
        confirmed: Some(ConfirmedPlan {
            plan: stored.plan.id.clone(),
            confirmation,
            observed: extra,
        }),
    };
    deps.writer.admit(req).map_err(writer_err)
}

fn terminal(s: OpState) -> bool {
    !matches!(s, OpState::Admitted | OpState::Applying)
}

/// `{op, state, commit?, rejection?}` for the op's current journal state (does not wait).
pub fn op_result(deps: &PlanDeps, op: &OpId) -> Result<Value, CommandError> {
    if let Ok(journal) = Journal::open(&Journal::path_in(&deps.instance))
        && let Ok(Some(row)) = journal.get(op)
    {
        let mut v = json!({ "op": op, "state": row.state });
        if let Some(c) = row.commit {
            v["commit"] = json!(c);
        }
        if let Some(r) = row.rejection {
            v["rejection"] = json!({ "reason": r.reason, "explanation": r.explanation });
        }
        if let Some(a) = row.action {
            v["action"] = json!(a);
        }
        return Ok(v);
    }
    let state = deps.writer.status(op).map_err(writer_err)?;
    Ok(json!({ "op": op, "state": state }))
}

/// Poll the writer every 50 ms until the op is terminal or the request deadline passes, then report it
/// (state `admitted`/`applying` when it is still running).
pub async fn wait_result(deps: &PlanDeps, op: &OpId) -> Result<Value, CommandError> {
    let deadline = budget::wait_until(Duration::from_secs(25));
    loop {
        match deps.writer.status(op).map_err(writer_err)? {
            Some(s) if terminal(s) => break,
            _ if tokio::time::Instant::now() >= deadline => break,
            _ => tokio::time::sleep(APPLY_POLL).await,
        }
    }
    op_result(deps, op)
}

fn words_of(args: &Value) -> Result<Vec<String>, CommandError> {
    args.get("words")
        .and_then(|w| w.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_owned))
                .collect()
        })
        .ok_or_else(|| CommandError::bad_request("plan.create needs {words: [...]}"))
}

fn str_arg<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(|v| v.as_str())
}

/// Registers `plan.create`, `plan.show` and `plan.apply`.
pub fn register_commands(reg: &mut Registry, deps: PlanDeps) {
    let deps = Arc::new(deps);
    let d = deps.clone();
    reg.command("plan.create", move |cx: CommandCtx, args: Value| {
        let d = d.clone();
        async move {
            let words = words_of(&args)?;
            tokio::task::spawn_blocking(move || create_plan(&d, &cx.caller, words))
                .await
                .map_err(|e| CommandError::internal(e.to_string()))?
        }
    });
    let d = deps.clone();
    reg.command("plan.show", move |_cx: CommandCtx, args: Value| {
        let d = d.clone();
        async move {
            let plan = str_arg(&args, "plan")
                .ok_or_else(|| CommandError::bad_request("plan.show needs {plan}"))?;
            show_plan(&d, plan)
        }
    });
    let d = deps;
    reg.command("plan.apply", move |cx: CommandCtx, args: Value| {
        let d = d.clone();
        async move {
            let plan = str_arg(&args, "plan")
                .ok_or_else(|| CommandError::bad_request("plan.apply needs {plan}"))?
                .to_owned();
            let confirm = str_arg(&args, "confirm").map(str::to_owned);
            let mode = str_arg(&args, "mode").unwrap_or("relay").to_owned();
            let op = {
                let d = d.clone();
                let caller = cx.caller.clone();
                tokio::task::spawn_blocking(move || {
                    admit_apply(&d, &caller, &plan, confirm.as_deref(), &mode)
                })
                .await
                .map_err(|e| CommandError::internal(e.to_string()))??
            };
            wait_result(&d, &op).await
        }
    });
}
