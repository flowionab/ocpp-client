use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RawOcpp1_6Error(pub u64, pub String, pub String, pub String, pub Value);
