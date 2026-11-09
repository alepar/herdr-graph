//! Test-only instance builder for inspect tests: a tempfile instance (`store::init::init_instance`) plus
//! committed teamspace/seat/clone/transcript records. In-process only: it spawns nothing.
use crate::model::action::{ActionKind, ActionRecord, AffectedObject};
use crate::model::change::{RequestKind, Requester};
use crate::model::clone::{CloneRecord, SeatIntent};
use crate::model::harness::Harness;
use crate::model::native_session::NativeSession;
use crate::model::operation::{OpState, OperationRecord};
use crate::model::seat::SeatRecord;
use crate::model::teamspace::TeamspaceRecord;
use crate::model::transcript::TranscriptRecord;
use crate::model::{
    ActionId, AnyId, CloneId, CloneLifecycle, CommitId, Lifecycle, NsId, Occupant, OpId, Runtime,
    SCHEMA_VERSION, SeatId, TeamspaceId, Timestamp, TranscriptId,
};
use crate::ports::store::{RepoPath, Store};
use crate::store::git::graph_signature;
use crate::store::{EditSet, GitStore, layout, record};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

pub(crate) struct Fx {
    _t: tempfile::TempDir,
    pub store: GitStore,
    teamspaces: BTreeMap<TeamspaceId, (RepoPath, TeamspaceRecord)>,
    seats: BTreeMap<SeatId, (RepoPath, SeatRecord)>,
    clones: BTreeMap<CloneId, (RepoPath, CloneRecord)>,
    transcripts: BTreeMap<TranscriptId, (RepoPath, TranscriptRecord)>,
    used: BTreeSet<RepoPath>,
    node_manager: Option<SeatId>,
    ops: BTreeMap<OpId, (RepoPath, OperationRecord)>,
    actions: BTreeMap<ActionId, (RepoPath, ActionRecord)>,
    /// Raw edits applied by the next commit only.
    extra: EditSet,
    /// Commit time for the next commits; the wall clock when unset.
    time: Option<Timestamp>,
}

pub(crate) fn ts(s: &str) -> Timestamp {
    chrono::DateTime::parse_from_rfc3339(s)
        .unwrap()
        .with_timezone(&chrono::Utc)
}

/// A session record; `started` is RFC 3339.
pub(crate) fn session(
    harness: Harness,
    native_id: &str,
    started: &str,
    transcript_path: Option<&str>,
    transcript: Option<TranscriptId>,
) -> NativeSession {
    NativeSession {
        id: NsId::new(),
        harness,
        native_session_id: native_id.into(),
        transcript_path: transcript_path.map(PathBuf::from),
        transcript,
        cwd: PathBuf::from("/work"),
        started: ts(started),
        ended: None,
        end_reason: None,
    }
}

impl Fx {
    pub fn new() -> Fx {
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().join("inst");
        crate::store::init::init_instance(&dir).unwrap();
        let store = GitStore::open(&dir).unwrap();
        Fx {
            _t: t,
            store,
            teamspaces: BTreeMap::new(),
            seats: BTreeMap::new(),
            clones: BTreeMap::new(),
            transcripts: BTreeMap::new(),
            used: BTreeSet::new(),
            node_manager: None,
            ops: BTreeMap::new(),
            actions: BTreeMap::new(),
            extra: EditSet::default(),
            time: None,
        }
    }

    /// `base` when free, else `base` with the id suffix; keeps same-named live siblings from colliding.
    fn free(&mut self, base: RepoPath, suffix: &str) -> RepoPath {
        let p = if self.used.contains(&base) {
            RepoPath::new(&format!("{}-{suffix}", base.as_str())).unwrap()
        } else {
            base
        };
        self.used.insert(p.clone());
        p
    }

    fn ts_rec(name: &str, lifecycle: Lifecycle) -> TeamspaceRecord {
        TeamspaceRecord {
            schema: SCHEMA_VERSION,
            id: TeamspaceId::new(),
            rev: 1,
            name: name.into(),
            name_history: vec![],
            lifecycle,
            retired: None,
            runtime: Default::default(),
            project_repo: None,
            default_manager: None,
            channel: Default::default(),
        }
    }

