//! Scope: the inspection views' shared scope grammar, resolution and membership.
use crate::model::{AnyId, CloneId, CloneLifecycle, CommitId, SeatId, TeamspaceId};
use crate::ports::store::{RepoPath, Store, StoreError};
use crate::store::instruction_source::{
    InstructionError, InstructionScope, PickMode, ScopeKind, parse_words, pick, resolve_scope_in,
};
use crate::store::layout;
use crate::store::tree::{CommitView, TreeRead};

/// A resolved scope: `global | teamspace <t> | seat <s> | clone <c>`, or the whole instance when omitted.
///
/// Owns: the scope grammar ([`ScopeArg::parse`]), name-or-id resolution at a given commit (retired and
/// `archive/` objects included, one rule for every view) and object membership ([`Scope::contains`]).
/// Path membership at a commit's own tree ([`Scope::paths_at`], [`Scope::contains_object_at`]) lives here too.
///
/// Must not: be confused with `InstructionScope` (that is the instruction-file scope and refuses retired
/// names); filter by object kind or requester (that is LogQuery); resolve HEAD (it takes a `CommitId`).
///
/// Where new behavior goes: callers never map scopes to folders or match names themselves. A new scope kind
/// is a new variant here, with its parse, resolve and membership rules beside the others.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    Instance,
    Global,
    Teamspace(TeamspaceId),
    Seat(SeatId),
    Clone(CloneId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScopeArgKind {
    Instance,
    Global,
    Teamspace,
    Seat,
    Clone,
}

/// Parsed, not yet resolved: the scope word and its target text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeArg {
    kind: ScopeArgKind,
    target: Option<String>,
}

/// Whether the command takes `clone <c>` (spec §2.5: `sessions` only). A deliberate mode flag, kept in the
/// parser alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllowClone {
    Yes,
    No,
}

/// An object asked about, with its ancestors' ids.
#[derive(Debug, Clone, Copy)]
pub enum Member<'a> {
    Teamspace(&'a TeamspaceId),
    Seat {
        teamspace: &'a TeamspaceId,
        seat: &'a SeatId,
    },
    Clone {
        teamspace: &'a TeamspaceId,
        seat: &'a SeatId,
        clone: &'a CloneId,
    },
}

fn usage(m: &str) -> InstructionError {
    InstructionError::Usage(m.to_owned())
}

impl ScopeArg {
    pub fn parse(words: &[String], allow: AllowClone) -> Result<ScopeArg, InstructionError> {
        let Some(first) = words.first() else {
            return Ok(ScopeArg {
                kind: ScopeArgKind::Instance,
                target: None,
            });
        };
        if first == "clone" {
            if allow == AllowClone::No {
                return Err(usage(
                    "clone scope is only for sessions; expected global|teamspace|seat",
                ));
            }
            return match &words[1..] {
                [] => Err(usage("clone needs a name or id")),
                [c] => Ok(ScopeArg {
                    kind: ScopeArgKind::Clone,
                    target: Some(c.clone()),
                }),
                _ => Err(usage("too many words")),
            };
        }
        // Scopes take no file: `global` is one word, `teamspace <t>` and `seat <s>` two.
        let max = if first == "global" { 1 } else { 2 };
        if words.len() > max {
            return Err(usage("too many words"));
        }
        let r = parse_words(words, false)?;
        Ok(ScopeArg {
            kind: match r.scope {
                ScopeKind::Global => ScopeArgKind::Global,
                ScopeKind::Teamspace => ScopeArgKind::Teamspace,
                ScopeKind::Seat => ScopeArgKind::Seat,
            },
            target: r.target,
        })
    }
}

