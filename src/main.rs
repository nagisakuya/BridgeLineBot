use tide::prelude::*;
use std::fs::{File,self};
//use serde_json::Value;

use once_cell::sync::Lazy;

mod message;
pub use message::*;

const TOKEN: Lazy<String> =
    Lazy::new(|| fs::read_to_string("token").expect("failed to read token file"));

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    // ログ記録開始
    tide::log::start();
    // インスタンス生成
    let mut app = tide::new();
    // ルーティングとHTTPメソッドを設定し、何をするか決める
    app.at("/").post(|_| async move {
        // Pong型のresをjsonで返します
        Ok(json!(""))
    });
    // webサーバーがどのIPアドレス、ポートで受け付けるか設定する
    let listener = app.listen("127.0.0.1:8080");
    listener.await?;
    // 何もなければOkでタプルを返す
    Ok(())
}

#[derive(Serialize)]
struct BloadcastMessage {
    messages: Vec<Box<dyn Message>>,
}
impl BloadcastMessage {
    async fn send(&self) {
        let resp = send_request(
            "https://api.line.me/v2/bot/message/broadcast",
            &serde_json::to_string(self).unwrap(),
        )
        .await;
        println!("{:?}", resp);
    }
}

async fn send_request(url: &str, body: &str) -> Result<reqwest::Response, reqwest::Error> {
    let client = reqwest::Client::new();
    client
        .post(url)
        .header("Content-Type", "application/json")
        .bearer_auth(TOKEN.to_string())
        .body(body.to_string())
        .send()
        .await
}

#[tokio::test]
async fn test() {
    let message = BloadcastMessage {
        messages: vec![
            //Box::new(SimpleMessage::new("あいうえお")),
            //Box::new(SimpleMessage::new("かきくけこ")),
            Box::new(FlexMessage::new(serde_json::from_reader(File::open("vote_flex_message.json").unwrap()).unwrap())),
        ],
    };
    message.send().await;
}