    pub fn teamspace(&mut self, name: &str, lifecycle: Lifecycle) -> TeamspaceId {
        let rec = Self::ts_rec(name, lifecycle);
        let dir = self.free(layout::teamspace_dir(name), rec.id.suffix6());
        let id = rec.id.clone();
        self.teamspaces.insert(id.clone(), (dir, rec));
        id
    }

    /// Under `archive/teamspaces`, lifecycle Retired.
    pub fn archived_teamspace(&mut self, name: &str) -> TeamspaceId {
        let rec = Self::ts_rec(name, Lifecycle::Retired);
        let dir = layout::archived_teamspace_dir(name, &rec.id);
        self.used.insert(dir.clone());
        let id = rec.id.clone();
        self.teamspaces.insert(id.clone(), (dir, rec));
        id
    }

    fn seat_rec(ts: &TeamspaceId, name: &str, lifecycle: Lifecycle) -> SeatRecord {
        SeatRecord {
            schema: SCHEMA_VERSION,
            id: SeatId::new(),
            rev: 1,
            name: name.into(),
            name_history: vec![],
            teamspace: ts.clone(),
            lifecycle,
            retired: None,
            system_duty: None,
            template_ref: None,
            manager: None,
            applications: vec![],
            overrides: Default::default(),
            participation: Default::default(),
            activation: Default::default(),
            runtime: Default::default(),
            channel: Default::default(),
            reload_required: false,
            moved_out: false,
        }
    }

    fn ts_dir(&self, ts: &TeamspaceId) -> RepoPath {
        self.teamspaces[ts].0.clone()
    }

    pub fn seat(&mut self, ts: &TeamspaceId, name: &str, lifecycle: Lifecycle) -> SeatId {
        let rec = Self::seat_rec(ts, name, lifecycle);
        let dir = self.free(layout::seat_dir(&self.ts_dir(ts), name), rec.id.suffix6());
        let id = rec.id.clone();
        self.seats.insert(id.clone(), (dir, rec));
        id
    }

    /// Under the teamspace's `archive/seats`, lifecycle Retired.
    pub fn archived_seat(&mut self, ts: &TeamspaceId, name: &str) -> SeatId {
        let rec = Self::seat_rec(ts, name, Lifecycle::Retired);
        let dir = layout::archived_seat_dir(&self.ts_dir(ts), name, &rec.id);
        self.used.insert(dir.clone());
        let id = rec.id.clone();
        self.seats.insert(id.clone(), (dir, rec));
        id
    }

    fn blank_clone(
        seat: &SeatId,
        id: CloneId,
        name: &str,
        lifecycle: CloneLifecycle,
    ) -> CloneRecord {
        CloneRecord {
            schema: SCHEMA_VERSION,
            id,
            rev: 1,
            seat: seat.clone(),
            name: name.into(),
            name_history: vec![],
            lifecycle,
            retired: None,
            runtime: Default::default(),
            occupant: None,
            sessions: vec![],
            opt_outs: vec![],
            invitations: vec![],
            reload_required: false,
            threads_seat: None,
            threads_seat_history: vec![],
            seat_intent: None,
            seat_fact: None,
            key_reservation: None,
        }
    }

    /// `occupant` names one of `sessions` by its record id; its harness is that session's.
    pub fn clone_rec(
        &mut self,
        seat: &SeatId,
        name: &str,
        lifecycle: CloneLifecycle,
        sessions: Vec<NativeSession>,
        occupant: Option<NsId>,
    ) -> CloneId {
        self.clone_rec_id(seat, CloneId::new(), name, lifecycle, sessions, occupant)
    }

    /// `clone_rec` with the clone id chosen by the caller (a transcript record needs it first).
    pub fn clone_rec_id(
        &mut self,
        seat: &SeatId,
        id: CloneId,
        name: &str,
        lifecycle: CloneLifecycle,
        sessions: Vec<NativeSession>,
        occupant: Option<NsId>,
    ) -> CloneId {
        let mut rec = Self::blank_clone(seat, id.clone(), name, lifecycle);
        rec.occupant = occupant.map(|ns| Occupant {
            harness: sessions
                .iter()
                .find(|s| s.id == ns)
                .map_or(Harness::Shell, |s| s.harness),
            native_session: ns,
            since: ts("2026-11-01T00:00:00Z"),
        });
        rec.sessions = sessions;
        self.place_clone(seat, rec)
    }

