use crate::ref_finance::history::statistics::Statistics;
use once_cell::sync::Lazy;
use std::sync::{Arc, RwLock};

#[derive(Clone, Debug)]
pub struct History {
    pub inputs: Statistics<u128>,
}

static HISTORY: Lazy<Arc<RwLock<History>>> = Lazy::new(|| {
    Arc::new(RwLock::new(History {
        inputs: Statistics::default(),
    }))
});

pub fn get_history() -> Arc<RwLock<History>> {
    Arc::clone(&*HISTORY)
}

pub mod statistics {
    #[derive(Debug, Clone)]
    pub struct Statistics<A> {
        max: A,
        average: A,
    }

    impl<A: Default> Default for Statistics<A> {
        fn default() -> Self {
            Statistics {
                max: A::default(),
                average: A::default(),
            }
        }
    }

    impl<A: Copy> Statistics<A> {
        pub fn max(&self) -> A {
            self.max
        }

        pub fn average(&self) -> A {
            self.average
        }
    }
}
