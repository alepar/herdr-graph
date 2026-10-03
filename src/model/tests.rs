use super::action::*;
use super::application::*;
use super::change::*;
use super::clone::*;
use super::effect::*;
use super::graph::*;
use super::harness::*;
use super::launch::*;
use super::native_session::*;
use super::operation::*;
use super::request::*;
use super::seat::*;
use super::teamspace::*;
use super::template::*;
use super::transcript::*;
use super::*;
use serde::{Serialize, de::DeserializeOwned};
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::path::{Path, PathBuf};

fn toml_rt<T: Serialize + DeserializeOwned + PartialEq + Debug>(v: &T) {
    let s = toml::to_string(v).expect("serialize toml");
    let back: T = toml::from_str(&s).unwrap_or_else(|e| panic!("parse toml: {e}\n{s}"));
    assert_eq!(&back, v, "toml round-trip\n{s}");
}
fn json_rt<T: Serialize + DeserializeOwned + PartialEq + Debug>(v: &T) {
    let s = serde_json::to_string(v).unwrap();
    let back: T = serde_json::from_str(&s).unwrap();
    assert_eq!(&back, v);
}
fn ts() -> Timestamp {
    chrono::DateTime::parse_from_rfc3339("2026-10-02T07:46:57.123456789Z")
        .unwrap()
        .with_timezone(&chrono::Utc)
}

// ---- ids -------------------------------------------------------------------------------------

mod ids {
    use super::*;

    #[test]
    fn id_new_has_prefix_and_ulid_body() {
        let id = SeatId::new();
        assert!(id.as_str().starts_with("st_"));
        assert_eq!(id.as_str().len(), 3 + 26);
    }

    #[test]
    fn id_parse_rejects_wrong_prefix_length_and_charset() {
        let clone = CloneId::new();
        assert!(SeatId::parse(clone.as_str()).is_err());
        assert!(SeatId::parse("st_short").is_err());
        let ok = SeatId::new();
        let lower = format!("st_{}", ok.as_str()[3..].to_lowercase());
        assert!(SeatId::parse(&lower).is_err());
        for bad in ['I', 'L', 'O', 'U'] {
            let body = format!("{}{}", &ok.as_str()[3..28], bad);
            assert!(SeatId::parse(&format!("st_{body}")).is_err(), "{bad}");
        }
        assert!(SeatId::parse(ok.as_str()).is_ok());
    }

