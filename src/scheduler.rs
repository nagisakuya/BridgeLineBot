use super::*;
use chrono::Weekday;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub enum Todo {
    CreateAttendanceCheck {
        hour: i64,
        group_id: String,
    },
    SendAttendanceInfo {
        attendance_id: String,
        group_id: String,
    },
    Test,
}

impl Todo {
    async fn excute(&self) -> Option<Schedule> {
        match self {
            Self::CreateAttendanceCheck { hour, group_id } => {
                let schedule =
                    create_attendance_check(Local::now() + Duration::hours(*hour), group_id).await;
                return Some(schedule);
            }
            Self::Test => {
                println!("called!!!")
            }
            Self::SendAttendanceInfo {
                attendance_id,
                group_id,
            } => {
                let attendance = get_attendance_status(attendance_id).await;
                let attend = attendance.attend.len();
                if attend < 4 {
                    let message = PushMessage {
                        to: group_id.to_string(),
                        messages: vec![Box::new(SimpleMessage::new(
                            "今のところ卓が立たなさそうです！！！やばいです！！！",
                        ))],
                    };
                    message.send().await;
                }
            }
        }
        None
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
                    temp -= 7
                }
                let target_day = *now + Duration::days(temp);
                let target_datetime = NaiveDateTime::new(target_day.date_naive(), *time)
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
    async fn check(
        &mut self,
        last: &DateTime<Local>,
        now: &DateTime<Local>,
    ) -> (bool, Option<Schedule>) {
        if Self::check_schedules(&mut self.exception, last, now).await > 0 {
            return (true, None);
        }
        if self.stype.check(last, now) {
            if let Some(o) = self.todo.excute().await {
                return (true, Some(o));
            }
            return (true, None);
        }
        (false, None)
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
            let (excuted, sch) = item.check(last, now).await;
            let delete_flag = item.stype.delete_check();
            if let Some(o) = sch {
                schedules.push(o);
            }
            if excuted {
                count += 1;
                if delete_flag {
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
        let timestamp: DateTime<Local> = sqlx::query("select * from systemdata")
            .fetch_one(DB.get().unwrap())
            .await
            .unwrap()
            .get::<Option<DateTime<Local>>, _>("timestamp")
            .unwrap_or_else(Local::now);

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
        let sql_result = sqlx::query("update systemdata set timestamp=?")
            .execute(DB.get().unwrap())
            .await;
        if sql_result.is_err() {
            return;
        }
        self.timestamp = now;

        if Schedule::check_schedules(&mut self.schedules, &last, &now).await > 0{
            self.save_shedule("schedule.json").await.unwrap();
        }

    }
    pub async fn push(&mut self, schedule: Schedule) {
        self.schedules.push(schedule)
    }
}

#[tokio::test]
async fn scheduler_gen() {
    let mut scheduler = Scheduler::default();
    let mon = ScheduleType::Weekly {
        weekday: Weekday::Mon,
        time: NaiveTime::from_hms_opt(10, 0, 0).unwrap(),
    };
    let thu = ScheduleType::Weekly {
        weekday: Weekday::Thu,
        time: NaiveTime::from_hms_opt(10, 0, 0).unwrap(),
    };
    scheduler
        .push(Schedule {
            stype: mon,
            todo: Todo::CreateAttendanceCheck { hour: 6, group_id: "Cfa4de6aca6e93eceb803de886e448330".to_string() },
            exception: vec![],
        })
        .await;
    scheduler
        .push(Schedule {
            stype: thu,
            todo: Todo::CreateAttendanceCheck { hour: 6, group_id: "Cfa4de6aca6e93eceb803de886e448330".to_string() },
            exception: vec![],
        })
        .await;
    scheduler.save_shedule("schedule.json").await.unwrap();
}

#[tokio::test]
async fn scheduler_test() {
    let mut scheduler = Scheduler::default();
    let _weekday = ScheduleType::Weekly {
        weekday: Weekday::Mon,
        time: NaiveTime::from_hms_opt(10, 0, 0).unwrap(),
    };
    let _onetime = ScheduleType::OneTime {
        datetime: Local::now() + Duration::seconds(10),
    };
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
