//! Clone pane -> threads seat (spec §7.1): a clone's occupant is invited under the threads seat that
//! herdr-threads resolves for the clone's exact Herdr host and pane.
use super::adapter::{api_error, call_budget, operation_id};
use crate::model::HerdrPaneId;
use crate::ports::threads::{
    OpKey, RetirementKind, SeatStatus, ThreadsError, ThreadsSeatRef, ThreadsSeatView,
};
use herdr_threads::client::local::LocalSocketClient;
use herdr_threads::protocol::commands::{Command, ResolveSeat, SeatInspectQuery};
use herdr_threads::protocol::ids::{HostLocator, HostTargetId, OperationId, SeatId};
use herdr_threads::protocol::pagination::PageRequest;
use herdr_threads::protocol::results::{
    CommandResult, ContinuityStatus, ErrorCode, SeatInspection,
};
use herdr_threads::protocol::time::Clock as ThreadsClock;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Outcome of a seat-lifecycle resolution of one pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeatResolution {
    Seat(ThreadsSeatRef),
    /// Threads refused for now (restore hold, held pane, pending structural reconciliation); retry later
    /// with the same operation key. Never a reason to allocate a fresh seat.
    NotYet(String),
}

#[async_trait::async_trait]
pub trait PaneSeatMap: Send + Sync {
    /// The threads seat currently mapped to `pane`. `Some` does not imply an enrolled agent.
    /// The real map may allocate a bare seat; it never enrolls an agent binding.
    async fn seat_for(&self, pane: &HerdrPaneId) -> Result<Option<ThreadsSeatRef>, ThreadsError>;

    /// Seat-lifecycle resolution of `pane` under the stable operation key `op`: replays with the same key
    /// return the same answer. A threads refusal is `NotYet`, never an error.
    async fn resolve_seat(
        &self,
        pane: &HerdrPaneId,
        op: &OpKey,
    ) -> Result<SeatResolution, ThreadsError>;
}

/// Resolve a graph-managed pane through the host-qualified, restore-guarded public API.
/// This may allocate a bare local seat, but never checks in, accepts invitations or ACKs.
/// A lost response can leave a bare seat; a fresh resolution returns its unique current owner.
pub struct ThreadsSeatMap {
    client: LocalSocketClient,
    clock: Arc<dyn ThreadsClock>,
    timeout: Duration,
    host: HostLocator,
    refusals: SeatRefusals,
}

impl ThreadsSeatMap {
    pub fn new(
        socket: PathBuf,
        instance: herdr_threads::node::NodeUuid,
        clock: Arc<dyn ThreadsClock>,
        host: HostLocator,
    ) -> Self {
        Self {
            client: LocalSocketClient::new(socket, clock.clone(), instance, None),
            clock,
            timeout: Duration::from_secs(5),
            host,
            refusals: SeatRefusals::default(),
        }
    }
}

#[async_trait::async_trait]
impl PaneSeatMap for ThreadsSeatMap {
    async fn seat_for(&self, pane: &HerdrPaneId) -> Result<Option<ThreadsSeatRef>, ThreadsError> {
        let target =
            HostTargetId::parse(pane.0.clone()).map_err(|e| ThreadsError::Rejected(e.into()))?;
        // A new operation is essential: replaying a pane-keyed resolution could return
        // the previous occupant's seat after a rebind. Allocation itself is unique per host/pane.
        let query = ResolveSeat {
            target,
            host: self.host.clone(),
            operation: OperationId::new(format!("graph-resolve-{}", uuid::Uuid::new_v4())),
            locator_hint: None,
            tmux_hint: None,
            continuity_key: None,
        };
        let budget = call_budget(&*self.clock, self.timeout);
        match self
            .client
            .call_async(Command::ResolveSeat(query), &budget)
            .await
        {
            Ok(CommandResult::SeatResolved(seat)) => {
                Ok(Some(ThreadsSeatRef(seat.as_str().to_owned())))
            }
            Ok(_) => Err(ThreadsError::Rejected(
                "unexpected result for host-qualified seat resolution".into(),
            )),
            Err(e) => Err(api_error(e)),
        }
    }

