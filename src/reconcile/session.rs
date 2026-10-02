//! Session replacement procedure (spec §4.5): idle gate, exit sequence, shell wait, restart with
//! resume arguments, as a per-effect state machine advanced once per loop step on the injected clock (never
//! sleeping, so the shared step is not held). A busy occupant or a failed exit ends in `needs_revision` and
//! never starts a new agent.
use super::ReconcilerConfig;
use super::executor::ExecOutcome;
use crate::journal::Journal;
use crate::model::EffectId;
use crate::model::common::{HerdrPaneId, Timestamp};
use crate::model::harness::{Harness, KeyStep, StartOutcome, profile};
use crate::ports::herdr::{AgentStatus, HerdrApi, HerdrError, KeyInput, StartAgent};
use serde::{Deserialize, Serialize};

/// Map a Herdr error of an idempotent call: unreachable or slow means retry later.
pub fn transient_or_failed(e: HerdrError) -> ExecOutcome {
    match e {
        HerdrError::Rejected { method, message } => ExecOutcome::Failed(format!("{method}: {message}")),
        other => ExecOutcome::Transient(other.to_string()),
    }
}

/// Map the result of `agent.start`. A timeout is unknown: the caller must inspect a snapshot first.
pub fn start_outcome(r: Result<StartOutcome, HerdrError>) -> ExecOutcome {
    match r {
        Ok(StartOutcome::Started) => ExecOutcome::Done,
        Ok(StartOutcome::BlockedNeedsHuman) => ExecOutcome::BlockedNeedsHuman,
        Ok(StartOutcome::NeedsRevision { reason }) => ExecOutcome::NeedsRevision(reason),
        Ok(StartOutcome::Unknown) | Err(HerdrError::Timeout) => ExecOutcome::Unknown,
        Err(e) => transient_or_failed(e),
    }
}

/// What to relaunch with.
pub struct Relaunch<'a> {
    pub kind: &'a str,
    pub args: Vec<String>,
}

fn key_input(step: KeyStep) -> Vec<KeyInput> {
    match step {
        KeyStep::Key(k) => vec![KeyInput::Key(k.to_owned())],
        KeyStep::SubmitText(t) => vec![KeyInput::Text(t.to_owned()), KeyInput::Key("enter".to_owned())],
    }
}

/// Where a replacement stands between loop steps; persisted in journal meta `replace:<effect id>`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum ReplacePhase {
    WaitingIdle { since: Timestamp, had_agent: bool },
    Exiting { since: Timestamp, fallback_sent: bool },
}

fn state_key(effect: &EffectId) -> String {
    format!("replace:{effect}")
}

fn load(journal: &Journal, effect: &EffectId) -> Option<ReplacePhase> {
    let raw = journal.meta_get(&state_key(effect)).ok().flatten()?;
    serde_json::from_str(&raw).ok()
}

fn save(journal: &Journal, effect: &EffectId, st: &ReplacePhase) {
    let _ = journal.meta_set(&state_key(effect), &serde_json::to_string(st).unwrap_or_default());
}

fn clear(journal: &Journal, effect: &EffectId) {
    let _ = journal.meta_delete(&state_key(effect));
}

fn elapsed(now: Timestamp, since: Timestamp, limit: std::time::Duration) -> bool {
    chrono::Duration::from_std(limit).is_ok_and(|l| now - since >= l)
}

async fn start(herdr: &dyn HerdrApi, journal: &Journal, effect: &EffectId, pane: &HerdrPaneId, to: Option<Relaunch<'_>>) -> ExecOutcome {
    clear(journal, effect);
    // The target harness may run no agent.
    let Some(to) = to else { return ExecOutcome::Done };
    start_outcome(herdr.start_agent(StartAgent { pane: pane.clone(), kind: to.kind.to_owned(), args: to.args }).await)
}

/// Advance the replacement of the occupant of `pane` (launched as `from`) by one loop step: at most one probe per
/// phase, no sleeping. `Deferred` means "ask again next step"; `Done` means the new agent was started.
#[allow(clippy::too_many_arguments)]
pub async fn advance_replacement(
    herdr: &dyn HerdrApi,
    cfg: &ReconcilerConfig,
    journal: &Journal,
    effect: &EffectId,
    pane: &HerdrPaneId,
    from: Harness,
    to: Option<Relaunch<'_>>,
    now: Timestamp,
) -> ExecOutcome {
    let state = load(journal, effect).unwrap_or(ReplacePhase::WaitingIdle { since: now, had_agent: false });
    if let ReplacePhase::WaitingIdle { since, had_agent: seen } = state {
        // 1. Idle gate: never interrupt a working agent.
        let mut had_agent = seen;
        match herdr.agent(pane).await {
            Ok(None) => {}
            Ok(Some(a)) if matches!(a.status, AgentStatus::Idle | AgentStatus::Done) => had_agent = true,
            Ok(Some(_)) => {
                if elapsed(now, since, cfg.idle_timeout) {
                    clear(journal, effect);
                    return ExecOutcome::NeedsRevision("occupant busy".into());
                }
                save(journal, effect, &ReplacePhase::WaitingIdle { since, had_agent: true });
                return ExecOutcome::Deferred("waiting for the occupant to go idle".into());
            }
            Err(e) => return transient_or_failed(e),
        }
        // 2. Exit sequence, unless the pane is already back at the shell (a retry after a transient start failure).
        match herdr.process_info(pane).await {
            Ok(p) if p.is_shell && !had_agent => return start(herdr, journal, effect, pane, to).await,
            Ok(_) => {}
            Err(e) => return transient_or_failed(e),
        }
        let seq = profile(from).exit;
        for step in seq.iter().flat_map(|s| s.steps.iter().copied()) {
            if let Err(e) = herdr.send_keys(pane, &key_input(step)).await {
                return transient_or_failed(e);
            }
        }
        save(journal, effect, &ReplacePhase::Exiting { since: now, fallback_sent: false });
        return match herdr.process_info(pane).await {
            Ok(p) if p.is_shell => start(herdr, journal, effect, pane, to).await,
            Ok(_) => ExecOutcome::Deferred("waiting for the occupant to exit".into()),
            Err(e) => transient_or_failed(e),
        };
    }

    // 3. Wait for the shell; after a short while send the fallback if the agent is still there.
    let ReplacePhase::Exiting { since, fallback_sent } = state else { unreachable!("WaitingIdle handled above") };
    match herdr.process_info(pane).await {
        Ok(p) if p.is_shell => return start(herdr, journal, effect, pane, to).await,
        Ok(_) => {}
        Err(e) => return transient_or_failed(e),
    }
    if elapsed(now, since, cfg.exit_timeout) {
        clear(journal, effect);
        return ExecOutcome::NeedsRevision("occupant did not exit".into());
    }
    if !fallback_sent
        && elapsed(now, since, cfg.exit_followup)
        && let Some(step) = profile(from).exit.and_then(|s| s.if_still_running)
    {
        if let Err(e) = herdr.send_keys(pane, &key_input(step)).await {
            return transient_or_failed(e);
        }
        save(journal, effect, &ReplacePhase::Exiting { since, fallback_sent: true });
    }
    ExecOutcome::Deferred("waiting for the occupant to exit".into())
}
