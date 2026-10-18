//! Reviewed namespace changes. All graph edits and preservation evidence share one Git commit.
use super::namespace::{self, Transition};
use crate::{
    daemon::registry::CallerInfo,
    model::ActionId,
    model::change::RequestKind,
    plan::{
        kind::{OrgKind, PlanBody, PlanCx, PlanError},
        types::{Plan, PlanEffect, Reserved},
    },
    ports::store::RepoPath,
    store::layout,
    writer::{Applied, MutationCx, MutationError},
};
use herdr_threads::node::NodeUuid;
use serde::{Deserialize, Serialize};
use serde_json::json;

pub struct Reconnect;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    from: String,
    to: NodeUuid,
    head: String,
}
fn invalid(e: impl std::fmt::Display) -> PlanError {
    PlanError::Invalid(e.to_string())
}
fn args(v: &serde_json::Value) -> Result<Args, PlanError> {
    serde_json::from_value(v.clone()).map_err(invalid)
}
fn verify(cx: &PlanCx<'_>, a: &Args) -> Result<(), PlanError> {
    if a.head != cx.at.0 {
        return Err(invalid(format!(
            "head changed: expected {}, current {}; create a new reconnect plan",
            a.head, cx.at.0
        )));
    }
    let old = namespace::bound(&namespace::default_intents(cx.instance))
        .map_err(invalid)?
        .map(|n| n.to_string())
        .unwrap_or_else(|| "legacy".into());
    if old != a.from {
        return Err(invalid(format!(
            "--from {} does not match local binding {old}",
            a.from
        )));
    }
    if a.from == a.to.to_string() {
        return Err(invalid(
            "reconnect requires a different node; same-node restart needs no reconnect",
        ));
    }
    if let Some(t) = namespace::read(cx.tree)? {
        if old != t.node.to_string() {
            return Err(invalid(
                "finish the committed reconnect by restoring its matching binding/store before planning another transition",
            ));
        }
    }
    Ok(())
}
/// Exact durable journal evidence, including operation payloads, effect provenance and cached results.
fn journal_snapshot(root: &std::path::Path) -> Result<serde_json::Value, MutationError> {
    let path = crate::journal::Journal::path_in(root);
    if !path.exists() {
        return Ok(json!({}));
    }
    let db =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| MutationError::Bug(e.to_string()))?;
    let tx = db
        .unchecked_transaction()
        .map_err(|e| MutationError::Bug(e.to_string()))?;
    let mut result = serde_json::Map::new();
    for table in ["ops", "effects", "meta", "notices"] {
        let mut q = tx
            .prepare(&format!("SELECT * FROM {table}"))
            .map_err(|e| MutationError::Bug(e.to_string()))?;
        let names: Vec<String> = q.column_names().into_iter().map(str::to_owned).collect();
        let mut rows = q.query([]).map_err(|e| MutationError::Bug(e.to_string()))?;
        let mut values = Vec::new();
        while let Some(r) = rows.next().map_err(|e| MutationError::Bug(e.to_string()))? {
            let mut obj = serde_json::Map::new();
            for (i, n) in names.iter().enumerate() {
                use rusqlite::types::ValueRef;
                let v = match r
                    .get_ref(i)
                    .map_err(|e| MutationError::Bug(e.to_string()))?
                {
                    ValueRef::Null => serde_json::Value::Null,
                    ValueRef::Integer(v) => json!(v),
                    ValueRef::Real(v) => json!(v),
                    ValueRef::Text(v) => json!(String::from_utf8_lossy(v)),
                    ValueRef::Blob(v) => json!(v),
                };
                obj.insert(n.clone(), v);
            }
            values.push(obj);
        }
        result.insert(table.into(), json!(values));
    }
    Ok(result.into())
}
impl OrgKind for Reconnect {
    fn kind(&self) -> RequestKind {
        RequestKind::ThreadsReconnect
    }
    fn verbs(&self) -> &'static [(&'static str, &'static str)] {
        &[("threads", "reconnect")]
    }
    fn parse(&self, words: &[String], _: &CallerInfo) -> Result<serde_json::Value, PlanError> {
        let usage = || {
            PlanError::Usage(
                "threads reconnect --from <node|legacy> --to <node> --head <current-main>".into(),
            )
        };
        let mut flags = std::collections::BTreeMap::new();
        for pair in words.chunks(2) {
            if pair.len() != 2
                || !["--from", "--to", "--head"].contains(&pair[0].as_str())
                || flags.insert(pair[0].as_str(), pair[1].as_str()).is_some()
            {
                return Err(usage());
            }
        }
        if flags.len() != 3 {
            return Err(usage());
        }
        let from = flags["--from"];
        if from != "legacy" {
            from.parse::<NodeUuid>().map_err(invalid)?;
        }
        let to = flags["--to"].parse::<NodeUuid>().map_err(invalid)?;
        let head = flags["--head"];
        if head.len() != 40
            || !head
                .bytes()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        {
            return Err(usage());
        }
        Ok(json!({"from":from,"to":to,"head":head}))
    }
    fn plan(
        &self,
        cx: &PlanCx<'_>,
        v: &serde_json::Value,
        r: &mut Reserved,
    ) -> Result<PlanBody, PlanError> {
        let a = args(v)?;
        verify(cx, &a)?;
        let epoch: ActionId = r.get_or_mint("namespace_epoch");
        let action: ActionId = r.get_or_mint("act");
        let ts = layout::list_teamspaces(cx.tree)?;
        let seats = layout::all_seats(cx.tree)?;
        let clones = layout::all_clones(cx.tree)?;
        let relationship_records:Vec<_>=layout::list_templates(cx.tree)?.into_iter().filter(|(_,r)|!r.relationships.is_empty()).map(|(loc,r)|json!({"path":loc.record_path.as_str(),"kind":"template","external_relationships":r.relationships.len()}))
            .chain(layout::list_applications(cx.tree)?.into_iter().filter(|(_,r)|!r.contributions.relationships.is_empty()).map(|(loc,r)|json!({"path":loc.record_path.as_str(),"kind":"application","external_relationships":r.contributions.relationships.len()}))).collect();
        let requests = layout::list_requests(cx.tree)?
            .into_iter()
            .filter(|(_, r)| {
                r.status != crate::model::request::RequestStatus::Completed
                    && !crate::transcripts::mutations::is_merged(r)
            })
            .count();
        Ok(PlanBody {
            effects: vec![PlanEffect::new("threads.reconnect", action, json!({
                "from": a.from, "to": a.to, "head": a.head, "epoch": epoch,
                "managed_channels": ts.len()+seats.len(), "clone_links": clones.len(),
                "incomplete_requests": requests, "external_relationship_records": relationship_records,
                "artifact": "mutations/threads-reconnect/<operation>.json",
                "old_intents": "preserved under their original identity",
                "external_participation": "preserved unresolved; no automatic invitations"
            }))],
            relied_on: vec![],
            warnings: vec!["External namespace boundary: ordinary undo is refused; reverse requires a new reviewed reconnect. All old pending intents remain dormant.".into()],
            repair_required: None,
            summary: format!("reconnect threads {} -> {}", a.from, a.to),
        })
    }
    fn mutate(
        &self,
        cx: &mut MutationCx<'_>,
        v: &serde_json::Value,
        plan: &Plan,
    ) -> Result<Applied, MutationError> {
        let a = args(v).map_err(crate::plan::core_kinds::mm)?;
        let epoch = plan
            .reserved
            .0
            .get("namespace_epoch")
            .ok_or_else(|| MutationError::Bug("missing namespace epoch".into()))?
            .clone();
        let previous_epoch = namespace::epoch(&cx.tree)?;
        let ts = layout::list_teamspaces(&cx.tree)?;
        let seats = layout::all_seats(&cx.tree)?;
        let clones = layout::all_clones(&cx.tree)?;
        let requests = layout::list_requests(&cx.tree)?;
        let templates = layout::list_templates(&cx.tree)?;
        let applications = layout::list_applications(&cx.tree)?;
        // Native transcript bytes, completed records, historical Git objects and old intent directories are never rewritten.
        let root = plan_instance(cx)?;
        let prior_actions = layout::list_actions(&cx.tree)?
            .into_iter()
            .map(|(_, a)| a.id.to_string())
            .collect::<Vec<_>>();
        let artifact = json!({
            "version": 1, "prior_actions": prior_actions, "from": a.from, "to": a.to,
            "base_head": a.head, "epoch": epoch, "previous_epoch": previous_epoch,
            "operation": cx.op,
            "teamspaces": ts.iter().map(|(_,r)|r).collect::<Vec<_>>(),
            "seats": seats.iter().map(|(_,r)|r).collect::<Vec<_>>(),
            "clones": clones.iter().map(|(_,r)|r).collect::<Vec<_>>(),
            "templates": templates.iter().map(|(_,r)|r).collect::<Vec<_>>(),
            "applications": applications.iter().map(|(_,r)|r).collect::<Vec<_>>(),
            "requests": requests.iter().map(|(_,r)|r).collect::<Vec<_>>(),
            "journal": journal_snapshot(&root)?,
            "old_intents": namespace::intent_dir(&namespace::default_intents(&root), &root).map_err(crate::ports::store::StoreError::Io)?
        });
        cx.tree.put_file(
            RepoPath::new(&format!("mutations/threads-reconnect/{}.json", cx.op))?,
            serde_json::to_vec_pretty(&artifact).map_err(|e| MutationError::Bug(e.to_string()))?,
        );
        for (loc, mut t) in templates {
            if !t.relationships.is_empty() {
                t.relationships.retain(|r| {
                    !matches!(
                        r.kind,
                        crate::model::template::RelationshipKind::ThreadParticipation
                    )
                });
                cx.tree.put_record(loc.record_path, &mut t)?;
            }
        }
        for (loc, mut app) in applications {
            if !app.contributions.relationships.is_empty() {
                app.contributions.relationships.retain(|r| {
                    !matches!(
                        r.kind,
                        crate::model::template::RelationshipKind::ThreadParticipation
                    )
                });
                cx.tree.put_record(loc.record_path, &mut app)?;
            }
        }
        for (loc, mut t) in ts {
            t.channel.thread_id = None;
            cx.tree.put_record(loc.record_path, &mut t)?;
        }
        for (loc, mut s) in seats {
            s.channel.thread_id = None;
            s.participation.seat_wide.clear();
            cx.tree.put_record(loc.record_path, &mut s)?;
        }
        for (loc, mut c) in clones {
            c.invitations.clear();
            c.opt_outs.clear();
            cx.tree.put_record(loc.record_path, &mut c)?;
        }
        for (path, mut rq) in requests {
            if rq.status == crate::model::request::RequestStatus::Completed
                || crate::transcripts::mutations::is_merged(&rq)
            {
                continue;
            }
            rq.status = crate::model::request::RequestStatus::Pending;
            rq.unresolved = None;
            rq.undeliverable = None;
            rq.delivery.message_id = None;
            rq.delivery.dispatched_at = None;
            rq.delivery.reminded_at = None;
            cx.tree.put_record(path.record_path, &mut rq)?;
        }
        let marker = Transition {
            version: 1,
            node: a.to,
            epoch,
            from: a.from.clone(),
            previous_epoch,
            base_head: a.head,
            op: cx.op.to_string(),
        };
        cx.tree.put_file(
            RepoPath::new(namespace::MARKER)?,
            serde_json::to_vec_pretty(&marker).map_err(|e| MutationError::Bug(e.to_string()))?,
        );
        let act: ActionId = plan.reserved.0["act"]
            .parse()
            .map_err(|e| MutationError::Bug(format!("{e}")))?;
        let mut comp = toml::Table::new();
        comp.insert("from".into(), a.from.clone().into());
        comp.insert("to".into(), a.to.to_string().into());
        crate::plan::core_kinds::write_action(
            cx,
            &act,
            crate::model::action::ActionKind::ThreadsReconnect,
            Default::default(),
            comp,
        )?;
        Ok(Applied {
            summary: format!("reconnect threads {} -> {}", a.from, a.to),
            action: Some(act),
        })
    }
}
// The overlay's source is the authoritative repository, not a path supplied by a client.
fn plan_instance(cx: &MutationCx<'_>) -> Result<std::path::PathBuf, MutationError> {
    cx.tree
        .instance_root()
        .ok_or_else(|| MutationError::Bug("reconnect requires a persistent graph store".into()))
}
