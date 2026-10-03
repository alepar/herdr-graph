//! Plan, effect and plan-time-id types (spec §3.3).
use crate::daemon::registry::CallerInfo;
use crate::model::change::{ReliedOn, RequestKind, Requester};
use crate::model::{
    ActionId, AnyId, AppId, CloneId, CommitId, MemberId, OpId, PlanId, SeatId, TeamspaceId,
    TemplateId, Timestamp,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One desired-state delta a plan will perform; the reconciler realizes the `runtime.*` ones.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanEffect {
    pub kind: String,
    pub object: AnyId,
    #[serde(default)]
    pub detail: serde_json::Value,
    #[serde(default)]
    pub induced: bool,
}

impl PlanEffect {
    pub fn new(kind: &str, object: impl Into<AnyId>, detail: serde_json::Value) -> Self {
        Self {
            kind: kind.to_owned(),
            object: object.into(),
            detail,
            induced: false,
        }
    }

    pub fn induced(mut self) -> Self {
        self.induced = true;
        self
    }

    /// One-line human rendering: `<kind> <object> <detail summary>`, with an `(induced)` suffix.
    pub fn describe(&self) -> String {
        let mut s = format!("{} {}", self.kind, self.object);
        match &self.detail {
            serde_json::Value::Null => {}
            serde_json::Value::Object(m) if m.is_empty() => {}
            serde_json::Value::Object(m) => {
                let parts: Vec<String> =
                    m.iter().map(|(k, v)| format!("{k}={}", short(v))).collect();
                s.push(' ');
                s.push_str(&parts.join(" "));
            }
            other => {
                s.push(' ');
                s.push_str(&short(other));
            }
        }
        if self.induced {
            s.push_str(" (induced)");
        }
        s
    }
}

fn short(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        other => crate::plan::hash::canonical_json(other),
    }
}

/// Ids minted at plan time (slot name → id string), stored with the plan and reused on every recompute.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reserved(pub BTreeMap<String, String>);

pub trait ReservedId: Sized {
    fn mint() -> Self;
    fn parse_str(s: &str) -> Option<Self>;
    fn as_string(&self) -> String;
}

macro_rules! reserved_id {
    ($($t:ty),* $(,)?) => {$(
        impl ReservedId for $t {
            fn mint() -> Self { <$t>::new() }
            fn parse_str(s: &str) -> Option<Self> { <$t>::parse(s).ok() }
            fn as_string(&self) -> String { self.to_string() }
        }
    )*};
}
reserved_id!(
    SeatId,
    CloneId,
    TeamspaceId,
    AppId,
    ActionId,
    MemberId,
    TemplateId
);

impl Reserved {
    /// The id in `slot`, minting and recording one when the slot is empty (or holds a foreign id).
    pub fn get_or_mint<I: ReservedId>(&mut self, slot: &str) -> I {
        if let Some(id) = self.get::<I>(slot) {
            return id;
        }
        let id = I::mint();
        self.0.insert(slot.to_owned(), id.as_string());
        id
    }

    pub fn get<I: ReservedId>(&self, slot: &str) -> Option<I> {
        self.0.get(slot).and_then(|s| I::parse_str(s))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanRequest {
    pub kind: RequestKind,
    pub args: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    pub id: PlanId,
    pub request: PlanRequest,
    pub committed_rev: CommitId,
    pub relied_on: Vec<ReliedOn>,
    pub effects: Vec<PlanEffect>,
    pub warnings: Vec<String>,
    #[serde(default)]
    pub repair_required: Option<String>,
    pub reserved: Reserved,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredPlan {
    pub plan: Plan,
    pub hash: String,
    pub created_at: Timestamp,
    pub caller: CallerInfo,
    pub requester: Requester,
    #[serde(default)]
    pub supersedes: Option<OpId>,
}
