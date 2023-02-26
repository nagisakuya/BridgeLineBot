use axum::body::Bytes;
use axum::extract::Path;
use axum::response::Html;
use axum::*;
use axum_server::tls_rustls::*;
use chrono::{prelude::*, Duration};
use once_cell::sync::Lazy;
use reqwest::StatusCode;
use serde_json::Value;
use sqlx::Row;
use std::net::SocketAddr;
use std::{fs, path::PathBuf};

mod message;
pub use message::*;

mod scheduler;
pub use scheduler::*;

#[allow(non_snake_case)]
#[derive(serde::Deserialize)]
struct Settings {
    TOKEN: String,
    TLS_KEY_DIR_PATH: PathBuf,
    HOST: String,
}

const SETTINGS: Lazy<Settings> =
    Lazy::new(|| toml::from_str(&fs::read_to_string("settings.toml").unwrap()).unwrap());

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[tokio::main]
async fn main() -> Result<()> {
    //let sqlite = sqlx::sqlite::SqliteConnection::connect("sqlite::memory:").await?;

    let app = Router::new()
        .route("/ping", routing::get(ping))
        .route("/test", routing::post(print_request))
        .route("/line/webhook", routing::post(resieve_webhook))
        .route(
            "/line/result/:id",
            routing::get(move |path| result_page(path)),
        );

    let rustls_config = RustlsConfig::from_pem_file(
        SETTINGS.TLS_KEY_DIR_PATH.join("fullchain.pem"),
        SETTINGS.TLS_KEY_DIR_PATH.join("privkey.pem"),
    )
    .await
    .unwrap();

    let addr = SocketAddr::from(([192, 168, 1, 19], 443));
    let excute_https_server =
        axum_server::bind_rustls(addr, rustls_config).serve(app.clone().into_make_service());

    let mut scheduler = Scheduler::from_file("schedule.json").await;
    let shedule_check = async {
        loop {
            scheduler.check().await;
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    };

    let (result, _) = tokio::join!(excute_https_server, shedule_check);
    result?;

    Ok(())
}

async fn ping() -> &'static str {
    println!("ping!");
    "Hello, World!"
}

async fn print_request(body: Bytes) -> StatusCode {
    println!("{}", String::from_utf8(body.to_vec()).unwrap());
    StatusCode::OK
}

async fn resieve_webhook(body: Bytes) {
    let body = match String::from_utf8(body.to_vec()) {
        Ok(x) => x,
        Err(_) => return,
    };
    println!("{}", body);
    let json: Value = match serde_json::from_str(&body) {
        Ok(x) => x,
        Err(_) => return,
    };
    let event = json.get("events").unwrap().get(0).unwrap();
    let event_type = event.get("type").map(|f| f.as_str().unwrap());
    if event_type == Some("postback") {
        insert_attendance(&event).await;
    }
}

async fn insert_attendance(event: &Value) -> Option<()> {
    let data = event.get("postback")?.get("data")?.as_str()?;
    let datas: Vec<_> = data.split(',').collect();
    let attendance_id = datas[0];
    let status = datas[1];
    let user_id = event.get("source")?.get("userId")?.as_str()?;

    let pool = sqlx::SqlitePool::connect("database.sqlite").await.unwrap();
    let result = sqlx::query(&format!("select * from {attendance_id} where user_id=?"))
        .bind(user_id)
        .fetch_one(&pool)
        .await;
    if result.is_ok() {
        let _ = sqlx::query(&format!(
            "update {attendance_id} set status=? where user_id=?"
        ))
        .bind(status)
        .bind(user_id)
        .execute(&pool)
        .await;
    } else {
        let _ = sqlx::query(&format!(
            "insert into {attendance_id}(user_id,status) values(?,?)"
        ))
        .bind(user_id)
        .bind(status)
        .execute(&pool)
        .await;
    }
    Some(())
}

struct Attendance {
    attend: Vec<String>,
    holding: Vec<String>,
    absent: Vec<String>,
}
async fn get_attendance_status(attendance_id: &str) -> Attendance {
    let pool = sqlx::SqlitePool::connect("database.sqlite").await.unwrap();
    let query = &format!("select * from {attendance_id} where status = ?");
    let attend: Vec<String> = sqlx::query_scalar(query)
        .bind("attend")
        .fetch_all(&pool)
        .await
        .unwrap();
    let holding: Vec<String> = sqlx::query_scalar(query)
        .bind("holding")
        .fetch_all(&pool)
        .await
        .unwrap();
    let absent: Vec<String> = sqlx::query_scalar(query)
        .bind("absent")
        .fetch_all(&pool)
        .await
        .unwrap();
    Attendance {
        attend,
        holding,
        absent,
    }
}

