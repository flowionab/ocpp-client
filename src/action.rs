use serde::Serialize;
use serde::de::DeserializeOwned;

/// One OCPP action: its wire name plus its request/response types.
///
/// Shared across every OCPP version and open to consumers - implement this for your own
/// marker type to send/register a custom (non-standard) action, or one wrapping vendor
/// fields via `#[serde(flatten)]`, through the exact same `Client::call`/`Client::on` used
/// for the built-in actions.
pub trait Action: Send + Sync + 'static {
    const NAME: &'static str;
    type Request: Serialize + DeserializeOwned + Send + Sync + 'static;
    type Response: Serialize + DeserializeOwned + Send + Sync + 'static;
}

/// One OCPP-J `SEND` (2.1 only) message: a single payload type with no separate
/// response, unlike [`Action`]. The wire frame is `[6, MessageId, NAME, Payload]` and the
/// receiver must never reply to it (see the OCPP-J specification's `SEND` message type) -
/// `Client::send_notification`/`Client::on_notification` are the fire-and-forget counterparts
/// to `Action`'s `call`/`on`.
pub trait SendAction: Send + Sync + 'static {
    const NAME: &'static str;
    type Payload: Serialize + DeserializeOwned + Send + Sync + 'static;
}
