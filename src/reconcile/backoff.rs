//! Transient-retry backoff: `min(1 s * 2^n, 5 min)` with +/-20 % jitter (spec §4.4).
use std::time::Duration;

pub const MIN: Duration = Duration::from_secs(1);
pub const MAX: Duration = Duration::from_secs(300);

/// Un-jittered delay after `attempts` failed attempts (the one that just failed included): 1 s, 2 s, 4 s, ... 5 min.
pub fn base(attempts: u32) -> Duration {
    let n = attempts.saturating_sub(1).min(20);
    MIN.saturating_mul(1u32 << n).min(MAX)
}

/// Delay before re-checking an open-ended deferral, after `n` consecutive deferrals (n >= 1):
/// `base * 2^(n-1)`, capped at `max`. No jitter: the loop's own tick bounds it anyway.
pub fn deferred(base: Duration, max: Duration, n: u32) -> Duration {
    let shift = n.saturating_sub(1).min(20);
    base.saturating_mul(1u32 << shift).min(max)
}

/// `base` scaled by a factor in `[0.8, 1.2]` chosen from `random`.
pub fn jittered(base: Duration, random: u128) -> Duration {
    // 0..=4000 maps to -20.00 % ..= +20.00 % in basis points.
    let bp = (random % 4001) as i64 - 2000;
    let millis = base.as_millis() as i64;
    Duration::from_millis((millis + millis * bp / 10_000).max(0) as u64)
}

/// Delay before the next attempt, jitter from a fresh ULID's random part.
pub fn next(attempts: u32) -> Duration {
    jittered(base(attempts), ulid::Ulid::new().random())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_doubles_and_caps() {
        assert_eq!(base(1), Duration::from_secs(1));
        assert_eq!(base(2), Duration::from_secs(2));
        assert_eq!(base(5), Duration::from_secs(16));
        assert_eq!(base(9), Duration::from_secs(256));
        assert_eq!(base(10), MAX);
        assert_eq!(base(500), MAX);
    }

    #[test]
    fn deferred_doubles_from_base_and_caps() {
        let (base, max) = (Duration::from_secs(2), Duration::from_secs(60));
        assert_eq!(deferred(base, max, 1), Duration::from_secs(2));
        assert_eq!(deferred(base, max, 2), Duration::from_secs(4));
        assert_eq!(deferred(base, max, 5), Duration::from_secs(32));
        assert_eq!(deferred(base, max, 6), Duration::from_secs(60));
        assert_eq!(deferred(base, max, 500), Duration::from_secs(60));
    }

    #[test]
    fn jitter_stays_within_twenty_percent() {
        let b = Duration::from_secs(10);
        assert_eq!(jittered(b, 0), Duration::from_millis(8000));
        assert_eq!(jittered(b, 4000), Duration::from_millis(12000));
        for r in 0..9000u128 {
            let d = jittered(b, r);
            assert!(
                d >= Duration::from_millis(8000) && d <= Duration::from_millis(12000),
                "{d:?}"
            );
        }
        let d = next(3);
        assert!(
            d >= Duration::from_millis(3200) && d <= Duration::from_millis(4800),
            "{d:?}"
        );
    }
}