async fn result_page(Path(attendance_id): Path<String>) -> Html<String> {
    use html_builder::*;
    use std::fmt::Write;

    let pool = sqlx::SqlitePool::connect("database.sqlite").await.unwrap();
    let Attendance {
        attend,
        holding,
        absent,
    } = get_attendance_status(&attendance_id).await;
    let group_id: String = sqlx::query("select * from schedule where attendance_id = ?")
        .bind(attendance_id)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get("group_id");

    let mut buf = Buffer::new();
    let mut html = buf.html().attr("lang='jp'");
    writeln!(html.title(), "結果").unwrap();
    writeln!(html.h1(), "参加{}人", attend.len()).unwrap();
    writeln!(html.h1(), "保留{}人", holding.len()).unwrap();
    writeln!(html.h1(), "不参加{}人", absent.len()).unwrap();
    let mut table = html.table();
    let mut tr = table.tr();
    writeln!(tr.th(), "参加").unwrap();
    writeln!(tr.th(), "保留").unwrap();
    writeln!(tr.th(), "不参加").unwrap();
    let mut tr = table.tr().attr("border=\"1\"");
    let mut td = tr.td();
    let unknown_user = "unknown_user";
    for user_id in attend {
        writeln!(
            td,
            "{}",
            get_user_profile_from_group(user_id, &group_id)
                .await
                .map_or(unknown_user.to_string(), |f| f.displayName)
        )
        .unwrap();
    }
    let mut td = tr.td();
    for user_id in holding {
        writeln!(
            td,
            "{}",
            get_user_profile_from_group(user_id, &group_id)
                .await
                .map_or(unknown_user.to_string(), |f| f.displayName)
        )
        .unwrap();
    }
    let mut td = tr.td();
    for user_id in absent {
        writeln!(
            td,
            "{}",
            get_user_profile_from_group(user_id, &group_id)
                .await
                .map_or(unknown_user.to_string(), |f| f.displayName)
        )
        .unwrap();
    }
    Html::from(buf.finish())
}

async fn create_attendance_check(finishing_time: DateTime<Local>, group_id: &str) -> Schedule {
    //ランダムid生成
    use rand::Rng;
    let attendance_id = "attendance".to_owned() + &rand::thread_rng().gen::<u64>().to_string();

    //sqlに登録
    let pool = sqlx::SqlitePool::connect("database.sqlite").await.unwrap();
    sqlx::query("insert into schedule(type,status,text,group_id,finishing_schedule,attendance_id) values(?,?,?,?,?,?)")
    .bind("attendance")
    .bind("unsended")
    .bind("出欠確認")
    .bind(group_id)
    .bind(Option::<String>::None)
    .bind(finishing_time)
    .bind(&attendance_id)
    .execute(&pool).await.unwrap();

    //出欠管理用のテーブル作成
    sqlx::query(&format!(
        "create table {attendance_id}(user_id string,status string)"
    ))
    .execute(&pool)
    .await
    .unwrap();

    //メッセージ送信
    let text = format!(
        "{}/{}({})練習会出欠",
        finishing_time.month(),
        finishing_time.day(),
        weekday_to_jp(finishing_time.weekday())
    );
    let message = PushMessage {
        to: group_id.to_string(),
        messages: vec![Box::new(FlexMessage::new(
            generate_flex(&attendance_id, &text),
            &text,
        ))],
    };
    message.send().await;

    Schedule {
        stype: ScheduleType::OneTime {
            datetime: finishing_time,
        },
        todo: Todo::SendAttendanceInfo {
            attendance_id,
            group_id: group_id.to_string(),
        },
        exception: vec![],
    }
}

fn weekday_to_jp(weekday: chrono::Weekday) -> String {
    match weekday {
        Weekday::Sun => "日".to_string(),
        Weekday::Mon => "月".to_string(),
        Weekday::Tue => "火".to_string(),
        Weekday::Wed => "水".to_string(),
        Weekday::Thu => "木".to_string(),
        Weekday::Fri => "金".to_string(),
        Weekday::Sat => "土".to_string(),
    }
}

fn generate_flex(id: &str, description: &str) -> serde_json::Value {
    let mut text = fs::read_to_string("vote_flex_message.json").unwrap();
    text = text.replace("%DESCRIPTION%", description);
    text = text.replace("%ID%", id);
    text = text.replace("%HOST%", &SETTINGS.HOST);
    serde_json::from_str(&text).unwrap()
}

#[tokio::test]
async fn test() {
    create_attendance_check(
        Local::now() + Duration::seconds(30),
        "Cfa4de6aca6e93eceb803de886e448330",
    )
    .await;
}

#[tokio::test]
async fn test2() {
    println!("{}", &"aiueo");
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
