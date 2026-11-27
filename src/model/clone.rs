//! Clone record (spec §2.4): a seat's live incarnation(s) and their session history.
use crate::model::HerdrPaneId;
use crate::model::common::{
    CloneLifecycle, NameChange, Occupant, Retirement, Runtime, Timestamp, is_false,
};
use crate::model::ids::{CloneId, OpId, SeatId};
use crate::model::native_session::NativeSession;
use serde::{Deserialize, Serialize};

/// Deserialized through `impl Deserialize` below, which settles a legacy seat fact's `keyed` from `keyless`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(remote = "Self")]
pub struct CloneRecord {
    pub schema: u32,
    pub id: CloneId,
    pub rev: u64,
    pub seat: SeatId,
    pub name: String,
    #[serde(default)]
    pub name_history: Vec<NameChange>,
    pub lifecycle: CloneLifecycle,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retired: Option<Retirement>,
    #[serde(default)]
    pub runtime: Runtime,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occupant: Option<Occupant>,
    #[serde(default)]
    pub sessions: Vec<NativeSession>,
    #[serde(default)]
    pub opt_outs: Vec<String>,
    #[serde(default)]
    pub invitations: Vec<Invitation>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub reload_required: bool,
    /// The herdr-threads seat this clone is bound to (spec §2). Written only by ThreadsSeatAssignment
    /// (`bookkeeping.threads_seat`): write-once, except seat succession against a matching Restore intent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threads_seat: Option<String>,
    /// Seats this clone held before a succession, oldest first (history only).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub threads_seat_history: Vec<String>,
    /// Graph-initiated threads lifecycle intent (spec §8, §14). Written only by ThreadsSeatAssignment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seat_intent: Option<SeatIntent>,
    /// What threads last said about `threads_seat` (spec §14.9). Written only by SeatObservation
    /// (`bookkeeping.seat_fact`); the key-conflict alert level is `state == KeyConflict`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seat_fact: Option<SeatFact>,
    /// The continuity key the graph reserved with threads for this clone (spec §5, §14). Written only by
    /// ThreadsSeatAssignment (`RecordReservation`); a succession leaves it in place (§16.1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_reservation: Option<KeyReservation>,
    /// Adopted clone (spec G4): graph never reserves or links a continuity key for it and never launches it
    /// with one; threads' own continuity keeps its seat.
    #[serde(default, skip_serializing_if = "is_false")]
    pub keyless: bool,
}

impl Serialize for CloneRecord {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        CloneRecord::serialize(self, s)
    }
}

impl<'de> Deserialize<'de> for CloneRecord {
    /// A retired `seat_fact` written before `keyed` existed takes it from the clone: the old graph against an
    /// upgraded threads daemon recorded a keyless seat's reversible close too, so `keyed = !keyless`.
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let mut v = serde_json::Value::deserialize(d)?;
        let keyless = v.get("keyless").and_then(|k| k.as_bool()).unwrap_or(false);
        if let Some(fact) = v.get_mut("seat_fact").and_then(|f| f.as_object_mut())
            && fact.get("state").and_then(|s| s.as_str()) == Some("retired")
            && !fact.contains_key("keyed")
        {
            fact.insert("keyed".into(), (!keyless).into());
        }
        CloneRecord::deserialize(v).map_err(D::Error::custom)
    }
}

/// A continuity key reserved for a clone. `seat` is the reservation's recorded binding: the seat threads last
/// answered the key is bound to (`None`: reserved, unbound). The key-link want (spec §14.12) compares it with
/// the clone's target seat, and the restore launch is gated on it (spec §14.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyReservation {
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seat: Option<String>,
    pub at: Timestamp,
}

impl KeyReservation {
    /// Whether the recorded binding names `seat`.
    pub fn bound_to(&self, seat: &str) -> bool {
        self.seat.as_deref() == Some(seat)
    }
}

/// One successful threads read of a clone's seat, with its read time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeatFact {
    pub seat: String,
    #[serde(flatten)]
    pub state: SeatFactState,
    pub read_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SeatFactState {
    Resolved {
        pane: HerdrPaneId,
    },
    Unresolved,
    KeyConflict {
        pane: Option<HerdrPaneId>,
    },
    Retired {
        reversible: bool,
        /// A fact written before this field existed reads `true` on its own (threads 3.1.0 reversed only a keyed
        /// seat's close; final facts take succession whatever the flag says). Inside a `CloneRecord` the
        /// clone's `keyless` decides instead (see its `Deserialize`).
        #[serde(default = "legacy_keyed")]
        keyed: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retired_at: Option<Timestamp>,
    },
}

fn legacy_keyed() -> bool {
    true
}

/// A `Restore` launch mark written as `launched = true` by older code: launched at an unknown time, earlier
/// than any incarnation change.
pub const LEGACY_LAUNCHED_AT: Timestamp = chrono::DateTime::UNIX_EPOCH;

