use chrono::{DateTime, NaiveDate, Utc};

pub(crate) fn start_of_day(date: NaiveDate) -> DateTime<Utc> {
    date.and_hms_opt(0, 0, 0).unwrap().and_utc()
}
