#![allow(dead_code)]

use crate::logging::*;
use crate::ref_finance::history::get_history;
use near_primitives::types::Balance;
use near_sdk::NearToken;
use num_traits::Zero;
use std::sync::atomic::{AtomicU64, Ordering};

const DEFAULT_REQUIRED_VALUE: Balance = NearToken::from_near(1).as_yoctonear();
const INTERVAL_OF_PUTBACK: u64 = 24 * 60 * 60;

static LAST_PUTBACK: AtomicU64 = AtomicU64::new(0);

fn is_time_to_putback() -> bool {
    let last = LAST_PUTBACK.load(Ordering::Relaxed);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    now - last > INTERVAL_OF_PUTBACK
}

fn update_last_putback() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    LAST_PUTBACK.store(now, Ordering::Relaxed);
}

pub fn start() {
    let log = DEFAULT.new(o!(
        "function" => "balances.start",
    ));
    let required_value = {
        let max = get_history().read().unwrap().inputs.max();
        if max.is_zero() {
            DEFAULT_REQUIRED_VALUE
        } else {
            max
        }
    };
    info!(log, "Starting balances";
        "required_value" => %required_value,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_time_to_putback() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        LAST_PUTBACK.store(now - INTERVAL_OF_PUTBACK - 1, Ordering::Relaxed);
        assert!(is_time_to_putback());
        LAST_PUTBACK.store(now - INTERVAL_OF_PUTBACK, Ordering::Relaxed);
        assert!(!is_time_to_putback());
        LAST_PUTBACK.store(now - INTERVAL_OF_PUTBACK + 1, Ordering::Relaxed);
        assert!(!is_time_to_putback());
        LAST_PUTBACK.store(now - INTERVAL_OF_PUTBACK + 2, Ordering::Relaxed);
        assert!(!is_time_to_putback());
    }

    #[test]
    fn test_update_last_putback() {
        LAST_PUTBACK.store(0, Ordering::Relaxed);
        update_last_putback();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert_eq!(now, LAST_PUTBACK.load(Ordering::Relaxed));
    }
}
