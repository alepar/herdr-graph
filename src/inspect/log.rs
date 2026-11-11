//! The `log` view (spec §2.4): which commits of `main` match a scope, a requester and the noise filter, and
//! how a matching commit is shown.
use crate::inspect::output::View;
use crate::inspect::scope::Scope;
use crate::model::action::ActionRecord;
use crate::model::change::{Category, RequestKind};
use crate::model::operation::{ConfirmMode, OperationRecord};
use crate::model::{ActionId, AnyId, CloneId, IdKind, OpId, SeatId, TeamspaceId, Timestamp};
use crate::ports::store::StoreError;
use crate::store::GitStore;
use crate::store::history::{Change, HistoryWalker, Malformed, ScanReport};
use crate::store::record::read_toml;
use crate::store::tree::TreeRead;
use crate::undo::candidates::is_undoable;
use crate::writer::mutation::kind_name;
use serde::Serialize;
use std::ops::ControlFlow;

/// The pointer to the journal-only operations `log` cannot see. The last line of the text output; on stderr
/// under `--json`.
pub const OPS_POINTER: &str =
    "journal-only ops (rejected, failed, in flight): herdr-graph ops [--seat <s>|--teamspace <t>]";

/// Change matching for `log`.
///
/// Owns: deciding which `Change`s match: scope path membership at the commit's own tree or an in-scope
/// affected/retired object, `--by` as an AND filter on the requester's seat id, the bookkeeping filter, and
/// `--since`/`--limit` (in [`LogQuery::run`]).
///
/// Must not: walk history (that is `HistoryWalker`), map scopes to folders or match names (that is `Scope`),
/// or resolve HEAD.
///
/// Where new behavior goes: a new log filter is a new field here, applied in [`LogQuery::matches`]. The
/// roster's `last_change` (hg-blw.9) calls `matches` with the seat's Scope and never filters changes itself.
#[derive(Debug, Clone)]
pub struct LogQuery {
    pub scope: Scope,
    /// The resolved `--by` seat; matched by id, so a rename or retirement of that seat does not matter.
    pub by: Option<SeatId>,
    /// Include `bookkeeping` kinds.
    pub all: bool,
    pub since: Option<Timestamp>,
    pub limit: usize,
}

