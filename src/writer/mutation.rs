//! Mutation registration API: feature beads register one `Mutation` per request kind (or
//! `kind.sub` for observed/bookkeeping/content_write) and the writer applies it to an overlay.
use crate::model::change::{ChangeRequest, ReliedOn, RequestKind};
use crate::model::operation::Rejection;
use crate::model::{ActionId, OpId, Timestamp};
use crate::ports::store::StoreError;
use crate::store::Overlay;
use std::collections::BTreeMap;
use std::sync::Arc;

pub struct MutationCx<'a> {
    /// Base = committed head at apply time.
    pub tree: Overlay<'a>,
    pub op: OpId,
    pub now: Timestamp,
    pub request: &'a ChangeRequest,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Applied {
    pub summary: String,
    pub action: Option<ActionId>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Reject {
    pub reason: String,
    pub explanation: String,
    pub current_revs: Vec<ReliedOn>,
}

impl From<Reject> for Rejection {
    fn from(r: Reject) -> Self {
        Rejection { reason: r.reason, explanation: r.explanation, current_revs: r.current_revs }
    }
}

#[derive(Debug)]
pub enum MutationError {
    /// Deterministic refusal: the op is rejected (state `rejected`).
    Reject(Reject),
    /// Infrastructure failure: the op is requeued and retried with backoff.
    Store(StoreError),
    /// Deterministic bug: the op becomes `failed`.
    Bug(String),
}

impl From<StoreError> for MutationError {
    fn from(e: StoreError) -> Self {
        MutationError::Store(e)
    }
}

pub trait Mutation: Send + Sync {
    fn apply(&self, cx: &mut MutationCx<'_>) -> Result<Applied, MutationError>;
}

/// Registry key: snake name of `kind` ("seat_create"); for observed/bookkeeping/content_write with
/// `args["sub"]` a string → "<kind>.<sub>" (e.g. "observed.cascade", "bookkeeping.binding").
pub fn mutation_key(req: &ChangeRequest) -> String {
    let kind = kind_name(req.kind);
    let has_sub = matches!(req.kind, RequestKind::Observed | RequestKind::Bookkeeping | RequestKind::ContentWrite);
    match req.args.get("sub").and_then(|v| v.as_str()) {
        Some(sub) if has_sub => format!("{kind}.{sub}"),
        _ => kind,
    }
}

/// serde snake_case name of a request kind.
pub fn kind_name(kind: RequestKind) -> String {
    serde_json::to_value(kind).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default()
}

#[derive(Default, Clone)]
pub struct MutationRegistry {
    map: BTreeMap<String, Arc<dyn Mutation>>,
}

impl MutationRegistry {
    /// Panics on a duplicate key: that is a wiring bug.
    pub fn register(&mut self, key: impl Into<String>, m: Arc<dyn Mutation>) {
        let key = key.into();
        assert!(!self.map.contains_key(&key), "mutation {key:?} registered twice");
        self.map.insert(key, m);
    }

    pub fn get(&self, key: &str) -> Option<Arc<dyn Mutation>> {
        self.map.get(key).cloned()
    }

    pub fn keys(&self) -> Vec<String> {
        self.map.keys().cloned().collect()
    }
}