impl Scope {
    /// Resolve at `at` (the invocation's pinned HEAD): ids resolve live or retired; a name resolves to its one
    /// live match, else its one retired match; several live (or, with none live, several retired) matches are
    /// ambiguous and list every id. Errors and their wording are `instructions show`'s.
    pub fn resolve(
        arg: &ScopeArg,
        store: &dyn Store,
        at: &CommitId,
    ) -> Result<Scope, InstructionError> {
        let view = CommitView {
            store,
            at: at.clone(),
        };
        let target = arg.target.as_deref();
        let instruction = |kind| resolve_scope_in(&view, kind, target, PickMode::RetiredAllowed);
        Ok(match arg.kind {
            ScopeArgKind::Instance => Scope::Instance,
            ScopeArgKind::Global => Scope::Global,
            ScopeArgKind::Teamspace => match instruction(ScopeKind::Teamspace)? {
                InstructionScope::Teamspace(id) => Scope::Teamspace(id),
                _ => unreachable!("teamspace kind resolves to a teamspace"),
            },
            ScopeArgKind::Seat => match instruction(ScopeKind::Seat)? {
                InstructionScope::Seat(id) => Scope::Seat(id),
                _ => unreachable!("seat kind resolves to a seat"),
            },
            ScopeArgKind::Clone => {
                let t = target.ok_or_else(|| usage("clone needs a name or id"))?;
                Scope::Clone(resolve_clone(&view, t)?)
            }
        })
    }

    /// Whether the object is in scope. `Instance` holds everything; `Global` holds no teamspace, seat or
    /// clone (only the global paths); `Teamspace(t)` holds its seats and clones and the teamspace itself;
    /// `Seat(s)` holds the seat and its clones, not the teamspace object; `Clone(c)` holds only that clone.
    pub fn contains(&self, m: Member<'_>) -> bool {
        match (self, m) {
            (Scope::Instance, _) => true,
            (Scope::Global, _) => false,
            (Scope::Teamspace(t), Member::Teamspace(ts)) => t == ts,
            (Scope::Teamspace(t), Member::Seat { teamspace, .. })
            | (Scope::Teamspace(t), Member::Clone { teamspace, .. }) => t == teamspace,
            (Scope::Seat(s), Member::Seat { seat, .. })
            | (Scope::Seat(s), Member::Clone { seat, .. }) => s == seat,
            (Scope::Clone(c), Member::Clone { clone, .. }) => c == clone,
            _ => false,
        }
    }
}

/// The operation-record directory every commit adds to; never part of any scope (spec §2.4).
const GRAPH_CHANGES: &str = "mutations/graph-changes/";

/// Path membership of a scope at one commit's own tree (spec §2.4): built by [`Scope::paths_at`], so renames
/// and moves are attributed by the tree the commit was made in, not by today's folders.
#[derive(Debug, Clone)]
pub struct ScopePaths {
    all: bool,
    exact: Vec<String>,
    prefixes: Vec<String>,
    seat_id: Option<String>,
}

impl ScopePaths {
    pub fn contains(&self, p: &RepoPath) -> bool {
        let p = p.as_str();
        if p.starts_with(GRAPH_CHANGES) {
            return false;
        }
        if self.all || self.exact.iter().any(|e| e == p) {
            return true;
        }
        let under = |dir: &str| {
            p == dir || (p.starts_with(dir) && p.as_bytes().get(dir.len()) == Some(&b'/'))
        };
        if self.prefixes.iter().any(|d| under(d)) {
            return true;
        }
        // `transcripts/<seat>/..` (legacy) or `transcripts/<slug>-<ts>/<slug>-<seat>/..` (registration).
        match (&self.seat_id, p.strip_prefix("transcripts/")) {
            (Some(id), Some(rest)) => {
                let tail = format!("-{id}");
                rest.split('/')
                    .any(|c| c == id.as_str() || c.ends_with(&tail))
            }
            _ => false,
        }
    }
}

impl Scope {
    /// This scope's path membership at `tree` (a commit's own tree). `Instance` holds every path but the
    /// operation records; `Global` the fixed global set; `Teamspace`/`Seat`/`Clone` the folder their id
    /// resolves to in `tree` (live or `archive/`), none when the id is absent there; `Seat` also the
    /// transcripts carrying its id.
    pub fn paths_at(&self, tree: &dyn TreeRead) -> Result<ScopePaths, StoreError> {
        let mut sp = ScopePaths {
            all: false,
            exact: vec![],
            prefixes: vec![],
            seat_id: None,
        };
        let mut folder = |id: AnyId| -> Result<(), StoreError> {
            if let Some(loc) = layout::locate(tree, &id)? {
                sp.prefixes.push(loc.folder.as_str().to_owned());
            }
            Ok(())
        };
        match self {
            Scope::Instance => sp.all = true,
            Scope::Global => {
                sp.exact = vec!["graph.toml".into(), "threads-namespace.json".into()];
                sp.prefixes = vec!["rules".into(), "templates".into(), "applications".into()];
            }
            Scope::Teamspace(t) => folder(t.to_any())?,
            Scope::Seat(s) => {
                folder(s.to_any())?;
                sp.seat_id = Some(s.to_string());
            }
            Scope::Clone(c) => folder(c.to_any())?,
        }
        Ok(sp)
    }

