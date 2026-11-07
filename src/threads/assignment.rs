//! ThreadsSeatAssignment (spec §2, §8, §13, §14): the write-once `threads_seat` of a clone and every
//! transition of its `seat_intent`. No other code writes either field; callers use the helpers below or
//! admit `bookkeeping.threads_seat`. This module makes no threads calls.
use crate::model::clone::{CloneRecord, KeyReservation, SeatIntent};
use crate::model::common::{CloneLifecycle, Runtime};
use crate::model::{CloneId, OpId};
use crate::ports::writer::{Writer, WriterError};
use crate::store::layout;
use crate::writer::{Applied, Mutation, MutationCx, MutationError, Reject};
use serde::{Deserialize, Serialize};

pub const THREADS_SEAT_KEY: &str = "bookkeeping.threads_seat";

/// One transition of a clone's threads seat or seat intent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum SeatTransition {
    /// Bind the clone to a threads seat (write-once).
    Assign { seat: String },
    /// Replace the final-retired seat `from` with `seat`, against the matching Restore intent.
    Succeed {
        seat: String,
        from: String,
        epoch: OpId,
    },
    /// Threads acknowledged the Restore of `epoch`: `threads_done = true`.
    Acknowledge { epoch: OpId },
    /// Threads refused the Restore's threads step of `epoch`: remember why, for status.
    RecordRefusal { epoch: OpId, reason: String },
    /// The executor is about to dispatch StartAgent for `epoch`; allowed once per epoch.
    MarkLaunched { epoch: OpId },
    /// The restored clone reattached: drop the finished Restore intent.
    ClearOnReattach { epoch: OpId },
    /// Threads acknowledged a Finalize/ReleaseKey: delete the tombstone record.
    TombstoneDone,
    /// A ReleaseKey tombstone's key turned out to be bound to `seat`: the seat's finalization releases it, so
    /// the tombstone becomes `Finalize{seat}` (§14.3).
    TombstoneFinalize { seat: String },
    /// Threads reserved the continuity key for this clone (`seat`: the seat it reported the key bound to).
    /// Records the reservation, or updates the binding of the recorded one; the key never changes.
    RecordReservation { key: String, seat: Option<String> },
}

/// Admit a transition for `clone`.
pub fn admit_threads_seat(
    writer: &dyn Writer,
    clone: &CloneId,
    t: &SeatTransition,
) -> Result<(), WriterError> {
    writer
        .admit(crate::model::change::ChangeRequest {
            kind: crate::model::change::RequestKind::Bookkeeping,
            args: serde_json::json!({ "sub": "threads_seat", "clone": clone, "transition": t }),
            relied_on: vec![],
            requester: crate::model::change::Requester::default(),
            supersedes: None,
            confirmed: None,
        })
        .map(|_| ())
}

/// Called by the undo/resurrection commit: a Restore intent for a seated clone, nothing for a seatless one
/// (§14.5). An older Restore is overwritten by the new epoch. The held `seat_fact` is dropped: observation
/// writes facts only for active clones, so a fact read before the clone was retired is stale, and succession
/// must act only on a read taken after this commit.
pub fn write_restore_intent(rec: &mut CloneRecord, epoch: &OpId) {
    if let Some(seat) = &rec.threads_seat {
        rec.seat_fact = None;
        rec.seat_intent = Some(SeatIntent::Restore {
            seat: seat.clone(),
            epoch: epoch.clone(),
            threads_done: false,
            launched: None,
            last_refusal: None,
        });
    }
}

/// Called by every commit that retires a clone: a Restore intent does not outlive the incarnation (§13, §14.6).
pub fn clear_restore_on_retire(rec: &mut CloneRecord) {
    if matches!(rec.seat_intent, Some(SeatIntent::Restore { .. })) {
        rec.seat_intent = None;
    }
}

