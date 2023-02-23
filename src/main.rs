use std::net::SocketAddr;
use std::{fs, path::PathBuf};
//use serde_json::Value;
use axum::body::Bytes;
use axum::*;
use axum_server::tls_rustls::*;
use once_cell::sync::Lazy;

use chrono::{prelude::*, Duration};

mod message;
pub use message::*;
use sqlx::Row;

const TOKEN: Lazy<String> =
    Lazy::new(|| fs::read_to_string("token").expect("failed to read token file"));
const TLS_KEY_DIR: Lazy<PathBuf> = Lazy::new(|| {
    PathBuf::from(
        &fs::read_to_string("TLS_KEY_DIR_PATH").expect("failed to read TLS_KEY_DIR_PATH file"),
    )
});

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    //let sqlite = sqlx::sqlite::SqliteConnection::connect("sqlite::memory:").await?;

    let app = Router::new()
        .route("/ping", routing::get(ping))
        .route("/", routing::post(print_request));

    let rustls_config = RustlsConfig::from_pem_file(
        TLS_KEY_DIR.join("fullchain.pem"),
        TLS_KEY_DIR.join("privkey.pem"),
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

async fn print_request(body: Bytes) -> &'static str {
    println!("{}", String::from_utf8(body.to_vec()).unwrap());
    "Ok"
}

async fn create_attendance_check(
    sql_pool: sqlx::Pool<sqlx::Sqlite>,
    time: DateTime<Local>,
    duration: Duration,
) {
    //ランダムid生成
    use rand::Rng;
    let attendance_id = "attendance".to_owned() + &rand::thread_rng().gen::<u64>().to_string();

    //sqlに登録
    sqlx::query("insert into schedule(type,status,sending_schedule,finishing_schedule,data1) values(?,?,?,?,?)")
    .bind("attendance")
    .bind("unsended")
    .bind(time - duration)
    .bind(time)
    .bind(&attendance_id)
    .execute(&sql_pool).await.unwrap();

    //出欠管理用のテーブル作成
    sqlx::query(&format!(
        "create table {attendance_id}(user_id string,status string)"
    ))
    .execute(&sql_pool)
    .await
    .unwrap();

    //(とりあえず)メッセージ送信
    let message = BloadcastMessage {
        messages: vec![Box::new(FlexMessage::new(generate_flex(&attendance_id)))],
    };

    message.send().await;
}

fn generate_flex(id: &str) -> serde_json::Value {
    let mut text = fs::read_to_string("vote_flex_message.json").unwrap();
    text = text.replace("%ID%", id);
    serde_json::from_str(&text).unwrap()
}

#[tokio::test]
async fn test() {
    let pool = sqlx::SqlitePool::connect("database.sqlite").await.unwrap();
    create_attendance_check(pool, Local::now(), Duration::seconds(100)).await;
}

#[tokio::test]
async fn sqlite_test() {
    let pool = sqlx::SqlitePool::connect("database.sqlite").await.unwrap();
    let var = sqlx::query("select * from test")
        .fetch_all(&pool)
        .await
        .unwrap();
    for item in var {
        let id: i32 = item.try_get("id").unwrap();
        let text: String = item.try_get("text").unwrap();
        let datatime: DateTime<Local> = item.try_get("datetime").unwrap();
        println!("{},{},{}", id, text, datatime);
    }
}

#[tokio::test]
async fn sqlite_insert_test() {
    let pool = sqlx::SqlitePool::connect("database.sqlite").await.unwrap();
    let _ = sqlx::query("insert into test(id,text,datetime) values (?,?,?)")
        .bind(123)
        .bind("あいうえお")
        .bind(Local::now())
        .execute(&pool)
        .await
        .unwrap();
}