    async fn resolve_seat(
        &self,
        pane: &HerdrPaneId,
        op: &OpKey,
    ) -> Result<SeatResolution, ThreadsError> {
        let target =
            HostTargetId::parse(pane.0.clone()).map_err(|e| ThreadsError::Rejected(e.into()))?;
        let query = ResolveSeat {
            target,
            host: self.host.clone(),
            operation: operation_id(op),
            locator_hint: None,
            tmux_hint: None,
            continuity_key: None,
        };
        let budget = call_budget(&*self.clock, self.timeout);
        match self
            .client
            .call_async(Command::ResolveSeat(query), &budget)
            .await
        {
            Ok(CommandResult::SeatResolved(seat)) => Ok(SeatResolution::Seat(ThreadsSeatRef(
                seat.as_str().to_owned(),
            ))),
            Ok(_) => Err(ThreadsError::Rejected(
                "unexpected result for host-qualified seat resolution".into(),
            )),
            // herdr-threads v3.0.0 refuses a restore hold (explicit or baseline), a target with an unresolved
            // seat mapping and an owner needing structural reconciliation with TargetUnresolved
            // (store/seats/resolution.rs); all of them lift without graph action.
            Err(e) if e.code == ErrorCode::TargetUnresolved => Ok(SeatResolution::NotYet(e.detail)),
            // v3.1.0: the pane's key is bound to an unresolved seat, so no seat is minted for it; it lifts when
            // that seat resolves.
            Err(e) if e.code == ErrorCode::ContinuityKeyPending => {
                Ok(SeatResolution::NotYet(e.detail))
            }
            Err(e) => Err(api_error(e)),
        }
    }
}

/// Map one `SeatInspect` result to the graph's closed seat states. A recorded continuity-key conflict (D1)
/// makes the seat `KeyConflict` unless it is retired: retirement wins, a retired seat is never a conflict.
pub(crate) fn seat_status(i: &SeatInspection) -> SeatStatus {
    let summary = &i.summary;
    if summary.continuity != ContinuityStatus::Retired {
        if let Some(conflict) = i.continuity_key.as_ref().and_then(|k| k.conflict.as_ref()) {
            return SeatStatus::KeyConflict {
                pane: Some(HerdrPaneId(conflict.pane.as_str().to_owned())),
            };
        }
    }
    match summary.continuity {
        ContinuityStatus::Resolved => match &summary.target {
            Some(target) => SeatStatus::Resolved {
                pane: HerdrPaneId(target.as_str().to_owned()),
            },
            None => SeatStatus::Unresolved,
        },
        ContinuityStatus::Unresolved => SeatStatus::Unresolved,
        // A seat homed on another node is never bound here.
        ContinuityStatus::Remote => SeatStatus::Unresolved,
        // Reversible retirement (D2) is not mapped yet, so every retirement is final.
        ContinuityStatus::Retired => SeatStatus::Retired {
            kind: RetirementKind::Final,
        },
    }
}

/// Seats the last `inspect` refused (seat -> message); see `ThreadsSeatMap::refusals`.
pub type SeatRefusals = Arc<Mutex<BTreeMap<ThreadsSeatRef, String>>>;

/// What one seat's inspection call came to: a status, or a per-seat refusal that omits the seat.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SeatRead {
    Status(SeatStatus),
    Refused(String),
}

/// Wire codes that answer for the named seat only: `SeatInspect` returns `NotFound` for an unknown seat
/// (store/queries.rs `seat_inspect`); no other error path of that handler is keyed on the seat. Every other
/// code describes the service (store, auth, version, budget) and fails the whole read.
const SEAT_SCOPED: &[ErrorCode] = &[ErrorCode::NotFound];