/// Called by the purge commit: the inert tombstone to keep in place of `rec`, or None to delete the record.
/// A seated clone always keeps `Finalize{seat}`, whatever its retirement kind, and the write needs no threads
/// read (§14.2). A seatless one keeps a ReleaseKey only when it has a key reservation (§14.3).
pub fn purge_tombstone(rec: &CloneRecord, reservation: Option<&str>) -> Option<CloneRecord> {
    let seat_intent = match (&rec.threads_seat, reservation) {
        (Some(seat), _) => SeatIntent::Finalize { seat: seat.clone() },
        (None, None) => return None,
        (None, Some(key)) => SeatIntent::ReleaseKey {
            key: key.to_owned(),
        },
    };
    Some(CloneRecord {
        schema: rec.schema,
        id: rec.id.clone(),
        rev: rec.rev,
        seat: rec.seat.clone(),
        name: rec.name.clone(),
        name_history: rec.name_history.clone(),
        lifecycle: CloneLifecycle::Retired,
        retired: rec.retired.clone(),
        runtime: Runtime::default(),
        occupant: None,
        sessions: vec![],
        opt_outs: vec![],
        invitations: vec![],
        reload_required: false,
        threads_seat: rec.threads_seat.clone(),
        threads_seat_history: rec.threads_seat_history.clone(),
        seat_intent: Some(seat_intent),
        seat_fact: None,
        key_reservation: None,
    })
}

#[derive(Deserialize)]
struct Args {
    clone: CloneId,
    transition: SeatTransition,
}

fn reject(reason: &str, explanation: String) -> MutationError {
    MutationError::Reject(Reject {
        reason: reason.into(),
        explanation,
        current_revs: vec![],
    })
}

/// The live (not retired, not tombstone) clone other than `me` that holds `seat`, if any.
fn holder_elsewhere(
    cx: &MutationCx<'_>,
    me: &CloneId,
    seat: &str,
) -> Result<Option<CloneId>, MutationError> {
    Ok(layout::all_clones(&cx.tree)?
        .into_iter()
        .map(|(_, c)| c)
        .find(|c| {
            &c.id != me
                && c.lifecycle != CloneLifecycle::Retired
                && !c.is_tombstone()
                && c.threads_seat.as_deref() == Some(seat)
        })
        .map(|c| c.id))
}

fn check_free(cx: &MutationCx<'_>, me: &CloneId, seat: &str) -> Result<(), MutationError> {
    match holder_elsewhere(cx, me, seat)? {
        Some(other) => Err(reject(
            "threads_seat_held",
            format!("threads seat {seat} is held by live clone {other}"),
        )),
        None => Ok(()),
    }
}

/// `bookkeeping.threads_seat`.
pub struct ThreadsSeatMutation;

impl Mutation for ThreadsSeatMutation {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError> {
        let a: Args = serde_json::from_value(cx.request.args.clone())
            .map_err(|e| MutationError::Bug(e.to_string()))?;
        let any = a.clone.to_any();
        let gone = || {
            reject(
                "object_missing",
                format!("{any} does not exist at the committed head"),
            )
        };
        let loc = cx.tree.locate(&any)?.ok_or_else(gone)?;
        let mut rec: CloneRecord = cx.tree.read_record(&loc.record_path)?.ok_or_else(gone)?;
        let before = rec.clone();
        let summary = format!("threads seat {:?} on {}", a.transition, a.clone);

