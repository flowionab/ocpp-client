use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The OCPP-J wire envelopes. Identical shape across 1.6/2.0.1/2.1, so this is shared by
/// every version instead of duplicated per version like the old `raw_ocpp_<version>_*` types.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct RawCall(pub u64, pub String, pub String, pub Value);

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct RawResult(pub u64, pub String, pub Value);

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct RawError(pub u64, pub String, pub String, pub String, pub Value);

pub(crate) const MESSAGE_TYPE_CALL: u64 = 2;
pub(crate) const MESSAGE_TYPE_RESULT: u64 = 3;
pub(crate) const MESSAGE_TYPE_ERROR: u64 = 4;
