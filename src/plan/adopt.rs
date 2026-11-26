//! `teamspace adopt <workspace>`: bind an existing Herdr workspace to a new teamspace record without creating
//! anything in Herdr (design 2026-11-07 G1, section 3). The tabs in the workspace stay unmanaged: they carry no
//! clone tokens, so the reconciler never closes, renames or reuses them.
//!
//! Planning has no Herdr access and the writer recomputes the plan from its args alone, so `plan.create` takes
//! one snapshot (`OrgKind::needs_live`) and `bind_live` folds the resolved workspace into the args.
use super::core_kinds::{Acc, lc_value, mm, parse_args, write_action};
use super::grammar::{flag, positional};
use super::kind::{OrgKind, PlanBody, PlanCx, PlanError};
use super::types::{Plan, PlanEffect, Reserved};
use crate::daemon::registry::CallerInfo;
use crate::model::action::{ActionKind, AffectedObject};
use crate::model::change::RequestKind;
use crate::model::common::{Availability, Binding, Incarnation, Lifecycle, Runtime};
use crate::model::launch::parse_graph_token;
use crate::model::teamspace::TeamspaceRecord;
use crate::model::{ActionId, HerdrWorkspaceId, SCHEMA_VERSION, TeamspaceId};
use crate::ports::herdr::HerdrSnapshot;
use crate::reconcile::planner::TOKEN_KEY;
use crate::store::layout;
use crate::store::slug::unique_slug;
use crate::store::tree::TreeRead;
use crate::writer::{Applied, MutationCx, MutationError};
use serde::Deserialize;
use serde_json::json;
use std::path::PathBuf;

/// The args `bind_live` produces: everything the plan and the mutation need, resolved against one snapshot.
#[derive(Deserialize)]
struct TeamspaceAdoptArgs {
    workspace_id: HerdrWorkspaceId,
    workspace_label: String,
    incarnation: Incarnation,
    name: String,
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    project_repo: Option<String>,
}

pub(crate) struct TeamspaceAdopt;

impl TeamspaceAdopt {
    /// Refuse a workspace that already belongs to a live teamspace: bound by id, or stamped with its token.
    fn check(tree: &dyn TreeRead, a: &TeamspaceAdoptArgs) -> Result<(), PlanError> {
        let stamped = a
            .token
            .as_deref()
            .and_then(parse_graph_token)
            .and_then(|id| TeamspaceId::try_from(id).ok());
        for (_, ts) in layout::list_teamspaces(tree)? {
            if ts.lifecycle == Lifecycle::Retired {
                continue;
            }
            let bound = ts
                .runtime
                .bound
                .as_ref()
                .and_then(|b| b.workspace_id.as_ref())
                == Some(&a.workspace_id);
            if bound || stamped.as_ref() == Some(&ts.id) {
                return Err(PlanError::Invalid(format!(
                    "Herdr workspace {} is already teamspace {}",
                    a.workspace_id, ts.name
                )));
            }
        }
        Ok(())
    }
}

