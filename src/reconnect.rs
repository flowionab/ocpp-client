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
/// `multiplier`) after each failed attempt, capped at `max_delay` - but the number of attempts
/// itself is unbounded: a charge point should keep trying to reach its CSMS indefinitely
/// rather than giving up after N tries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReconnectPolicy {
    pub initial_delay: Duration,
    pub max_delay: Duration,
    pub multiplier: u32,
}

impl Default for ReconnectPolicy {
    fn default() -> Self {
        Self {
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            multiplier: 2,
        }
    }
}

impl ReconnectPolicy {
    /// The delay to wait before reconnect attempt number `attempt` (0-indexed: `0` is the
    /// delay before the first retry, right after the initial disconnect).
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
        };
        assert_eq!(policy.delay_for(0), Duration::from_secs(1));
        assert_eq!(policy.delay_for(1), Duration::from_secs(2));
        assert_eq!(policy.delay_for(2), Duration::from_secs(4));
        assert_eq!(policy.delay_for(3), Duration::from_secs(8));
        assert_eq!(policy.delay_for(4), Duration::from_secs(10));
        assert_eq!(policy.delay_for(10), Duration::from_secs(10));
    }
}