        match &a.transition {
            SeatTransition::Assign { seat } => match &rec.threads_seat {
                Some(cur) if cur == seat => {}
                Some(cur) => {
                    return Err(reject(
                        "threads_seat_conflict",
                        format!(
                            "clone {} is bound to threads seat {cur}, not {seat}",
                            a.clone
                        ),
                    ));
                }
                None => {
                    check_free(cx, &a.clone, seat)?;
                    rec.threads_seat = Some(seat.clone());
                }
            },
            SeatTransition::Succeed { seat, from, epoch } => {
                let already = rec.threads_seat.as_deref() == Some(seat)
                    && rec.threads_seat_history.last() == Some(from)
                    && matches!(
                        &rec.seat_intent,
                        Some(SeatIntent::Restore { seat: s, epoch: e, threads_done: true, .. })
                            if s == seat && e == epoch
                    );
                if !already {
                    let admitted = rec.threads_seat.as_deref() == Some(from)
                        && matches!(
                            &rec.seat_intent,
                            Some(SeatIntent::Restore { seat: s, epoch: e, threads_done: false, .. })
                                if s == from && e == epoch
                        );
                    if !admitted {
                        return Err(reject(
                            "seat_intent_mismatch",
                            format!(
                                "no Restore intent for seat {from} at epoch {epoch} on clone {}",
                                a.clone
                            ),
                        ));
                    }
                    check_free(cx, &a.clone, seat)?;
                    rec.threads_seat_history.push(from.clone());
                    rec.threads_seat = Some(seat.clone());
                    rec.seat_intent = Some(SeatIntent::Restore {
                        seat: seat.clone(),
                        epoch: epoch.clone(),
                        threads_done: true,
                        launched: None,
                        last_refusal: None,
                    });
                }
            }
            SeatTransition::Acknowledge { epoch } => {
                if let Some(SeatIntent::Restore {
                    epoch: e,
                    threads_done,
                    last_refusal,
                    ..
                }) = &mut rec.seat_intent
                    && e == epoch
                {
                    *threads_done = true;
                    *last_refusal = None;
                }
            }
            SeatTransition::RecordRefusal { epoch, reason } => {
                if let Some(SeatIntent::Restore {
                    epoch: e,
                    threads_done: false,
                    last_refusal,
                    ..
                }) = &mut rec.seat_intent
                    && e == epoch
                {
                    *last_refusal = Some(reason.clone());
                }
            }
            SeatTransition::MarkLaunched { epoch } => match &mut rec.seat_intent {
                Some(SeatIntent::Restore {
                    epoch: e,
                    launched: Some(_),
                    ..
                }) if e == epoch => {
                    return Err(reject(
                        "already_launched",
                        format!("epoch {epoch} was already marked launched on {}", a.clone),
                    ));
                }
                Some(SeatIntent::Restore {
                    epoch: e,
                    threads_done: true,
                    launched,
                    ..
                }) if e == epoch => *launched = Some(cx.now),
                _ => {
                    return Err(reject(
                        "seat_intent_mismatch",
                        format!(
                            "no acknowledged Restore intent at epoch {epoch} on clone {}",
                            a.clone
                        ),
                    ));
                }
            },
            SeatTransition::ClearOnReattach { epoch } => {
                if matches!(
                    &rec.seat_intent,
                    Some(SeatIntent::Restore { epoch: e, threads_done: true, .. }) if e == epoch
                ) {
                    rec.seat_intent = None;
                }
            }
            SeatTransition::RecordReservation { key, seat } => {
                if rec.is_tombstone() {
                    return Err(reject(
                        "tombstone",
                        format!("clone {} is a purge tombstone", a.clone),
                    ));
                }
                match &mut rec.key_reservation {
                    None => {
                        rec.key_reservation = Some(KeyReservation {
                            key: key.clone(),
                            seat: seat.clone(),
                            at: cx.now,
                        });
                    }
                    Some(held) if &held.key == key => held.seat = seat.clone(),
                    Some(held) => {
                        return Err(reject(
                            "key_reservation_conflict",
                            format!("clone {} holds key {}, not {key}", a.clone, held.key),
                        ));
                    }
                }
            }
            SeatTransition::TombstoneFinalize { seat } => match &rec.seat_intent {
                Some(SeatIntent::ReleaseKey { .. }) => {
                    rec.seat_intent = Some(SeatIntent::Finalize { seat: seat.clone() });
                }
                Some(SeatIntent::Finalize { seat: s }) if s == seat => {}
                _ => {
                    return Err(reject(
                        "not_a_release_key_tombstone",
                        format!("clone {} is not a ReleaseKey tombstone", a.clone),
                    ));
                }
            },
            SeatTransition::TombstoneDone => {
                if !rec.is_tombstone() {
                    return Err(reject(
                        "not_a_tombstone",
                        format!("clone {} is not a purge tombstone", a.clone),
                    ));
                }
                cx.tree.delete_file(&loc.record_path);
                return Ok(Applied {
                    summary,
                    action: None,
                });
            }
        }
        if rec != before {
            cx.tree.put_record(loc.record_path, &mut rec)?;
        }
        Ok(Applied {
            summary,
            action: None,
        })
    }
}
