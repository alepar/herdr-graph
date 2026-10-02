//! The real adapter against an in-process stand-in for the threads service socket: framing, registration,
//! busy handling, replay of an unresolved mutation, and the membership mapping. (The real daemon is exercised
//! by the opt-in `tests/threads_real.rs`.)
use super::adapter::ServiceThreads;
use crate::model::clone::{InvitationState, InviteConstraint};
use crate::ports::threads::*;
use herdr_threads::daemon::transport::{read_frame, write_frame};
use herdr_threads::protocol::ids::{InvitationId, RequirementId, SeatId, ServiceAuthorId, ThreadId};
use herdr_threads::protocol::results::{ApiError, ErrorCode};
use herdr_threads::protocol::service::{
    ManagedThread, RequiredMembership, RequirementState, ServiceInvitation, ServiceMembership, ServiceOperation,
    ServiceRegistration, ServiceRequest, ServiceResult, ServiceWireRequest, ServiceWireResponse,
    VoluntaryMembershipState,
};
use herdr_threads::protocol::wire::PROTOCOL_VERSION;
use serde_json::json;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

const BOOT: &str = "123e4567-e89b-12d3-a456-426614174001";

#[derive(Default)]
struct Service {
    busy: AtomicBool,
    /// Close the connection on the next mutation without answering (an unknown outcome).
    drop_next_mutation: AtomicBool,
    registers: AtomicUsize,
    /// `(operation key, request id)` of every mutation frame received.
    mutations: Mutex<Vec<(String, String)>>,
    members: Mutex<Vec<ServiceMembership>>,
}

fn api_error(code: ErrorCode) -> ApiError {
    ApiError { detail: format!("{code:?}"), code, restart_argv: None, required_minimum_bytes: None }
}

fn requirement(thread: &ThreadId, seat: &SeatId, state: RequirementState) -> RequiredMembership {
    RequiredMembership {
        requirement: RequirementId::new("requirement-1"),
        revision: 3,
        invitation: InvitationId::new("inv-1"),
        thread: thread.clone(),
        seat: seat.clone(),
        issuer: ServiceAuthorId::new("graph"),
        state,
        accepted_by: None,
        accepted_at: None,
    }
}

impl Service {
    fn answer(&self, request: &ServiceWireRequest) -> Option<Result<ServiceResult, ApiError>> {
        let author = ServiceAuthorId::new("graph");
        match &request.service {
            ServiceRequest::Register(_) => {
                self.registers.fetch_add(1, Ordering::SeqCst);
                if self.busy.load(Ordering::SeqCst) {
                    return Some(Err(api_error(ErrorCode::ServiceBusy)));
                }
                Some(Ok(ServiceResult::Registered(ServiceRegistration {
                    author,
                    daemon_boot: BOOT.into(),
                    connection_generation: 1,
                })))
            }
            ServiceRequest::Operation(op) => {
                if let Some(key) = op.operation_key() {
                    self.mutations.lock().unwrap().push((key.as_str().to_owned(), request.request_id.clone()));
                    if self.drop_next_mutation.swap(false, Ordering::SeqCst) {
                        return None;
                    }
                }
                Some(Ok(match op {
                    ServiceOperation::EnsureThread(e) => {
                        ServiceResult::ThreadEnsured(ManagedThread { thread: e.thread.clone(), owner: author, archived: false })
                    }
                    ServiceOperation::SetTopic(t) => {
                        ServiceResult::TopicChanged(ManagedThread { thread: t.thread.clone(), owner: author, archived: false })
                    }
                    ServiceOperation::Invite(i) => {
                        let required = i.constraint == herdr_threads::protocol::service::InvitationConstraint::Required;
                        let req = required.then(|| requirement(&i.thread, &i.seat, RequirementState::Pending));
                        self.members.lock().unwrap().push(ServiceMembership {
                            thread: i.thread.clone(),
                            seat: i.seat.clone(),
                            voluntary_state: VoluntaryMembershipState::Invited,
                            requirement: req.clone(),
                        });
                        ServiceResult::Invitation(ServiceInvitation { invitation: InvitationId::new("inv-1"), requirement: req })
                    }
                    ServiceOperation::Notify(_) => serde_json::from_value(json!({
                        "kind": "notification", "data": { "summary": {
                            "message": "notify-1", "thread": "t", "author": null, "kind": "info", "sequence": 1,
                            "created_at": 1, "actor_label": "graph", "preview_data": "x", "preview_omitted": false,
                            "preview_detail_argv": null }, "author": "graph" }
                    }))
                    .unwrap(),
                    ServiceOperation::Membership(q) => {
                        let items: Vec<_> = self
                            .members
                            .lock()
                            .unwrap()
                            .iter()
                            .filter(|m| m.thread == q.thread && q.seat.as_ref().is_none_or(|s| s == &m.seat))
                            .cloned()
                            .collect();
                        serde_json::from_value(json!({
                            "kind": "membership", "data": { "items": items, "next_cursor": null, "next_argv": null,
                                "high_water_ordinal": 0, "scope_revision": null, "has_more": false,
                                "stop_reason": "complete", "consistency": "bounded_live" }
                        }))
                        .unwrap()
                    }
                    ServiceOperation::ReleaseRequirement(r) => {
                        let mut members = self.members.lock().unwrap();
                        let m = members.iter_mut().find(|m| m.thread == r.thread && m.seat == r.seat).unwrap();
                        m.requirement = Some(requirement(&r.thread, &r.seat, RequirementState::Released));
                        ServiceResult::RequirementReleased(m.requirement.clone().unwrap())
                    }
                    ServiceOperation::Archive(_) | ServiceOperation::Reopen(_) => return Some(Err(api_error(ErrorCode::Unsupported))),
                }))
            }
        }
    }
}

