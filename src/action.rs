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