impl OrgKind for TeamspaceAdopt {
    fn kind(&self) -> RequestKind {
        RequestKind::TeamspaceAdopt
    }
    fn verbs(&self) -> &'static [(&'static str, &'static str)] {
        &[("teamspace", "adopt")]
    }
    fn parse(&self, words: &[String], _: &CallerInfo) -> Result<serde_json::Value, PlanError> {
        let workspace = positional(words, 0).ok_or_else(|| {
            PlanError::Usage(
                "teamspace adopt <workspace-id|label> [--name <name>] [--project-repo <path>]"
                    .into(),
            )
        })?;
        Ok(json!({
            "workspace": workspace,
            "name": flag(words, "--name"),
            "project_repo": flag(words, "--project-repo"),
        }))
    }
    fn needs_live(&self) -> bool {
        true
    }
    fn bind_live(
        &self,
        args: serde_json::Value,
        snap: &HerdrSnapshot,
    ) -> Result<serde_json::Value, PlanError> {
        let workspace = args["workspace"].as_str().unwrap_or_default();
        let by_id: Vec<_> = snap
            .workspaces
            .iter()
            .filter(|w| w.id.0 == workspace)
            .collect();
        let found = if by_id.is_empty() {
            snap.workspaces
                .iter()
                .filter(|w| w.label == workspace)
                .collect()
        } else {
            by_id
        };
        let w = match found.as_slice() {
            [] => {
                return Err(PlanError::Invalid(format!(
                    "no Herdr workspace {workspace:?}"
                )));
            }
            [w] => *w,
            many => {
                let list = many
                    .iter()
                    .map(|w| format!("{} ({})", w.id, w.label))
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(PlanError::Invalid(format!(
                    "Herdr workspace {workspace:?} is ambiguous: {list}; use the id"
                )));
            }
        };
        let name = args["name"]
            .as_str()
            .map_or_else(|| w.label.clone(), str::to_owned);
        Ok(json!({
            "workspace": workspace,
            "workspace_id": w.id,
            "workspace_label": w.label,
            "token": w.metadata.get(TOKEN_KEY),
            "incarnation": snap.incarnation,
            "name": name,
            "project_repo": args["project_repo"],
        }))
    }
    fn plan(
        &self,
        cx: &PlanCx<'_>,
        args: &serde_json::Value,
        reserved: &mut Reserved,
    ) -> Result<PlanBody, PlanError> {
        let a: TeamspaceAdoptArgs = parse_args(args)?;
        Self::check(cx.tree, &a)?;
        let id: TeamspaceId = reserved.get_or_mint("teamspace");
        let _act: ActionId = reserved.get_or_mint("act");
        let taken = layout::taken_slugs(cx.tree, &layout::teamspaces_root())?;
        let slug = unique_slug(&a.name, id.suffix6(), &taken);
        let path = layout::teamspace_dir(&slug);
        let label = &a.workspace_label;
        let relabel = a.name != *label;
        let mut effects = vec![PlanEffect::new(
            "teamspace.adopt",
            id.clone(),
            json!({
                "name": a.name,
                "path": path.as_str(),
                "workspace_id": a.workspace_id,
                "workspace_label": label,
                "project_repo": a.project_repo,
            }),
        )];
        if relabel {
            effects.push(PlanEffect::new(
                "runtime.rename_workspace",
                id,
                json!({ "name": a.name }),
            ));
        }
        let mut warnings = vec![
            format!("tabs in workspace {label} stay unmanaged; graph never closes, renames or reuses them"),
            "graph stamps its token on the Herdr workspace; closing the workspace in Herdr retires the teamspace".to_owned(),
            "retiring or deactivating this teamspace closes the whole Herdr workspace, unmanaged tabs and their agents included".to_owned(),
        ];
        if relabel {
            warnings.push(format!(
                "the Herdr workspace is relabeled {label:?} -> {:?}",
                a.name
            ));
        }
        Ok(PlanBody {
            effects,
            relied_on: vec![],
            warnings,
            repair_required: None,
            summary: format!("adopt Herdr workspace {label} as teamspace {}", a.name),
        })
    }
    fn mutate(
        &self,
        cx: &mut MutationCx<'_>,
        args: &serde_json::Value,
        plan: &Plan,
    ) -> Result<Applied, MutationError> {
        let a: TeamspaceAdoptArgs = parse_args(args).map_err(mm)?;
        Self::check(&cx.tree, &a).map_err(mm)?;
        let id: TeamspaceId = plan
            .reserved
            .get("teamspace")
            .ok_or_else(|| MutationError::Bug("plan reserved no teamspace id".into()))?;
        let act: ActionId = plan
            .reserved
            .get("act")
            .ok_or_else(|| MutationError::Bug("plan reserved no action id".into()))?;
        let taken = layout::taken_slugs(&cx.tree, &layout::teamspaces_root())?;
        let dir = layout::teamspace_dir(&unique_slug(&a.name, id.suffix6(), &taken));
        let mut rec = TeamspaceRecord {
            schema: SCHEMA_VERSION,
            id: id.clone(),
            rev: 0,
            name: a.name.clone(),
            name_history: vec![],
            lifecycle: Lifecycle::Active,
            retired: None,
            runtime: Runtime {
                availability: Availability::Present,
                bound: Some(Binding {
                    token: None,
                    workspace_id: Some(a.workspace_id.clone()),
                    tab_id: None,
                    pane_id: None,
                    terminal_id: None,
                    incarnation: a.incarnation.clone(),
                }),
                observed_at: Some(cx.now),
            },
            project_repo: a.project_repo.map(PathBuf::from),
            default_manager: None,
            channel: Default::default(),
        };
        cx.tree
            .put_record(layout::teamspace_record(&dir), &mut rec)?;
        let acc = Acc {
            affected: vec![AffectedObject {
                object: id.to_any(),
                before: None,
                after: Some(lc_value("active")),
            }],
            ..Acc::default()
        };
        let mut comp = toml::Table::new();
        comp.insert("teamspace".into(), toml::Value::String(id.to_string()));
        comp.insert(
            "workspace_id".into(),
            toml::Value::String(a.workspace_id.to_string()),
        );
        write_action(cx, &act, ActionKind::Adopt, acc, comp)?;
        Ok(Applied {
            summary: format!(
                "adopt Herdr workspace {} as teamspace {}",
                a.workspace_label, a.name
            ),
            action: Some(act),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::commands::{create_plan, create_plan_live};
    use super::super::kinds_extra::testkit::{Fx, apply_plan, fx, fx_with, words};
    use super::super::types::StoredPlan;
    use super::*;
    use crate::daemon::registry::CallerInfo;
    use crate::model::PlanId;
    use crate::model::action::ActionKind;
    use crate::model::operation::OpState;
    use crate::model::teamspace::TeamspaceRecord;
    use crate::ports::herdr::{HerdrSnapshot, TabInfo, WorkspaceInfo};
    use crate::ports::store::Store;
    use crate::store::tree::CommitView;
    use std::collections::BTreeMap;

    fn ws(id: &str, label: &str, tabs: &[&str], hg: Option<&str>) -> WorkspaceInfo {
        WorkspaceInfo {
            id: HerdrWorkspaceId(id.into()),
            label: label.into(),
            metadata: hg
                .map(|t| BTreeMap::from([(TOKEN_KEY.to_owned(), t.to_owned())]))
                .unwrap_or_default(),
            tabs: tabs
                .iter()
                .map(|t| TabInfo {
                    id: crate::model::HerdrTabId(format!("{id}:{t}")),
                    label: (*t).into(),
                    panes: vec![],
                })
                .collect(),
        }
    }

    fn snap(workspaces: Vec<WorkspaceInfo>) -> HerdrSnapshot {
        HerdrSnapshot {
            incarnation: Incarnation {
                generation: 1,
                server_pid: Some(42),
                server_started: None,
            },
            workspaces,
        }
    }

    fn base_snap() -> HerdrSnapshot {
        snap(vec![
            ws("w1", "legacy", &["main", "logs"], None),
            ws("w2", "other", &["main"], None),
        ])
    }

    fn plan_live(fx: &Fx, change: &str, s: &HerdrSnapshot) -> Result<StoredPlan, String> {
        let v = create_plan_live(&fx.deps, &CallerInfo::default(), words(change), Some(s))
            .map_err(|e| e.message)?;
        let id: PlanId = v["plan_id"].as_str().unwrap().parse().unwrap();
        Ok(fx.deps.plans.get(&id).unwrap().unwrap())
    }

    fn teamspaces(fx: &Fx) -> Vec<TeamspaceRecord> {
        let at = fx.store.head().unwrap();
        let view = CommitView {
            store: &*fx.store,
            at,
        };
        layout::list_teamspaces(&view)
            .unwrap()
            .into_iter()
            .map(|(_, t)| t)
            .collect()
    }

    fn adopt_ok(fx: &Fx, change: &str, s: &HerdrSnapshot) -> crate::journal::OpRow {
        let row = apply_plan(fx, &plan_live(fx, change, s).unwrap());
        assert_eq!(
            row.state,
            OpState::Committed,
            "{change}: {:?}",
            row.rejection
        );
        row
    }

    fn effect_kinds(sp: &StoredPlan) -> Vec<&str> {
        sp.plan.effects.iter().map(|e| e.kind.as_str()).collect()
    }

    #[test]
    fn adopt_by_label_binds_the_workspace_without_runtime_effects() {
        let fx = fx();
        let s = base_snap();
        let sp = plan_live(&fx, "teamspace adopt legacy", &s).unwrap();
        assert_eq!(effect_kinds(&sp), ["teamspace.adopt"]);
        assert_eq!(sp.plan.warnings.len(), 3);
        let row = apply_plan(&fx, &sp);
        assert_eq!(row.state, OpState::Committed, "{:?}", row.rejection);
        let all = teamspaces(&fx);
        assert_eq!(all.len(), 1);
        let ts = &all[0];
        assert_eq!(ts.name, "legacy");
        assert_eq!(ts.lifecycle, Lifecycle::Active);
        assert_eq!(ts.runtime.availability, Availability::Present);
        let b = ts.runtime.bound.as_ref().unwrap();
        assert_eq!(b.workspace_id, Some(HerdrWorkspaceId("w1".into())));
        assert_eq!(b.incarnation, s.incarnation);
        assert!(b.tab_id.is_none() && b.pane_id.is_none());
        let act = row.action.expect("adopt writes an action");
        let at = fx.store.head().unwrap();
        let view = CommitView {
            store: &*fx.store,
            at,
        };
        let rec = crate::undo::candidates::read_action(&view, &act)
            .unwrap()
            .unwrap();
        assert_eq!(rec.kind, ActionKind::Adopt);
        assert_eq!(rec.compensation["teamspace"].as_str(), Some(ts.id.as_str()));
        assert_eq!(rec.compensation["workspace_id"].as_str(), Some("w1"));
    }

    #[test]
    fn adopt_by_id_wins_over_a_label() {
        let fx = fx();
        let s = snap(vec![
            ws("w1", "w2", &["a"], None),
            ws("w2", "second", &["b"], None),
        ]);
        let sp = plan_live(&fx, "teamspace adopt w2", &s).unwrap();
        assert_eq!(sp.plan.request.args["workspace_id"], "w2");
        assert_eq!(sp.plan.request.args["name"], "second");
    }

    #[test]
    fn adopt_name_flag_relabels() {
        let fx = fx();
        let sp = plan_live(
            &fx,
            "teamspace adopt legacy --name other-name --project-repo /p",
            &base_snap(),
        )
        .unwrap();
        assert_eq!(
            effect_kinds(&sp),
            ["teamspace.adopt", "runtime.rename_workspace"]
        );
        assert_eq!(sp.plan.warnings.len(), 4);
        assert!(sp.plan.warnings[3].contains("relabeled \"legacy\" -> \"other-name\""));
        let row = apply_plan(&fx, &sp);
        assert_eq!(row.state, OpState::Committed, "{:?}", row.rejection);
        let ts = &teamspaces(&fx)[0];
        assert_eq!(ts.name, "other-name");
        assert_eq!(ts.project_repo, Some(PathBuf::from("/p")));
    }

    #[test]
    fn adopt_refuses_an_unknown_and_an_ambiguous_ref() {
        let fx = fx();
        let e = plan_live(&fx, "teamspace adopt nope", &base_snap()).unwrap_err();
        assert!(e.contains("no Herdr workspace \"nope\""), "{e}");
        let dup = snap(vec![
            ws("w1", "same", &["a"], None),
            ws("w2", "same", &["b"], None),
        ]);
        let e = plan_live(&fx, "teamspace adopt same", &dup).unwrap_err();
        assert!(e.contains("w1 (same)") && e.contains("w2 (same)"), "{e}");
    }

    #[test]
    fn adopt_refuses_a_workspace_already_bound_or_stamped() {
        let fx = fx();
        let s = base_snap();
        adopt_ok(&fx, "teamspace adopt legacy", &s);
        let e = plan_live(&fx, "teamspace adopt w1", &s).unwrap_err();
        assert!(
            e.contains("Herdr workspace w1 is already teamspace legacy"),
            "{e}"
        );

        // Stamped with a live teamspace's token, in a new Herdr incarnation where the id differs.
        let live = teamspaces(&fx)[0].id.clone();
        let token = crate::model::launch::graph_token(&live.to_any());
        let stamped = snap(vec![ws("w9", "renumbered", &["a"], Some(&token))]);
        let e = plan_live(&fx, "teamspace adopt w9", &stamped).unwrap_err();
        assert!(e.contains("already teamspace legacy"), "{e}");

        // A token naming a retired teamspace does not block adoption.
        let retired = crate::model::launch::graph_token(&TeamspaceId::new().to_any());
        let ok = snap(vec![ws("w9", "renumbered", &["a"], Some(&retired))]);
        plan_live(&fx, "teamspace adopt w9", &ok).unwrap();
    }

    #[test]
    fn adopt_accepts_a_workspace_stamped_for_a_retired_teamspace() {
        let fx = fx();
        let s = base_snap();
        adopt_ok(&fx, "teamspace adopt legacy", &s);
        let ts = teamspaces(&fx)[0].id.clone();
        crate::plan::kinds_extra::testkit::commit(&fx, &format!("teamspace retire {ts}"));
        let again = plan_live(&fx, "teamspace adopt w1", &s).unwrap();
        assert_eq!(effect_kinds(&again), ["teamspace.adopt"]);
    }

    #[test]
    fn adopt_without_a_snapshot_is_unavailable() {
        let fx = fx();
        let e =
            create_plan(&fx.deps, &CallerInfo::default(), words("teamspace adopt x")).unwrap_err();
        assert!(
            e.message.contains("daemon has no Herdr snapshot"),
            "{}",
            e.message
        );
    }

    #[test]
    fn adopt_action_is_not_undoable_yet() {
        let fx = fx_with(|kinds| {
            crate::templates::register_kinds(kinds);
            crate::undo::register_kinds(kinds);
        });
        let row = adopt_ok(&fx, "teamspace adopt legacy", &base_snap());
        let act = row.action.unwrap();
        let e = create_plan(
            &fx.deps,
            &CallerInfo::default(),
            words(&format!("undo {act}")),
        )
        .unwrap_err();
        assert!(e.message.contains("cannot be undone"), "{}", e.message);
    }
}
