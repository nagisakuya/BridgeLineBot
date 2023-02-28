use super::*;
use serde::Serialize;

pub mod message;
pub use message::*;

pub mod profile;
pub use profile::*;

#[derive(Serialize)]
pub struct BloadcastMessage {
    pub messages: Vec<Box<dyn Message>>,
}
impl BloadcastMessage {
    pub async fn send(&self) {
        let _ = send_post_request(
            "https://api.line.me/v2/bot/message/broadcast",
            &serde_json::to_string(self).unwrap(),
        )
        .await;
    }
}

#[tokio::test]
async fn send_bloadcast_test() {
    let message = BloadcastMessage {
        messages: vec![
            //Box::new(SimpleMessage::new("あいうえお")),
            //Box::new(SimpleMessage::new("かきくけこ")),
            Box::new(FlexMessage::new(
                serde_json::from_reader(fs::File::open("vote_flex_message.json").unwrap()).unwrap(),
                "てすと",
            )),
        ],
    };
    message.send().await;
}

#[derive(Serialize)]
pub struct PushMessage {
    pub to: String,
    pub messages: Vec<Box<dyn Message>>,
}
impl PushMessage {
    pub async fn send(&self) {
        println!("{}", serde_json::to_string(self).unwrap());
        let _ = send_post_request(
            "https://api.line.me/v2/bot/message/push",
            &serde_json::to_string(self).unwrap(),
        )
        .await;
    }
}


async fn send_get_request(url: &str) -> Result<reqwest::Response> {
    let client = reqwest::Client::new();
    Ok(client
        .get(url)
        .bearer_auth(SETTINGS.TOKEN.to_string())
        .send()
        .await?)
}

async fn send_post_request(url: &str, body: &str) -> Result<reqwest::Response> {
    let client = reqwest::Client::new();
    Ok(client
        .post(url)
        .header("Content-Type", "application/json")
        .bearer_auth(SETTINGS.TOKEN.to_string())
        .body(body.to_string())
        .send()
        .await?)
}