    fn place_clone(&mut self, seat: &SeatId, rec: CloneRecord) -> CloneId {
        let dir = self.free(
            layout::clone_dir(&self.seats[seat].0, &rec.name),
            rec.id.suffix6(),
        );
        let id = rec.id.clone();
        self.clones.insert(id.clone(), (dir, rec));
        id
    }

    /// A purge tombstone (`threads::assignment::purge_tombstone` shape: retired, `Finalize`, no sessions).
    pub fn tombstone(&mut self, seat: &SeatId, name: &str) -> CloneId {
        let mut rec = Self::blank_clone(seat, CloneId::new(), name, CloneLifecycle::Retired);
        rec.threads_seat = Some("seat-1".into());
        rec.seat_intent = Some(SeatIntent::Finalize {
            seat: "seat-1".into(),
        });
        self.place_clone(seat, rec)
    }

    pub fn clone_mut(&mut self, id: &CloneId) -> &mut CloneRecord {
        &mut self.clones.get_mut(id).unwrap().1
    }

    pub fn set_runtime(&mut self, id: &CloneId, runtime: Runtime) {
        self.clone_mut(id).runtime = runtime;
    }

    /// A transcript index record for a session; the caller then sets that session's `transcript` field.
    pub fn transcript(
        &mut self,
        seat: &SeatId,
        clone: &CloneId,
        ns: &NsId,
        path: &str,
    ) -> TranscriptId {
        let id = TranscriptId::new();
        let rec = TranscriptRecord {
            schema: SCHEMA_VERSION,
            id: id.clone(),
            rev: 1,
            transcript_path: PathBuf::from(path),
            native_session: ns.clone(),
            seat: seat.clone(),
            clone: clone.clone(),
            capture_attribution: None,
            source_seat_summaries_enabled_at_capture: false,
            coverage: vec![],
            gaps: vec![],
            unresolved: None,
        };
        self.transcripts
            .insert(id.clone(), (layout::transcript_record(seat, &id), rec));
        id
    }

    pub fn seat_mut(&mut self, id: &SeatId) -> &mut SeatRecord {
        &mut self.seats.get_mut(id).unwrap().1
    }

    /// `graph.toml`'s `node_manager_seat`, written by the next `commit`.
    pub fn set_node_manager(&mut self, seat: &SeatId) {
        self.node_manager = Some(seat.clone());
    }

    /// Commit time of the following commits (default: now).
    pub fn set_time(&mut self, t: &str) {
        self.time = Some(ts(t));
    }

    /// A raw file edit for the next commit (e.g. a record that must not parse).
    pub fn put_file(&mut self, path: RepoPath, bytes: Vec<u8>) {
        self.extra.put(path, bytes);
    }

    /// An `OperationRecord` (committed state), written by every following commit from then on, so only the
    /// first one shows it as a change.
    pub fn operation(&mut self, kind: RequestKind, summary: &str, requester: Requester) -> OpId {
        let id = OpId::new();
        let at = self.time.unwrap_or_else(chrono::Utc::now);
        let rec = OperationRecord {
            schema: SCHEMA_VERSION,
            id: id.clone(),
            rev: 1,
            kind,
            summary: summary.into(),
            requester,
            state: OpState::Committed,
            confirmation: None,
            plan: None,
            commit: None,
            action: None,
            supersedes: None,
            superseded_by: None,
            admitted_at: at,
            finished_at: None,
            rejection: None,
        };
        self.ops
            .insert(id.clone(), (layout::operation_record(at, &id), rec));
        id
    }

    pub fn operation_mut(&mut self, id: &OpId) -> &mut OperationRecord {
        &mut self.ops.get_mut(id).unwrap().1
    }

    /// An `ActionRecord`, kept and rewritten like `operation`.
    pub fn action(
        &mut self,
        kind: ActionKind,
        ops: &[OpId],
        affected: Vec<AffectedObject>,
        retired: Vec<AnyId>,
    ) -> ActionId {
        let id = ActionId::new();
        let at = self.time.unwrap_or_else(chrono::Utc::now);
        let rec = ActionRecord {
            schema: SCHEMA_VERSION,
            id: id.clone(),
            rev: 1,
            kind,
            at,
            ops: ops.to_vec(),
            affected,
            retired,
            already_retired: vec![],
            compensation: Default::default(),
            undoes: None,
            undone_by: vec![],
        };
        self.actions
            .insert(id.clone(), (layout::action_record(at, &id), rec));
        id
    }