async fn serve(listener: tokio::net::UnixListener, svc: Arc<Service>) {
    loop {
        let Ok((mut stream, _)) = listener.accept().await else { return };
        let svc = svc.clone();
        tokio::spawn(async move {
            while let Ok(bytes) = read_frame(&mut stream).await {
                let Ok(request) = serde_json::from_slice::<ServiceWireRequest>(&bytes) else { return };
                let Some(result) = svc.answer(&request) else { return };
                let response = ServiceWireResponse {
                    version: PROTOCOL_VERSION,
                    request_id: request.request_id.clone(),
                    instance: request.expected_instance.clone(),
                    daemon_boot: BOOT.into(),
                    result,
                };
                if write_frame(&mut stream, &serde_json::to_vec(&response).unwrap()).await.is_err() {
                    return;
                }
            }
        });
    }
}

struct Fx {
    _dir: tempfile::TempDir,
    svc: Arc<Service>,
    threads: ServiceThreads,
    intents: PathBuf,
    instance: uuid::Uuid,
}

async fn fx() -> Fx {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("t.sock");
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    let svc = Arc::new(Service::default());
    tokio::spawn(serve(listener, svc.clone()));
    let intents = ServiceThreads::intents_dir_in(dir.path());
    let instance = uuid::Uuid::new_v4();
    let threads = ServiceThreads::with_system_clock(socket, intents.clone(), instance).unwrap();
    Fx { _dir: dir, svc, threads, intents, instance }
}

fn seat(s: &str) -> ThreadsSeatRef {
    ThreadsSeatRef(s.into())
}