/// Reads `launched` as the legacy bool (`true` is `LEGACY_LAUNCHED_AT`, `false` is unlaunched) or a timestamp.
fn launched_compat<'de, D>(d: D) -> Result<Option<Timestamp>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Wire {
        Legacy(bool),
        At(Option<Timestamp>),
    }
    Ok(match Wire::deserialize(d)? {
        Wire::Legacy(true) => Some(LEGACY_LAUNCHED_AT),
        Wire::Legacy(false) => None,
        Wire::At(at) => at,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SeatIntent {
    /// Written by an undo/resurrection commit; `epoch` is that commit's operation id.
    Restore {
        seat: String,
        epoch: OpId,
        #[serde(default)]
        threads_done: bool,
        /// When the launch mark was written; `None` until the epoch's one launch is marked. Records written as
        /// `launched = true` by older code read as `LEGACY_LAUNCHED_AT`.
        #[serde(
            default,
            deserialize_with = "launched_compat",
            skip_serializing_if = "Option::is_none"
        )]
        launched: Option<Timestamp>,
        /// The last threads refusal of this epoch's threads step, for status (§14.7).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        last_refusal: Option<String>,
    },
    /// Purge tombstone of a clone that holds a seat (§14.2).
    Finalize { seat: String },
    /// Purge tombstone of a seatless clone with a recorded key reservation (§14.3).
    ReleaseKey { key: String },
}

impl CloneRecord {
    /// A purged clone kept only to drive a finalize/release want (§13 "Tombstones are inert").
    pub fn is_tombstone(&self) -> bool {
        matches!(
            self.seat_intent,
            Some(SeatIntent::Finalize { .. } | SeatIntent::ReleaseKey { .. })
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invitation {
    pub thread: String,
    pub constraint: InviteConstraint,
    pub state: InvitationState,
    /// Which threads seat was invited (attributed to which occupant), and the ids threads reported (hg-zmi.11).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<ThreadsLink>,
}

/// Threads-side identity of an invitation: enough to release a requirement after the occupant is gone and to
/// print the exact `accept-required` command (spec §7.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadsLink {
    /// The threads seat (mapped from the clone's pane) that was invited.
    pub seat: String,
    /// Native session record of the occupant at the time of the invitation: attribution only. Nothing
    /// branches on it; the invitation belongs to the clone's `threads_seat` (`seat` above).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occupant: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invitation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requirement: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InviteConstraint {
    Required,
    Ordinary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvitationState {
    Pending,
    Accepted,
    Released,
    Retired,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keyless_clone(keyless: bool, keyed: bool) -> CloneRecord {
        CloneRecord {
            schema: 1,
            id: CloneId::new(),
            rev: 1,
            seat: SeatId::new(),
            name: "c".into(),
            name_history: vec![],
            lifecycle: CloneLifecycle::Active,
            retired: None,
            runtime: Runtime::default(),
            occupant: None,
            sessions: vec![],
            opt_outs: vec![],
            invitations: vec![],
            reload_required: false,
            threads_seat: Some("s1".into()),
            threads_seat_history: vec![],
            seat_intent: None,
            seat_fact: Some(SeatFact {
                seat: "s1".into(),
                state: SeatFactState::Retired {
                    reversible: true,
                    keyed,
                    retired_at: Some(chrono::DateTime::from_timestamp_millis(5_000).unwrap()),
                },
                read_at: chrono::DateTime::from_timestamp_millis(6_000).unwrap(),
            }),
            key_reservation: None,
            keyless,
        }
    }

    /// The record as a build before `keyed` existed wrote it: the same TOML without the `keyed` line.
    fn legacy_parse(c: &CloneRecord) -> CloneRecord {
        let text = toml::to_string(c).unwrap();
        assert!(text.contains("\nkeyed = "), "{text}");
        let legacy: String = text
            .lines()
            .filter(|l| !l.starts_with("keyed = "))
            .map(|l| format!("{l}\n"))
            .collect();
        toml::from_str(&legacy).unwrap_or_else(|e| panic!("{e}\n{legacy}"))
    }

    fn keyed_of(c: &CloneRecord) -> bool {
        match c.seat_fact.as_ref().map(|f| &f.state) {
            Some(SeatFactState::Retired { keyed, .. }) => *keyed,
            other => panic!("not retired: {other:?}"),
        }
    }

    #[test]
    fn legacy_retired_fact_takes_keyed_from_the_clone() {
        // Keyless clone: a missing `keyed` reads false, whatever the in-memory value was.
        assert!(!keyed_of(&legacy_parse(&keyless_clone(true, true))));
        // Keyed (keyless absent): a missing `keyed` reads true.
        assert!(keyed_of(&legacy_parse(&keyless_clone(false, false))));
        // An explicit `keyed = true` on a keyless clone is kept: only a missing field is defaulted.
        let c = keyless_clone(true, true);
        let back: CloneRecord = toml::from_str(&toml::to_string(&c).unwrap()).unwrap();
        assert!(keyed_of(&back));
    }

    #[test]
    fn current_clone_record_round_trips() {
        for (keyless, keyed) in [(true, false), (true, true), (false, true), (false, false)] {
            let c = keyless_clone(keyless, keyed);
            let text = toml::to_string(&c).unwrap();
            assert_eq!(toml::from_str::<CloneRecord>(&text).unwrap(), c, "{text}");
            let json = serde_json::to_string(&c).unwrap();
            assert_eq!(serde_json::from_str::<CloneRecord>(&json).unwrap(), c);
        }
    }
}
