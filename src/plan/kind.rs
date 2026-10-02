//! Mutation-kind framework for organizational kinds (spec §3.2-3.3). A kind parses CLI words, plans
//! concrete effects against one committed revision, and performs the confirmed change on the writer overlay.
use super::store::PlanStore;
use super::types::{Plan, PlanEffect, PlanRequest, Reserved};
use crate::daemon::registry::CallerInfo;
use crate::model::change::{ReliedOn, RequestKind};
use crate::model::{CommitId, PlanId, Timestamp};
use crate::ports::store::StoreError;
use crate::store::tree::TreeRead;
use crate::writer::{Applied, MutationCx, MutationError, MutationRegistry};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::Arc;

/// Read context for planning: one committed revision.
pub struct PlanCx<'a> {
    pub tree: &'a dyn TreeRead,
    pub at: CommitId,
    pub caller: &'a CallerInfo,
    pub now: Timestamp,
    pub instance: &'a Path,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlanBody {
    pub effects: Vec<PlanEffect>,
    pub relied_on: Vec<ReliedOn>,
    pub warnings: Vec<String>,
    pub repair_required: Option<String>,
    pub summary: String,
}

#[derive(Debug, thiserror::Error)]
pub enum PlanError {
    #[error("usage: {0}")]
    Usage(String),
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Store(#[from] StoreError),
}

pub trait OrgKind: Send + Sync {
    fn kind(&self) -> RequestKind;
    /// CLI words handled, e.g. `&[("seat","create")]`; `undo` uses `("undo","")`.
    fn verbs(&self) -> &'static [(&'static str, &'static str)];
    /// Words AFTER noun/verb → kind args (JSON).
    fn parse(&self, words: &[String], caller: &CallerInfo) -> Result<serde_json::Value, PlanError>;
    /// Concrete plan against one revision; plan-time ids are minted only through `reserved`.
    fn plan(
        &self,
        cx: &PlanCx<'_>,
        args: &serde_json::Value,
        reserved: &mut Reserved,
    ) -> Result<PlanBody, PlanError>;
    /// Writer side: perform the confirmed change on the overlay. `plan` is the confirmed plan.
    fn mutate(
        &self,
        cx: &mut MutationCx<'_>,
        args: &serde_json::Value,
        plan: &Plan,
    ) -> Result<Applied, MutationError>;
}

/// Run `kind.plan` and assemble a fresh (unstored) `Plan` with a new `pl_` id.
pub fn make_plan(
    kind: &dyn OrgKind,
    cx: &PlanCx<'_>,
    args: serde_json::Value,
    mut reserved: Reserved,
) -> Result<Plan, PlanError> {
    let body = kind.plan(cx, &args, &mut reserved)?;
    Ok(Plan {
        id: PlanId::new(),
        request: PlanRequest { kind: kind.kind(), args },
        committed_rev: cx.at.clone(),
        relied_on: body.relied_on,
        effects: body.effects,
        warnings: body.warnings,
        repair_required: body.repair_required,
        reserved,
    })
}

#[derive(Default, Clone)]
pub struct KindRegistry {
    by_kind: HashMap<RequestKind, Arc<dyn OrgKind>>,
    by_verb: BTreeMap<(String, String), RequestKind>,
}

impl KindRegistry {
    /// Panics on a duplicate kind or verb, and on a kind that does not require confirmation.
    pub fn register(&mut self, k: Arc<dyn OrgKind>) {
        let kind = k.kind();
        assert!(kind.requires_confirmation(), "{kind:?} is not an organizational kind");
        assert!(!self.by_kind.contains_key(&kind), "kind {kind:?} registered twice");
        for (noun, verb) in k.verbs() {
            let key = ((*noun).to_owned(), (*verb).to_owned());
            assert!(!self.by_verb.contains_key(&key), "verb {noun} {verb} registered twice");
            self.by_verb.insert(key, kind);
        }
        self.by_kind.insert(kind, k);
    }

    pub fn get(&self, k: RequestKind) -> Option<Arc<dyn OrgKind>> {
        self.by_kind.get(&k).cloned()
    }

    /// `noun verb rest…` → the kind and the words after the verb; `("undo","")` matches a single word.
    pub fn resolve(&self, words: &[String]) -> Result<(Arc<dyn OrgKind>, Vec<String>), PlanError> {
        let Some(noun) = words.first() else {
            return Err(PlanError::Usage(self.usage()));
        };
        let two = words.get(1).map(|verb| (noun.clone(), verb.clone()));
        if let Some(k) = two.as_ref().and_then(|key| self.by_verb.get(key)) {
            return Ok((self.by_kind[k].clone(), words[2..].to_vec()));
        }
        if let Some(k) = self.by_verb.get(&(noun.clone(), String::new())) {
            return Ok((self.by_kind[k].clone(), words[1..].to_vec()));
        }
        let given = words.iter().take(2).cloned().collect::<Vec<_>>().join(" ");
        Err(PlanError::Usage(format!("unknown change `{given}`; known changes: {}", self.usage())))
    }

    fn usage(&self) -> String {
        self.by_verb
            .keys()
            .map(|(n, v)| if v.is_empty() { n.clone() } else { format!("{n} {v}") })
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn kinds(&self) -> Vec<RequestKind> {
        let mut v: Vec<RequestKind> = self.by_kind.keys().copied().collect();
        v.sort_by_key(|k| crate::writer::kind_name(*k));
        v
    }

    /// Register one writer `Mutation` per organizational kind: the apply adapter (`apply.rs`).
    pub fn register_mutations(self: &Arc<Self>, reg: &mut MutationRegistry, plans: Arc<PlanStore>) {
        let instance = plans.instance();
        for kind in self.kinds() {
            let m = super::apply::OrgMutation { kinds: self.clone(), plans: plans.clone(), instance: instance.clone() };
            reg.register(crate::writer::kind_name(kind), Arc::new(m));
        }
    }
}
