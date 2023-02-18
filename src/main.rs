use std::net::SocketAddr;
use std::{
    fs,
    path::PathBuf,
};
//use serde_json::Value;
use axum::*;
use axum::body::Bytes;
use axum_server::tls_rustls::*;
use once_cell::sync::Lazy;
use serde::Serialize;

mod message;
pub use message::*;

const TOKEN: Lazy<String> =
    Lazy::new(|| fs::read_to_string("token").expect("failed to read token file"));
const TLS_KEY_DIR: Lazy<PathBuf> =
    Lazy::new(|| PathBuf::from(&fs::read_to_string("TLS_KEY_DIR_PATH").expect("failed to read TLS_KEY_DIR_PATH file")));

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    let app = Router::new().route("/ping", routing::get(ping))
    .route("/", routing::post(print_request));

    let rustls_config = RustlsConfig::from_pem_file(
        TLS_KEY_DIR.join("fullchain.pem"), 
        TLS_KEY_DIR.join("privkey.pem")
    )
    .await
    .unwrap();

    let addr = SocketAddr::from(([192, 168, 1, 19], 443));
    axum_server::bind_rustls(addr, rustls_config)
        .serve(app.clone().into_make_service())
        .await
        .unwrap();

    //let addr = SocketAddr::from(([127, 0, 0, 1], 80));
    //axum::Server::bind(&addr)
    //    .serve(app.into_make_service())
    //    .await
    //    .unwrap();

    Ok(())
}

async fn ping() -> &'static str {
    println!("ping!");
    "Hello, World!"
}

async fn print_request(body:Bytes) -> &'static str {
    println!("{}",String::from_utf8(body.to_vec()).unwrap());
    "Ok"
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
            Box::new(FlexMessage::new(
                serde_json::from_reader(fs::File::open("vote_flex_message.json").unwrap()).unwrap(),
            )),
        ],
    };
    message.send().await;
}
