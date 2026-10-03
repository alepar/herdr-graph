//! Undo candidates: recent undoable `act_` records, newest first (spec §6).
use crate::model::action::{ActionKind, ActionRecord};
use crate::model::{ActionId, AnyId, IdKind, Timestamp};
use crate::ports::store::StoreError;
use crate::store::layout;
use crate::store::record::read_toml;
use crate::store::tree::TreeRead;
use serde::Serialize;
use std::collections::BTreeMap;

pub const DEFAULT_LIMIT: usize = 20;

/// One undoable action. `index` is the 1-based number the CLI selects by.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Candidate {
    pub index: usize,
    pub act: ActionId,
    pub kind: ActionKind,
    pub at: Timestamp,
    pub summary: String,
    pub undone: bool,
}

/// Closure cascades, retirements, hydrations, template edits and application retirements can be undone;
/// resurrections and undos are history only.
pub fn is_undoable(kind: ActionKind) -> bool {
    matches!(
        kind,
        ActionKind::ClosureCascade
            | ActionKind::Retire
            | ActionKind::Hydrate
            | ActionKind::TemplateEdit
            | ActionKind::ApplicationRetire
    )
}

/// The action record `act`, or None when it does not exist at this revision.
pub fn read_action(
    tree: &dyn TreeRead,
    act: &ActionId,
) -> Result<Option<ActionRecord>, StoreError> {
    let Some(loc) = layout::locate(tree, &act.to_any())? else {
        return Ok(None);
    };
    read_toml(tree, &loc.record_path)
}

/// Display names of graph objects, so summaries read as names rather than ids.
struct Names(BTreeMap<AnyId, String>);

impl Names {
    fn build(tree: &dyn TreeRead) -> Result<Self, StoreError> {
        let mut m = BTreeMap::new();
        for (_, t) in layout::list_teamspaces(tree)? {
            m.insert(t.id.to_any(), t.name);
        }
        for (_, s) in layout::all_seats(tree)? {
            m.insert(s.id.to_any(), s.name);
        }
        for (_, a) in layout::list_applications(tree)? {
            m.insert(a.id.to_any(), a.name);
        }
        for (_, t) in layout::list_templates(tree)? {
            m.insert(t.id.to_any(), t.name);
        }
        Ok(Self(m))
    }

    fn of(&self, id: &AnyId) -> String {
        self.0.get(id).cloned().unwrap_or_else(|| id.to_string())
    }

    fn of_str(&self, s: Option<&str>) -> String {
        s.and_then(|s| AnyId::parse(s).ok())
            .map(|i| self.of(&i))
            .unwrap_or_default()
    }
}

fn counts(a: &ActionRecord) -> String {
    let n = |k: IdKind| a.retired.iter().filter(|i| i.kind() == k).count();
    let mut parts = Vec::new();
    for (k, word) in [
        (IdKind::Teamspace, "teamspace"),
        (IdKind::Seat, "seat"),
        (IdKind::Clone, "clone"),
        (IdKind::Application, "application"),
    ] {
        match n(k) {
            0 => {}
            1 => parts.push(format!("1 {word}")),
            c => parts.push(format!("{c} {word}s")),
        }
    }
    if parts.is_empty() {
        "nothing".into()
    } else {
        parts.join(", ")
    }
}

fn first_named(a: &ActionRecord, names: &Names) -> String {
    a.retired.first().map(|i| names.of(i)).unwrap_or_default()
}

fn comp_str<'a>(a: &'a ActionRecord, key: &str) -> Option<&'a str> {
    a.compensation.get(key).and_then(|v| v.as_str())
}

fn summarize(a: &ActionRecord, names: &Names) -> String {
    match a.kind {
        ActionKind::ClosureCascade => format!(
            "{} closure retired {} ({})",
            comp_str(a, "rule").unwrap_or("observed"),
            counts(a),
            first_named(a, names)
        ),
        ActionKind::Retire => format!("retired {} ({})", counts(a), first_named(a, names)),
        ActionKind::ApplicationRetire => {
            format!(
                "retired application {}; withdrew {}",
                names.of_str(comp_str(a, "application")),
                counts(a)
            )
        }
        ActionKind::Hydrate => {
            let created = a
                .compensation
                .get("created")
                .and_then(|v| v.as_array())
                .map_or(0, |v| v.len());
            format!(
                "applied template as {} ({created} objects created)",
                names.of_str(comp_str(a, "application"))
            )
        }
        ActionKind::TemplateEdit => {
            let fields: Vec<&str> = a
                .compensation
                .get("changed_fields")
                .and_then(|v| v.as_array())
                .map(|v| v.iter().filter_map(|x| x.as_str()).collect())
                .unwrap_or_default();
            format!(
                "edited template {} ({})",
                names.of_str(comp_str(a, "template")),
                fields.join(", ")
            )
        }
        ActionKind::Resurrect => "resurrection".into(),
        ActionKind::Undo => "undo".into(),
    }
}

/// Up to `limit` undoable actions, newest first; already-undone ones are listed and marked.
pub fn list_candidates(tree: &dyn TreeRead, limit: usize) -> Result<Vec<Candidate>, StoreError> {
    let mut acts: Vec<ActionRecord> = layout::list_actions(tree)?
        .into_iter()
        .map(|(_, a)| a)
        .filter(|a| is_undoable(a.kind))
        .collect();
    acts.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| b.id.cmp(&a.id)));
    acts.truncate(limit);
    let names = Names::build(tree)?;
    Ok(acts
        .into_iter()
        .enumerate()
        .map(|(i, a)| Candidate {
            index: i + 1,
            summary: summarize(&a, &names),
            undone: !a.undone_by.is_empty(),
            act: a.id,
            kind: a.kind,
            at: a.at,
        })
        .collect())
}

/// Numbered text listing: `<n>. <act> <time> <summary>` with ` [undone]` on undone actions.
pub fn render_list(list: &[Candidate]) -> String {
    if list.is_empty() {
        return "nothing to undo\n".into();
    }
    let mut out = String::new();
    for c in list {
        out.push_str(&format!(
            "{:>3}. {} {} {}{}\n",
            c.index,
            c.act,
            c.at.format("%Y-%m-%d %H:%M:%S"),
            c.summary,
            if c.undone { " [undone]" } else { "" }
        ));
    }
    out
}
