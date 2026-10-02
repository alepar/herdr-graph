//! Port traits: every external boundary sits behind one of these, with a fake (spec §1).
pub mod clock;
pub mod herdr;
pub mod store;
pub mod threads;
pub mod writer;

#[cfg(test)]
mod tests {
    use super::*;
    // Compile-time check: every port is dyn-compatible.
    #[allow(dead_code)]
    fn assert_dyn(
        _: &dyn store::Store,
        _: &dyn writer::Writer,
        _: &dyn herdr::HerdrApi,
        _: &dyn threads::ThreadsPort,
        _: &dyn clock::Clock,
    ) {
    }

    #[test]
    fn manual_clock_advances() {
        let t0 = chrono::DateTime::parse_from_rfc3339("2026-10-02T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let c = clock::ManualClock::new(t0);
        assert_eq!(clock::Clock::now(&c), t0);
        c.advance(chrono::Duration::seconds(90));
        assert_eq!(clock::Clock::now(&c), t0 + chrono::Duration::seconds(90));
    }

    #[test]
    fn repo_path_rejects_escapes() {
        assert!(store::RepoPath::new("teamspaces/a/teamspace.toml").is_ok());
        assert!(store::RepoPath::new("/abs").is_err());
        assert!(store::RepoPath::new("a/../b").is_err());
        assert!(store::RepoPath::new("a\\b").is_err());
    }

    #[test]
    fn herdr_threads_dependency_links() {
        // The path dependency resolves and its public protocol API is reachable.
        assert!(
            std::any::type_name::<herdr_threads::protocol::ids::MessageId>().contains("MessageId")
        );
    }
}
