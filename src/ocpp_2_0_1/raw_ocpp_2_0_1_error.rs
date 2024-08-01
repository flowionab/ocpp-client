use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RawOcpp2_0_1Error(pub u64, pub String, pub String, pub String, pub Value);
