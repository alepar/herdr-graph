use super::git::graph_signature;
use super::layout;
use super::slug::{slugify, unique_slug};
use super::*;
use crate::model::clone::CloneRecord;
use crate::model::seat::SeatRecord;
use crate::model::teamspace::TeamspaceRecord;
use crate::model::{
    ActionId, AnyId, AppId, CloneId, CloneLifecycle, CommitId, Lifecycle, OpId, RequestId, SeatId, TeamspaceId,
    TemplateId, TranscriptId, SCHEMA_VERSION,
};
use crate::ports::store::{read_record, RepoPath, Store, StoreError};
use chrono::TimeZone;
use std::collections::BTreeSet;

fn rp(s: &str) -> RepoPath {
    RepoPath::new(s).unwrap()
}

fn teamspace(name: &str) -> TeamspaceRecord {
    TeamspaceRecord {
        schema: SCHEMA_VERSION,
        id: TeamspaceId::new(),
        rev: 1,
        name: name.into(),
        name_history: vec![],
        lifecycle: Lifecycle::Dormant,
        retired: None,
        runtime: Default::default(),
        project_repo: None,
        channel: Default::default(),
    }
}

fn seat(name: &str, ts: &TeamspaceId) -> SeatRecord {
    SeatRecord {
        schema: SCHEMA_VERSION,
        id: SeatId::new(),
        rev: 1,
        name: name.into(),
        name_history: vec![],
        teamspace: ts.clone(),
        lifecycle: Lifecycle::Dormant,
        retired: None,
        role: None,
        template_ref: None,
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

fn clone_rec(name: &str, seat: &SeatId) -> CloneRecord {
    CloneRecord {
        schema: SCHEMA_VERSION,
        id: CloneId::new(),
        rev: 1,
        seat: seat.clone(),
        name: name.into(),
        name_history: vec![],
        lifecycle: CloneLifecycle::Active,
        retired: None,
        runtime: Default::default(),
        occupant: None,
        sessions: vec![],
        opt_outs: vec![],
        invitations: vec![],
        reload_required: false,
    }
}

fn toml_bytes<R: serde::Serialize>(r: &R) -> Vec<u8> {
    record::to_toml_bytes(r).unwrap()
}

fn new_instance() -> (tempfile::TempDir, GitStore) {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("inst");
    crate::store::init::init_instance(&dir).unwrap();
    let store = GitStore::open(&dir).unwrap();
    (tmp, store)
}

/// Build the tree from head + edits, commit on top of head and move refs/heads/main.
fn commit_edits(store: &GitStore, edits: &EditSet, msg: &str) -> CommitId {
    let head = store.head().unwrap();
    let tree = store.build_tree(&head, edits).unwrap();
    store
        .with_repo(|r| {
            let sig = graph_signature()?;
            let parent = r.find_commit(git2::Oid::from_str(&head.0)?)?;
            let oid = r.commit(None, &sig, &sig, msg, &r.find_tree(tree)?, &[&parent])?;
            r.reference("refs/heads/main", oid, true, msg)?;
            Ok(CommitId(oid.to_string()))
        })
        .unwrap()
}

/// Commit a teamspace dir `teamspaces/<slug>` with its record; returns the record.
fn commit_teamspace(store: &GitStore, slug: &str, name: &str) -> TeamspaceRecord {
    let ts = teamspace(name);
    let mut e = EditSet::default();
    e.put(layout::teamspace_record(&layout::teamspace_dir(slug)), toml_bytes(&ts));
    commit_edits(store, &e, "add teamspace");
    ts
}

mod slug {
    use super::*;

    #[test]
    fn slug_basic() {
        assert_eq!(slugify("Foreman Bot"), "foreman-bot");
        assert_eq!(slugify("  A__B  "), "a-b");
        assert_eq!(slugify("api/v2.1"), "api-v2-1");
    }

    #[test]
    fn slug_non_ascii_becomes_dash() {
        assert_eq!(slugify("Ünïcode"), "n-code");
    }

    #[test]
    fn slug_truncates_to_48() {
        assert_eq!(slugify(&"a".repeat(60)).len(), 48);
        // 47 alnum chars, then '-', then more: the 48th char is '-' and is trimmed.
        let name = format!("{}-{}", "a".repeat(47), "b".repeat(10));
        let s = slugify(&name);
        assert_eq!(s.len(), 47);
        assert!(!s.ends_with('-'));
    }

    #[test]
    fn slug_empty_becomes_x() {
        assert_eq!(slugify(""), "x");
        assert_eq!(slugify("!!!"), "x");
    }

    #[test]
    fn unique_slug_no_collision() {
        assert_eq!(unique_slug("Foreman", "01ABCD", &BTreeSet::new()), "foreman");
    }

    #[test]
    fn unique_slug_collision_appends_lower_id6() {
        let taken: BTreeSet<String> = ["foreman".to_string()].into();
        assert_eq!(unique_slug("Foreman", "01ABCD", &taken), "foreman-01abcd");
    }
}

mod layout_paths {
    use super::*;

    #[test]
    fn layout_paths_exact() {
        let ts = TeamspaceId::new();
        let st = SeatId::new();
        let tsd = layout::teamspace_dir("alpha");
        let sd = layout::seat_dir(&tsd, "foreman");
        let cd = layout::clone_dir(&sd, "main");
        let tpl = layout::template_dir("pair");
        let app = AppId::new();
        let tr = TranscriptId::new();
        let rq = RequestId::new();
        let act = ActionId::new();
        let op = OpId::new();
        let at = chrono::Utc.with_ymd_and_hms(2026, 3, 9, 10, 0, 0).unwrap();
        let _ = &ts;
        assert_eq!(layout::graph_toml().as_str(), "graph.toml");
        assert_eq!(layout::teamspaces_root().as_str(), "teamspaces");
        assert_eq!(tsd.as_str(), "teamspaces/alpha");
        assert_eq!(layout::teamspace_record(&tsd).as_str(), "teamspaces/alpha/teamspace.toml");
        assert_eq!(sd.as_str(), "teamspaces/alpha/seats/foreman");
        assert_eq!(layout::seat_record(&sd).as_str(), "teamspaces/alpha/seats/foreman/seat.toml");
        assert_eq!(cd.as_str(), "teamspaces/alpha/seats/foreman/clones/main");
        assert_eq!(layout::clone_record(&cd).as_str(), "teamspaces/alpha/seats/foreman/clones/main/clone.toml");
        assert_eq!(tpl.as_str(), "templates/pair");
        assert_eq!(layout::template_record(&tpl).as_str(), "templates/pair/template.toml");
        assert_eq!(layout::member_agents_md(&tpl, "lead").as_str(), "templates/pair/members/lead/AGENTS.md");
        assert_eq!(layout::application_record(&app).as_str(), format!("applications/{app}.toml"));
        assert_eq!(layout::transcript_record(&st, &tr).as_str(), format!("transcripts/{st}/{tr}.toml"));
        assert_eq!(layout::request_record(&rq).as_str(), format!("requests/{rq}.toml"));
        assert_eq!(layout::action_record(at, &act).as_str(), format!("actions/2026-03/{act}.toml"));
        assert_eq!(layout::operation_record(at, &op).as_str(), format!("operations/2026-03/{op}.toml"));
    }

    #[test]
    fn archive_paths_use_lower_id6() {
        let ts = TeamspaceId::new();
        let st = SeatId::new();
        let tsd = layout::teamspace_dir("alpha");
        assert_eq!(
            layout::archived_teamspace_dir("alpha", &ts).as_str(),
            format!("archive/teamspaces/alpha-{}", ts.suffix6().to_lowercase())
        );
        assert_eq!(
            layout::archived_seat_dir(&tsd, "foreman", &st).as_str(),
            format!("teamspaces/alpha/archive/seats/foreman-{}", st.suffix6().to_lowercase())
        );
    }

    #[test]
    fn action_and_operation_paths_use_year_month() {
        let act = ActionId::new();
        let op = OpId::new();
        let dec = chrono::Utc.with_ymd_and_hms(2025, 12, 31, 23, 59, 59).unwrap();
        let jan = chrono::Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        assert!(layout::action_record(dec, &act).as_str().starts_with("actions/2025-12/"));
        assert!(layout::action_record(jan, &act).as_str().starts_with("actions/2026-01/"));
        assert!(layout::operation_record(dec, &op).as_str().starts_with("operations/2025-12/"));
        assert!(layout::operation_record(jan, &op).as_str().starts_with("operations/2026-01/"));
    }
}

mod tree {
    use super::*;

    /// Instance with teamspace `t` (rev 1) and seat `foreman` committed; returns (guard, store, seat).
    fn with_seat(rev: u64) -> (tempfile::TempDir, GitStore, SeatRecord) {
        let (tmp, store) = new_instance();
        let ts = commit_teamspace(&store, "t", "T");
        let mut st = seat("Foreman", &ts.id);
        st.rev = rev;
        let sd = layout::seat_dir(&layout::teamspace_dir("t"), "foreman");
        let mut e = EditSet::default();
        e.put(layout::seat_record(&sd), toml_bytes(&st));
        e.put(sd.join("AGENTS.md").unwrap(), b"hello".to_vec());
        e.put(sd.join("notes/a.md").unwrap(), b"note".to_vec());
        commit_edits(&store, &e, "add seat");
        (tmp, store, st)
    }

    #[test]
    fn overlay_reads_through_edits() {
        let (_t, store, _st) = with_seat(1);
        let mut ov = Overlay::new(&store, store.head().unwrap());
        let f = rp("teamspaces/t/seats/foreman/AGENTS.md");
        assert_eq!(ov.read_file(&f).unwrap().unwrap(), b"hello");
        ov.put_file(f.clone(), b"changed".to_vec());
        assert_eq!(ov.read_file(&f).unwrap().unwrap(), b"changed");
        // The base is untouched.
        assert_eq!(store.read_file(ov.base(), &f).unwrap().unwrap(), b"hello");
    }

    #[test]
    fn overlay_delete_hides_base_file() {
        let (_t, store, _st) = with_seat(1);
        let mut ov = Overlay::new(&store, store.head().unwrap());
        let f = rp("teamspaces/t/seats/foreman/AGENTS.md");
        ov.delete_file(&f);
        assert_eq!(ov.read_file(&f).unwrap(), None);
        let names: Vec<_> =
            ov.list_dir(&rp("teamspaces/t/seats/foreman")).unwrap().into_iter().map(|e| e.name).collect();
        assert!(!names.contains(&"AGENTS.md".to_string()), "{names:?}");
        assert!(names.contains(&"seat.toml".to_string()));
    }

    #[test]
    fn overlay_list_dir_merges_and_drops_empty_dirs() {
        let (_t, store, _st) = with_seat(1);
        let mut ov = Overlay::new(&store, store.head().unwrap());
        let sd = rp("teamspaces/t/seats/foreman");
        ov.put_file(rp("teamspaces/t/seats/foreman/new/x.md"), b"x".to_vec());
        ov.delete_file(&rp("teamspaces/t/seats/foreman/notes/a.md"));
        let entries = ov.list_dir(&sd).unwrap();
        let find = |n: &str| entries.iter().find(|e| e.name == n).map(|e| e.kind);
        assert_eq!(find("new"), Some(crate::ports::store::EntryKind::Dir));
        assert_eq!(find("notes"), None, "dir whose files are all deleted disappears: {entries:?}");
        assert_eq!(find("AGENTS.md"), Some(crate::ports::store::EntryKind::File));
    }

    #[test]
    fn overlay_move_dir_moves_all_files() {
        let (_t, store, _st) = with_seat(1);
        let mut ov = Overlay::new(&store, store.head().unwrap());
        let from = rp("teamspaces/t/seats/foreman");
        let to = rp("teamspaces/t/seats/lead");
        let moved = ov.move_dir(&from, &to).unwrap();
        assert_eq!(moved.len(), 3);
        assert_eq!(ov.read_file(&rp("teamspaces/t/seats/lead/notes/a.md")).unwrap().unwrap(), b"note");
        assert_eq!(ov.read_file(&rp("teamspaces/t/seats/lead/AGENTS.md")).unwrap().unwrap(), b"hello");
        assert!(ov.files_under(&from).unwrap().is_empty());
        assert_eq!(ov.files_under(&to).unwrap().len(), 3);
        // Applying the edits to the repo leaves no old directory behind.
        let c = commit_edits(&store, ov.edits(), "move");
        assert!(store.list_dir(&c, &from).unwrap().is_empty());
        assert_eq!(store.read_file(&c, &rp("teamspaces/t/seats/lead/notes/a.md")).unwrap().unwrap(), b"note");
    }

    #[test]
    fn overlay_put_record_bumps_rev_once() {
        let (_t, store, st) = with_seat(3);
        let mut ov = Overlay::new(&store, store.head().unwrap());
        let p = rp("teamspaces/t/seats/foreman/seat.toml");
        let mut rec = st.clone();
        ov.put_record(p.clone(), &mut rec).unwrap();
        assert_eq!(rec.rev, 4);
        rec.name = "Foreman2".into();
        ov.put_record(p.clone(), &mut rec).unwrap();
        let back: SeatRecord = ov.read_record(&p).unwrap().unwrap();
        assert_eq!(back.rev, 4);
        assert_eq!(back.name, "Foreman2");
    }

    #[test]
    fn overlay_put_record_refuses_a_second_record_for_an_existing_id() {
        let (_t, store, st) = with_seat(3);
        let mut ov = Overlay::new(&store, store.head().unwrap());
        let mut rec = st.clone();
        let err = ov.put_record(rp("teamspaces/t/seats/other/seat.toml"), &mut rec).unwrap_err();
        match err {
            StoreError::Corrupt { reason, .. } => assert!(reason.contains(&st.id.to_string()), "{reason}"),
            other => panic!("expected Corrupt, got {other:?}"),
        }
        assert!(ov.read_file(&rp("teamspaces/t/seats/other/seat.toml")).unwrap().is_none());
    }

    #[test]
    fn overlay_put_record_after_move_dir_is_allowed() {
        let (_t, store, st) = with_seat(3);
        let mut ov = Overlay::new(&store, store.head().unwrap());
        ov.move_dir(&rp("teamspaces/t/seats/foreman"), &rp("teamspaces/t/seats/moved")).unwrap();
        let mut rec = st.clone();
        ov.put_record(rp("teamspaces/t/seats/moved/seat.toml"), &mut rec).unwrap();
        assert_eq!(rec.rev, 4);
    }

    #[test]
    fn overlay_put_record_new_object_gets_rev_1() {
        let (_t, store, _st) = with_seat(1);
        let mut ov = Overlay::new(&store, store.head().unwrap());
        let ts = teamspace("Fresh");
        let mut rec = seat("New", &ts.id);
        rec.rev = 99;
        ov.put_record(rp("teamspaces/t/seats/new/seat.toml"), &mut rec).unwrap();
        assert_eq!(rec.rev, 1);
    }
}

mod git {
    use super::*;
    use crate::store::layout::{self, taken_slugs};

    #[test]
    fn open_rejects_non_instance() {
        let tmp = tempfile::tempdir().unwrap();
        git2::Repository::init(tmp.path()).unwrap();
        assert!(matches!(GitStore::open(tmp.path()), Err(StoreError::NoInstance(_))));
        let plain = tempfile::tempdir().unwrap();
        assert!(matches!(GitStore::open(plain.path()), Err(StoreError::NoInstance(_))));
    }

    #[test]
    fn read_at_old_commit_returns_that_revision() {
        let (_t, store) = new_instance();
        let ts = teamspace("T");
        let mut st = seat("a", &ts.id);
        let path = layout::seat_record(&layout::seat_dir(&layout::teamspace_dir("t"), "a"));
        let mut e = EditSet::default();
        e.put(path.clone(), toml_bytes(&st));
        let c1 = commit_edits(&store, &e, "c1");
        st.name = "b".into();
        st.rev = 2;
        let mut e = EditSet::default();
        e.put(path.clone(), toml_bytes(&st));
        let c2 = commit_edits(&store, &e, "c2");

        let old: SeatRecord = read_record(&store, &c1, &path).unwrap().unwrap();
        assert_eq!((old.name.as_str(), old.rev), ("a", 1));
        let new: SeatRecord = read_record(&store, &c2, &path).unwrap().unwrap();
        assert_eq!((new.name.as_str(), new.rev), ("b", 2));
        assert_eq!(store.head().unwrap(), c2);

        // Hand-editing the working tree never changes what readers see.
        let on_disk = store.root().join(path.as_str());
        std::fs::create_dir_all(on_disk.parent().unwrap()).unwrap();
        std::fs::write(&on_disk, "garbage = true").unwrap();
        let still: SeatRecord = read_record(&store, &c2, &path).unwrap().unwrap();
        assert_eq!(still.name, "b");
        assert!(store.blob_hash(&c1, &path).unwrap() != store.blob_hash(&c2, &path).unwrap());
    }

    #[test]
    fn read_unknown_commit_is_error() {
        let (_t, store) = new_instance();
        let bad = CommitId("0".repeat(40));
        assert!(matches!(store.read_file(&bad, &rp("graph.toml")), Err(StoreError::UnknownCommit(_))));
        let junk = CommitId("zz".into());
        assert!(matches!(store.list_dir(&junk, &rp("")), Err(StoreError::UnknownCommit(_))));
    }

    #[test]
    fn rename_moves_path() {
        let (_t, store) = new_instance();
        let ts = commit_teamspace(&store, "t", "T");
        let tsd = layout::teamspace_dir("t");
        let mut st = seat("Foreman", &ts.id);
        let sd = layout::seat_dir(&tsd, "foreman");
        let mut e = EditSet::default();
        e.put(layout::seat_record(&sd), toml_bytes(&st));
        e.put(sd.join("AGENTS.md").unwrap(), b"rules".to_vec());
        let c1 = commit_edits(&store, &e, "seat");

        let mut ov = Overlay::new(&store, c1.clone());
        st.name = "Lead".into();
        let taken = taken_slugs(&ov, &tsd.join("seats").unwrap()).unwrap();
        let slug = unique_slug(&st.name, st.id.suffix6(), &taken);
        assert_eq!(slug, "lead");
        let new_dir = layout::seat_dir(&tsd, &slug);
        ov.move_dir(&sd, &new_dir).unwrap();
        ov.put_record(layout::seat_record(&new_dir), &mut st).unwrap();
        let c2 = commit_edits(&store, &ov.into_edits(), "rename");

        let id = st.id.to_any();
        let new_loc = store.locate(&c2, &id).unwrap().unwrap();
        assert_eq!(new_loc.record_path.as_str(), "teamspaces/t/seats/lead/seat.toml");
        assert_eq!(new_loc.folder.as_str(), "teamspaces/t/seats/lead");
        let old_loc = store.locate(&c1, &id).unwrap().unwrap();
        assert_eq!(old_loc.record_path.as_str(), "teamspaces/t/seats/foreman/seat.toml");
        assert_eq!(store.read_file(&c2, &rp("teamspaces/t/seats/lead/AGENTS.md")).unwrap().unwrap(), b"rules");
        assert_eq!(store.read_file(&c2, &rp("teamspaces/t/seats/foreman/AGENTS.md")).unwrap(), None);
        let moved: SeatRecord = read_record(&store, &c2, &new_loc.record_path).unwrap().unwrap();
        assert_eq!((moved.name.as_str(), moved.rev), ("Lead", 2));
    }

    #[test]
    fn rename_collision_gets_suffix() {
        let (_t, store) = new_instance();
        let ts = commit_teamspace(&store, "t", "T");
        let tsd = layout::teamspace_dir("t");
        let mut a = seat("A", &ts.id);
        let mut b = seat("B", &ts.id);
        let mut e = EditSet::default();
        e.put(layout::seat_record(&layout::seat_dir(&tsd, "a")), toml_bytes(&a));
        e.put(layout::seat_record(&layout::seat_dir(&tsd, "b")), toml_bytes(&b));
        let c1 = commit_edits(&store, &e, "seats");

        let mut ov = Overlay::new(&store, c1);
        let seats = tsd.join("seats").unwrap();
        let mut slugs = vec![];
        for (rec, old) in [(&mut a, "a"), (&mut b, "b")] {
            rec.name = "Lead".into();
            let taken = taken_slugs(&ov, &seats).unwrap();
            let slug = unique_slug(&rec.name, rec.id.suffix6(), &taken);
            let nd = layout::seat_dir(&tsd, &slug);
            ov.move_dir(&layout::seat_dir(&tsd, old), &nd).unwrap();
            ov.put_record(layout::seat_record(&nd), rec).unwrap();
            slugs.push(slug);
        }
        assert_eq!(slugs[0], "lead");
        assert_eq!(slugs[1], format!("lead-{}", b.id.suffix6().to_lowercase()));
        let c2 = commit_edits(&store, &ov.into_edits(), "renames");
        let loc = store.locate(&c2, &b.id.to_any()).unwrap().unwrap();
        assert_eq!(loc.folder.as_str(), format!("teamspaces/t/seats/{}", slugs[1]));
    }

    #[test]
    fn archive_paths_and_locate_archived() {
        let (_t, store) = new_instance();
        let ts = commit_teamspace(&store, "t", "T");
        let tsd = layout::teamspace_dir("t");
        let st = seat("Foreman", &ts.id);
        let sd = layout::seat_dir(&tsd, "foreman");
        let cl = clone_rec("main", &st.id);
        let mut e = EditSet::default();
        e.put(layout::seat_record(&sd), toml_bytes(&st));
        e.put(layout::clone_record(&layout::clone_dir(&sd, "main")), toml_bytes(&cl));
        let c1 = commit_edits(&store, &e, "seat");

        // Archive the seat inside its teamspace.
        let mut ov = Overlay::new(&store, c1);
        let asd = layout::archived_seat_dir(&tsd, "foreman", &st.id);
        ov.move_dir(&sd, &asd).unwrap();
        let c2 = commit_edits(&store, &ov.into_edits(), "retire seat");
        let loc = store.locate(&c2, &st.id.to_any()).unwrap().unwrap();
        assert_eq!(loc.folder, asd);
        let view = store.at(&c2);
        let seats = layout::list_seats(&view, &tsd).unwrap();
        assert_eq!(seats.len(), 1);
        assert_eq!(seats[0].0.folder, asd);
        let cloc = store.locate(&c2, &cl.id.to_any()).unwrap().unwrap();
        assert!(cloc.record_path.as_str().starts_with(asd.as_str()));

        // Archive the whole teamspace: its seats and clones are still found.
        let mut ov = Overlay::new(&store, c2);
        let ats = layout::archived_teamspace_dir("t", &ts.id);
        ov.move_dir(&tsd, &ats).unwrap();
        let c3 = commit_edits(&store, &ov.into_edits(), "retire teamspace");
        let tloc = store.locate(&c3, &ts.id.to_any()).unwrap().unwrap();
        assert_eq!(tloc.folder, ats);
        let view = store.at(&c3);
        let all = layout::all_seats(&view).unwrap();
        assert_eq!(all.len(), 1);
        assert!(all[0].0.record_path.as_str().starts_with(ats.as_str()));
        assert_eq!(layout::all_clones(&view).unwrap().len(), 1);
        assert!(layout::list_teamspaces(&view).unwrap().iter().all(|(l, _)| l.folder == ats));
    }

    #[test]
    fn locate_each_kind() {
        use crate::model::action::{ActionKind, ActionRecord};
        use crate::model::application::{ApplicationRecord, CreatedBy};
        use crate::model::request::{ProcessingRequest, RequestStatus};
        use crate::model::template::TemplateRecord;
        use crate::model::transcript::TranscriptRecord;
        use crate::model::{ByteRange, NsId};

        let (_t, store) = new_instance();
        let at = chrono::Utc.with_ymd_and_hms(2026, 3, 9, 10, 0, 0).unwrap();
        let seat_id = SeatId::new();
        let tpl = TemplateRecord {
            schema: 1,
            id: TemplateId::new(),
            rev: 1,
            name: "Pair".into(),
            name_history: vec![],
            defaults: Default::default(),
            members: vec![],
            relationships: vec![],
            copied_from: None,
        };
        let app = ApplicationRecord {
            schema: 1,
            id: AppId::new(),
            rev: 1,
            name: "app".into(),
            template: tpl.id.clone(),
            teamspace: TeamspaceId::new(),
            lifecycle: Default::default(),
            retired: None,
            member_map: Default::default(),
            additions: vec![],
            exclusions: vec![],
            reused: vec![],
            contributions: Default::default(),
            created_by: CreatedBy { op: OpId::new(), action: ActionId::new() },
        };
        let rq = ProcessingRequest {
            schema: 1,
            id: RequestId::new(),
            rev: 1,
            transcript: TranscriptId::new(),
            range: ByteRange { start: 0, end: 10 },
            status: RequestStatus::Pending,
            unresolved: None,
            undeliverable: None,
            created_by_op: OpId::new(),
            delivery: Default::default(),
            result: None,
        };
        let tr = TranscriptRecord {
            schema: 1,
            id: TranscriptId::new(),
            rev: 1,
            transcript_path: "/x".into(),
            native_session: NsId::new(),
            seat: seat_id.clone(),
            clone: CloneId::new(),
            source_seat_summaries_enabled_at_capture: false,
            coverage: vec![],
            gaps: vec![],
            unresolved: None,
        };
        let act = ActionRecord {
            schema: 1,
            id: ActionId::new(),
            rev: 1,
            kind: ActionKind::Retire,
            at,
            ops: vec![],
            affected: vec![],
            retired: vec![],
            already_retired: vec![],
            compensation: Default::default(),
            undoes: None,
            undone_by: vec![],
        };
        let mut e = EditSet::default();
        let tpl_path = layout::template_record(&layout::template_dir("pair"));
        let app_path = layout::application_record(&app.id);
        let rq_path = layout::request_record(&rq.id);
        let tr_path = layout::transcript_record(&seat_id, &tr.id);
        let act_path = layout::action_record(at, &act.id);
        e.put(tpl_path.clone(), toml_bytes(&tpl));
        e.put(app_path.clone(), toml_bytes(&app));
        e.put(rq_path.clone(), toml_bytes(&rq));
        e.put(tr_path.clone(), toml_bytes(&tr));
        e.put(act_path.clone(), toml_bytes(&act));
        let c = commit_edits(&store, &e, "records");

        let want = |id: AnyId, path: &RepoPath| {
            let loc = store.locate(&c, &id).unwrap().unwrap_or_else(|| panic!("{id} not located"));
            assert_eq!(&loc.record_path, path);
            assert_eq!(loc.id, id);
        };
        want(tpl.id.to_any(), &tpl_path);
        want(app.id.to_any(), &app_path);
        want(rq.id.to_any(), &rq_path);
        want(tr.id.to_any(), &tr_path);
        want(act.id.to_any(), &act_path);
        assert_eq!(store.locate(&c, &act.id.to_any()).unwrap().unwrap().folder.as_str(), "actions/2026-03");
        // Absent objects, and kinds that are not standalone objects.
        assert_eq!(store.locate(&c, &OpId::new().to_any()).unwrap(), None);
        assert_eq!(store.locate(&c, &crate::model::MemberId::new().to_any()).unwrap(), None);
        assert_eq!(store.locate(&c, &NsId::new().to_any()).unwrap(), None);
        // Enumerators agree.
        let view = store.at(&c);
        assert_eq!(layout::list_templates(&view).unwrap().len(), 1);
        assert_eq!(layout::list_applications(&view).unwrap().len(), 1);
        assert_eq!(layout::list_requests(&view).unwrap().len(), 1);
        assert_eq!(layout::list_transcripts(&view).unwrap().len(), 1);
        assert_eq!(layout::list_actions(&view).unwrap().len(), 1);
        assert_eq!(layout::list_operations(&view).unwrap().len(), 0);
    }

    #[test]
    fn build_tree_applies_puts_and_deletes() {
        let (_t, store) = new_instance();
        let mut e = EditSet::default();
        e.put(rp("a/b/one.txt"), b"1".to_vec());
        e.put(rp("a/two.txt"), b"2".to_vec());
        let c1 = commit_edits(&store, &e, "add");

        let mut e = EditSet::default();
        e.put(rp("a/two.txt"), b"22".to_vec());
        e.put(rp("c.txt"), b"3".to_vec());
        e.delete(rp("a/b/one.txt"));
        e.delete(rp("never/existed.txt"));
        let c2 = commit_edits(&store, &e, "edit");

        assert_eq!(store.read_file(&c2, &rp("a/two.txt")).unwrap().unwrap(), b"22");
        assert_eq!(store.read_file(&c2, &rp("c.txt")).unwrap().unwrap(), b"3");
        assert_eq!(store.read_file(&c2, &rp("a/b/one.txt")).unwrap(), None);
        assert!(store.list_dir(&c2, &rp("a/b")).unwrap().is_empty(), "empty dir is pruned");
        // Old commit unchanged; untouched files carried over.
        assert_eq!(store.read_file(&c1, &rp("a/b/one.txt")).unwrap().unwrap(), b"1");
        assert!(store.read_file(&c2, &rp(".gitignore")).unwrap().is_some());
        // build_tree alone moves no ref.
        assert_eq!(store.head().unwrap(), c2);
        let before = store.head().unwrap();
        let mut e = EditSet::default();
        e.put(rp("zzz"), vec![]);
        store.build_tree(&before, &e).unwrap();
        assert_eq!(store.head().unwrap(), before);
    }

    #[test]
    fn corrupt_record_is_error() {
        let (_t, store) = new_instance();
        let ts = commit_teamspace(&store, "t", "T");
        let _ = ts;
        let mut e = EditSet::default();
        e.put(rp("teamspaces/t/seats/bad/seat.toml"), b"this is = not [valid".to_vec());
        let c = commit_edits(&store, &e, "bad");
        let view = store.at(&c);
        let err = layout::all_seats(&view).unwrap_err();
        assert!(
            matches!(&err, StoreError::Corrupt { path, .. } if path == "teamspaces/t/seats/bad/seat.toml"),
            "{err:?}"
        );
    }

    #[test]
    fn list_filters_non_toml_and_missing_records() {
        let (_t, store) = new_instance();
        commit_teamspace(&store, "t", "T");
        let mut e = EditSet::default();
        e.put(rp("teamspaces/t/seats/empty/notes.md"), b"no seat.toml here".to_vec());
        let c = commit_edits(&store, &e, "dir without record");
        assert!(layout::all_seats(&store.at(&c)).unwrap().is_empty());
        assert_eq!(layout::list_teamspaces(&store.at(&c)).unwrap().len(), 1);
    }
}

mod init {
    use super::*;
    use crate::store::init::{init_instance, write_user_config, UserConfigOutcome};

    #[test]
    fn init_creates_valid_instance() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("inst");
        let c = init_instance(&dir).unwrap();
        let store = GitStore::open(&dir).unwrap();
        assert_eq!(store.head().unwrap(), c);
        let g = layout::read_graph(&store.at(&c)).unwrap();
        assert_eq!(g.schema_version, 1);
        assert!(!g.instance_id.is_empty());
        let ignore = store.read_file(&c, &rp(".gitignore")).unwrap().unwrap();
        assert!(String::from_utf8(ignore).unwrap().contains("/.graph-local/"));
        for keep in ["teamspaces/.gitkeep", "templates/.gitkeep", "rules/.gitkeep"] {
            assert!(store.read_file(&c, &rp(keep)).unwrap().is_some(), "{keep}");
        }
        assert!(dir.join(".graph-local").is_dir());
        store
            .with_repo(|r| {
                assert_eq!(r.find_reference("HEAD")?.symbolic_target(), Some("refs/heads/main"));
                let mut so = git2::StatusOptions::new();
                so.include_ignored(false).include_untracked(true);
                let st = r.statuses(Some(&mut so))?;
                let dirty: Vec<_> = st.iter().map(|e| (e.path().map(str::to_owned), e.status())).collect();
                assert!(dirty.is_empty(), "working tree must be clean: {dirty:?}");
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn init_refuses_existing_instance() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("inst");
        init_instance(&dir).unwrap();
        match init_instance(&dir) {
            Err(StoreError::Io(e)) => assert_eq!(e.kind(), std::io::ErrorKind::AlreadyExists),
            other => panic!("expected AlreadyExists, got {other:?}"),
        }
    }

    #[test]
    fn write_user_config_outcomes() {
        let home = tempfile::tempdir().unwrap();
        let a = home.path().join("inst-a");
        let b = home.path().join("inst-b");
        let cfg = home.path().join(".config/herdr-graph/config.toml");
        assert_eq!(write_user_config(home.path(), &a).unwrap(), UserConfigOutcome::Written(cfg.clone()));
        let text = std::fs::read_to_string(&cfg).unwrap();
        assert!(text.contains(a.to_str().unwrap()));
        assert_eq!(write_user_config(home.path(), &a).unwrap(), UserConfigOutcome::AlreadyPointsHere(cfg.clone()));
        assert_eq!(write_user_config(home.path(), &b).unwrap(), UserConfigOutcome::LeftExisting(cfg.clone()));
        assert_eq!(std::fs::read_to_string(&cfg).unwrap(), text, "existing config is never overwritten");
    }
}