/// Classify one seat's call answer. An answer whose code is in `SEAT_SCOPED` refuses that seat only; every
/// other error (service-wide `Rejected` codes, transport errors, `Unsupported`) and an unexpected result shape
/// fail the whole read.
pub(crate) fn classify_inspect(
    answer: Result<CommandResult, herdr_threads::protocol::results::ApiError>,
) -> Result<SeatRead, ThreadsError> {
    match answer {
        Ok(CommandResult::SeatInspect(inspection)) => {
            Ok(SeatRead::Status(seat_status(&inspection)))
        }
        Ok(_) => Err(ThreadsError::Rejected(
            "unexpected result for seat inspection".into(),
        )),
        Err(e) if SEAT_SCOPED.contains(&e.code) => {
            Ok(SeatRead::Refused(format!("{:?}: {}", e.code, e.detail)))
        }
        Err(e) => Err(api_error(e)),
    }
}

impl ThreadsSeatMap {
    /// The seats the last `inspect` omitted as refused, with the message; replaced on every call.
    pub fn refusals(&self) -> SeatRefusals {
        self.refusals.clone()
    }
}

#[async_trait::async_trait]
impl ThreadsSeatView for ThreadsSeatMap {
    /// A seat whose id does not parse client-side, or whose answer is seat-scoped (`SEAT_SCOPED`), is omitted
    /// from the map and kept in `refusals`; the other seats are still read. Any other error fails the whole
    /// read, which `observe_seats` turns into `ThreadsRead::Unavailable`: no binding change and no refusal
    /// clock advanced. `refusals` is left as it was on that path; `track_refusals` ignores it on `Unavailable`.
    async fn inspect(
        &self,
        seats: &[ThreadsSeatRef],
    ) -> Result<BTreeMap<ThreadsSeatRef, SeatStatus>, ThreadsError> {
        let mut out = BTreeMap::new();
        let mut refused = BTreeMap::new();
        for seat in seats {
            let id = match SeatId::parse(seat.0.clone()) {
                Ok(id) => id,
                Err(e) => {
                    refused.insert(seat.clone(), format!("seat id: {e}"));
                    continue;
                }
            };
            let query = SeatInspectQuery {
                seat: id,
                page: PageRequest::default(),
            };
            let budget = call_budget(&*self.clock, self.timeout);
            let answer = self
                .client
                .call_async(Command::SeatInspect(query), &budget)
                .await;
            match classify_inspect(answer)? {
                SeatRead::Status(status) => {
                    out.insert(seat.clone(), status);
                }
                SeatRead::Refused(message) => {
                    refused.insert(seat.clone(), message);
                }
            }
        }
        *self.refusals.lock().unwrap_or_else(|e| e.into_inner()) = refused;
        Ok(out)
    }
}

/// Test map: explicit pane -> seat entries.
#[derive(Default)]
pub struct FakePaneSeatMap {
    seats: Mutex<BTreeMap<String, String>>,
    failing: Mutex<bool>,
    not_yet: Mutex<BTreeMap<String, String>>,
    resolve_ops: Mutex<Vec<OpKey>>,
}

impl FakePaneSeatMap {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn set(&self, pane: &HerdrPaneId, seat: &str) {
        self.seats
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(pane.0.clone(), seat.to_owned());
    }
    pub fn clear(&self, pane: &HerdrPaneId) {
        self.seats
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&pane.0);
    }
    /// `resolve_seat` of `pane` is refused with `reason` until the entry is cleared with `clear_not_yet`.
    pub fn set_not_yet(&self, pane: &HerdrPaneId, reason: &str) {
        self.not_yet
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(pane.0.clone(), reason.to_owned());
    }
    pub fn clear_not_yet(&self, pane: &HerdrPaneId) {
        self.not_yet
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&pane.0);
    }
    /// Every operation key `resolve_seat` was called with, in order.
    pub fn resolve_ops(&self) -> Vec<OpKey> {
        self.resolve_ops
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
    /// While failing, lookups report a disconnected service.
    pub fn set_failing(&self, failing: bool) {
        *self.failing.lock().unwrap_or_else(|e| e.into_inner()) = failing;
    }
}