    pub fn action_mut(&mut self, id: &ActionId) -> &mut ActionRecord {
        &mut self.actions.get_mut(id).unwrap().1
    }

    /// Drop an action record from the instance (the next commit deletes its file).
    pub fn remove_action(&mut self, id: &ActionId) {
        let (path, _) = self.actions.remove(id).unwrap();
        self.extra.delete(path);
    }

    /// Move a seat (without clones) to a new folder name: the next commit deletes the old record and writes
    /// the new one.
    pub fn rename_seat(&mut self, id: &SeatId, new_name: &str) {
        let (old, mut rec) = self.seats.remove(id).unwrap();
        self.extra.delete(layout::seat_record(&old));
        rec.name = new_name.into();
        let ts_dir = self.ts_dir(&rec.teamspace);
        let dir = self.free(layout::seat_dir(&ts_dir, new_name), rec.id.suffix6());
        self.seats.insert(id.clone(), (dir, rec));
    }

    /// Drop a seat (without clones) from the instance; the next commit deletes its record.
    pub fn remove_seat(&mut self, id: &SeatId) {
        let (dir, _) = self.seats.remove(id).unwrap();
        self.extra.delete(layout::seat_record(&dir));
    }

    /// Writes every record as one commit on main and returns its id.
    pub fn commit(&mut self, msg: &str) -> CommitId {
        self.commit_with_trailers(msg, None, None)
    }

    /// `commit` with the writer's `Graph-Op:` / `Graph-Action:` trailers (`src/writer/mod.rs` format).
    pub fn commit_with_trailers(
        &mut self,
        summary: &str,
        op: Option<&OpId>,
        action: Option<&ActionId>,
    ) -> CommitId {
        let mut msg = summary.to_owned();
        if op.is_some() || action.is_some() {
            msg.push('\n');
        }
        if let Some(op) = op {
            msg.push_str(&format!("\nGraph-Op: {op}\n"));
        }
        if let Some(a) = action {
            msg.push_str(&format!("Graph-Action: {a}\n"));
        }
        let mut e = std::mem::take(&mut self.extra);
        for (path, r) in self
            .ops
            .values()
            .map(|(p, r)| (p, record::to_toml_bytes(r)))
        {
            e.put(path.clone(), r.unwrap());
        }
        for (path, r) in self
            .actions
            .values()
            .map(|(p, r)| (p, record::to_toml_bytes(r)))
        {
            e.put(path.clone(), r.unwrap());
        }
        for (dir, r) in self.teamspaces.values() {
            e.put(
                layout::teamspace_record(dir),
                record::to_toml_bytes(r).unwrap(),
            );
        }
        for (dir, r) in self.seats.values() {
            e.put(layout::seat_record(dir), record::to_toml_bytes(r).unwrap());
        }
        for (dir, r) in self.clones.values() {
            e.put(layout::clone_record(dir), record::to_toml_bytes(r).unwrap());
        }
        for (path, r) in self.transcripts.values() {
            e.put(path.clone(), record::to_toml_bytes(r).unwrap());
        }
        let head = self.store.head().unwrap();
        if let Some(nm) = &self.node_manager {
            let view = crate::store::tree::CommitView {
                store: &self.store,
                at: head.clone(),
            };
            let mut graph = layout::read_graph(&view).unwrap();
            graph.node_manager_seat = Some(nm.clone());
            e.put(layout::graph_toml(), record::to_toml_bytes(&graph).unwrap());
        }
        let time = self.time;
        let tree = self.store.build_tree(&head, &e).unwrap();
        self.store
            .with_repo(|r| {
                let sig = match time {
                    Some(t) => git2::Signature::new(
                        "herdr-graph",
                        "herdr-graph@localhost",
                        &git2::Time::new(t.timestamp(), 0),
                    )?,
                    None => graph_signature()?,
                };
                let parent = r.find_commit(git2::Oid::from_str(&head.0)?)?;
                let oid = r.commit(None, &sig, &sig, &msg, &r.find_tree(tree)?, &[&parent])?;
                r.reference("refs/heads/main", oid, true, &msg)?;
                Ok(CommitId(oid.to_string()))
            })
            .unwrap()
    }
}