impl LogQuery {
    /// Scope, `--by` and the bookkeeping filter; `--since`/`--limit` are applied by `run`. `Instance` holds
    /// every change; a change with no readable operation record cannot be told apart from bookkeeping and is
    /// kept.
    pub fn matches(&self, store: &GitStore, c: &Change) -> Result<bool, StoreError> {
        if let Some(by) = &self.by {
            let requested_by = c.operation.as_ref().and_then(|o| o.requester.seat.as_ref());
            if requested_by != Some(by) {
                return Ok(false);
            }
        }
        if !self.all
            && c.operation
                .as_ref()
                .is_some_and(|o| o.kind.category() == Category::Bookkeeping)
        {
            return Ok(false);
        }
        if self.scope == Scope::Instance {
            return Ok(true);
        }
        let tree = store.at(&c.commit);
        let paths = self.scope.paths_at(&tree)?;
        if c.paths.iter().any(|p| paths.contains(p)) {
            return Ok(true);
        }
        let Some(action) = &c.action else {
            return Ok(false);
        };
        let parent = c.parent.as_ref().map(|p| store.at(p));
        let fallback = parent.as_ref().map(|p| p as &dyn TreeRead);
        let named = action
            .affected
            .iter()
            .map(|a| &a.object)
            .chain(action.retired.iter());
        for id in named {
            if self.scope.contains_object_at(&tree, fallback, id)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Walk newest first, keeping matching changes until `limit`, stopping at the first commit older than
    /// `since` (history is time-ordered). Returns the view and the warnings (notices without a JSON field,
    /// not counting [`OPS_POINTER`]).
    pub fn run(&self, walker: &HistoryWalker<'_>) -> Result<(LogView, Vec<String>), StoreError> {
        let store = walker.store();
        let mut kept: Vec<Change> = Vec::new();
        let mut failed: Option<StoreError> = None;
        let scan = walker.walk(|c| {
            if kept.len() >= self.limit || self.since.is_some_and(|s| c.at < s) {
                return ControlFlow::Break(());
            }
            match self.matches(store, c) {
                Ok(true) => kept.push(c.clone()),
                Ok(false) => {}
                Err(e) => {
                    failed = Some(e);
                    return ControlFlow::Break(());
                }
            }
            if kept.len() >= self.limit {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        })?;
        if let Some(e) = failed {
            return Err(e);
        }
        let rows = kept
            .iter()
            .map(|c| row(walker, c))
            .collect::<Result<Vec<_>, _>>()?;
        let bad = rows.iter().filter(|r| !r.malformed.is_empty()).count();
        let warnings = if bad > 0 {
            vec![format!(
                "warning: {bad} commits with unreadable records (listed with malformed)"
            )]
        } else {
            vec![]
        };
        Ok((LogView { scan, rows }, warnings))
    }
}

#[derive(Debug, Serialize)]
pub struct LogView {
    pub scan: ScanReport,
    pub rows: Vec<LogRow>,
}

#[derive(Debug, Serialize)]
pub struct LogRow {
    pub commit: String,
    pub at: Timestamp,
    /// `null` for a commit without a `Graph-Op` trailer or with an operation record that does not parse.
    pub op: Option<OpId>,
    pub kind: Option<RequestKind>,
    pub summary: String,
    pub requester: Option<RequesterView>,
    pub confirmation: Option<ConfirmationView>,
    pub action: Option<ActionView>,
    pub objects: Vec<ObjectView>,
    pub paths: Vec<String>,
    pub malformed: Vec<Malformed>,
}

#[derive(Debug, Serialize)]
pub struct RequesterView {
    pub teamspace: Option<TeamspaceId>,
    pub seat: Option<SeatId>,
    pub clone: Option<CloneId>,
    pub human: bool,
}

#[derive(Debug, Serialize)]
pub struct ConfirmationView {
    pub mode: ConfirmMode,
    pub plan_hash: String,
}

#[derive(Debug, Serialize)]
pub struct ActionView {
    pub id: ActionId,
    pub kind: crate::model::action::ActionKind,
    pub undoable: bool,
    /// Read at the pinned HEAD; `null` when the record no longer exists there.
    pub undone: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct ObjectView {
    pub id: AnyId,
    pub kind: String,
    pub name: String,
    pub change: ObjectChange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectChange {
    Created,
    Modified,
    Removed,
    Retired,
}

fn row(walker: &HistoryWalker<'_>, c: &Change) -> Result<LogRow, StoreError> {
    let store = walker.store();
    let tree = store.at(&c.commit);
    let parent = c.parent.as_ref().map(|p| store.at(p));
    let names = |id: &AnyId| object_name(&tree, parent.as_ref().map(|p| p as &dyn TreeRead), id);
    let mut objects = Vec::new();
    if let Some(a) = &c.action {
        for o in &a.affected {
            objects.push(ObjectView {
                id: o.object.clone(),
                kind: kind_word(&o.object),
                name: names(&o.object),
                change: match (&o.before, &o.after) {
                    (None, _) => ObjectChange::Created,
                    (_, None) => ObjectChange::Removed,
                    _ => ObjectChange::Modified,
                },
            });
        }
        for id in &a.retired {
            objects.push(ObjectView {
                id: id.clone(),
                kind: kind_word(id),
                name: names(id),
                change: ObjectChange::Retired,
            });
        }
    }
    let op = c.operation.as_ref();
    let action = match (&c.trailers.action, &c.action) {
        (Some(id), Some(rec)) => {
            let at_head = walker.action_at_head(id)?;
            Some(ActionView {
                id: id.clone(),
                kind: rec.kind,
                undoable: is_undoable(rec.kind),
                undone: at_head.map(|a: ActionRecord| !a.undone_by.is_empty()),
            })
        }
        _ => None,
    };
    Ok(LogRow {
        commit: c.commit.0.clone(),
        at: c.at,
        op: op.map(|o| o.id.clone()),
        kind: op.map(|o| o.kind),
        summary: op.map_or_else(|| c.summary.clone(), |o| o.summary.clone()),
        requester: op.map(requester_view),
        confirmation: op.and_then(|o| {
            o.confirmation.as_ref().map(|c| ConfirmationView {
                mode: c.mode,
                plan_hash: c.plan_hash.clone(),
            })
        }),
        action,
        objects,
        paths: c.paths.iter().map(|p| p.as_str().to_owned()).collect(),
        malformed: c.malformed.clone(),
    })
}

fn requester_view(o: &OperationRecord) -> RequesterView {
    let r = &o.requester;
    RequesterView {
        teamspace: r.teamspace.clone(),
        seat: r.seat.clone(),
        clone: r.clone.clone(),
        human: r.human,
    }
}

fn kind_word(id: &AnyId) -> String {
    crate::inspect::roster::serde_word(&id.kind())
}

/// The object's name in the commit's tree (else the parent's, for an object the commit deleted); its id when
/// it has no name or cannot be read: a name is decoration and never fails the log.
fn object_name(tree: &dyn TreeRead, fallback: Option<&dyn TreeRead>, id: &AnyId) -> String {
    use crate::model::{
        application::ApplicationRecord, clone::CloneRecord, seat::SeatRecord,
        teamspace::TeamspaceRecord, template::TemplateRecord,
    };
    for t in std::iter::once(tree).chain(fallback) {
        let Ok(Some(loc)) = crate::store::layout::locate(t, id) else {
            continue;
        };
        let p = &loc.record_path;
        let name = match id.kind() {
            IdKind::Teamspace => read_toml::<TeamspaceRecord>(t, p).map(|r| r.map(|r| r.name)),
            IdKind::Seat => read_toml::<SeatRecord>(t, p).map(|r| r.map(|r| r.name)),
            IdKind::Clone => read_toml::<CloneRecord>(t, p).map(|r| r.map(|r| r.name)),
            IdKind::Template => read_toml::<TemplateRecord>(t, p).map(|r| r.map(|r| r.name)),
            IdKind::Application => read_toml::<ApplicationRecord>(t, p).map(|r| r.map(|r| r.name)),
            _ => return id.to_string(),
        };
        if let Ok(Some(n)) = name {
            return n;
        }
    }
    id.to_string()
}

impl View for LogView {
    fn render_text(&self, out: &mut String) {
        if self.rows.is_empty() {
            out.push_str("no matching changes\n");
        }
        for r in &self.rows {
            out.push_str(&r.at.format("%Y-%m-%d %H:%MZ").to_string());
            out.push_str(&format!(
                "  {}  {}  {}",
                r.op.as_ref().map_or("-".to_owned(), |o| o.to_string()),
                r.kind.map_or("-".to_owned(), kind_name),
                r.summary
            ));
            if let Some(q) = &r.requester {
                let who = if q.human {
                    Some("human".to_owned())
                } else {
                    match (&q.seat, &q.clone) {
                        (Some(s), Some(c)) => Some(format!("{s}/{c}")),
                        (Some(s), None) => Some(s.to_string()),
                        (None, Some(c)) => Some(c.to_string()),
                        (None, None) => None,
                    }
                };
                if let Some(who) = who {
                    out.push_str(&format!("  by {who}"));
                    if let Some(c) = &r.confirmation {
                        out.push_str(&format!(
                            " ({})",
                            crate::inspect::roster::serde_word(&c.mode)
                        ));
                    }
                }
            }
            if let Some(a) = &r.action {
                out.push_str(&format!("  {}", a.id));
                if a.undone == Some(true) {
                    out.push_str(" undone");
                }
            }
            if !r.malformed.is_empty() {
                out.push_str("  [unreadable records]");
            }
            out.push('\n');
        }
        if self.scan.stopped_early {
            out.push_str(&format!(
                "scanned {} commits, older history not searched\n",
                self.scan.scanned
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspect::fixture::{Fx, ts};
    use crate::inspect::output::{Envelope, Format, emit};
    use crate::inspect::scope::{AllowClone, ScopeArg};
    use crate::model::Lifecycle;
    use crate::model::action::{ActionKind, AffectedObject};
    use crate::model::change::Requester;
    use crate::model::{CommitId, OpId};
    use crate::ports::store::{RepoPath, Store};
    use crate::store::layout;

    fn rp(s: &str) -> RepoPath {
        RepoPath::new(s).unwrap()
    }

    fn by_seat(s: &SeatId) -> Requester {
        Requester {
            seat: Some(s.clone()),
            ..Default::default()
        }
    }

    fn human() -> Requester {
        Requester {
            human: true,
            ..Default::default()
        }
    }

    fn query(scope: Scope) -> LogQuery {
        LogQuery {
            scope,
            by: None,
            all: false,
            since: None,
            limit: 30,
        }
    }

    fn rows(fx: &Fx, q: &LogQuery) -> Vec<String> {
        view(fx, q, 5000)
            .0
            .rows
            .into_iter()
            .map(|r| r.commit)
            .collect()
    }

    fn view(fx: &Fx, q: &LogQuery, max_scan: usize) -> (LogView, Vec<String>) {
        let head = fx.store.head().unwrap();
        q.run(&HistoryWalker::new(&fx.store, &head, max_scan))
            .unwrap()
    }

    /// One teamspace with seats a and b, an op-attributed commit for each action below.
    struct World {
        fx: Fx,
        ts: crate::model::TeamspaceId,
        a: SeatId,
        b: SeatId,
        init: CommitId,
    }

    fn world() -> World {
        let mut fx = Fx::new();
        let init = fx.store.head().unwrap();
        let ts = fx.teamspace("alpha", Lifecycle::Active);
        let a = fx.seat(&ts, "a", Lifecycle::Active);
        let b = fx.seat(&ts, "b", Lifecycle::Active);
        World { fx, ts, a, b, init }
    }

    /// Commit the pending records under a new op of `kind` requested by `req`.
    fn op_commit(fx: &mut Fx, kind: RequestKind, req: Requester) -> CommitId {
        let op = fx.operation(kind, "did a thing", req);
        fx.commit_with_trailers("did a thing", Some(&op), None)
    }

    #[test]
    fn scope_by_path() {
        let mut w = world();
        let c0 = op_commit(&mut w.fx, RequestKind::SeatCreate, human());
        // Seat b only.
        w.fx.put_file(rp("teamspaces/alpha/seats/b/notes.md"), b"x".to_vec());
        let c1 = op_commit(&mut w.fx, RequestKind::SeatOverride, human());
        w.fx.put_file(rp("rules/r.md"), b"x".to_vec());
        let c2 = op_commit(&mut w.fx, RequestKind::SeatOverride, human());
        let fx = &w.fx;
        assert_eq!(
            rows(fx, &query(Scope::Instance)),
            [&c2, &c1, &c0, &w.init].map(|c| c.0.clone())
        );
        assert_eq!(
            rows(fx, &query(Scope::Seat(w.b.clone()))),
            [&c1, &c0].map(|c| c.0.clone())
        );
        assert_eq!(
            rows(fx, &query(Scope::Seat(w.a.clone()))),
            [&c0].map(|c| c.0.clone())
        );
        assert_eq!(
            rows(fx, &query(Scope::Teamspace(w.ts.clone()))),
            [&c1, &c0].map(|c| c.0.clone())
        );
    }

    #[test]
    fn global_lists_only_global_paths() {
        let mut w = world();
        let c0 = op_commit(&mut w.fx, RequestKind::SeatCreate, human());
        w.fx.put_file(rp("rules/r.md"), b"x".to_vec());
        let c1 = op_commit(&mut w.fx, RequestKind::SeatOverride, human());
        let got = rows(&w.fx, &query(Scope::Global));
        // The init commit wrote graph.toml and rules/.gitkeep; c0 changed no global path.
        assert_eq!(got, [c1.0.clone(), w.init.0.clone()]);
        assert!(!got.contains(&c0.0));
        let all = rows(&w.fx, &query(Scope::Instance));
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn scope_by_affected_object() {
        let mut w = world();
        // The first commit writes the seats' records; the ones below touch no seat folder.
        op_commit(&mut w.fx, RequestKind::SeatCreate, human());
        // No path under seat a's folder; only the action's affected names it.
        w.fx.put_file(rp("rules/r.md"), b"x".to_vec());
        let op =
            w.fx.operation(RequestKind::SeatOverride, "touch a", human());
        let act = w.fx.action(
            ActionKind::TemplateEdit,
            std::slice::from_ref(&op),
            vec![AffectedObject {
                object: w.a.to_any(),
                before: None,
                after: Some(toml::Value::Boolean(true)),
            }],
            vec![],
        );
        let named = w.fx.commit_with_trailers("touch a", Some(&op), Some(&act));
        // Same path, no action.
        w.fx.put_file(rp("rules/r.md"), b"y".to_vec());
        op_commit(&mut w.fx, RequestKind::SeatOverride, human());
        // Retired object, evidence only in `retired`.
        w.fx.put_file(rp("rules/r.md"), b"z".to_vec());
        let op = w.fx.operation(RequestKind::SeatRetire, "retire b", human());
        let act = w.fx.action(
            ActionKind::Retire,
            std::slice::from_ref(&op),
            vec![],
            vec![w.b.to_any()],
        );
        let retired = w.fx.commit_with_trailers("retire b", Some(&op), Some(&act));
        let seat_a = rows(&w.fx, &query(Scope::Seat(w.a.clone())));
        assert!(seat_a.contains(&named.0), "{seat_a:?}");
        assert!(!seat_a.contains(&retired.0));
        let seat_b = rows(&w.fx, &query(Scope::Seat(w.b.clone())));
        assert!(
            seat_b.contains(&retired.0) && !seat_b.contains(&named.0),
            "{seat_b:?}"
        );
        let (v, _) = view(&w.fx, &query(Scope::Seat(w.b.clone())), 100);
        let r = v.rows.iter().find(|r| r.commit == retired.0).unwrap();
        assert_eq!(r.objects.len(), 1);
        assert_eq!(
            (r.objects[0].change, r.objects[0].name.as_str()),
            (ObjectChange::Retired, "b")
        );
        assert_eq!(r.objects[0].kind, "seat");
    }

    #[test]
    fn by_is_and_filter() {
        let mut w = world();
        let c0 = op_commit(&mut w.fx, RequestKind::SeatCreate, human());
        // b requests a change to seat a; a requests a change to seat a.
        w.fx.put_file(rp("teamspaces/alpha/seats/a/x.md"), b"1".to_vec());
        let by_b = op_commit(&mut w.fx, RequestKind::SeatOverride, by_seat(&w.b));
        w.fx.put_file(rp("teamspaces/alpha/seats/a/x.md"), b"2".to_vec());
        let by_a = op_commit(&mut w.fx, RequestKind::SeatOverride, by_seat(&w.a));
        // Requesterless commits touching a's folder.
        w.fx.put_file(rp("teamspaces/alpha/seats/a/x.md"), b"3".to_vec());
        let observed = op_commit(&mut w.fx, RequestKind::Observed, Requester::default());
        w.fx.put_file(rp("teamspaces/alpha/seats/a/x.md"), b"4".to_vec());
        let book = op_commit(&mut w.fx, RequestKind::Bookkeeping, Requester::default());
        let q = |scope, by: Option<&SeatId>, all| LogQuery {
            by: by.cloned(),
            all,
            ..query(scope)
        };
        let ids = |v: Vec<CommitId>| v.into_iter().map(|c| c.0).collect::<Vec<_>>();
        // log --by b: instance AND requester b.
        assert_eq!(
            rows(&w.fx, &q(Scope::Instance, Some(&w.b), true)),
            ids(vec![by_b.clone()])
        );
        // log seat a --by b: changes to a that b requested.
        assert_eq!(
            rows(&w.fx, &q(Scope::Seat(w.a.clone()), Some(&w.b), true)),
            ids(vec![by_b.clone()])
        );
        assert_eq!(
            rows(&w.fx, &q(Scope::Seat(w.a.clone()), Some(&w.a), true)),
            ids(vec![by_a.clone()])
        );
        // log seat b --by b: nothing b requested touched b.
        assert!(rows(&w.fx, &q(Scope::Seat(w.b.clone()), Some(&w.b), true)).is_empty());
        // Requesterless commits never match --by, even with --all; without --by they do match.
        let with_all = rows(&w.fx, &q(Scope::Seat(w.a.clone()), None, true));
        assert!(with_all.contains(&observed.0) && with_all.contains(&book.0));
        for by in [&w.a, &w.b] {
            let got = rows(&w.fx, &q(Scope::Instance, Some(by), true));
            assert!(!got.contains(&observed.0) && !got.contains(&book.0));
            assert!(!got.contains(&c0.0), "human-requested commit matched --by");
        }
        // Renamed: still matched by id. The old name no longer resolves, the new one does.
        w.fx.rename_seat(&w.b, "bee");
        op_commit(&mut w.fx, RequestKind::SeatRename, human());
        let head = w.fx.store.head().unwrap();
        let resolve = |name: &str| {
            let arg = ScopeArg::parse(&["seat".into(), name.into()], AllowClone::No).unwrap();
            Scope::resolve(&arg, &w.fx.store, &head)
        };
        let Scope::Seat(bee) = resolve("bee").unwrap() else {
            panic!()
        };
        assert_eq!(bee, w.b);
        assert_eq!(
            rows(&w.fx, &q(Scope::Instance, Some(&bee), true)),
            ids(vec![by_b.clone()])
        );
        // Retired: a retired seat's requests are matched by id, and its name resolves (retired allowed).
        let gone = w.fx.archived_seat(&w.ts, "gone");
        w.fx.put_file(rp("rules/r.md"), b"x".to_vec());
        let by_gone = op_commit(&mut w.fx, RequestKind::SeatOverride, by_seat(&gone));
        let head = w.fx.store.head().unwrap();
        let arg = ScopeArg::parse(&["seat".into(), "gone".into()], AllowClone::No).unwrap();
        let Scope::Seat(resolved) = Scope::resolve(&arg, &w.fx.store, &head).unwrap() else {
            panic!()
        };
        assert_eq!(resolved, gone);
        assert_eq!(
            rows(&w.fx, &q(Scope::Instance, Some(&resolved), false)),
            ids(vec![by_gone])
        );
    }

    #[test]
    fn bookkeeping_hidden_unless_all() {
        let mut w = world();
        let human_c = op_commit(&mut w.fx, RequestKind::SeatCreate, human());
        let observed = op_commit(&mut w.fx, RequestKind::Observed, Requester::default());
        let book = op_commit(&mut w.fx, RequestKind::Bookkeeping, Requester::default());
        let default = rows(&w.fx, &query(Scope::Instance));
        assert!(
            default.contains(&human_c.0) && default.contains(&observed.0),
            "observed shown"
        );
        assert!(!default.contains(&book.0));
        let all = rows(
            &w.fx,
            &LogQuery {
                all: true,
                ..query(Scope::Instance)
            },
        );
        assert!(all.contains(&book.0));
        assert_eq!(all.len(), default.len() + 1);
    }

    #[test]
    fn since_and_limit() {
        let mut w = world();
        let mut commits = vec![];
        for day in ["01", "02", "03", "04", "05"] {
            w.fx.set_time(&format!("2026-11-{day}T12:00:00Z"));
            commits.push(op_commit(&mut w.fx, RequestKind::SeatOverride, human()).0);
        }
        let got = |q: &LogQuery| rows(&w.fx, q);
        let newest_first: Vec<_> = commits.iter().rev().cloned().collect();
        assert_eq!(
            got(&LogQuery {
                limit: 2,
                ..query(Scope::Instance)
            }),
            newest_first[..2]
        );
        let since = LogQuery {
            since: Some(ts("2026-11-03T12:00:00Z")),
            ..query(Scope::Instance)
        };
        assert_eq!(got(&since), newest_first[..3], "at-or-after is inclusive");
        let since = LogQuery {
            since: Some(ts("2026-11-03T12:00:01Z")),
            ..query(Scope::Instance)
        };
        assert_eq!(got(&since), newest_first[..2]);
        let (v, _) = view(&w.fx, &since, 5000);
        assert!(!v.scan.stopped_early, "a --since break is not a cap stop");
        let (v, _) = view(
            &w.fx,
            &LogQuery {
                limit: 0,
                ..query(Scope::Instance)
            },
            5000,
        );
        assert!(v.rows.is_empty());
    }

    #[test]
    fn max_scan_report() {
        let mut w = world();
        for _ in 0..3 {
            op_commit(&mut w.fx, RequestKind::SeatOverride, human());
        }
        // A filter nothing matches: rows are empty either way, the report says which kind of empty.
        let nobody = LogQuery {
            by: Some(SeatId::new()),
            ..query(Scope::Instance)
        };
        let json = |max| {
            let (v, _) = view(&w.fx, &nobody, max);
            let head = w.fx.store.head().unwrap();
            serde_json::to_value(Envelope::new(&head, v)).unwrap()
        };
        let full = json(100);
        assert_eq!(full["rows"], serde_json::json!([]));
        assert_eq!(
            full["scan"],
            serde_json::json!({"scanned": 4, "max_scan": 100, "stopped_early": false})
        );
        let capped = json(2);
        assert_eq!(capped["rows"], serde_json::json!([]));
        assert_eq!(
            capped["scan"],
            serde_json::json!({"scanned": 2, "max_scan": 2, "stopped_early": true})
        );
        let (v, _) = view(&w.fx, &nobody, 2);
        let mut text = String::new();
        v.render_text(&mut text);
        assert!(
            text.ends_with("scanned 2 commits, older history not searched\n"),
            "{text}"
        );
        let (v, _) = view(&w.fx, &nobody, 100);
        let mut text = String::new();
        v.render_text(&mut text);
        assert!(!text.contains("older history"), "{text}");
    }

    #[test]
    fn undo_state_from_head() {
        let mut w = world();
        w.fx.put_file(rp("rules/r.md"), b"x".to_vec());
        let op = w.fx.operation(RequestKind::SeatRetire, "retire b", human());
        let act = w.fx.action(
            ActionKind::Retire,
            std::slice::from_ref(&op),
            vec![],
            vec![w.b.to_any()],
        );
        let c = w.fx.commit_with_trailers("retire b", Some(&op), Some(&act));
        let action_of = |fx: &Fx| {
            let (v, _) = view(fx, &query(Scope::Instance), 100);
            let r = v.rows.into_iter().find(|r| r.commit == c.0).unwrap();
            r.action.unwrap()
        };
        let a = action_of(&w.fx);
        assert_eq!((a.undoable, a.undone), (true, Some(false)));
        // An undo commit later appends to undone_by; the original commit's own record is unchanged.
        w.fx.action_mut(&act).undone_by.push(OpId::new());
        w.fx.commit("undo landed");
        assert_eq!(action_of(&w.fx).undone, Some(true));
        // A resurrection is history only.
        let op2 = w.fx.operation(RequestKind::SeatResurrect, "res", human());
        let act2 = w.fx.action(
            ActionKind::Resurrect,
            std::slice::from_ref(&op2),
            vec![],
            vec![],
        );
        let c2 = w.fx.commit_with_trailers("res", Some(&op2), Some(&act2));
        let (v, _) = view(&w.fx, &query(Scope::Instance), 100);
        let r2 = v
            .rows
            .iter()
            .find(|r| r.commit == c2.0)
            .unwrap()
            .action
            .as_ref()
            .unwrap();
        assert!(!r2.undoable);
        // The record disappears at HEAD: null, not false.
        w.fx.remove_action(&act);
        w.fx.commit("record gone");
        assert_eq!(action_of(&w.fx).undone, None);
    }

    #[test]
    fn malformed_one_warning() {
        let mut w = world();
        let at = ts("2026-11-01T00:00:00Z");
        let bad1 = OpId::new();
        w.fx.put_file(layout::operation_record(at, &bad1), b"nope = [".to_vec());
        let c1 = w.fx.commit_with_trailers("bad one", Some(&bad1), None);
        let bad2 = OpId::new();
        w.fx.put_file(layout::operation_record(at, &bad2), b"{".to_vec());
        let c2 = w.fx.commit_with_trailers("bad two", Some(&bad2), None);
        let good = op_commit(&mut w.fx, RequestKind::SeatOverride, human());
        let (v, warnings) = view(&w.fx, &query(Scope::Instance), 100);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(
            warnings[0].starts_with("warning: 2 commits"),
            "{warnings:?}"
        );
        let r = |c: &CommitId| v.rows.iter().find(|r| r.commit == c.0).unwrap();
        for c in [&c1, &c2] {
            let row = r(c);
            assert_eq!((row.op.is_none(), row.kind.is_none()), (true, true));
            assert_eq!(row.malformed, [Malformed::Operation]);
        }
        assert!(r(&good).op.is_some() && r(&good).malformed.is_empty());
        // The walk continued past the bad commits to the init commit (no trailer: op null, not malformed).
        let init = v.rows.last().unwrap();
        assert!(init.op.is_none() && init.malformed.is_empty());
        let json = serde_json::to_value(&v).unwrap();
        assert_eq!(
            json["rows"][1]["malformed"],
            serde_json::json!(["operation"])
        );
        assert_eq!(json["rows"][1]["op"], serde_json::Value::Null);
        // No warning without a malformed row on the page.
        let (_, none) = view(
            &w.fx,
            &LogQuery {
                limit: 1,
                ..query(Scope::Instance)
            },
            100,
        );
        assert!(none.is_empty());
    }

    #[test]
    fn json_row_shape() {
        let mut w = world();
        let op =
            w.fx.operation(RequestKind::SeatCreate, "made seat", by_seat(&w.a));
        w.fx.operation_mut(&op).confirmation = Some(crate::model::operation::Confirmation {
            mode: ConfirmMode::Tty,
            plan_hash: "abc".into(),
            at: ts("2026-11-01T00:00:00Z"),
        });
        let c =
            w.fx.commit_with_trailers("seat_create: made seat", Some(&op), None);
        let (v, _) = view(&w.fx, &query(Scope::Instance), 100);
        let head = w.fx.store.head().unwrap();
        let j = serde_json::to_value(Envelope::new(&head, v)).unwrap();
        let top: Vec<_> = j.as_object().unwrap().keys().cloned().collect();
        assert_eq!(top, ["head", "rows", "scan", "schema"]);
        let row = &j["rows"][0];
        let mut keys: Vec<_> = row.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        assert_eq!(
            keys,
            [
                "action",
                "at",
                "commit",
                "confirmation",
                "kind",
                "malformed",
                "objects",
                "op",
                "paths",
                "requester",
                "summary"
            ]
        );
        assert_eq!(row["commit"], c.0);
        assert_eq!(row["kind"], "seat_create");
        assert_eq!(row["summary"], "made seat");
        assert_eq!(
            row["confirmation"],
            serde_json::json!({"mode": "tty", "plan_hash": "abc"})
        );
        assert_eq!(row["requester"]["seat"], w.a.to_string());
        assert_eq!(row["requester"]["human"], false);
        assert_eq!(row["action"], serde_json::Value::Null);
    }

    #[test]
    fn ops_pointer_last() {
        let mut w = world();
        op_commit(&mut w.fx, RequestKind::SeatOverride, human());
        op_commit(&mut w.fx, RequestKind::SeatOverride, human());
        let (v, warnings) = view(&w.fx, &query(Scope::Instance), 2);
        assert!(warnings.is_empty());
        assert!(v.scan.stopped_early);
        let head = w.fx.store.head().unwrap();
        let notices = [OPS_POINTER.to_owned()];
        let (mut out, mut err) = (Vec::new(), Vec::new());
        emit(
            &Envelope::new(&head, v),
            Format::Text,
            &notices,
            &mut out,
            &mut err,
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        // The pointer follows even the "older history not searched" line.
        let lines: Vec<_> = text.lines().collect();
        assert_eq!(lines.last().unwrap(), &OPS_POINTER);
        assert!(
            lines[lines.len() - 2].contains("older history not searched"),
            "{text}"
        );
        assert!(err.is_empty());
        let (v, _) = view(&w.fx, &query(Scope::Instance), 2);
        let (mut out, mut err) = (Vec::new(), Vec::new());
        emit(
            &Envelope::new(&head, v),
            Format::Json,
            &notices,
            &mut out,
            &mut err,
        )
        .unwrap();
        let docs: Vec<serde_json::Value> = serde_json::Deserializer::from_slice(&out)
            .into_iter()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(docs.len(), 1);
        assert!(!String::from_utf8(out).unwrap().contains("journal-only"));
        assert_eq!(String::from_utf8(err).unwrap(), format!("{OPS_POINTER}\n"));
    }

    #[test]
    fn text_row_line() {
        let mut w = world();
        w.fx.set_time("2026-11-05T09:07:00Z");
        let op =
            w.fx.operation(RequestKind::SeatCreate, "made seat", by_seat(&w.a));
        let act = w.fx.action(
            ActionKind::Retire,
            std::slice::from_ref(&op),
            vec![],
            vec![],
        );
        w.fx.action_mut(&act).undone_by.push(OpId::new());
        w.fx.commit_with_trailers("seat_create: made seat", Some(&op), Some(&act));
        let (v, _) = view(
            &w.fx,
            &LogQuery {
                limit: 1,
                ..query(Scope::Instance)
            },
            100,
        );
        let mut t = String::new();
        v.render_text(&mut t);
        assert!(t.starts_with("2026-11-05 09:07Z  op_"), "{t}");
        assert!(t.contains("  seat_create  made seat  by st_"), "{t}");
        assert!(t.trim_end().ends_with(&format!("{act} undone")), "{t}");
    }

    #[test]
    fn native_session_kind_is_the_serde_word() {
        let id = crate::model::ids::NsId::new().to_any();
        assert_eq!(kind_word(&id), "native_session");
    }
}
