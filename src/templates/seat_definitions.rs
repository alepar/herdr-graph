//! Live seat-definition dependencies and propagation, sharing the normal template edit transaction.
use super::*;

pub(super) fn template_dependencies(
    tree: &dyn TreeRead,
    template: &TemplateRecord,
) -> Result<Vec<crate::model::change::ReliedOn>, PlanError> {
    let mut revisions = Vec::new();
    let mut seen = BTreeSet::new();
    for member in &template.members {
        if let Some(shared) = referenced_seat_template(tree, Some(member))?
            && seen.insert(shared.id.clone())
        {
            revisions.push(rev_of(shared.id, shared.rev));
        }
    }
    Ok(revisions)
}

pub(super) fn compute_seat_edit(
    tree: &dyn TreeRead,
    old: &TemplateRecord,
    new: &TemplateRecord,
) -> Result<EditPlan, PlanError> {
    let graph = layout::read_graph(tree)?;
    let mut apps = Vec::new();
    let mut dependencies = Vec::new();
    let mut seats = BTreeMap::new();
    // A seat may outlive every application after explicit reuse. Its own live reference remains
    // authoritative, and each occupied clone appears only once regardless of application count.
    for (loc, seat) in layout::all_seats(tree)? {
        if seat.lifecycle == Lifecycle::Retired {
            continue;
        }
        let Some(reference) = &seat.template_ref else {
            continue;
        };
        let Some((_, owner)) = read_template(tree, &reference.template)? else {
            continue;
        };
        let member = owner.members.iter().find(|m| m.id == reference.member);
        if member.and_then(|m| m.seat_template.as_ref()) != Some(&old.id) {
            continue;
        }
        if read_ts(tree, &seat.teamspace)?.rec.lifecycle == Lifecycle::Retired {
            continue;
        }
        dependencies.push(rev_of(owner.id.clone(), owner.rev));
        dependencies.push(rev_of(seat.id.clone(), seat.rev));
        let old_cfg =
            resolve_with_seat_template(&graph.defaults, Some(&owner), member, Some(old), &seat);
        let new_cfg =
            resolve_with_seat_template(&graph.defaults, Some(&owner), member, Some(new), &seat);
        let clones = layout::list_clones(tree, &loc.folder)?
            .into_iter()
            .map(|(_, c)| c)
            .collect::<Vec<_>>();
        dependencies.extend(clones.iter().map(|c| rev_of(c.id.clone(), c.rev)));
        seats.insert(
            seat.id.clone(),
            session_replacements(&seat, &clones, &old_cfg, &new_cfg),
        );
    }
    for (_, app) in layout::list_applications(tree)? {
        if app.lifecycle != AppLifecycle::Active {
            continue;
        }
        let Some((_, team)) = read_template(tree, &app.template)? else {
            continue;
        };
        let members = membership(&app);
        let consumes = team
            .members
            .iter()
            .any(|m| m.seat_template.as_ref() == Some(&old.id))
            || members.iter().any(|id| seats.contains_key(id));
        if !consumes {
            continue;
        }
        let ts = read_ts(tree, &app.teamspace)?;
        if ts.rec.lifecycle == Lifecycle::Retired {
            continue;
        }
        let replaced_seats = members
            .into_iter()
            .filter(|id| seats.get(id).is_some_and(|e| !e.is_empty()))
            .collect();
        apps.push(AppEdit {
            app,
            ts,
            withdraw: vec![],
            adds: vec![],
            rel_added: vec![],
            rel_removed: vec![],
            replaced_seats,
        });
    }
    Ok(EditPlan {
        apps,
        replacements: seats.into_values().flatten().collect(),
        dependencies,
        warnings: vec![],
        repair_required: None,
    })
}
