use super::*;
use chrono::Weekday;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub enum Todo {
    CreateAttendanceCheck { hour: i64, group_id: String },
    Test,
}

impl Todo {
    async fn excute(&self) {
        match self {
            Self::CreateAttendanceCheck { hour, group_id } => {
                create_attendance_check(Local::now(), Duration::hours(*hour)).await
            }
            Self::Test => {
                println!("called!!!")
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
pub enum ScheduleType {
    OneTime { datetime: DateTime<Local> },
    Weekly { weekday: Weekday, time: NaiveTime },
}
impl ScheduleType {
    fn check(&self, last: &DateTime<Local>, now: &DateTime<Local>) -> bool {
        match self {
            Self::OneTime { datetime } => last < datetime && datetime <= now,
            Self::Weekly { weekday, time } => {
                //get latest date where certain weekday and time
                let mut temp = weekday.num_days_from_monday() as i64
                    - now.weekday().num_days_from_monday() as i64;
                if temp > 0 {
                    temp = temp - 7
                }
                let target_day = now.clone() + Duration::days(temp);
                let target_datetime = NaiveDateTime::new(target_day.date_naive(), time.clone())
                    .and_local_timezone(Local)
                    .unwrap();
                //and compare
                last < &target_datetime && &target_datetime <= now
            }
        }
    }
    fn delete_check(&self) -> bool {
        match self {
            Self::OneTime { datetime: _ } => true,
            Self::Weekly {
                weekday: _,
                time: _,
            } => false,
        }
    }
}

#[test]
fn schedule_type_test() {
    let temp = ScheduleType::Weekly {
        weekday: Weekday::Wed,
        time: NaiveTime::from_hms_opt(12, 0, 0).unwrap(),
    };
    let last = NaiveDateTime::new(
        NaiveDate::from_ymd_opt(2022, 12, 7).unwrap(),
        NaiveTime::from_hms_opt(13, 0, 0).unwrap(),
    )
    .and_local_timezone(Local)
    .unwrap();
    let now = NaiveDateTime::new(
        NaiveDate::from_ymd_opt(2022, 12, 13).unwrap(),
        NaiveTime::from_hms_opt(13, 0, 0).unwrap(),
    )
    .and_local_timezone(Local)
    .unwrap();
    println!("{},{}", last.weekday(), now.weekday());
    let result = temp.check(&last, &now);
    println!("{result}");
}

#[derive(Serialize, Deserialize)]
pub struct Schedule {
    pub stype: ScheduleType,
    pub todo: Todo,
    pub exception: Vec<Schedule>,
}

impl Schedule {
    //returns is it excuted
    #[async_recursion::async_recursion]
    async fn check(&mut self, last: &DateTime<Local>, now: &DateTime<Local>) -> bool {
        if Self::check_schedules(&mut self.exception, last, now).await > 0 {
            return true;
        }
        if self.stype.check(last, now) {
            self.todo.excute().await;
            return true;
        }
        false
    }
    async fn check_schedules(
        schedules: &mut Vec<Schedule>,
        last: &DateTime<Local>,
        now: &DateTime<Local>,
    ) -> u64 {
        let mut count = 0;
        let mut index = 0;
        while index < schedules.len() {
            let item = schedules.get_mut(index).unwrap();
            if item.check(&last, &now).await {
                count += 1;
                if item.stype.delete_check() {
                    schedules.remove(index);
                    continue;
                }
            } 
            index += 1;
        }
        count
    }
}

#[derive(Default)]
pub struct Scheduler {
    schedules: Vec<Schedule>,
    timestamp: DateTime<Local>,
}

impl Scheduler {
    pub async fn from_file(path: &str) -> Self {
        let schedules: Vec<Schedule> =
            serde_json::from_reader(fs::File::open(path).unwrap()).unwrap();
        let pool = sqlx::SqlitePool::connect("database.sqlite").await.unwrap();
        let timestamp: DateTime<Local> = sqlx::query("select * from systemdata")
            .fetch_one(&pool)
            .await
            .unwrap()
            .get::<Option<DateTime<Local>>, _>("timestamp")
            .unwrap_or_else(|| Local::now());

        Scheduler {
            schedules,
            timestamp,
        }
    }
    pub async fn save_shedule(&self, path: &str) -> Result<()> {
        let writer = fs::File::create(path)?;
        serde_json::to_writer(writer, &self.schedules)?;
        Ok(())
    }
    pub async fn check(&mut self) {
        let last = self.timestamp;
        let now = Local::now();
        let pool = sqlx::SqlitePool::connect("database.sqlite").await.unwrap();
        let sql_result = sqlx::query("update systemdata set timestamp=?")
            .execute(&pool)
            .await;
        if sql_result.is_err() {
            return;
        }
        self.timestamp = now;

        Schedule::check_schedules(&mut self.schedules, &last, &now).await;
    }
    pub async fn push(&mut self, schedule: Schedule) {
        self.schedules.push(schedule)
    }
}

#[tokio::test]
async fn scheduler_test() {
    let mut scheduler = Scheduler::from_file("schedule.json").await;
    let _weekday = ScheduleType::Weekly { 
        weekday: Weekday::Sun, time: NaiveTime::from_hms_opt(17,30,45).unwrap() 
    };
    let _onetime = ScheduleType::OneTime { datetime: Local::now() + Duration::seconds(10) };
    scheduler
        .push(Schedule {
            stype: _onetime,
            todo: Todo::Test,
            exception: vec![],
        })
        .await;
    let shedule_check = async {
        loop {
            scheduler.check().await;
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    };
    shedule_check.await;
}

#[tokio::test]
async fn serde_test() {
    let mut scheduler = Scheduler::default();
    let schedule = Schedule {
        stype: ScheduleType::OneTime {
            datetime: Local::now(),
        },
        todo: Todo::CreateAttendanceCheck {
            hour: 7,
            group_id: "group_id".to_string(),
        },
        exception: vec![],
    };
    scheduler.schedules.push(schedule);
    scheduler.save_shedule("schedule.json").await.unwrap();
}
