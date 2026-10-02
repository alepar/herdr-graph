//! Session replacement procedure (spec §4.5): idle gate, exit sequence, shell wait, restart with
//! resume arguments. A busy occupant or a failed exit ends in `needs_revision` and never starts a new agent.
use super::ReconcilerConfig;
use super::executor::ExecOutcome;
use crate::model::common::HerdrPaneId;
use crate::model::harness::{Harness, KeyStep, StartOutcome, profile};
use crate::ports::herdr::{AgentStatus, HerdrApi, HerdrError, KeyInput, StartAgent};
use tokio::time::{Instant, sleep};

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

/// Replace the occupant of `pane`, launched as `from`, with `to`. `Done` means the new agent was started.
pub async fn replace_session(
    herdr: &dyn HerdrApi,
    cfg: &ReconcilerConfig,
    pane: &HerdrPaneId,
    from: Harness,
    to: Option<Relaunch<'_>>,
) -> ExecOutcome {
    // 1. Idle gate: never interrupt a working agent.
    let deadline = Instant::now() + cfg.idle_timeout;
    let mut had_agent = false;
    loop {
        match herdr.agent(pane).await {
            Ok(None) => break,
            Ok(Some(a)) if matches!(a.status, AgentStatus::Idle | AgentStatus::Done) => {
                had_agent = true;
                break;
            }
            Ok(Some(_)) => had_agent = true,
            Err(e) => return transient_or_failed(e),
        }
        if Instant::now() >= deadline {
            return ExecOutcome::NeedsRevision("occupant busy".into());
        }
        sleep(cfg.poll_interval).await;
    }

    // 2. Exit sequence, unless the pane is already back at the shell (a retry after a transient start failure).
    let already_shell = match herdr.process_info(pane).await {
        Ok(p) => p.is_shell && !had_agent,
        Err(e) => return transient_or_failed(e),
    };
    if !already_shell {
        let seq = profile(from).exit;
        for step in seq.iter().flat_map(|s| s.steps.iter().copied()) {
            if let Err(e) = herdr.send_keys(pane, &key_input(step)).await {
                return transient_or_failed(e);
            }
        }
        // 3. Wait for the shell; after a short while send the fallback if the agent is still there.
        let started = Instant::now();
        let deadline = started + cfg.exit_timeout;
        let mut fallback_sent = false;
        loop {
            match herdr.process_info(pane).await {
                Ok(p) if p.is_shell => break,
                Ok(_) => {}
                Err(e) => return transient_or_failed(e),
            }
            if Instant::now() >= deadline {
                return ExecOutcome::NeedsRevision("occupant did not exit".into());
            }
            if !fallback_sent
                && started.elapsed() >= cfg.exit_followup
                && let Some(step) = seq.and_then(|s| s.if_still_running)
            {
                fallback_sent = true;
                if let Err(e) = herdr.send_keys(pane, &key_input(step)).await {
                    return transient_or_failed(e);
                }
            }
            sleep(cfg.poll_interval).await;
        }
    }

    // 4. New agent with resume arguments (none when the target harness runs no agent).
    let Some(to) = to else { return ExecOutcome::Done };
    start_outcome(herdr.start_agent(StartAgent { pane: pane.clone(), kind: to.kind.to_owned(), args: to.args }).await)
}
