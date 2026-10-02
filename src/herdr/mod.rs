//! Herdr runtime: NDJSON socket client (HerdrApi impl), FakeHerdr, test isolation guard (spec §4.1, §11).
pub mod client;
pub mod fake;
pub mod incarnation;
pub mod isolation;
pub mod wire;
#[cfg(test)]
mod tests;
pub use client::HerdrClient;
pub use fake::FakeHerdr;
