use axum::body::Bytes;
use axum::extract::Path;
use axum::response::Html;
use axum::*;
use axum_server::tls_rustls::*;
use chrono::{prelude::*, Duration};
use once_cell::sync::{Lazy, OnceCell};
use reqwest::StatusCode;
use serde_json::Value;
use sqlx::{Row, Sqlite};
use tokio::sync::Mutex;
use std::net::SocketAddr;
use std::str::FromStr;
use std::{fs, path::PathBuf};

pub mod line;
pub use line::*;

pub mod scheduler;
pub use scheduler::*;

#[allow(non_snake_case)]
#[derive(serde::Deserialize)]
struct Settings {
    TOKEN: String,
    TLS_KEY_DIR_PATH: PathBuf,
    HOST: String,
    LISTENING_ADDRESS: String,
}

static SETTINGS: Lazy<Settings> =
    Lazy::new(|| toml::from_str(&fs::read_to_string("settings.toml").unwrap()).unwrap());

static DB: OnceCell<sqlx::pool::Pool<Sqlite>> = OnceCell::new();
async fn initialize_db() {
    DB.set(sqlx::SqlitePool::connect("database.sqlite").await.unwrap())
        .unwrap();
}

static SCHEDULER: OnceCell<Mutex<Scheduler>> = OnceCell::new();
async fn initialize_scheduler() {
    SCHEDULER
        .set(
            Mutex::new(
            Scheduler::from_file("schedule.json").await
            )
        )
        .unwrap();
}

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[tokio::main]
async fn main() -> Result<()> {
    initialize_db().await;
    initialize_scheduler().await;

    let app = Router::new()
        .route("/ping", routing::get(ping))
        .route("/test", routing::post(print_request))
        .route("/line/webhook", routing::post(resieve_webhook))
        .route("/line/result/:id", routing::get(result_page));

    let rustls_config = RustlsConfig::from_pem_file(
        SETTINGS.TLS_KEY_DIR_PATH.join("fullchain.pem"),
        SETTINGS.TLS_KEY_DIR_PATH.join("privkey.pem"),
    )
    .await
    .unwrap();

    let addr = SocketAddr::from_str(&SETTINGS.LISTENING_ADDRESS).unwrap();
    let excute_https_server =
        axum_server::bind_rustls(addr, rustls_config).serve(app.clone().into_make_service());

    let shedule_check = async {
        loop {
            SCHEDULER.get().unwrap().lock().await.check().await;
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

async fn resieve_webhook(body: Bytes) -> StatusCode {
    let body = match String::from_utf8(body.to_vec()) {
        Ok(x) => x,
        Err(_) => return StatusCode::BAD_REQUEST,
    };
    println!("{}", body);
    let json: Value = match serde_json::from_str(&body) {
        Ok(x) => x,
        Err(_) => return StatusCode::BAD_REQUEST,
    };

    let event = match json.get("events") {
        Some(e) => match e.get(0) {
            Some(e) => e,
            None => return StatusCode::BAD_REQUEST,
        },
        None => return StatusCode::BAD_REQUEST,
    };

    let event_type = event.get("type").map(|f| f.as_str().unwrap_or_default());
    match event_type {
        Some("postback") => {
            insert_attendance(event).await;
        }
        Some("message") => {
            resieve_message(event).await;
        }
        _ => (),
    }

    StatusCode::OK
}

async fn insert_attendance(event: &Value) -> Option<()> {
    let data = event.get("postback")?.get("data")?.as_str()?;
    let datas: Vec<_> = data.split(',').collect();
    let attendance_id = datas[0];
    let status = datas[1];
    let user_id = event.get("source")?.get("userId")?.as_str()?;

    let result = sqlx::query(&format!("select * from {attendance_id} where user_id=?"))
        .bind(user_id)
        .fetch_one(DB.get().unwrap())
        .await;
    if result.is_ok() {
        let _ = sqlx::query(&format!(
            "update {attendance_id} set status=? where user_id=?"
        ))
        .bind(status)
        .bind(user_id)
        .execute(DB.get().unwrap())
        .await;
    } else {
        let _ = sqlx::query(&format!(
            "insert into {attendance_id}(user_id,status) values(?,?)"
        ))
        .bind(user_id)
        .bind(status)
        .execute(DB.get().unwrap())
        .await;
    }
    Some(())
}

async fn resieve_message(event: &Value) -> Option<()> {
    let message: &Value = event.get("message")?;
    if message.get("type")? != "text" {
        return None;
    }
    let reply_token = event.get("replyToken")?.as_str()?;
    let text = message.get("text")?.as_str()?.to_string();
    let lines: Vec<&str> = text.lines().collect();
    if *lines.get(0)? != "休み登録" {
        return None;
    }
    let name = lines.get(1)?;
    let lock = SCHEDULER.get().unwrap();
    let date = match NaiveDate::from_str(lines.get(2)?) {
        Ok(x) => x,
        Err(_) => return None,
    };
    if let ScheduleType::Weekly {
        weekday,
        time,
        ref mut exception,
    } = lock.lock().await.get_mut(name)?.schedule_type
    {
        if weekday != date.weekday() {
            return None;
        }
        let temp = Schedule {
            id: "休み".to_string(),
            schedule_type: ScheduleType::OneTime {
                datetime: NaiveDateTime::new(date, time)
                    .and_local_timezone(Local)
                    .unwrap(),
            },
            todo: Todo::Test,
        };
        exception.push(temp);
    } else {
        return None;
    }
    lock.lock().await.save_shedule("schedule.json").await.unwrap();
    Some(())
}

struct Attendance {
    attend: Vec<String>,
    holding: Vec<String>,
    absent: Vec<String>,
}
async fn get_attendance_status(attendance_id: &str) -> Attendance {
    let query = &format!("select * from {attendance_id} where status = ?");
    let attend: Vec<String> = sqlx::query_scalar(query)
        .bind("attend")
        .fetch_all(DB.get().unwrap())
        .await
        .unwrap();
    let holding: Vec<String> = sqlx::query_scalar(query)
        .bind("holding")
        .fetch_all(DB.get().unwrap())
        .await
        .unwrap();
    let absent: Vec<String> = sqlx::query_scalar(query)
        .bind("absent")
        .fetch_all(DB.get().unwrap())
        .await
        .unwrap();
    Attendance {
        attend,
        holding,
        absent,
    }
}

async fn result_page(Path(attendance_id): Path<String>) -> Html<String> {
    let attendance = get_attendance_status(&attendance_id);
    let attendance_data = sqlx::query("select * from attendances where attendance_id = ?")
        .bind(&attendance_id)
        .fetch_one(DB.get().unwrap());

    let (attendance, attendance_data) = tokio::join!(attendance, attendance_data);

    let Attendance {
        attend,
        holding,
        absent,
    } = attendance;

    let attendance_data = attendance_data.unwrap();

    let group_id: String = attendance_data.get("group_id");

    let title: String = attendance_data.get("description");

    let mut html = fs::read_to_string("result_page.html").unwrap();
    html = html.replace("%TITLE%", &title.to_string());
    html = html.replace("%ATTEND%", &attend.len().to_string());
    html = html.replace("%HOLDING%", &holding.len().to_string());
    html = html.replace("%ABSENT%", &absent.len().to_string());

    async fn ids_to_name(user_ids: &Vec<String>, group_id: &str) -> String {
        let mut buf = String::default();
        for user_id in user_ids {
            buf += &get_user_profile_from_group(user_id, group_id).await.map_or(
                "UNKNOWN_USER".to_string(),
                |profile| {
                    profile.pictureUrl.map_or(String::default(), |url| {
                        format!(r####"<img src="{url}" alt="icon" class="icon">"####)
                    }) + &profile.displayName
                        + "<br>"
                },
            );
        }
        buf
    }

    let attends = ids_to_name(&attend, &group_id);
    let holdings = ids_to_name(&holding, &group_id);
    let absents = ids_to_name(&absent, &group_id);

    let (attends, holdings, absents) = tokio::join!(attends, holdings, absents);
    html = html.replace("%ATTENDS%", &attends);
    html = html.replace("%HOLDINGS%", &holdings);
    html = html.replace("%ABSENTS%", &absents);

    Html::from(html)
}

async fn create_attendance_check(finishing_time: DateTime<Local>, group_id: &str) -> Schedule {
    //ランダムid生成
    use rand::Rng;
    let attendance_id = "attendance".to_owned() + &rand::thread_rng().gen::<u64>().to_string();

    let text = format!(
        "{}/{}({})練習会",
        finishing_time.month(),
        finishing_time.day(),
        weekday_to_jp(finishing_time.weekday())
    );

    //sqlに登録
    sqlx::query("insert into attendances(description,group_id,finishing_schedule,attendance_id) values(?,?,?,?)")
    .bind(&text)
    .bind(group_id)
    .bind(finishing_time)
    .bind(&attendance_id)
    .execute(DB.get().unwrap()).await.unwrap();

    //出欠管理用のテーブル作成
    sqlx::query(&format!(
        "create table {attendance_id}(user_id string,status string)"
    ))
    .execute(DB.get().unwrap())
    .await
    .unwrap();

    //メッセージ送信
    let message = PushMessage {
        to: group_id.to_string(),
        messages: vec![Box::new(FlexMessage::new(
            generate_flex(&attendance_id, &text),
            &text,
        ))],
    };
    message.send().await;

    Schedule {
        id: "".to_string(),
        schedule_type: ScheduleType::OneTime {
            datetime: finishing_time,
        },
        todo: Todo::SendAttendanceInfo {
            attendance_id,
            group_id: group_id.to_string(),
        },
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
async fn send_attendance_check_test() {
    initialize_db().await;
    create_attendance_check(
        Local::now() + Duration::seconds(30),
        "Cfa4de6aca6e93eceb803de886e448330",
    )
    .await;
}

#[tokio::test]
async fn sqlite_test() {
    let var = sqlx::query("select * from test")
        .fetch_all(DB.get().unwrap())
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
