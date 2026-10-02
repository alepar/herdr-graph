//! Launch env contract, graph token, creation nonce label, cwd rule (spec §4.1, §4.2).
use crate::model::ids::{AnyId, CloneId, EffectId, SeatId};
use std::path::{Path, PathBuf};

pub const ENV_GRAPH: &str = "HERDR_GRAPH";
pub const ENV_INSTANCE: &str = "HERDR_GRAPH_INSTANCE";
pub const ENV_SEAT: &str = "HERDR_GRAPH_SEAT";
pub const ENV_CLONE: &str = "HERDR_GRAPH_CLONE";

/// Env passed via `env` on workspace/tab/pane creation. Graph never reads env back.
pub fn launch_env(
    instance: &Path,
    seat: Option<&SeatId>,
    clone: Option<&CloneId>,
) -> Vec<(String, String)> {
    let mut v = vec![
        (ENV_GRAPH.to_string(), "1".to_string()),
        (ENV_INSTANCE.to_string(), instance.display().to_string()),
    ];
    if let Some(s) = seat {
        v.push((ENV_SEAT.to_string(), s.to_string()));
    }
    if let Some(c) = clone {
        v.push((ENV_CLONE.to_string(), c.to_string()));
    }
    v
}

pub const TOKEN_PREFIX: &str = "hg=";
/// Graph-owned identity token stamped via `pane.report_metadata` / `workspace.report_metadata`.
pub fn graph_token(id: &AnyId) -> String {
    format!("{TOKEN_PREFIX}{id}")
}
pub fn parse_graph_token(s: &str) -> Option<AnyId> {
    s.strip_prefix(TOKEN_PREFIX).and_then(|r| AnyId::parse(r).ok())
}

pub const NONCE_SEP: &str = " ·";
/// Creation-atomic nonce label `<name> ·<ef6>` (spec §4.2).
pub fn nonce_label(name: &str, effect: &EffectId) -> String {
    format!("{name}{NONCE_SEP}{}", effect.suffix6())
}
/// Split a nonce label into `(name, ef6)`; None if it carries no nonce.
pub fn parse_nonce_label(label: &str) -> Option<(&str, &str)> {
    let (name, suffix) = label.rsplit_once(NONCE_SEP)?;
    (suffix.chars().count() == 6).then_some((name, suffix))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CwdSource {
    SeatOverride,
    ProjectRepo,
    InstanceFallback,
}

/// cwd rule: seat override, else teamspace project_repo, else `<instance>/.graph-local/cwd/<seat-id>`.
pub fn resolve_cwd(
    seat_override: Option<&Path>,
    project_repo: Option<&Path>,
    instance: &Path,
    seat: &SeatId,
) -> (PathBuf, CwdSource) {
    if let Some(p) = seat_override {
        return (p.to_path_buf(), CwdSource::SeatOverride);
    }
    if let Some(p) = project_repo {
        return (p.to_path_buf(), CwdSource::ProjectRepo);
    }
    (
        instance.join(".graph-local").join("cwd").join(seat.as_str()),
        CwdSource::InstanceFallback,
    )
}
