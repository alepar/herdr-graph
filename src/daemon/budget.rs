//! One timeout budget for CLI request paths (roast C3). Everything derives from `CALL_TIMEOUT`.
use std::time::Duration;

/// The client's socket read timeout.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(30);
/// Reply transit plus CLI printing.
pub const REPLY_MARGIN: Duration = Duration::from_secs(3);
/// Everything the server may spend on one request, from frame receipt to reply.
pub const SERVER_BUDGET: Duration = CALL_TIMEOUT.saturating_sub(REPLY_MARGIN);
/// The least a handler keeps after a request was held at the start gate.
pub const MIN_HANDLER: Duration = Duration::from_secs(5);
/// How long a request may wait for the daemon to finish starting.
pub const START_HOLD: Duration = SERVER_BUDGET.saturating_sub(MIN_HANDLER);
/// How long `daemon --ensure` waits for a daemon to answer `hello` (it answers as soon as it binds).
pub const STARTUP_WAIT: Duration = CALL_TIMEOUT;

tokio::task_local! {
    static DEADLINE: tokio::time::Instant;
}

/// Run a handler with the request's deadline in scope.
pub async fn within<F: std::future::Future>(deadline: tokio::time::Instant, f: F) -> F::Output {
    DEADLINE.scope(deadline, f).await
}

/// The deadline of the request being served, if any.
pub fn request_deadline() -> Option<tokio::time::Instant> {
    DEADLINE.try_with(|d| *d).ok()
}

/// Wait limit for a handler: the request deadline when serving one, else `now + fallback` (internal callers).
pub fn wait_until(fallback: Duration) -> tokio::time::Instant {
    request_deadline().unwrap_or_else(|| tokio::time::Instant::now() + fallback)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::compose;

    #[test]
    fn budget_is_ordered() {
        assert!(START_HOLD < SERVER_BUDGET);
        assert!(SERVER_BUDGET < CALL_TIMEOUT);
        assert!(MIN_HANDLER > Duration::ZERO);
        assert_eq!(SERVER_BUDGET - START_HOLD, MIN_HANDLER);
        assert!(STARTUP_WAIT >= compose::FIRST_PASS_LIMIT + Duration::from_secs(5));
    }

    #[tokio::test]
    async fn request_deadline_is_scoped() {
        assert!(request_deadline().is_none());
        let d = tokio::time::Instant::now() + Duration::from_secs(7);
        within(d, async { assert_eq!(request_deadline(), Some(d)) }).await;
        assert!(request_deadline().is_none());
    }

    #[tokio::test]
    async fn wait_until_prefers_request_deadline() {
        let now = tokio::time::Instant::now();
        let fallback = wait_until(Duration::from_secs(25));
        assert!(fallback >= now + Duration::from_secs(25));
        let d = now + Duration::from_secs(1);
        within(d, async {
            assert_eq!(wait_until(Duration::from_secs(25)), d)
        })
        .await;
    }
}
