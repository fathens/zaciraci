#![allow(dead_code)]

use crate::ref_finance::history::statistics::Statistics;
use crate::ref_finance::token_account::{TokenInAccount, TokenOutAccount};
use once_cell::sync::Lazy;
use std::sync::{Arc, RwLock};

#[derive(Clone, Debug)]
pub struct History {
    entries: Vec<HistoryEntry>,

    pub inputs: Statistics<u64>,
    pub outputs: Statistics<u64>,
    pub gains: Statistics<u64>,
}

#[derive(Clone, Debug)]
pub struct HistoryEntry {
    logs: Vec<SwapLog>,

    inputs: Statistics<u64>,
    outputs: Statistics<u64>,
    gains: Statistics<u64>,
}

#[derive(Clone, Debug)]
pub struct SwapLog {
    start: TokenInAccount,
    goal: TokenOutAccount,

    input_value: u64,
    output_value: u64,
}

static HISTORY: Lazy<Arc<RwLock<History>>> = Lazy::new(|| {
    Arc::new(RwLock::new(History {
        entries: vec![],

        inputs: Statistics::default(),
        outputs: Statistics::default(),
        gains: Statistics::default(),
    }))
});

pub fn get_history() -> Arc<RwLock<History>> {
    Arc::clone(&*HISTORY)
}

impl HistoryEntry {
    pub fn new(logs: Vec<SwapLog>) -> Self {
        let inputs: Vec<_> = logs.iter().map(|entry| entry.input_value).collect();
        let outputs: Vec<_> = logs.iter().map(|entry| entry.output_value).collect();
        let gains: Vec<_> = logs
            .iter()
            .map(|entry| entry.output_value - entry.input_value)
            .collect();
        let inputs = Statistics::new(&inputs);
        let outputs = Statistics::new(&outputs);
        let gains = Statistics::new(&gains);
        HistoryEntry {
            logs,
            inputs,
            outputs,
            gains,
        }
    }
}

impl History {
    fn new(entries: Vec<HistoryEntry>) -> Self {
        let inputs: Vec<_> = entries
            .iter()
            .map(|entry| (&entry.inputs, entry.logs.len() as u32))
            .collect();
        let outputs: Vec<_> = entries
            .iter()
            .map(|entry| (&entry.outputs, entry.logs.len() as u32))
            .collect();
        let gains: Vec<_> = entries
            .iter()
            .map(|entry| (&entry.gains, entry.logs.len() as u32))
            .collect();
        let inputs = Statistics::gather(&inputs);
        let outputs = Statistics::gather(&outputs);
        let gains = Statistics::gather(&gains);
        History {
            entries,
            inputs,
            outputs,
            gains,
        }
    }

    const LIMIT: usize = 100;

    pub fn add(&mut self, entry: HistoryEntry) {
        let mut entries = self.entries.clone();
        entries.push(entry);
        if entries.len() > Self::LIMIT {
            entries.drain(0..(entries.len() - Self::LIMIT));
        }
        let next = Self::new(entries);
        self.entries = next.entries;
        self.inputs = next.inputs;
        self.outputs = next.outputs;
        self.gains = next.gains;
    }
}

pub mod statistics {
    use num_traits::Zero;
    use std::ops::{Add, Div, Mul};

    #[derive(Debug, Clone, Copy)]
    pub struct Statistics<A> {
        max: A,
        min: A,
        average: A,
    }

    impl<A: Default> Default for Statistics<A> {
        fn default() -> Self {
            Statistics {
                max: A::default(),
                min: A::default(),
                average: A::default(),
            }
        }
    }

    impl<A> Statistics<A>
    where
        A: Zero,
        A: Add<Output = A> + Div<Output = A> + Mul<Output = A>,
        A: Copy,
        A: Ord,
        A: From<u32>,
    {
        pub fn new(values: &[A]) -> Self {
            let mut max = A::zero();
            let mut min = A::zero();
            let mut sum = A::zero();
            for &value in values.iter() {
                max = max.max(value);
                min = min.min(value);
                sum = sum + value;
            }
            let average = if values.is_empty() {
                A::zero()
            } else {
                sum / (values.len() as u32).into()
            };
            Statistics { max, min, average }
        }

        pub fn gather(stats: &[(&Self, u32)]) -> Self {
            let mut max = A::zero();
            let mut min = A::zero();
            let mut sum = A::zero();
            let mut count = A::zero();
            for (stat, n) in stats.iter() {
                max = max.max(stat.max);
                min = min.min(stat.min);
                let c: A = (*n).into();
                sum = sum + stat.average * c;
                count = count + c;
            }
            let average = if count.is_zero() {
                A::zero()
            } else {
                sum / count
            };
            Statistics { max, min, average }
        }
    }
}
