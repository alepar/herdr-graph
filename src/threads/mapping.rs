//! Clone pane -> threads seat (spec §7.1): a clone's occupant is invited under the threads seat that
//! herdr-threads resolves for the clone's exact Herdr host and pane.
use super::adapter::{api_error, call_budget};
use crate::model::HerdrPaneId;
use crate::ports::threads::{ThreadsError, ThreadsSeatRef};
use herdr_threads::client::local::LocalSocketClient;
use herdr_threads::protocol::commands::{Command, ResolveSeat};
use herdr_threads::protocol::ids::{HostLocator, HostTargetId, OperationId};
use herdr_threads::protocol::results::CommandResult;
use herdr_threads::protocol::time::Clock as ThreadsClock;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[async_trait::async_trait]
pub trait PaneSeatMap: Send + Sync {
    /// The threads seat currently mapped to `pane`. `Some` does not imply an enrolled agent.
    /// The real map may allocate a bare seat; it never enrolls an agent binding.
    async fn seat_for(&self, pane: &HerdrPaneId) -> Result<Option<ThreadsSeatRef>, ThreadsError>;
}

/// Resolve a graph-managed pane through the host-qualified, restore-guarded public API.
/// This may allocate a bare local seat, but never checks in, accepts invitations or ACKs.
/// A lost response can leave a bare seat; a fresh resolution returns its unique current owner.
pub struct ThreadsSeatMap {
    client: LocalSocketClient,
    clock: Arc<dyn ThreadsClock>,
    timeout: Duration,
    host: HostLocator,
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
}

/// Test map: explicit pane -> seat entries.
#[derive(Default)]
pub struct FakePaneSeatMap {
    seats: Mutex<BTreeMap<String, String>>,
    failing: Mutex<bool>,
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
}
