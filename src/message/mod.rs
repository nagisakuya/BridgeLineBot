use serde::Serialize;
use erased_serde::serialize_trait_object;
use std::fs::File;

pub trait Message : erased_serde::Serialize + 'static + Send + Sync{
    fn json(&self) -> String;
}
serialize_trait_object!(Message);

#[derive(Serialize)]
pub struct SimpleMessage{
    #[serde(rename = "type")]
    type_:String,
    pub text:String,
}
impl SimpleMessage{
    pub fn new(string:&str) -> Self{
        SimpleMessage{
            type_:"text".to_string(),
            text:string.to_string()
        }
    }
}
impl Message for SimpleMessage{
    fn json(&self) -> String{
        serde_json::to_string(self).unwrap()
    }
}

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct FlexMessage{
    #[serde(rename = "type")]
    type_:String,
    altText:String,
    #[serde(rename = "contents")]
    pub json:serde_json::Value,
}
impl FlexMessage{
    pub fn new(json:serde_json::Value) -> Self{
        FlexMessage{
            type_:"flex".to_string(),
            altText:"altText:flexMessageです".to_string(),
            json:json
        }
    }
}
impl Message for FlexMessage{
    fn json(&self) -> String{
        serde_json::to_string(self).unwrap()
    }
}

trait FlexJson:Into<serde_json::Value>{
}

struct VoteFlexJson{
    json:serde_json::Value
}
impl VoteFlexJson{
    fn read_from_file(path:&str)->Self{
        VoteFlexJson{
            json:serde_json::from_reader(File::open(path).unwrap()).unwrap()
        }
    }
}
impl Into<serde_json::Value> for VoteFlexJson{
    fn into(self) -> serde_json::Value{
        self.json
    }
}
impl FlexJson for VoteFlexJson{

}