    /// Whether an object an action names (affected or retired) is in scope: located at `tree`, else at
    /// `fallback` (the commit's parent tree, for an object that commit deleted), and its record path is
    /// contained by that same tree's [`ScopePaths`]. Not found in either: false.
    pub fn contains_object_at(
        &self,
        tree: &dyn TreeRead,
        fallback: Option<&dyn TreeRead>,
        id: &AnyId,
    ) -> Result<bool, StoreError> {
        for t in std::iter::once(tree).chain(fallback) {
            if let Some(loc) = layout::locate(t, id)? {
                return Ok(self.paths_at(t)?.contains(&loc.record_path));
            }
        }
        Ok(false)
    }
}

fn resolve_clone(view: &CommitView<'_>, target: &str) -> Result<CloneId, InstructionError> {
    let all = layout::all_clones(view)?;
    if let Ok(id) = CloneId::parse(target) {
        return all
            .iter()
            .find(|(_, r)| r.id == id)
            .map(|_| id)
            .ok_or_else(|| InstructionError::UnknownTarget {
                what: "clone".to_owned(),
                target: target.to_owned(),
            });
    }
    let matches = all
        .iter()
        .filter(|(_, r)| r.name == target)
        .map(|(_, r)| {
            (
                r.id.clone(),
                r.id.to_string(),
                r.lifecycle != CloneLifecycle::Retired,
            )
        })
        .collect();
    pick("clone", target, matches, PickMode::RetiredAllowed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::instructions::show_text;
    use crate::inspect::fixture::Fx;
    use crate::model::Lifecycle;
    use crate::ports::store::Store;

    fn words(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_owned).collect()
    }

    fn arg(s: &str) -> ScopeArg {
        ScopeArg::parse(&words(s), AllowClone::Yes).unwrap()
    }

    fn resolve(fx: &Fx, s: &str) -> Result<Scope, InstructionError> {
        let at = fx.store.head().unwrap();
        Scope::resolve(&arg(s), &fx.store, &at)
    }

    // pick: the exposed function directly.

    fn m(id: &str, live: bool) -> (String, String, bool) {
        (id.to_owned(), id.to_owned(), live)
    }

    #[test]
    fn pick_live_only_keeps_retired_error() {
        let r = pick("seat", "x", vec![m("a", false)], PickMode::LiveOnly);
        assert!(matches!(r, Err(InstructionError::Retired { .. })), "{r:?}");
        let r = pick(
            "seat",
            "x",
            vec![m("a", false), m("b", false)],
            PickMode::LiveOnly,
        );
        assert!(matches!(r, Err(InstructionError::Retired { .. })), "{r:?}");
    }

    #[test]
    fn pick_retired_allowed_rule() {
        let p = |v| pick("seat", "x", v, PickMode::RetiredAllowed);
        assert_eq!(p(vec![m("a", true), m("b", false)]).unwrap(), "a");
        assert_eq!(p(vec![m("b", false), m("a", true)]).unwrap(), "a");
        let e = p(vec![m("a", true), m("b", true)]).unwrap_err();
        assert!(matches!(e, InstructionError::Ambiguous(_)), "{e:?}");
        assert_eq!(p(vec![m("r", false)]).unwrap(), "r");
        let e = p(vec![m("r1", false), m("r2", false)]).unwrap_err();
        assert!(
            matches!(&e, InstructionError::Ambiguous(s) if s.contains("r1") && s.contains("r2")),
            "{e:?}"
        );
        let e = p(vec![]).unwrap_err();
        assert!(matches!(e, InstructionError::UnknownTarget { .. }), "{e:?}");
    }

    #[test]
    fn instructions_show_still_rejects_retired_name() {
        let mut fx = Fx::new();
        let t = fx.teamspace("alpha", Lifecycle::Active);
        fx.archived_seat(&t, "gone");
        fx.commit("c");
        let view = CommitView {
            store: &fx.store,
            at: fx.store.head().unwrap(),
        };
        let e =
            crate::store::instruction_source::resolve_scope(&view, ScopeKind::Seat, Some("gone"))
                .unwrap_err();
        assert!(matches!(e, InstructionError::Retired { .. }), "{e:?}");
    }

    #[test]
    fn parse_grammar() {
        let p = |s: &str, a| ScopeArg::parse(&words(s), a);
        let k = |a: ScopeArg| (a.kind, a.target);
        assert_eq!(
            k(p("", AllowClone::Yes).unwrap()),
            (ScopeArgKind::Instance, None)
        );
        assert_eq!(
            k(p("global", AllowClone::No).unwrap()),
            (ScopeArgKind::Global, None)
        );
        assert_eq!(
            k(p("teamspace t", AllowClone::No).unwrap()),
            (ScopeArgKind::Teamspace, Some("t".into()))
        );
        assert_eq!(
            k(p("seat s", AllowClone::No).unwrap()),
            (ScopeArgKind::Seat, Some("s".into()))
        );
        assert_eq!(
            k(p("clone c", AllowClone::Yes).unwrap()),
            (ScopeArgKind::Clone, Some("c".into()))
        );
        for (bad, allow) in [
            ("clone c", AllowClone::No),
            ("clone", AllowClone::Yes),
            ("clone c d", AllowClone::Yes),
            ("bogus", AllowClone::Yes),
            ("seat", AllowClone::Yes),
            ("seat s AGENTS.md", AllowClone::Yes),
            ("teamspace t rules/x.md", AllowClone::Yes),
            ("global x", AllowClone::Yes),
        ] {
            let e = p(bad, allow).unwrap_err();
            assert!(matches!(e, InstructionError::Usage(_)), "{bad}: {e:?}");
        }
        let e = p("clone c", AllowClone::No).unwrap_err().to_string();
        assert!(e.contains("only for sessions"), "{e}");
    }

    #[test]
    fn resolve_live_and_retired() {
        let mut fx = Fx::new();
        let t1 = fx.teamspace("alpha", Lifecycle::Active);
        let t2 = fx.teamspace("beta", Lifecycle::Active);
        let at = fx.archived_teamspace("old");
        // Seats: retired-only name, two retired-only, live beats retired, two live.
        let solo = fx.archived_seat(&t1, "solo");
        let r1 = fx.archived_seat(&t1, "twin");
        let r2 = fx.archived_seat(&t2, "twin");
        let live = fx.seat(&t1, "mixed", Lifecycle::Active);
        fx.archived_seat(&t2, "mixed");
        let l1 = fx.seat(&t1, "dup", Lifecycle::Active);
        let l2 = fx.seat(&t2, "dup", Lifecycle::Dormant);
        let in_archived_ts = fx.archived_seat(&at, "deep");
        // Clones: same set.
        let s = fx.seat(&t1, "host", Lifecycle::Active);
        let csolo = fx.clone_rec(&s, "csolo", CloneLifecycle::Retired, vec![], None);
        let c1 = fx.clone_rec(&s, "ctwin", CloneLifecycle::Retired, vec![], None);
        let c2 = fx.clone_rec(&s, "ctwin", CloneLifecycle::Retired, vec![], None);
        let clive = fx.clone_rec(&s, "cmixed", CloneLifecycle::Active, vec![], None);
        fx.clone_rec(&s, "cmixed", CloneLifecycle::Retired, vec![], None);
        let ca = fx.clone_rec(&s, "cdup", CloneLifecycle::Active, vec![], None);
        let cb = fx.clone_rec(&s, "cdup", CloneLifecycle::Active, vec![], None);
        let tomb = fx.tombstone(&s, "tomb");
        fx.commit("c");

        let seat = |r: Result<Scope, _>| match r {
            Ok(Scope::Seat(id)) => id,
            other => panic!("{other:?}"),
        };
        assert_eq!(seat(resolve(&fx, "seat solo")), solo);
        assert_eq!(seat(resolve(&fx, "seat mixed")), live);
        assert_eq!(seat(resolve(&fx, &format!("seat {solo}"))), solo);
        assert_eq!(seat(resolve(&fx, "seat deep")), in_archived_ts);
        let e = resolve(&fx, "seat twin").unwrap_err().to_string();
        assert!(
            e.contains("ambiguous") && e.contains(r1.as_str()) && e.contains(r2.as_str()),
            "{e}"
        );
        let e = resolve(&fx, "seat dup").unwrap_err().to_string();
        assert!(e.contains(l1.as_str()) && e.contains(l2.as_str()), "{e}");
        // Dormant is live for matching: both listed above. Unknown:
        assert!(matches!(
            resolve(&fx, "seat nobody").unwrap_err(),
            InstructionError::UnknownTarget { .. }
        ));
        assert!(matches!(
            resolve(&fx, &format!("seat {}", SeatId::new())).unwrap_err(),
            InstructionError::UnknownTarget { .. }
        ));

        // Teamspaces: archived resolves by name and id; live by name.
        assert_eq!(
            resolve(&fx, "teamspace old").unwrap(),
            Scope::Teamspace(at.clone())
        );
        assert_eq!(
            resolve(&fx, &format!("teamspace {at}")).unwrap(),
            Scope::Teamspace(at)
        );
        assert_eq!(
            resolve(&fx, "teamspace alpha").unwrap(),
            Scope::Teamspace(t1)
        );

        let clone = |r: Result<Scope, _>| match r {
            Ok(Scope::Clone(id)) => id,
            other => panic!("{other:?}"),
        };
        assert_eq!(clone(resolve(&fx, "clone csolo")), csolo);
        assert_eq!(clone(resolve(&fx, "clone cmixed")), clive);
        assert_eq!(clone(resolve(&fx, &format!("clone {csolo}"))), csolo);
        assert_eq!(clone(resolve(&fx, "clone tomb")), tomb);
        let e = resolve(&fx, "clone ctwin").unwrap_err().to_string();
        assert!(e.contains(c1.as_str()) && e.contains(c2.as_str()), "{e}");
        let e = resolve(&fx, "clone cdup").unwrap_err().to_string();
        assert!(e.contains(ca.as_str()) && e.contains(cb.as_str()), "{e}");
        assert!(matches!(
            resolve(&fx, "clone nobody").unwrap_err(),
            InstructionError::UnknownTarget { .. }
        ));
        assert!(matches!(
            resolve(&fx, &format!("clone {}", CloneId::new())).unwrap_err(),
            InstructionError::UnknownTarget { .. }
        ));
        assert_eq!(resolve(&fx, "").unwrap(), Scope::Instance);
        assert_eq!(resolve(&fx, "global").unwrap(), Scope::Global);
    }

    #[test]
    fn resolve_uses_given_commit() {
        let mut fx = Fx::new();
        let t = fx.teamspace("alpha", Lifecycle::Active);
        fx.seat(&t, "old", Lifecycle::Active);
        let a = fx.commit("a");
        let added = fx.seat(&t, "fresh", Lifecycle::Active);
        let b = fx.commit("b");
        assert_ne!(a, b);
        let at_a = Scope::resolve(&arg("seat fresh"), &fx.store, &a);
        assert!(
            matches!(at_a, Err(InstructionError::UnknownTarget { .. })),
            "{at_a:?}"
        );
        assert_eq!(
            Scope::resolve(&arg("seat fresh"), &fx.store, &b).unwrap(),
            Scope::Seat(added)
        );
    }

    #[test]
    fn membership() {
        let (t, t2) = (TeamspaceId::new(), TeamspaceId::new());
        let (s, s2) = (SeatId::new(), SeatId::new());
        let (c, c2) = (CloneId::new(), CloneId::new());
        let (s3, c3) = (SeatId::new(), CloneId::new());
        // (scope, [teamspace t, teamspace t2, seat s in t, seat s2 in t, seat in t2, clone c in s, clone c2 in s2, clone in t2])
        let rows: Vec<(Scope, [bool; 8])> = vec![
            (Scope::Instance, [true; 8]),
            (Scope::Global, [false; 8]),
            (
                Scope::Teamspace(t.clone()),
                [true, false, true, true, false, true, true, false],
            ),
            (
                Scope::Seat(s.clone()),
                [false, false, true, false, false, true, false, false],
            ),
            (
                Scope::Clone(c.clone()),
                [false, false, false, false, false, true, false, false],
            ),
        ];
        for (scope, want) in rows {
            let seat_m = |teamspace, seat| Member::Seat { teamspace, seat };
            let got = [
                scope.contains(Member::Teamspace(&t)),
                scope.contains(Member::Teamspace(&t2)),
                scope.contains(seat_m(&t, &s)),
                scope.contains(seat_m(&t, &s2)),
                scope.contains(seat_m(&t2, &s3)),
                scope.contains(Member::Clone {
                    teamspace: &t,
                    seat: &s,
                    clone: &c,
                }),
                scope.contains(Member::Clone {
                    teamspace: &t,
                    seat: &s2,
                    clone: &c2,
                }),
                scope.contains(Member::Clone {
                    teamspace: &t2,
                    seat: &s3,
                    clone: &c3,
                }),
            ];
            assert_eq!(got, want, "{scope:?}");
        }
    }

    #[test]
    fn errors_match_instructions_show() {
        let mut fx = Fx::new();
        let t1 = fx.teamspace("alpha", Lifecycle::Active);
        let t2 = fx.teamspace("beta", Lifecycle::Active);
        fx.seat(&t1, "dup", Lifecycle::Active);
        fx.seat(&t2, "dup", Lifecycle::Active);
        fx.commit("c");
        let view = CommitView {
            store: &fx.store,
            at: fx.store.head().unwrap(),
        };
        for case in ["seat nobody", "seat dup", "teamspace nobody"] {
            let shown = show_text(&view, &words(case)).unwrap_err();
            let scoped = resolve(&fx, case).unwrap_err();
            assert_eq!(scoped.to_string(), shown.to_string(), "{case}");
            let scoped: anyhow::Error = scoped.into();
            assert_eq!(
                format!("{:?}", crate::inspect::fail(&scoped)),
                format!("{:?}", std::process::ExitCode::from(1)),
                "{case}"
            );
        }
    }

    // Path membership at a commit's own tree (hg-blw.5).

    fn rp(s: &str) -> RepoPath {
        RepoPath::new(s).unwrap()
    }

    fn paths_at(fx: &Fx, scope: &Scope, c: &CommitId) -> ScopePaths {
        scope.paths_at(&fx.store.at(c)).unwrap()
    }

    #[test]
    fn paths_global_vs_instance() {
        let mut fx = Fx::new();
        let c = fx.commit("c");
        let g = paths_at(&fx, &Scope::Global, &c);
        for yes in [
            "graph.toml",
            "threads-namespace.json",
            "rules/a.md",
            "templates/t/template.toml",
            "applications/app_x.toml",
        ] {
            assert!(g.contains(&rp(yes)), "{yes}");
        }
        for no in ["teamspaces/a/teamspace.toml", "rulesx/a.md", "other"] {
            assert!(!g.contains(&rp(no)), "{no}");
        }
        let i = paths_at(&fx, &Scope::Instance, &c);
        assert!(i.contains(&rp("teamspaces/a/teamspace.toml")));
        assert!(i.contains(&rp("graph.toml")));
    }

    #[test]
    fn graph_changes_never_in_scope() {
        let mut fx = Fx::new();
        let t = fx.teamspace("alpha", Lifecycle::Active);
        let c = fx.commit("c");
        let op = rp("mutations/graph-changes/2026-11/op_x.toml");
        for scope in [Scope::Instance, Scope::Global, Scope::Teamspace(t.clone())] {
            assert!(!paths_at(&fx, &scope, &c).contains(&op), "{scope:?}");
        }
        // The undoable-actions folder is not excluded from the instance.
        assert!(
            paths_at(&fx, &Scope::Instance, &c)
                .contains(&rp("mutations/undoable-actions/2026-11/act_x.toml"))
        );
    }

    #[test]
    fn paths_follow_rename_at_commit() {
        let mut fx = Fx::new();
        let t = fx.teamspace("alpha", Lifecycle::Active);
        let s = fx.seat(&t, "old", Lifecycle::Active);
        let a = fx.commit("a");
        fx.rename_seat(&s, "new");
        let b = fx.commit("b");
        let scope = Scope::Seat(s);
        let old_file = rp("teamspaces/alpha/seats/old/notes.md");
        let new_file = rp("teamspaces/alpha/seats/new/notes.md");
        let at_a = paths_at(&fx, &scope, &a);
        assert!(at_a.contains(&old_file) && !at_a.contains(&new_file));
        let at_b = paths_at(&fx, &scope, &b);
        assert!(at_b.contains(&new_file) && !at_b.contains(&old_file));
    }

    #[test]
    fn paths_archived_seat() {
        let mut fx = Fx::new();
        let t = fx.teamspace("alpha", Lifecycle::Active);
        let s = fx.archived_seat(&t, "gone");
        let live = fx.seat(&t, "live", Lifecycle::Active);
        let c = fx.commit("c");
        let p = paths_at(&fx, &Scope::Seat(s.clone()), &c);
        let loc = layout::locate(&fx.store.at(&c), &s.to_any())
            .unwrap()
            .unwrap();
        assert!(loc.folder.as_str().contains("archive"), "{loc:?}");
        assert!(p.contains(&loc.folder.join("seat.toml").unwrap()));
        assert!(!p.contains(&rp("teamspaces/alpha/seats/live/seat.toml")));
        let _ = live;
        // A seat that does not exist in the tree contains no folder paths.
        let none = paths_at(&fx, &Scope::Seat(SeatId::new()), &c);
        assert!(!none.contains(&rp("teamspaces/alpha/seats/live/seat.toml")));
    }

    #[test]
    fn seat_transcripts_included() {
        let mut fx = Fx::new();
        let t = fx.teamspace("alpha", Lifecycle::Active);
        let s = fx.seat(&t, "a", Lifecycle::Active);
        let other = fx.seat(&t, "b", Lifecycle::Active);
        let c = fx.commit("c");
        let tr = crate::model::TranscriptId::new();
        let p = paths_at(&fx, &Scope::Seat(s.clone()), &c);
        assert!(p.contains(&layout::transcript_record(&s, &tr)));
        assert!(p.contains(&layout::transcript_registration_record(
            "Alpha", &t, "A", &s, &tr
        )));
        assert!(!p.contains(&layout::transcript_record(&other, &tr)));
        assert!(!p.contains(&layout::transcript_registration_record(
            "Alpha", &t, "B", &other, &tr
        )));
        // Teamspace scope does not take transcripts.
        let pt = paths_at(&fx, &Scope::Teamspace(t.clone()), &c);
        assert!(!pt.contains(&layout::transcript_record(&s, &tr)));
    }

    #[test]
    fn object_membership_affected_retired() {
        let mut fx = Fx::new();
        let t = fx.teamspace("alpha", Lifecycle::Active);
        let a = fx.seat(&t, "a", Lifecycle::Active);
        let b = fx.seat(&t, "b", Lifecycle::Active);
        let before = fx.commit("both");
        fx.remove_seat(&b);
        let after = fx.commit("b removed");
        let (tb, ta) = (fx.store.at(&before), fx.store.at(&after));
        let in_a = Scope::Seat(a.clone());
        let in_b = Scope::Seat(b.clone());
        assert!(
            in_a.contains_object_at(&ta, Some(&tb), &a.to_any())
                .unwrap()
        );
        assert!(
            !in_a
                .contains_object_at(&ta, Some(&tb), &b.to_any())
                .unwrap()
        );
        assert!(
            Scope::Teamspace(t.clone())
                .contains_object_at(&ta, None, &a.to_any())
                .unwrap()
        );
        // b is gone from the commit's own tree: only the parent tree places it.
        assert!(
            in_b.contains_object_at(&ta, Some(&tb), &b.to_any())
                .unwrap()
        );
        assert!(!in_b.contains_object_at(&ta, None, &b.to_any()).unwrap());
        assert!(
            !Scope::Global
                .contains_object_at(&tb, None, &a.to_any())
                .unwrap()
        );
        assert!(
            Scope::Instance
                .contains_object_at(&tb, None, &a.to_any())
                .unwrap()
        );
    }
}
