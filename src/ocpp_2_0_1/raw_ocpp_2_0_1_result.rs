use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RawOcpp2_0_1Result(pub u64, pub String, pub Value);
