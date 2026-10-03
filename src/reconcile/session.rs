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
use std::time::Duration;

/// Map a Herdr error of an idempotent call: unreachable or slow means retry later.
pub fn transient_or_failed(e: HerdrError) -> ExecOutcome {
    match e {
        HerdrError::Rejected { method, message } => {
            ExecOutcome::Failed(format!("{method}: {message}"))
        }
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
        KeyStep::SubmitText(t) => vec![
            KeyInput::Text(t.to_owned()),
            KeyInput::Key("enter".to_owned()),
        ],
    }
}

/// Where a replacement stands between loop steps; persisted in journal meta `replace:<effect id>`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum ReplacePhase {
    WaitingIdle {
        since: Timestamp,
        had_agent: bool,
    },
    Exiting {
        since: Timestamp,
        fallback_sent: bool,
        #[serde(default)]
        fallback_at: Option<Timestamp>,
    },
    /// Saved before `agent.start`: a start was dispatched and its outcome is not known until the pane is inspected.
    Starting {
        since: Timestamp,
    },
}

fn state_key(effect: &EffectId) -> String {
    format!("replace:{effect}")
}

fn load(journal: &Journal, effect: &EffectId) -> Option<ReplacePhase> {
    let raw = journal.meta_get(&state_key(effect)).ok().flatten()?;
    serde_json::from_str(&raw).ok()
}

fn save(journal: &Journal, effect: &EffectId, st: &ReplacePhase) {
    let _ = journal.meta_set(
        &state_key(effect),
        &serde_json::to_string(st).unwrap_or_default(),
    );
}

fn clear(journal: &Journal, effect: &EffectId) {
    let _ = journal.meta_delete(&state_key(effect));
}

/// Has the replacement of `effect` begun (a `replace:<effect>` phase is persisted)?
pub fn has_state(journal: &Journal, effect: &EffectId) -> bool {
    journal
        .meta_get(&state_key(effect))
        .ok()
        .flatten()
        .is_some()
}

/// Forget the replacement state of `effect` (it ended).
pub fn clear_state(journal: &Journal, effect: &EffectId) {
    clear(journal, effect);
}

fn elapsed(now: Timestamp, since: Timestamp, limit: std::time::Duration) -> bool {
    chrono::Duration::from_std(limit).is_ok_and(|l| now - since >= l)
}

async fn start(
    herdr: &dyn HerdrApi,
    journal: &Journal,
    effect: &EffectId,
    pane: &HerdrPaneId,
    to: Option<Relaunch<'_>>,
    now: Timestamp,
) -> ExecOutcome {
    // The target harness may run no agent.
    let Some(to) = to else {
        clear(journal, effect);
        return ExecOutcome::Done;
    };
    // Write-ahead: with this phase on record, a lost start outcome is recognised instead of repeated.
    save(journal, effect, &ReplacePhase::Starting { since: now });
    let started = herdr
        .start_agent(StartAgent {
            pane: pane.clone(),
            kind: to.kind.to_owned(),
            args: to.args,
        })
        .await;
    crate::failpoint!("reconcile.after_call.replace_session");
    start_outcome(started)
}

/// Next look for a time-bound replacement wait: the earliest of the polling period and the phase deadlines
/// still ahead.
fn recheck_at(cfg: &ReconcilerConfig, now: Timestamp, deadlines: &[Timestamp]) -> Timestamp {
    let poll = now + chrono::Duration::from_std(cfg.deferred_recheck).unwrap_or_default();
    deadlines
        .iter()
        .copied()
        .filter(|d| *d > now)
        .fold(poll, |a, d| a.min(d))
}

