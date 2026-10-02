//! Small CLI-word helpers shared by every organizational kind, and object-reference resolution.
use super::kind::PlanError;
use crate::model::{AnyId, CloneId, CloneLifecycle, IdKind, Lifecycle, SeatId, TeamspaceId};
use crate::store::layout;
use crate::store::tree::TreeRead;

/// Flags that take no value; every other `--flag` consumes the following word.
pub const BOOLEAN_FLAGS: &[&str] = &["--active", "--dormant", "--json", "--yes"];

/// Value of `--name <value>` or `--name=<value>`.
pub fn flag(words: &[String], name: &str) -> Option<String> {
    let eq = format!("{name}=");
    for (i, w) in words.iter().enumerate() {
        if w == name {
            return words.get(i + 1).filter(|v| !v.starts_with("--")).cloned();
        }
        if let Some(v) = w.strip_prefix(&eq) {
            return Some(v.to_owned());
        }
    }
    None
}

/// Whether the bare flag `--x` is present.
pub fn has(words: &[String], name: &str) -> bool {
    words.iter().any(|w| w == name)
}

/// The `i`-th positional word: words that are neither flags nor the value of a value-taking flag.
pub fn positional(words: &[String], i: usize) -> Option<String> {
    positionals(words).into_iter().nth(i)
}

pub fn positionals(words: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut skip = false;
    for w in words {
        if skip {
            skip = false;
            continue;
        }
        if w.starts_with("--") {
            skip = !w.contains('=') && !BOOLEAN_FLAGS.contains(&w.as_str());
            continue;
        }
        out.push(w.clone());
    }
    out
}

/// Which side of retirement a name reference may resolve in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Live,
    Retired,
}

fn pick<T: std::fmt::Display>(what: &str, s: &str, mut found: Vec<(T, String)>) -> Result<T, PlanError> {
    match found.len() {
        0 => Err(PlanError::Invalid(format!("no {what} named {s:?}"))),
        1 => Ok(found.remove(0).0),
        _ => {
            let list: Vec<String> = found.iter().map(|(id, ctx)| format!("{id} ({ctx})")).collect();
            Err(PlanError::Invalid(format!("{what} name {s:?} is ambiguous: {}; use an id", list.join(", "))))
        }
    }
}

pub fn resolve_teamspace(tree: &dyn TreeRead, s: &str, scope: Scope) -> Result<TeamspaceId, PlanError> {
    if let Ok(id) = TeamspaceId::parse(s) {
        return match layout::locate(tree, &id.to_any())? {
            Some(_) => Ok(id),
            None => Err(PlanError::Invalid(format!("no teamspace {id}"))),
        };
    }
    let found = layout::list_teamspaces(tree)?
        .into_iter()
        .filter(|(_, t)| t.name == s && (t.lifecycle == Lifecycle::Retired) == (scope == Scope::Retired))
        .map(|(_, t)| (t.id, "teamspace".to_owned()))
        .collect();
    pick("teamspace", s, found)
}

pub fn resolve_seat(tree: &dyn TreeRead, s: &str, scope: Scope) -> Result<SeatId, PlanError> {
    if let Ok(id) = SeatId::parse(s) {
        return match layout::locate(tree, &id.to_any())? {
            Some(_) => Ok(id),
            None => Err(PlanError::Invalid(format!("no seat {id}"))),
        };
    }
    let teamspaces = layout::list_teamspaces(tree)?;
    let found = layout::all_seats(tree)?
        .into_iter()
        .filter(|(_, r)| r.name == s && (r.lifecycle == Lifecycle::Retired) == (scope == Scope::Retired))
        .map(|(_, r)| {
            let ts = teamspaces.iter().find(|(_, t)| t.id == r.teamspace).map(|(_, t)| t.name.clone());
            (r.id, format!("teamspace {}", ts.unwrap_or_default()))
        })
        .collect();
    pick("seat", s, found)
}

pub fn resolve_clone(tree: &dyn TreeRead, s: &str, scope: Scope) -> Result<CloneId, PlanError> {
    if let Ok(id) = CloneId::parse(s) {
        return match layout::locate(tree, &id.to_any())? {
            Some(_) => Ok(id),
            None => Err(PlanError::Invalid(format!("no clone {id}"))),
        };
    }
    let seats = layout::all_seats(tree)?;
    let found = layout::all_clones(tree)?
        .into_iter()
        .filter(|(_, c)| c.name == s && (c.lifecycle == CloneLifecycle::Retired) == (scope == Scope::Retired))
        .map(|(_, c)| {
            let seat = seats.iter().find(|(_, st)| st.id == c.seat).map(|(_, st)| st.name.clone());
            (c.id, format!("seat {}", seat.unwrap_or_default()))
        })
        .collect();
    pick("clone", s, found)
}

/// An id, or a name resolved uniquely among the live teamspaces, seats and clones.
pub fn resolve_object(tree: &dyn TreeRead, s: &str) -> Result<AnyId, PlanError> {
    if let Ok(id) = AnyId::parse(s) {
        return match layout::locate(tree, &id)? {
            Some(_) => Ok(id),
            None if matches!(id.kind(), IdKind::Member | IdKind::NativeSession | IdKind::Effect | IdKind::Plan) => Ok(id),
            None => Err(PlanError::Invalid(format!("no object {id}"))),
        };
    }
    let mut found: Vec<(AnyId, String)> = Vec::new();
    for (_, t) in layout::list_teamspaces(tree)? {
        if t.name == s && t.lifecycle != Lifecycle::Retired {
            found.push((t.id.to_any(), "teamspace".into()));
        }
    }
    for (_, r) in layout::all_seats(tree)? {
        if r.name == s && r.lifecycle != Lifecycle::Retired {
            found.push((r.id.to_any(), "seat".into()));
        }
    }
    for (_, c) in layout::all_clones(tree)? {
        if c.name == s && c.lifecycle != CloneLifecycle::Retired {
            found.push((c.id.to_any(), "clone".into()));
        }
    }
    pick("object", s, found)
}
