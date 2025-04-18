use chrono::NaiveDateTime;

mod plots;

pub struct ValueAtTime {
    value: f64,
    time: NaiveDateTime,
}
