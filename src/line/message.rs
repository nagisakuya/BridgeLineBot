use erased_serde::serialize_trait_object;
use serde::{Deserialize, Serialize};

pub trait Message: erased_serde::Serialize + 'static + Send + Sync {
    fn json(&self) -> String;
}
serialize_trait_object!(Message);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimpleMessage {
    #[serde(rename = "type")]
    type_: String,
    pub text: String,
}
impl SimpleMessage {
    pub fn new(string: &str) -> Self {
        SimpleMessage {
            type_: "text".to_string(),
            text: string.to_string(),
        }
    }
}
impl Message for SimpleMessage {
    fn json(&self) -> String {
        serde_json::to_string(self).unwrap()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_snake_case)]
pub struct FlexMessage {
    #[serde(rename = "type")]
    type_: String,
    altText: String,
    #[serde(rename = "contents")]
    pub json: serde_json::Value,
}
impl FlexMessage {
    pub fn new(json: serde_json::Value, text: &str) -> Self {
        FlexMessage {
            type_: "flex".to_string(),
            altText: text.to_string(),
            json,
        }
    }
}
impl Message for FlexMessage {
    fn json(&self) -> String {
        serde_json::to_string(self).unwrap()
    }
}