    #[test]
    fn id_serde_json_and_toml_roundtrip() {
        #[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
        struct W {
            id: SeatId,
        }
        let w = W { id: SeatId::new() };
        json_rt(&w);
        toml_rt(&w);
        let wrong = format!(r#"{{"id":"{}"}}"#, CloneId::new());
        assert!(serde_json::from_str::<W>(&wrong).is_err());
    }

    #[test]
    fn any_id_kind() {
        let c = CloneId::new();
        let any = AnyId::parse(c.as_str()).unwrap();
        assert_eq!(any.kind(), IdKind::Clone);
        let body = &c.as_str()[3..];
        assert!(AnyId::parse(&format!("xx_{body}")).is_err());
        assert!(SeatId::try_from(any).is_err());
    }

    #[test]
    fn effect_id_derive_is_deterministic_and_rev_sensitive() {
        let op = OpId::new();
        let obj = SeatId::new().to_any();
        let a = EffectId::derive(&op, &obj, "create_tab", 3);
        let b = EffectId::derive(&op, &obj, "create_tab", 3);
        assert_eq!(a, b);
        assert_ne!(a, EffectId::derive(&op, &obj, "create_tab", 4));
        assert_eq!(a.as_str().len(), 3 + 26);
        assert!(a.as_str().starts_with("ef_"));
        assert_eq!(EffectId::parse(a.as_str()).unwrap(), a);
    }

    #[test]
    fn suffix6() {
        let id = SeatId::new();
        assert_eq!(id.suffix6().len(), 6);
        assert_eq!(id.suffix6(), &id.as_str()[id.as_str().len() - 6..]);
    }
}

// ---- samples ---------------------------------------------------------------------------------

fn name_change() -> NameChange {
    NameChange {
        old: "a".into(),
        new: "b".into(),
        observed_at: ts(),
        event_at: Some(ts()),
        source: NameSource::Observed,
    }
}
fn retirement() -> Retirement {
    Retirement {
        op: OpId::new(),
        action: Some(ActionId::new()),
        at: ts(),
        mechanism: RetireMechanism::ObservedPaneClose,
    }
}
fn full_runtime() -> Runtime {
    Runtime {
        availability: Availability::Present,
        bound: Some(Binding {
            token: Some("hg=x".into()),
            workspace_id: Some(HerdrWorkspaceId("w1".into())),
            tab_id: Some(HerdrTabId("t1".into())),
            pane_id: Some(HerdrPaneId("p1".into())),
            terminal_id: Some(HerdrTerminalId("term1".into())),
            incarnation: Incarnation {
                generation: 7,
                server_pid: Some(42),
                server_started: Some("2026-10-02T00:00:00Z".into()),
            },
        }),
        observed_at: Some(ts()),
    }
}
fn minimal_runtime() -> Runtime {
    Runtime {
        availability: Availability::Unknown,
        bound: None,
        observed_at: None,
    }
}

#[test]
fn graph_record_roundtrip() {
    toml_rt(&GraphRecord {
        schema: 1,
        instance_id: "inst".into(),
        schema_version: 1,
        summarizer_seat: Some(SeatId::new()),
        defaults: GraphDefaults {
            harness: Some(Harness::Codex),
            model: Some("o3".into()),
        },
    });
    toml_rt(&GraphRecord {
        schema: 1,
        instance_id: "inst".into(),
        schema_version: 1,
        summarizer_seat: None,
        defaults: GraphDefaults::default(),
    });
}

#[test]
fn teamspace_record_roundtrip() {
    toml_rt(&TeamspaceRecord {
        schema: 1,
        id: TeamspaceId::new(),
        rev: 3,
        name: "ts".into(),
        name_history: vec![name_change()],
        lifecycle: Lifecycle::Retired,
        retired: Some(retirement()),
        runtime: full_runtime(),
        project_repo: Some(PathBuf::from("/repo")),
        channel: Channel {
            thread_id: Some("th1".into()),
        },
    });
    toml_rt(&TeamspaceRecord {
        schema: 1,
        id: TeamspaceId::new(),
        rev: 1,
        name: "ts".into(),
        name_history: vec![],
        lifecycle: Lifecycle::Dormant,
        retired: None,
        runtime: minimal_runtime(),
        project_repo: None,
        channel: Channel::default(),
    });
}

#[test]
fn seat_record_roundtrip() {
    toml_rt(&SeatRecord {
        schema: 1,
        id: SeatId::new(),
        rev: 9,
        name: "foreman".into(),
        name_history: vec![name_change()],
        teamspace: TeamspaceId::new(),
        lifecycle: Lifecycle::Active,
        retired: Some(retirement()),
        role: Some(Role::Summarizer),
        template_ref: Some(TemplateRef {
            template: TemplateId::new(),
            member: MemberId::new(),
        }),
        applications: vec![AppId::new()],
        overrides: SeatOverrides {
            harness: Some(Harness::Claude),
            model: Some("opus".into()),
            args: Some(vec!["--x".into()]),
            summaries: Some(false),
            cwd: Some(PathBuf::from("/w")),
            instructions_sections: vec![InstructionSection {
                name: "n".into(),
                body: "b\nline".into(),
            }],
        },
        participation: Participation {
            seat_wide: vec!["th".into()],
        },
        activation: Activation {
            last_op: Some(OpId::new()),
        },
        runtime: full_runtime(),
        channel: Channel {
            thread_id: Some("t".into()),
        },
        reload_required: true,
        moved_out: true,
    });
    toml_rt(&SeatRecord {
        schema: 1,
        id: SeatId::new(),
        rev: 1,
        name: "foreman".into(),
        name_history: vec![],
        teamspace: TeamspaceId::new(),
        lifecycle: Lifecycle::Dormant,
        retired: None,
        role: None,
        template_ref: None,
        applications: vec![],
        overrides: SeatOverrides::default(),
        participation: Participation::default(),
        activation: Activation::default(),
        runtime: minimal_runtime(),
        channel: Channel::default(),
        reload_required: false,
        moved_out: false,
    });
}

fn native_session_full() -> NativeSession {
    NativeSession {
        id: NsId::new(),
        harness: Harness::Claude,
        native_session_id: "abc".into(),
        transcript_path: Some(PathBuf::from("/t.jsonl")),
        transcript: Some(TranscriptId::new()),
        cwd: PathBuf::from("/w"),
        started: ts(),
        ended: Some(ts()),
        end_reason: Some(SessionEndReason::Replaced),
    }
}

#[test]
fn clone_record_roundtrip() {
    toml_rt(&CloneRecord {
        schema: 1,
        id: CloneId::new(),
        rev: 4,
        seat: SeatId::new(),
        name: "c".into(),
        name_history: vec![name_change()],
        lifecycle: CloneLifecycle::Retired,
        retired: Some(retirement()),
        runtime: full_runtime(),
        occupant: Some(Occupant {
            native_session: NsId::new(),
            harness: Harness::Codex,
            since: ts(),
        }),
        sessions: vec![
            native_session_full(),
            NativeSession {
                id: NsId::new(),
                harness: Harness::Codex,
                native_session_id: "x".into(),
                transcript_path: None,
                transcript: None,
                cwd: PathBuf::from("/w"),
                started: ts(),
                ended: None,
                end_reason: None,
            },
        ],
        opt_outs: vec!["th".into()],
        invitations: vec![Invitation {
            thread: "th".into(),
            constraint: InviteConstraint::Required,
            state: InvitationState::Pending,
            link: Some(ThreadsLink {
                seat: "seat-k3Fq9a2B".into(),
                occupant: Some(NsId::new().to_string()),
                invitation: Some("inv-1".into()),
                requirement: Some("requirement-1".into()),
                revision: Some(2),
            }),
        }],
        reload_required: true,
    });
    toml_rt(&CloneRecord {
        schema: 1,
        id: CloneId::new(),
        rev: 1,
        seat: SeatId::new(),
        name: "c".into(),
        name_history: vec![],
        lifecycle: CloneLifecycle::Active,
        retired: None,
        runtime: minimal_runtime(),
        occupant: None,
        sessions: vec![],
        opt_outs: vec![],
        invitations: vec![],
        reload_required: false,
    });
}

fn full_defaults() -> MemberDefaults {
    MemberDefaults {
        harness: Some(Harness::Claude),
        model: Some("m".into()),
        args: Some(vec!["a".into()]),
        summaries: Some(true),
    }
}

#[test]
fn template_record_roundtrip() {
    let m1 = MemberId::new();
    toml_rt(&TemplateRecord {
        schema: 1,
        id: TemplateId::new(),
        rev: 2,
        name: "t".into(),
        name_history: vec![name_change()],
        defaults: full_defaults(),
        members: vec![
            TemplateMember {
                id: m1.clone(),
                name: "boss".into(),
                role_ref: Some("r".into()),
                role: Some(Role::Dispatcher),
                startup: Startup::Deferred,
                defaults: full_defaults(),
            },
            TemplateMember {
                id: MemberId::new(),
                name: "w".into(),
                role_ref: None,
                role: None,
                startup: Startup::Active,
                defaults: MemberDefaults::default(),
            },
        ],
        relationships: vec![
            Relationship {
                kind: RelationshipKind::ThreadParticipation,
                thread: "a".into(),
                members: MemberSelector::All,
            },
            Relationship {
                kind: RelationshipKind::ThreadParticipation,
                thread: "b".into(),
                members: MemberSelector::Ids(vec![m1]),
            },
        ],
        copied_from: Some(CopiedFrom {
            template: TemplateId::new(),
            at: ts(),
        }),
    });
    toml_rt(&TemplateRecord {
        schema: 1,
        id: TemplateId::new(),
        rev: 1,
        name: "t".into(),
        name_history: vec![],
        defaults: MemberDefaults::default(),
        members: vec![],
        relationships: vec![],
        copied_from: None,
    });
}

#[test]
fn application_record_roundtrip() {
    let mut member_map = BTreeMap::new();
    member_map.insert(MemberId::new(), SeatId::new());
    member_map.insert(MemberId::new(), SeatId::new());
    toml_rt(&ApplicationRecord {
        schema: 1,
        id: AppId::new(),
        rev: 2,
        name: "app".into(),
        template: TemplateId::new(),
        teamspace: TeamspaceId::new(),
        lifecycle: AppLifecycle::Retired,
        retired: Some(retirement()),
        member_map,
        additions: vec![SeatId::new()],
        exclusions: vec![MemberId::new()],
        reused: vec![
            Reuse {
                seat: SeatId::new(),
                from: Some(AppId::new()),
            },
            Reuse {
                seat: SeatId::new(),
                from: None,
            },
        ],
        contributions: Contributions {
            relationships: vec![Relationship {
                kind: RelationshipKind::ThreadParticipation,
                thread: "t".into(),
                members: MemberSelector::All,
            }],
        },
        created_by: CreatedBy {
            op: OpId::new(),
            action: ActionId::new(),
        },
    });
    toml_rt(&ApplicationRecord {
        schema: 1,
        id: AppId::new(),
        rev: 1,
        name: "app".into(),
        template: TemplateId::new(),
        teamspace: TeamspaceId::new(),
        lifecycle: AppLifecycle::Active,
        retired: None,
        member_map: BTreeMap::new(),
        additions: vec![],
        exclusions: vec![],
        reused: vec![],
        contributions: Contributions::default(),
        created_by: CreatedBy {
            op: OpId::new(),
            action: ActionId::new(),
        },
    });
}

#[test]
fn transcript_record_roundtrip() {
    toml_rt(&TranscriptRecord {
        schema: 1,
        id: TranscriptId::new(),
        rev: 5,
        transcript_path: PathBuf::from("/t.jsonl"),
        native_session: NsId::new(),
        seat: SeatId::new(),
        clone: CloneId::new(),
        source_seat_summaries_enabled_at_capture: true,
        coverage: vec![
            ByteRange { start: 0, end: 100 },
            ByteRange {
                start: 150,
                end: 200,
            },
        ],
        gaps: vec![ByteRange {
            start: 100,
            end: 150,
        }],
        unresolved: Some("not found".into()),
    });
    toml_rt(&TranscriptRecord {
        schema: 1,
        id: TranscriptId::new(),
        rev: 1,
        transcript_path: PathBuf::from("/t.jsonl"),
        native_session: NsId::new(),
        seat: SeatId::new(),
        clone: CloneId::new(),
        source_seat_summaries_enabled_at_capture: false,
        coverage: vec![],
        gaps: vec![],
        unresolved: None,
    });
}

#[test]
fn processing_request_roundtrip() {
    toml_rt(&ProcessingRequest {
        schema: 1,
        id: RequestId::new(),
        rev: 3,
        transcript: TranscriptId::new(),
        range: ByteRange { start: 10, end: 20 },
        status: RequestStatus::Completed,
        unresolved: Some("u".into()),
        undeliverable: Some("d".into()),
        created_by_op: OpId::new(),
        delivery: Delivery {
            message_id: Some("m".into()),
            dispatched_at: Some(ts()),
            attempts: vec![
                DeliveryAttempt {
                    op_key: "k".into(),
                    message_id: Some("m".into()),
                    at: ts(),
                    retry: true,
                },
                DeliveryAttempt {
                    op_key: "k2".into(),
                    message_id: None,
                    at: ts(),
                    retry: false,
                },
            ],
            reminded_at: Some(ts()),
        },
        result: Some(RequestResult {
            output_ref: "o.md".into(),
            covered_range: ByteRange { start: 10, end: 20 },
            reported_by: SeatId::new().to_any(),
            at: ts(),
        }),
    });
    toml_rt(&ProcessingRequest {
        schema: 1,
        id: RequestId::new(),
        rev: 1,
        transcript: TranscriptId::new(),
        range: ByteRange { start: 0, end: 0 },
        status: RequestStatus::Pending,
        unresolved: None,
        undeliverable: None,
        created_by_op: OpId::new(),
        delivery: Delivery::default(),
        result: None,
    });
}

#[test]
fn action_record_roundtrip() {
    let mut before = toml::Table::new();
    before.insert("lifecycle".into(), toml::Value::String("active".into()));
    before.insert("rev".into(), toml::Value::Integer(3));
    let mut after = toml::Table::new();
    after.insert("lifecycle".into(), toml::Value::String("retired".into()));
    let mut comp = toml::Table::new();
    comp.insert("restore_rev".into(), toml::Value::Integer(3));
    comp.insert(
        "names".into(),
        toml::Value::Array(vec![toml::Value::String("a".into())]),
    );
    toml_rt(&ActionRecord {
        schema: 1,
        id: ActionId::new(),
        rev: 1,
        kind: ActionKind::ClosureCascade,
        at: ts(),
        ops: vec![OpId::new(), OpId::new()],
        affected: vec![
            AffectedObject {
                object: SeatId::new().to_any(),
                before: Some(toml::Value::Table(before)),
                after: Some(toml::Value::Table(after)),
            },
            AffectedObject {
                object: CloneId::new().to_any(),
                before: None,
                after: None,
            },
        ],
        retired: vec![SeatId::new().to_any()],
        already_retired: vec![CloneId::new().to_any()],
        compensation: comp,
        undoes: Some(ActionId::new()),
        undone_by: vec![OpId::new()],
    });
    toml_rt(&ActionRecord {
        schema: 1,
        id: ActionId::new(),
        rev: 1,
        kind: ActionKind::Undo,
        at: ts(),
        ops: vec![],
        affected: vec![],
        retired: vec![],
        already_retired: vec![],
        compensation: toml::Table::new(),
        undoes: None,
        undone_by: vec![],
    });
}

#[test]
fn operation_record_roundtrip() {
    let states = [
        OpState::Admitted,
        OpState::Applying,
        OpState::Committed,
        OpState::Rejected,
        OpState::Cancelled,
        OpState::Superseded,
        OpState::Failed,
    ];
    for state in states {
        toml_rt(&OperationRecord {
            schema: 1,
            id: OpId::new(),
            rev: 1,
            kind: RequestKind::SeatCreate,
            summary: "s".into(),
            requester: Requester::default(),
            state,
            confirmation: None,
            plan: None,
            commit: None,
            action: None,
            supersedes: None,
            superseded_by: None,
            admitted_at: ts(),
            finished_at: None,
            rejection: None,
        });
    }
    toml_rt(&OperationRecord {
        schema: 1,
        id: OpId::new(),
        rev: 6,
        kind: RequestKind::Undo,
        summary: "undo it".into(),
        requester: Requester {
            teamspace: Some(TeamspaceId::new()),
            seat: Some(SeatId::new()),
            clone: Some(CloneId::new()),
            native_session: Some(NsId::new()),
            human: true,
        },
        state: OpState::Rejected,
        confirmation: Some(Confirmation {
            mode: ConfirmMode::Relay,
            plan_hash: "h".into(),
            at: ts(),
        }),
        plan: Some(PlanId::new()),
        commit: Some(CommitId("abc123".into())),
        action: Some(ActionId::new()),
        supersedes: Some(OpId::new()),
        superseded_by: Some(OpId::new()),
        admitted_at: ts(),
        finished_at: Some(ts()),
        rejection: Some(Rejection {
            reason: "stale".into(),
            explanation: "rev moved".into(),
            current_revs: vec![
                ReliedOn {
                    object: SeatId::new().to_any(),
                    version: Version::Rev(4),
                },
                ReliedOn {
                    object: TemplateId::new().to_any(),
                    version: Version::Blob(BlobHash("beef".into())),
                },
            ],
        }),
    });
}

fn full_effect() -> EffectRecord {
    EffectRecord {
        id: EffectId::new(),
        op: OpId::new(),
        object: SeatId::new().to_any(),
        kind: EffectKind::Custom("my_kind".into()),
        object_rev: 2,
        fencing_rev: 3,
        status: EffectStatus::BlockedNeedsHuman,
        predicted: vec![
            PredictedEnd {
                object: SeatId::new().to_any(),
                container: ContainerKind::Tab,
                end: EndState::Renamed { name: "x".into() },
                induced: true,
            },
            PredictedEnd {
                object: CloneId::new().to_any(),
                container: ContainerKind::Workspace,
                end: EndState::Closed,
                induced: false,
            },
            PredictedEnd {
                object: CloneId::new().to_any(),
                container: ContainerKind::Pane,
                end: EndState::Present,
                induced: false,
            },
        ],
        nonce_label: Some("foreman ·ABCDEF".into()),
        attempts: 2,
        last_error: Some("boom".into()),
        updated_at: ts(),
        sched: EffectSched {
            dispatched: Some(Dispatch {
                attempt: 3,
                at: ts(),
            }),
            retry_at: Some(ts()),
            wake_at: Some(ts()),
            defer_n: 2,
            deps: vec![EffectId::new()],
        },
    }
}

#[test]
fn effect_record_without_sched_deserializes() {
    // A row journaled before the scheduling state moved onto it has no `sched` key.
    let mut v = serde_json::to_value(full_effect()).unwrap();
    assert!(
        v.as_object_mut().unwrap().remove("sched").is_some(),
        "the full row carries a sched key"
    );
    let got: EffectRecord = serde_json::from_value(v).unwrap();
    assert_eq!(got.sched, EffectSched::default());
}

#[test]
fn effect_record_roundtrip() {
    let full = full_effect();
    toml_rt(&full);
    json_rt(&full);
    let min = EffectRecord {
        kind: EffectKind::CreateTab,
        status: EffectStatus::Pending,
        predicted: vec![],
        nonce_label: None,
        last_error: None,
        attempts: 0,
        ..full_effect()
    };
    toml_rt(&min);
    json_rt(&min);
}

#[test]
fn effect_kind_as_str_matches_serde_name() {
    let kinds = [
        EffectKind::CreateWorkspace,
        EffectKind::CreateTab,
        EffectKind::SplitPane,
        EffectKind::StampToken,
        EffectKind::StartAgent,
        EffectKind::RenameWorkspace,
        EffectKind::RenameTab,
        EffectKind::RenamePane,
        EffectKind::CloseWorkspace,
        EffectKind::ClosePane,
        EffectKind::CloseTab,
        EffectKind::ReplaceSession,
        EffectKind::EnsureThread,
        EffectKind::Invite,
        EffectKind::ReleaseRequirement,
        EffectKind::Notify,
        EffectKind::SetTopic,
        EffectKind::DeliverRequest,
        EffectKind::RelaunchOccupant,
    ];
    for k in kinds {
        let json = serde_json::to_string(&k).unwrap();
        assert_eq!(json, format!("\"{}\"", k.as_str()));
    }
    assert_eq!(EffectKind::Custom("zz".into()).as_str(), "zz");
}

#[test]
fn effect_identity_uses_kind_name() {
    let op = OpId::new();
    let obj = SeatId::new().to_any();
    assert_eq!(
        EffectRecord::identity(&op, &obj, &EffectKind::CreateTab, 3),
        EffectId::derive(&op, &obj, "create_tab", 3)
    );
}

#[test]
fn change_request_json_roundtrip() {
    json_rt(&ChangeRequest {
        kind: RequestKind::SeatRename,
        args: serde_json::json!({"name": "x"}),
        relied_on: vec![
            ReliedOn {
                object: SeatId::new().to_any(),
                version: Version::Rev(1),
            },
            ReliedOn {
                object: SeatId::new().to_any(),
                version: Version::Blob(BlobHash("ab".into())),
            },
        ],
        requester: Requester {
            human: true,
            seat: Some(SeatId::new()),
            ..Default::default()
        },
        supersedes: Some(OpId::new()),
        confirmed: None,
    });
}

#[test]
fn change_request_confirmed_round_trips() {
    use crate::model::change::ConfirmedPlan;
    use crate::model::operation::{ConfirmMode, Confirmation};
    let mut req = ChangeRequest {
        kind: RequestKind::SeatRetire,
        args: serde_json::json!({"seat": "x"}),
        relied_on: vec![],
        requester: Requester::default(),
        supersedes: None,
        confirmed: None,
    };
    // Without `confirmed`: the key is absent from the JSON and an old row without it parses as None.
    let old = serde_json::to_value(&req).unwrap();
    assert!(old.get("confirmed").is_none());
    assert_eq!(
        serde_json::from_value::<ChangeRequest>(old)
            .unwrap()
            .confirmed,
        None
    );
    // With `confirmed` (including observed facts).
    let mut observed = serde_json::Map::new();
    observed.insert("adopt_binding".into(), serde_json::json!({"pane_id": "p1"}));
    req.confirmed = Some(ConfirmedPlan {
        plan: PlanId::new(),
        confirmation: Confirmation {
            mode: ConfirmMode::Relay,
            plan_hash: "h".into(),
            at: ts(),
        },
        observed,
    });
    json_rt(&req);
    assert!(
        serde_json::to_value(&req).unwrap()["confirmed"]["observed"]["adopt_binding"].is_object()
    );
}

// ---- contract tests --------------------------------------------------------------------------

#[test]
fn minimal_seat_toml_is_readable() {
    let text = format!(
        "schema = 1\nid = \"{}\"\nrev = 1\nname = \"foreman\"\nteamspace = \"{}\"\nlifecycle = \"dormant\"\n[runtime]\navailability = \"unknown\"\n",
        SeatId::new(),
        TeamspaceId::new()
    );
    let seat: SeatRecord = toml::from_str(&text).unwrap();
    assert_eq!(seat.lifecycle, Lifecycle::Dormant);
    assert_eq!(seat.overrides.summaries, None);
    assert!(!seat.reload_required);
}

#[test]
fn default_summaries_by_role() {
    assert!(default_summaries(None));
    for r in [Role::Summarizer, Role::System, Role::Cron, Role::Dispatcher] {
        assert!(!default_summaries(Some(r)), "{r:?}");
    }
}

#[test]
fn request_kind_categories() {
    for k in [
        RequestKind::SeatCreate,
        RequestKind::Undo,
        RequestKind::TemplateEdit,
    ] {
        assert!(k.requires_confirmation(), "{k:?}");
    }
    for k in [
        RequestKind::Observed,
        RequestKind::Bookkeeping,
        RequestKind::ContentWrite,
    ] {
        assert!(!k.requires_confirmation(), "{k:?}");
    }
}

#[test]
fn byte_range_rejects_inverted() {
    assert!(ByteRange::new(5, 3).is_none());
    assert_eq!(ByteRange::new(3, 5).unwrap().len(), 2);
    assert!(ByteRange::new(4, 4).unwrap().is_empty());
}

#[test]
fn harness_profiles() {
    assert_eq!(
        profile(Harness::Claude).argv(Some("opus"), Some("abc"), &[]),
        ["--resume", "abc", "--model", "opus"]
    );
    assert_eq!(
        profile(Harness::Codex).argv(Some("o3"), Some("x"), &[]),
        ["--no-daemon", "resume", "x", "-m", "o3"]
    );
    assert_eq!(
        profile(Harness::Codex).argv(None, None, &[]),
        ["--no-daemon"]
    );
    assert!(profile(Harness::Shell).agent_kind.is_none());
    assert!(!profile(Harness::Shell).supports_resume());
    let exit = profile(Harness::Claude).exit.unwrap();
    assert_eq!(
        exit.steps,
        &[KeyStep::Key("ctrl+c"), KeyStep::Key("ctrl+c")]
    );
    assert_eq!(exit.if_still_running, Some(KeyStep::SubmitText("/exit")));
}

#[test]
fn claude_slug_and_paths() {
    assert_eq!(
        claude_project_slug(Path::new("/Users/a/my.repo_x")),
        "-Users-a-my-repo-x"
    );
    assert_eq!(
        claude_config_root(None, Path::new("/h")),
        PathBuf::from("/h/.claude")
    );
    assert_eq!(
        claude_config_root(Some(Path::new("/c")), Path::new("/h")),
        PathBuf::from("/c")
    );
    assert_eq!(
        claude_transcript_path(Path::new("/c"), Path::new("/w/p"), "id1"),
        PathBuf::from("/c/projects/-w-p/id1.jsonl")
    );
    assert!(claude_transcript_glob(Path::new("/c"), "id1").ends_with("/projects/*/id1.jsonl"));
}

#[test]
fn session_id_from_codex_argv() {
    let argv: Vec<String> = ["codex", "--no-daemon", "resume", "abc"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(
        session_id_from_argv(&argv, "resume"),
        Some("abc".to_string())
    );
    assert_eq!(session_id_from_argv(&argv[..2], "resume"), None);
}

#[test]
fn launch_env_contract() {
    let seat = SeatId::new();
    let clone = CloneId::new();
    let env = launch_env(Path::new("/inst"), Some(&seat), Some(&clone));
    assert!(env.contains(&(ENV_GRAPH.to_string(), "1".to_string())));
    assert!(env.contains(&("HERDR_GRAPH_INSTANCE".to_string(), "/inst".to_string())));
    assert!(env.contains(&("HERDR_GRAPH_SEAT".to_string(), seat.to_string())));
    assert!(env.contains(&("HERDR_GRAPH_CLONE".to_string(), clone.to_string())));
    let bare = launch_env(Path::new("/inst"), None, None);
    assert_eq!(bare.len(), 2);
    assert!(bare.iter().all(|(k, _)| k != ENV_SEAT && k != ENV_CLONE));
}

#[test]
fn graph_token_roundtrip() {
    let id = SeatId::new();
    assert_eq!(
        parse_graph_token(&graph_token(&id.to_any())),
        Some(id.to_any())
    );
    assert_eq!(parse_graph_token("x=1"), None);
}

#[test]
fn nonce_label_roundtrip() {
    let ef = EffectId::new();
    assert_eq!(
        parse_nonce_label(&nonce_label("foreman", &ef)),
        Some(("foreman", ef.suffix6()))
    );
    assert_eq!(parse_nonce_label("foreman"), None);
}

#[test]
fn cwd_rule_precedence() {
    let seat = SeatId::new();
    let inst = Path::new("/inst");
    let (p, s) = resolve_cwd(Some(Path::new("/o")), Some(Path::new("/r")), inst, &seat);
    assert_eq!((p, s), (PathBuf::from("/o"), CwdSource::SeatOverride));
    let (p, s) = resolve_cwd(None, Some(Path::new("/r")), inst, &seat);
    assert_eq!((p, s), (PathBuf::from("/r"), CwdSource::ProjectRepo));
    let (p, s) = resolve_cwd(None, None, inst, &seat);
    assert_eq!(s, CwdSource::InstanceFallback);
    assert_eq!(p, PathBuf::from(format!("/inst/.graph-local/cwd/{seat}")));
}
