//! Effective seat config resolver (seat overrides > template member > template > graph defaults > built-in).
//! Owned by hg-zmi.6.
use crate::model::clone::CloneRecord;
use crate::model::common::{CloneLifecycle, Role, default_summaries};
use crate::model::graph::GraphDefaults;
use crate::model::harness::{Harness, profile};
use crate::model::seat::{InstructionSection, SeatRecord};
use crate::model::template::{TemplateMember, TemplateRecord};
use crate::plan::types::PlanEffect;
use crate::ports::store::StoreError;
use crate::store::layout;
use crate::store::record::read_toml;
use crate::store::tree::TreeRead;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectiveSeatConfig {
    pub harness: Harness,
    pub model: Option<String>,
    pub args: Vec<String>,
    pub summaries: bool,
    pub role: Option<Role>,
    pub cwd: Option<PathBuf>,
    pub instructions_sections: Vec<InstructionSection>,
}

/// Precedence: seat overrides > template-member defaults > template defaults > graph defaults > built-in
/// (harness `claude`, no model, no args, `summaries = default_summaries(role)`). Role = seat.role, else member.role.
pub fn resolve(
    graph: &GraphDefaults,
    template: Option<&TemplateRecord>,
    member: Option<&TemplateMember>,
    seat: &SeatRecord,
) -> EffectiveSeatConfig {
    let o = &seat.overrides;
    let md = member.map(|m| &m.defaults);
    let td = template.map(|t| &t.defaults);
    let role = seat.role.or_else(|| member.and_then(|m| m.role));
    let harness = o
        .harness
        .or_else(|| md.and_then(|d| d.harness))
        .or_else(|| td.and_then(|d| d.harness))
        .or(graph.harness)
        .unwrap_or(Harness::Claude);
    let model = o
        .model
        .clone()
        .or_else(|| md.and_then(|d| d.model.clone()))
        .or_else(|| td.and_then(|d| d.model.clone()))
        .or_else(|| graph.model.clone());
    let args = o
        .args
        .clone()
        .or_else(|| md.and_then(|d| d.args.clone()))
        .or_else(|| td.and_then(|d| d.args.clone()))
        .unwrap_or_default();
    let summaries = o
        .summaries
        .or_else(|| md.and_then(|d| d.summaries))
        .or_else(|| td.and_then(|d| d.summaries))
        .unwrap_or_else(|| default_summaries(role));
    EffectiveSeatConfig {
        harness,
        model,
        args,
        summaries,
        role,
        cwd: o.cwd.clone(),
        instructions_sections: o.instructions_sections.clone(),
    }
}

/// Convenience: read graph.toml, the seat's template/member (if `template_ref`) from `tree`, then resolve.
/// A template that no longer exists is treated as absent.
pub fn resolve_in(tree: &dyn TreeRead, seat: &SeatRecord) -> Result<EffectiveSeatConfig, StoreError> {
    let graph = layout::read_graph(tree)?;
    let mut template = None;
    let mut member = None;
    if let Some(r) = &seat.template_ref
        && let Some(loc) = layout::locate(tree, &r.template.to_any())?
    {
        let rec: Option<TemplateRecord> = read_toml(tree, &loc.record_path)?;
        member = rec.as_ref().and_then(|t| t.members.iter().find(|m| m.id == r.member).cloned());
        template = rec;
    }
    Ok(resolve(&graph.defaults, template.as_ref(), member.as_ref(), seat))
}

fn run_shape(c: &EffectiveSeatConfig) -> serde_json::Value {
    serde_json::json!({ "harness": c.harness, "model": c.model, "args": c.args })
}

/// Spec §4.5: for each active clone of `seat` with an occupant, a `session.replace` effect when
/// harness/model/args differ: detail `{clone, from, to, resume}`.
pub fn session_replacements(
    seat: &SeatRecord,
    clones: &[CloneRecord],
    old: &EffectiveSeatConfig,
    new: &EffectiveSeatConfig,
) -> Vec<PlanEffect> {
    if run_shape(old) == run_shape(new) {
        return Vec::new();
    }
    let resume = profile(new.harness).supports_resume() && old.harness == new.harness;
    clones
        .iter()
        .filter(|c| c.seat == seat.id && c.lifecycle == CloneLifecycle::Active && c.occupant.is_some())
        .map(|c| {
            PlanEffect::new(
                "session.replace",
                c.id.clone(),
                serde_json::json!({ "clone": c.id, "from": run_shape(old), "to": run_shape(new), "resume": resume }),
            )
        })
        .collect()
}
