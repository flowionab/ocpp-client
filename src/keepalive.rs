//! Client-initiated WebSocket keepalive: ping the peer on a fixed interval and, when it stops
//! answering, force the connection to be redialled.
//!
//! This lives in the engine rather than in the WebSocket transport because
//! `TransportSink::ping`/`TransportEvent::Pong` are already part of the transport abstraction -
//! so `ocpp-transport-embassy-net`, or any future framed transport, gets keepalive for free
//! without reimplementing the timing or the dead-peer logic.
//!
//! Shaped after `src/reconnect.rs`: a policy struct plus an `Enabled`/`Disabled` behavior enum,
//! so `ConnectOptions` reads the same way for both.
//!
//! The OCPP mapping is `OCPPCommCtrlr.WebSocketPingInterval` in 2.0.1/2.1 and the
//! `WebSocketPingInterval` configuration key in the 1.6 security whitepaper. This crate does not
//! implement a device model; it owns the timer and exposes the value through
//! `Client::ping_interval`/`Client::set_ping_interval` so the layer that does own the device
//! model can report and update it.

use core::time::Duration;

/// How often to ping, how long to wait for each pong, and how many consecutive misses mean the
/// connection is dead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeepalivePolicy {
    /// Delay between pings. `Duration::ZERO` disables pinging, matching how OCPP's
    /// `WebSocketPingInterval` reads a 0 value.
    pub interval: Duration,
    /// How long to wait for each pong before counting it as missed. `None` uses the client's
    /// own request timeout, which is what almost every caller wants - a pong that takes longer
    /// than a CALL is already a broken link.
    pub timeout: Option<Duration>,
    /// Consecutive missed pongs before the connection is treated as dead and redialled. `0` is
    /// treated as `1`. Defaults to `2` so a single dropped frame - or a peer that answers one
    /// ping oddly - doesn't cost a reconnect.
    pub max_missed: u32,
}

impl Default for KeepalivePolicy {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(60),
            timeout: None,
            max_missed: 2,
        }
    }
}

impl KeepalivePolicy {
    /// A policy pinging every `interval`, with default timeout and miss tolerance.
    pub fn every(interval: Duration) -> Self {
        Self {
            interval,
            ..Self::default()
        }
    }

    /// `max_missed`, with `0` normalized to `1` - a policy that tolerated zero misses before
    /// declaring the link dead would still have to act on the first one.
    pub(crate) fn misses_allowed(&self) -> u32 {
        self.max_missed.max(1)
    }
}

/// Whether a client pings its peer on a schedule.
///
/// `ConnectOptions` defaults this to `Enabled(KeepalivePolicy::default())` - a charge point on a
/// NAT'd or mobile link needs keepalive to notice a half-open connection at all, the same
/// reasoning that makes `ReconnectBehavior` default to enabled. The lower-level
/// `Client::from_transport*` constructors default to `Disabled`, since a caller assembling a
/// client from raw transport halves has said nothing about wanting background traffic on it.
///
/// `Disabled` is not permanent: `Client::set_ping_interval` can turn pinging on later, which is
/// what a CSMS writing `WebSocketPingInterval` via `SetVariables` needs to be able to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KeepaliveBehavior {
    Enabled(KeepalivePolicy),
    #[default]
    Disabled,
}

impl KeepaliveBehavior {
    /// The starting interval, or `None` when keepalive is off. A `ZERO` interval collapses to
    /// `None` so "disabled" has one representation inside the client.
    pub(crate) fn initial_interval(&self) -> Option<Duration> {
        match self {
            KeepaliveBehavior::Enabled(policy) if !policy.interval.is_zero() => {
                Some(policy.interval)
            }
            _ => None,
        }
    }

    /// The policy to apply to pings, whether or not pinging starts out enabled - it still
    /// governs pings that `Client::set_ping_interval` turns on later.
    pub(crate) fn policy(&self) -> KeepalivePolicy {
        match self {
            KeepaliveBehavior::Enabled(policy) => *policy,
            KeepaliveBehavior::Disabled => KeepalivePolicy::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_interval_reads_as_disabled() {
        let behavior = KeepaliveBehavior::Enabled(KeepalivePolicy::every(Duration::ZERO));
        assert_eq!(behavior.initial_interval(), None);
    }

    #[test]
    fn disabled_still_carries_a_policy_for_later_enabling() {
        assert_eq!(
            KeepaliveBehavior::Disabled.policy(),
            KeepalivePolicy::default()
        );
        assert_eq!(KeepaliveBehavior::Disabled.initial_interval(), None);
    }

    #[test]
    fn zero_misses_allowed_normalizes_to_one() {
        let policy = KeepalivePolicy {
            max_missed: 0,
            ..KeepalivePolicy::default()
        };
        assert_eq!(policy.misses_allowed(), 1);
    }

    #[test]
    fn default_tolerates_one_miss_before_redialling() {
        assert_eq!(KeepalivePolicy::default().misses_allowed(), 2);
        assert_eq!(KeepalivePolicy::default().interval, Duration::from_secs(60));
    }
}