#[tokio::test]
async fn ensure_invite_membership_release_round_trip() {
    let fx = fx().await;
    let key = OpKey("ensure:st_01ABC".into());
    let thread = fx.threads.ensure_thread(ChannelScope::Seat, "alpha/foreman", &key).await.unwrap();
    assert_eq!(thread, ThreadRef("hg-st_01abc".into()), "thread id derived from the object id");
    assert!(fx.threads.membership(&thread, &seat("seat-A")).await.unwrap().is_none(), "no invitation yet");

    fx.threads.invite(&thread, &seat("seat-A"), InviteConstraint::Required, &OpKey("invite:1".into())).await.unwrap();
    let d = fx.threads.membership_detail(&thread, &seat("seat-A")).await.unwrap().unwrap();
    assert_eq!(d.state, InvitationState::Pending, "never accepted by anyone");
    assert_eq!(d.invitation.as_deref(), Some("inv-1"));
    assert_eq!(d.requirement.as_deref(), Some("requirement-1"));
    assert_eq!(d.revision, Some(3));
    assert_eq!(fx.threads.membership(&thread, &seat("seat-A")).await.unwrap(), Some(InvitationState::Pending));

    fx.threads.set_topic(&thread, "alpha/boss", &OpKey("topic:1".into())).await.unwrap();
    fx.threads.notify(&thread, Severity::Info, "hello", &OpKey("notify:1".into())).await.unwrap();

    fx.threads.release_requirement(&thread, &seat("seat-A"), &OpKey("release:1".into())).await.unwrap();
    assert_eq!(fx.threads.membership(&thread, &seat("seat-A")).await.unwrap(), Some(InvitationState::Released));
    // Releasing an already-released requirement is clean and sends nothing.
    let before = fx.svc.mutations.lock().unwrap().len();
    fx.threads.release_requirement(&thread, &seat("seat-A"), &OpKey("release:2".into())).await.unwrap();
    fx.threads.release_requirement(&thread, &seat("seat-B"), &OpKey("release:3".into())).await.unwrap();
    assert_eq!(fx.svc.mutations.lock().unwrap().len(), before);

    assert_eq!(fx.svc.registers.load(Ordering::SeqCst), 1, "one registration for the whole session");
    let keys: Vec<String> = fx.svc.mutations.lock().unwrap().iter().map(|m| m.0.clone()).collect();
    assert_eq!(keys.len(), 5);
    assert!(keys.contains(&"ensure:st_01ABC".to_owned()));
    assert!(keys.iter().any(|k| k.starts_with("release:1:requirement-1")), "release key carries the queried requirement: {keys:?}");
    assert!(std::fs::read_dir(&fx.intents).unwrap().next().is_none(), "completed intents are cleaned up");
}

#[tokio::test]
async fn ordinary_membership_maps_voluntary_state() {
    let fx = fx().await;
    let thread = fx.threads.ensure_thread(ChannelScope::Teamspace, "t", &OpKey("ensure:ts_X".into())).await.unwrap();
    fx.threads.invite(&thread, &seat("seat-A"), InviteConstraint::Ordinary, &OpKey("invite:o".into())).await.unwrap();
    let d = fx.threads.membership_detail(&thread, &seat("seat-A")).await.unwrap().unwrap();
    assert_eq!((d.state, d.requirement), (InvitationState::Pending, None));
    for (voluntary, want) in [
        (VoluntaryMembershipState::Joined, Some(InvitationState::Accepted)),
        (VoluntaryMembershipState::Left, Some(InvitationState::Released)),
        (VoluntaryMembershipState::Retired, Some(InvitationState::Retired)),
        (VoluntaryMembershipState::Absent, None),
    ] {
        fx.svc.members.lock().unwrap()[0].voluntary_state = voluntary;
        assert_eq!(fx.threads.membership(&thread, &seat("seat-A")).await.unwrap(), want, "{voluntary:?}");
    }
}

#[tokio::test]
async fn busy_service_is_reported_once_per_call_never_forced() {
    let fx = fx().await;
    fx.svc.busy.store(true, Ordering::SeqCst);
    for n in 1..=3 {
        let r = fx.threads.ensure_thread(ChannelScope::Seat, "t", &OpKey("ensure:st_B".into())).await;
        assert!(matches!(r, Err(ThreadsError::ServiceBusy)), "{r:?}");
        assert_eq!(fx.svc.registers.load(Ordering::SeqCst), n, "exactly one registration attempt per call");
    }
    assert!(fx.svc.mutations.lock().unwrap().is_empty());
    fx.svc.busy.store(false, Ordering::SeqCst);
    fx.threads.ensure_thread(ChannelScope::Seat, "t", &OpKey("ensure:st_B".into())).await.unwrap();
}