/// Advance the replacement of the occupant of `pane` (launched as `from`) by one loop step: at most one probe per
/// phase, no sleeping. `DeferredUntil` names when to ask again; `Done` means the new agent was started.
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
    let dur = |d: Duration| chrono::Duration::from_std(d).unwrap_or_default();
    let state = load(journal, effect).unwrap_or(ReplacePhase::WaitingIdle {
        since: now,
        had_agent: false,
    });
    if let ReplacePhase::Starting { .. } = state {
        // A start was dispatched and its outcome is unknown: an agent on the pane means it went through; a bare
        // shell means it never happened; anything else is the new agent coming up.
        match herdr.agent(pane).await {
            Ok(Some(_)) => return ExecOutcome::Done,
            Ok(None) => {}
            Err(e) => return transient_or_failed(e),
        }
        return match herdr.process_info(pane).await {
            Ok(p) if p.is_shell => start(herdr, journal, effect, pane, to, now).await,
            Ok(_) => ExecOutcome::DeferredUntil(
                recheck_at(cfg, now, &[]),
                "waiting for the started agent to appear".into(),
            ),
            Err(e) => transient_or_failed(e),
        };
    }
    if let ReplacePhase::WaitingIdle {
        since,
        had_agent: seen,
    } = state
    {
        // 1. Idle gate: never interrupt a working agent.
        let mut had_agent = seen;
        match herdr.agent(pane).await {
            Ok(None) => {}
            Ok(Some(a)) if matches!(a.status, AgentStatus::Idle | AgentStatus::Done) => {
                had_agent = true
            }
            Ok(Some(_)) => {
                if elapsed(now, since, cfg.idle_timeout) {
                    clear(journal, effect);
                    return ExecOutcome::NeedsRevision("occupant busy".into());
                }
                save(
                    journal,
                    effect,
                    &ReplacePhase::WaitingIdle {
                        since,
                        had_agent: true,
                    },
                );
                let at = recheck_at(cfg, now, &[since + dur(cfg.idle_timeout)]);
                return ExecOutcome::DeferredUntil(
                    at,
                    "waiting for the occupant to go idle".into(),
                );
            }
            Err(e) => return transient_or_failed(e),
        }
        // 2. Exit sequence, unless the pane is already back at the shell (a retry after a transient start failure).
        match herdr.process_info(pane).await {
            Ok(p) if p.is_shell && !had_agent => {
                return start(herdr, journal, effect, pane, to, now).await;
            }
            Ok(_) => {}
            Err(e) => return transient_or_failed(e),
        }
        let seq = profile(from).exit;
        for step in seq.iter().flat_map(|s| s.steps.iter().copied()) {
            if let Err(e) = herdr.send_keys(pane, &key_input(step)).await {
                return transient_or_failed(e);
            }
        }
        save(
            journal,
            effect,
            &ReplacePhase::Exiting {
                since: now,
                fallback_sent: false,
                fallback_at: None,
            },
        );
        return match herdr.process_info(pane).await {
            Ok(p) if p.is_shell => start(herdr, journal, effect, pane, to, now).await,
            Ok(_) => {
                let mut deadlines = vec![now + dur(cfg.exit_timeout)];
                if profile(from)
                    .exit
                    .and_then(|s| s.if_still_running)
                    .is_some()
                {
                    deadlines.push(now + dur(cfg.exit_followup));
                }
                ExecOutcome::DeferredUntil(
                    recheck_at(cfg, now, &deadlines),
                    "waiting for the occupant to exit".into(),
                )
            }
            Err(e) => transient_or_failed(e),
        };
    }

    // 3. Wait for the shell; after a short while send the fallback if the agent is still there.
    let ReplacePhase::Exiting {
        since,
        fallback_sent,
        fallback_at,
    } = state
    else {
        unreachable!("WaitingIdle handled above")
    };
    match herdr.process_info(pane).await {
        Ok(p) if p.is_shell => return start(herdr, journal, effect, pane, to, now).await,
        Ok(_) => {}
        Err(e) => return transient_or_failed(e),
    }
    // The follow-up goes out before the timeout is judged, so a late step never skips it.
    let fallback = profile(from).exit.and_then(|s| s.if_still_running);
    if let Some(step) = fallback
        && !fallback_sent
        && elapsed(now, since, cfg.exit_followup)
    {
        if let Err(e) = herdr.send_keys(pane, &key_input(step)).await {
            return transient_or_failed(e);
        }
        save(
            journal,
            effect,
            &ReplacePhase::Exiting {
                since,
                fallback_sent: true,
                fallback_at: Some(now),
            },
        );
        let at = recheck_at(
            cfg,
            now,
            &[now + dur(cfg.exit_followup), since + dur(cfg.exit_timeout)],
        );
        return ExecOutcome::DeferredUntil(at, "waiting for the occupant to exit".into());
    }
    let followup_settled = fallback.is_none()
        || !fallback_sent
        || fallback_at.is_none_or(|f| elapsed(now, f, cfg.exit_followup));
    if elapsed(now, since, cfg.exit_timeout) && followup_settled {
        clear(journal, effect);
        return ExecOutcome::NeedsRevision("occupant did not exit".into());
    }
    let mut deadlines = vec![since + dur(cfg.exit_timeout)];
    if fallback.is_some() && !fallback_sent {
        deadlines.push(since + dur(cfg.exit_followup));
    }
    if fallback_sent && let Some(f) = fallback_at {
        deadlines.push(f + dur(cfg.exit_followup));
    }
    ExecOutcome::DeferredUntil(
        recheck_at(cfg, now, &deadlines),
        "waiting for the occupant to exit".into(),
    )
}
