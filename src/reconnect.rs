//! Automatic-reconnect support for `Client`'s background read loop. `Reconnector` mirrors the
//! `Executor`/`Timer` pattern in `src/runtime.rs` - a dyn-safe trait so `Client<E>` stays
//! generic over one type parameter only. `connect_1_6`/`connect_2_0_1`/`connect_2_1` wire up a
//! WebSocket-backed impl automatically; embedded users implement this trait for their own
//! transport to get the same behavior.

use crate::transport::{TransportError, TransportSink, TransportStream};
use alloc::boxed::Box;
use core::future::Future;
use core::pin::Pin;
use core::time::Duration;

/// (Re-)establishes a transport connection from scratch. Called by `Client`'s background read
/// loop after the current transport reports it closed (`TransportStream::recv` returning
/// `Ok(None)` or `Err(_)`).
pub trait Reconnector: Send + Sync + 'static {
    #[allow(clippy::type_complexity)]
    fn connect<'a>(
        &'a self,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<
                        (Box<dyn TransportSink>, Box<dyn TransportStream>),
                        TransportError,
                    >,
                > + Send
                + 'a,
        >,
    >;
}

/// Bounded exponential backoff between reconnect attempts. The delay doubles (by
/// `multiplier`) after each attempt that didn't produce a working connection, capped at
/// `max_delay` - but the number of attempts itself is unbounded: a charge point should keep
/// trying to reach its CSMS indefinitely rather than giving up after N tries.
///
/// The backoff resets as soon as a connection carries any inbound traffic, so an ordinary
/// transient drop costs one `initial_delay` rather than an escalating one. Escalation is
/// reserved for connections that never work - notably a peer that accepts and then immediately
/// closes, which is indistinguishable from a successful dial until nothing arrives on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReconnectPolicy {
    pub initial_delay: Duration,
    pub max_delay: Duration,
    pub multiplier: u32,
    /// Randomize each delay within `[delay / 2, delay]` ("equal jitter"). Defaults to `true`.
    ///
    /// Without it, every charge point that lost the same CSMS retries in lockstep, so the
    /// endpoint coming back gets the whole fleet at once, repeatedly. Jitter spreads that out.
    /// Half the delay is kept un-jittered so a randomly tiny value can't defeat the rate bound
    /// the backoff exists to provide.
    ///
    /// Set `false` when exact retry timing matters more than fleet behavior - a deterministic
    /// test, mostly.
    pub jitter: bool,
}

impl Default for ReconnectPolicy {
    fn default() -> Self {
        Self {
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            multiplier: 2,
            jitter: true,
        }
    }
}

impl ReconnectPolicy {
    /// The un-jittered delay before reconnect attempt number `attempt` (0-indexed: `0` is the
    /// delay before the first retry, right after the disconnect).
    pub(crate) fn delay_for(&self, attempt: u32) -> Duration {
        let mut delay = self.initial_delay;
        for _ in 0..attempt {
            delay = match delay.checked_mul(self.multiplier) {
                Some(d) if d < self.max_delay => d,
                _ => return self.max_delay,
            };
        }
        delay
    }

    /// [`ReconnectPolicy::delay_for`] with equal jitter applied: uniformly distributed over
    /// `[delay / 2, delay]`, or exactly `delay` when `jitter` is off.
    ///
    /// Randomness comes from a throwaway v4 UUID because `uuid` is already a dependency (it
    /// generates OCPP message ids) and its RNG already works on this crate's bare-metal target -
    /// so this needs no additional RNG dependency and no new embedded plumbing. The arithmetic is
    /// integer-only, avoiding a float dependency on no-FPU targets.
    pub(crate) fn jittered_delay_for(&self, attempt: u32) -> Duration {
        let delay = self.delay_for(attempt);
        if !self.jitter {
            return delay;
        }
        let half = delay / 2;
        let fraction = uuid::Uuid::new_v4().as_u128() as u32;
        let extra = (half.as_nanos() * fraction as u128) / u32::MAX as u128;
        half + Duration::from_nanos(extra as u64)
    }
}

/// Whether a `connect_*` call should reconnect automatically on disconnect. Defaults to
/// `Enabled` with `ReconnectPolicy::default()` - production charge points are expected to keep
/// retrying the CSMS connection, so that's the out-of-the-box behavior; set
/// `ConnectOptions::reconnect` to `Disabled` to opt out.
#[derive(Debug, Clone, Copy)]
pub enum ReconnectBehavior {
    Enabled(ReconnectPolicy),
    Disabled,
}

impl Default for ReconnectBehavior {
    fn default() -> Self {
        Self::Enabled(ReconnectPolicy::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delay_doubles_and_caps() {
        let policy = ReconnectPolicy {
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(10),
            multiplier: 2,
            jitter: false,
        };
        assert_eq!(policy.delay_for(0), Duration::from_secs(1));
        assert_eq!(policy.delay_for(1), Duration::from_secs(2));
        assert_eq!(policy.delay_for(2), Duration::from_secs(4));
        assert_eq!(policy.delay_for(3), Duration::from_secs(8));
        assert_eq!(policy.delay_for(4), Duration::from_secs(10));
        assert_eq!(policy.delay_for(10), Duration::from_secs(10));
    }

    #[test]
    fn jitter_off_is_exact() {
        let policy = ReconnectPolicy {
            initial_delay: Duration::from_secs(4),
            jitter: false,
            ..ReconnectPolicy::default()
        };
        for attempt in 0..6 {
            assert_eq!(
                policy.jittered_delay_for(attempt),
                policy.delay_for(attempt)
            );
        }
    }

    #[test]
    fn jitter_stays_within_half_the_delay_and_the_full_delay() {
        let policy = ReconnectPolicy {
            initial_delay: Duration::from_secs(8),
            max_delay: Duration::from_secs(64),
            multiplier: 2,
            jitter: true,
        };

        for attempt in 0..6 {
            let full = policy.delay_for(attempt);
            for _ in 0..200 {
                let jittered = policy.jittered_delay_for(attempt);
                assert!(
                    jittered >= full / 2 && jittered <= full,
                    "attempt {attempt}: {jittered:?} outside [{:?}, {full:?}]",
                    full / 2
                );
            }
        }
    }

    #[test]
    fn jitter_actually_varies() {
        // The floor matters more than the spread, but a constant "jitter" would defeat the
        // point - a fleet would still retry in lockstep.
        let policy = ReconnectPolicy::default();
        let first = policy.jittered_delay_for(3);
        let varies = (0..50).any(|_| policy.jittered_delay_for(3) != first);
        assert!(varies, "jittered delays should not all be identical");
    }

    #[test]
    fn a_zero_delay_survives_jittering() {
        let policy = ReconnectPolicy {
            initial_delay: Duration::ZERO,
            max_delay: Duration::ZERO,
            multiplier: 2,
            jitter: true,
        };
        assert_eq!(policy.jittered_delay_for(0), Duration::ZERO);
    }
}