#[async_trait::async_trait]
impl PaneSeatMap for FakePaneSeatMap {
    async fn seat_for(&self, pane: &HerdrPaneId) -> Result<Option<ThreadsSeatRef>, ThreadsError> {
        if *self.failing.lock().unwrap_or_else(|e| e.into_inner()) {
            return Err(ThreadsError::Disconnected(
                "fake seat map unavailable".into(),
            ));
        }
        Ok(self
            .seats
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&pane.0)
            .cloned()
            .map(ThreadsSeatRef))
    }

    async fn resolve_seat(
        &self,
        pane: &HerdrPaneId,
        op: &OpKey,
    ) -> Result<SeatResolution, ThreadsError> {
        self.resolve_ops
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(op.clone());
        if *self.failing.lock().unwrap_or_else(|e| e.into_inner()) {
            return Err(ThreadsError::Disconnected(
                "fake seat map unavailable".into(),
            ));
        }
        if let Some(reason) = self
            .not_yet
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&pane.0)
        {
            return Ok(SeatResolution::NotYet(reason.clone()));
        }
        match self
            .seats
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&pane.0)
        {
            Some(seat) => Ok(SeatResolution::Seat(ThreadsSeatRef(seat.clone()))),
            None => Ok(SeatResolution::NotYet(format!("no seat for pane {pane}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use herdr_threads::protocol::results::ApiError;

    fn api(code: ErrorCode) -> ApiError {
        ApiError::constructor_for(code)("detail")
    }

    #[test]
    fn per_seat_rejections_refuse_the_seat_and_transport_problems_fail_the_read() {
        let refused = classify_inspect(Err(api(ErrorCode::NotFound))).unwrap();
        assert!(matches!(refused, SeatRead::Refused(m) if m.contains("detail")));
        assert!(matches!(
            classify_inspect(Err(api(ErrorCode::Unsupported))),
            Err(ThreadsError::Unsupported)
        ));
        assert!(matches!(
            classify_inspect(Err(api(ErrorCode::ServiceBusy))),
            Err(ThreadsError::ServiceBusy)
        ));
        assert!(matches!(
            classify_inspect(Err(api(ErrorCode::HostUnavailable))),
            Err(ThreadsError::Disconnected(_))
        ));
        assert!(matches!(
            classify_inspect(Ok(CommandResult::SeatResolved(
                SeatId::parse("seat-a".to_owned()).unwrap()
            ))),
            Err(ThreadsError::Rejected(_))
        ));
    }

    #[test]
    fn service_wide_rejections_fail_the_whole_read() {
        for code in [
            ErrorCode::InstanceMismatch,
            ErrorCode::Unauthorized,
            ErrorCode::CallerUnverified,
            ErrorCode::PermitExpired,
            ErrorCode::StoreBusy,
            ErrorCode::StoreCorrupt,
            ErrorCode::StoreFull,
            ErrorCode::IncompatibleSchema,
            ErrorCode::DaemonVersionMismatch,
            ErrorCode::UnknownWireVersion,
            ErrorCode::InvalidBudget,
            ErrorCode::ReadBudgetExhausted,
            ErrorCode::Conflict,
        ] {
            assert!(
                matches!(classify_inspect(Err(api(code.clone()))), Err(ThreadsError::Rejected(m)) if m.contains("detail")),
                "{code:?} must fail the whole read"
            );
        }
    }

    #[tokio::test]
    async fn an_unparseable_seat_id_is_omitted_not_an_error() {
        let map = ThreadsSeatMap::new(
            PathBuf::from("/nonexistent/threads.sock"),
            herdr_threads::node::NodeUuid::from_bytes([1; 16]),
            Arc::new(herdr_threads::app::SystemClock::new()),
            HostLocator::parse("/host-a.sock").unwrap(),
        );
        let bad = ThreadsSeatRef(String::new());
        let read = map.inspect(std::slice::from_ref(&bad)).await.unwrap();
        assert!(read.is_empty());
        let refusals = map.refusals().lock().unwrap().clone();
        assert!(refusals[&bad].starts_with("seat id:"), "{refusals:?}");
    }
}