#[tokio::test]
async fn no_daemon_is_a_disconnect() {
    let dir = tempfile::tempdir().unwrap();
    let t = ServiceThreads::with_system_clock(dir.path().join("absent.sock"), dir.path().join("i/intents"), uuid::Uuid::new_v4()).unwrap();
    let r = t.ensure_thread(ChannelScope::Seat, "t", &OpKey("ensure:st_X".into())).await;
    assert!(matches!(r, Err(ThreadsError::Disconnected(_))), "{r:?}");
    assert!(matches!(t.membership(&ThreadRef("t".into()), &seat("s")).await, Err(ThreadsError::Disconnected(_))));
}

#[tokio::test]
async fn unresolved_mutation_is_replayed_with_its_original_envelope() {
    let fx = fx().await;
    fx.svc.drop_next_mutation.store(true, Ordering::SeqCst);
    let key = OpKey("ensure:st_R".into());
    let first = fx.threads.ensure_thread(ChannelScope::Seat, "t", &key).await;
    assert!(matches!(first, Err(ThreadsError::Disconnected(_))), "outcome unknown: {first:?}");
    // The next attempt registers again and replays the saved intent instead of building a new one.
    let thread = fx.threads.ensure_thread(ChannelScope::Seat, "t", &key).await.unwrap();
    assert_eq!(thread, ThreadRef("hg-st_r".into()));
    let seen = fx.svc.mutations.lock().unwrap().clone();
    assert_eq!(seen.len(), 2, "{seen:?}");
    assert_eq!(seen[0], seen[1], "same operation key and the same request id: an exact replay");
    assert_eq!(fx.svc.registers.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn unresolved_intents_survive_a_restart_of_the_adapter() {
    let fx = fx().await;
    fx.svc.drop_next_mutation.store(true, Ordering::SeqCst);
    let key = OpKey("ensure:st_P".into());
    assert!(fx.threads.ensure_thread(ChannelScope::Seat, "t", &key).await.is_err());
    // A new adapter over the same intents directory (the daemon restarted) replays it before anything new.
    let socket = fx._dir.path().join("t.sock");
    let again = ServiceThreads::with_system_clock(socket, fx.intents.clone(), fx.instance).unwrap();
    again.ensure_thread(ChannelScope::Seat, "t", &OpKey("ensure:st_Q".into())).await.unwrap();
    let keys: Vec<String> = fx.svc.mutations.lock().unwrap().iter().map(|m| m.0.clone()).collect();
    assert_eq!(keys, vec!["ensure:st_P", "ensure:st_P", "ensure:st_Q"], "the old intent first, then the new operation");
}

#[cfg(feature = "threads-service-ack")]
mod ack {
    use super::*;

    /// A service that answers the v2 probe with `Unsupported`, like every service before the ACK amendment.
    #[tokio::test]
    async fn v2_probe_unsupported_falls_back_and_keeps_the_session() {
        let fx = fx().await;
        // The stand-in's Register ignores the capability and succeeds, i.e. it supports v2.
        assert_eq!(fx.threads.delivery_capability().await.unwrap(), DeliveryCapability::ServiceAck);
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("old.sock");
        let listener = tokio::net::UnixListener::bind(&socket).unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut s, _)) = listener.accept().await else { return };
                tokio::spawn(async move {
                    let Ok(bytes) = read_frame(&mut s).await else { return };
                    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                    let response = json!({
                        "version": PROTOCOL_VERSION, "request_id": v["request_id"], "instance": v["expected_instance"],
                        "daemon_boot": BOOT,
                        "result": { "Err": { "code": "unsupported", "detail": "unsupported service capability",
                                              "restart_argv": null, "required_minimum_bytes": null } }
                    });
                    let _ = write_frame(&mut s, &serde_json::to_vec(&response).unwrap()).await;
                });
            }
        });
        let old = ServiceThreads::with_system_clock(socket, dir.path().join("i/intents"), uuid::Uuid::new_v4()).unwrap();
        assert_eq!(old.delivery_capability().await.unwrap(), DeliveryCapability::NotifyFallback);
        assert_eq!(old.delivery_capability().await.unwrap(), DeliveryCapability::NotifyFallback, "cached");
        let none = ServiceThreads::with_system_clock(dir.path().join("none.sock"), dir.path().join("j/intents"), uuid::Uuid::new_v4()).unwrap();
        assert!(matches!(none.delivery_capability().await, Err(ThreadsError::Disconnected(_))));
    }
}
