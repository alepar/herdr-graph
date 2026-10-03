//! Clone pane -> threads seat (spec §7.1): a clone's occupant is invited under the threads seat that
//! herdr-threads mapped to the clone's Herdr pane (`Seats{target}` is keyed by the pane id).
use super::adapter::{api_error, call_budget};
use crate::model::HerdrPaneId;
use crate::ports::threads::{ThreadsError, ThreadsSeatRef};
use herdr_threads::client::local::LocalSocketClient;
use herdr_threads::protocol::commands::{Command, SeatsQuery};
use herdr_threads::protocol::ids::HostTargetId;
use herdr_threads::protocol::pagination::PageRequest;
use herdr_threads::protocol::results::{CommandResult, ContinuityStatus};
use herdr_threads::protocol::time::Clock as ThreadsClock;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[async_trait::async_trait]
pub trait PaneSeatMap: Send + Sync {
    /// The threads seat currently mapped to `pane`; `None` while the agent in it has not registered with
    /// herdr-threads (a seat appears when the native agent checks in).
    async fn seat_for(&self, pane: &HerdrPaneId) -> Result<Option<ThreadsSeatRef>, ThreadsError>;
}

/// Real map: claimless `Seats{target}` reads through the one-shot local socket client.
pub struct ThreadsSeatMap {
    client: LocalSocketClient,
    clock: Arc<dyn ThreadsClock>,
    timeout: Duration,
}

impl ThreadsSeatMap {
    pub fn new(socket: PathBuf, instance: uuid::Uuid, clock: Arc<dyn ThreadsClock>) -> Self {
        Self {
            client: LocalSocketClient::new(socket, clock.clone(), instance, None),
            clock,
            timeout: Duration::from_secs(5),
        }
    }
}

#[async_trait::async_trait]
impl PaneSeatMap for ThreadsSeatMap {
    async fn seat_for(&self, pane: &HerdrPaneId) -> Result<Option<ThreadsSeatRef>, ThreadsError> {
        let target =
            HostTargetId::parse(pane.0.clone()).map_err(|e| ThreadsError::Rejected(e.into()))?;
        let query = SeatsQuery {
            page: PageRequest::default(),
            target: Some(target),
        };
        let budget = call_budget(&*self.clock, self.timeout);
        match self.client.call_async(Command::Seats(query), &budget).await {
            Ok(CommandResult::Seats(page)) => Ok(page
                .items
                .into_iter()
                .find(|s| s.continuity == ContinuityStatus::Resolved && s.retired_at.is_none())
                .map(|s| ThreadsSeatRef(s.seat.as_str().to_owned()))),
            Ok(_) => Err(ThreadsError::Rejected(
                "unexpected result for a seats query".into(),
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
