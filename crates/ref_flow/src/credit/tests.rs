use super::*;
use std::sync::Mutex;

fn day(d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, d).unwrap()
}

#[test]
fn take_counts_down_and_stops_at_zero() {
    let mut r = Reservation::new(day(26), 2);
    assert_eq!(r.remaining(), 2);
    let a = r.take(day(26)).unwrap();
    assert_eq!(a.day(), day(26));
    assert!(r.take(day(26)).is_some());
    assert!(r.take(day(26)).is_none(), "exhausted");
    assert_eq!(r.remaining(), 0);
}

#[test]
fn another_day_yields_nothing_even_with_remaining() {
    let mut r = Reservation::new(day(26), 10);
    assert!(
        r.take(day(27)).is_none(),
        "tomorrow must not spend today's block"
    );
    assert!(r.take(day(25)).is_none());
    assert_eq!(r.remaining(), 10, "nothing was taken");
    assert!(r.take(day(26)).is_some());
}

#[test]
fn zero_reservation_never_yields() {
    let mut r = Reservation::new(day(26), 0);
    assert!(r.take(day(26)).is_none());
}

#[test]
fn shared_source_is_a_mutex_over_the_reservation() {
    let source = Mutex::new(Reservation::new(day(26), 1));
    let src: &dyn CreditSource = &source;
    assert!(src.take(day(26)).is_some());
    assert!(src.take(day(26)).is_none());
    *source.lock().unwrap() = Reservation::new(day(26), 3);
    assert!(src.take(day(26)).is_some());
    assert_eq!(source.lock().unwrap().remaining(), 2);
}
