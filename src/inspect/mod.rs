//! Daemon-free, read-only inspection views (spec §2): roster, instruction stack, sessions and the scoped
//! log read committed Git state only. This module holds the shared plumbing the views consume (`Scope`,
//! `Inventory`, the `View` output envelope and the pinned HEAD) and nothing view-specific; each view's
//! builder lives with its own bead.
//!
//! Single-HEAD rule (spec §2.1 "One HEAD per invocation"): a command calls [`pin_head`] once at start and
//! hands that `CommitId` to every consumer. No consumer resolves HEAD itself.
use crate::cli::daemon_error::store_failure;
use crate::model::{CommitId, Timestamp};
use crate::ports::store::{Store, StoreError};
use crate::store::instruction_source::InstructionError;
use std::process::ExitCode;

pub mod inventory;
pub mod log;
pub mod output;
pub mod roster;
pub mod scope;
pub mod sessions;

#[cfg(test)]
pub(crate) mod fixture;

/// The one HEAD of a daemon-free invocation (spec §2.1 "One HEAD per invocation"). Each command calls this
/// once at start and passes the CommitId to Scope, Inventory, seat_parts, the loaded comparison,
/// HistoryWalker and the Envelope. No consumer resolves HEAD itself, so a writer commit landing mid-run
/// cannot tear the view.
pub fn pin_head(store: &dyn Store) -> Result<CommitId, StoreError> {
    store.head()
}

/// `--since <date|dur>`: RFC 3339, `YYYY-MM-DD` (00:00Z), or `<N>{s,m,h,d,w}` before now. Shared by
/// `sessions` and `log` as their clap value parser so neither writes its own.
pub fn parse_since(s: &str) -> Result<Timestamp, String> {
    parse_since_at(s, chrono::Utc::now())
}

pub fn parse_since_at(s: &str, now: Timestamp) -> Result<Timestamp, String> {
    let bad = || {
        format!("invalid time {s:?}: expected RFC 3339, YYYY-MM-DD, or a duration like 90m, 2d, 1w")
    };
    if let Ok(t) = chrono::DateTime::parse_from_rfc3339(s) {
        return Ok(t.with_timezone(&chrono::Utc));
    }
    if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Ok(d.and_hms_opt(0, 0, 0).ok_or_else(bad)?.and_utc());
    }
    let unit = s.chars().last().ok_or_else(bad)?;
    let digits = &s[..s.len() - unit.len_utf8()];
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(bad());
    }
    let n: i64 = digits.parse().map_err(|_| bad())?;
    let dur = match unit {
        's' => chrono::Duration::try_seconds(n),
        'm' => chrono::Duration::try_minutes(n),
        'h' => chrono::Duration::try_hours(n),
        'd' => chrono::Duration::try_days(n),
        'w' => chrono::Duration::try_weeks(n),
        _ => None,
    }
    .ok_or_else(bad)?;
    now.checked_sub_signed(dur).ok_or_else(bad)
}

/// The integrity failure inside `e`, bare or wrapped in `InstructionError::Store`.
fn integrity_of(e: &anyhow::Error) -> Option<crate::cli::daemon_error::Failure> {
    if let Some(s) = e.downcast_ref::<StoreError>() {
        return store_failure(s);
    }
    match e.downcast_ref::<InstructionError>() {
        Some(InstructionError::Store(s)) => store_failure(s),
        _ => None,
    }
}

/// The exit code `fail` returns for `e`: 6 for a graph integrity failure, 1 for everything else.
fn code_for(e: &anyhow::Error) -> u8 {
    integrity_of(e).map_or(1, |f| f.code)
}

/// A whole-command failure (spec §2.1: nothing left to show): prints it and returns the exit code.
/// An integrity `StoreError` — bare, or inside `InstructionError::Store` — is exit 6 through
/// `cli::daemon_error::store_failure`, as `show` and `seat --instructions` map it. Everything else
/// (usage, unknown target, ambiguous name, other store errors) prints `herdr-graph: {e:#}` and is exit 1,
/// the same as `instructions show`.
pub fn fail(e: &anyhow::Error) -> ExitCode {
    match integrity_of(e) {
        Some(f) => eprintln!("{}", f.line),
        None => eprintln!("herdr-graph: {e:#}"),
    }
    ExitCode::from(code_for(e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::store::unresolved;

    fn at(s: &str) -> Timestamp {
        chrono::DateTime::parse_from_rfc3339(s)
            .unwrap()
            .with_timezone(&chrono::Utc)
    }

    #[test]
    fn parse_since_forms() {
        let now = at("2026-11-08T12:00:00Z");
        assert_eq!(
            parse_since_at("2026-11-01T10:30:00+02:00", now).unwrap(),
            at("2026-11-01T08:30:00Z")
        );
        assert_eq!(
            parse_since_at("2026-11-01", now).unwrap(),
            at("2026-11-01T00:00:00Z")
        );
        assert_eq!(
            parse_since_at("90m", now).unwrap(),
            at("2026-11-08T10:30:00Z")
        );
        assert_eq!(
            parse_since_at("2d", now).unwrap(),
            at("2026-11-06T12:00:00Z")
        );
        assert_eq!(
            parse_since_at("1w", now).unwrap(),
            at("2026-11-01T12:00:00Z")
        );
        for bad in [
            "",
            "d",
            "nonsense",
            "2x",
            "-2d",
            "1.5h",
            "2026-13-01",
            "2 d",
        ] {
            assert!(parse_since_at(bad, now).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn fail_maps_integrity_to_6() {
        let integrity = || unresolved("cl_x", "seat", "st_y");
        let bare: anyhow::Error = integrity().into();
        assert_eq!(code_for(&bare), 6);
        let wrapped: anyhow::Error = InstructionError::Store(integrity()).into();
        assert_eq!(code_for(&wrapped), 6);
        let usage: anyhow::Error = InstructionError::Usage("x".into()).into();
        assert_eq!(code_for(&usage), 1);
        let other: anyhow::Error = StoreError::Git("boom".into()).into();
        assert_eq!(code_for(&other), 1);
        assert_eq!(
            format!("{:?}", fail(&bare)),
            format!("{:?}", ExitCode::from(6))
        );
        assert_eq!(
            format!("{:?}", fail(&usage)),
            format!("{:?}", ExitCode::from(1))
        );
    }
}
