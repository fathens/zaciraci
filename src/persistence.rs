use speedb::{DBWithThreadMode, MultiThreaded, DB};
use std::path::PathBuf;

pub use error::Error;
mod error;

type Result<T> = std::result::Result<T, Error>;

fn get_storage_path() -> PathBuf {
    let mut path = std::env::current_dir().unwrap();
    path.push("storage");
    path
}

pub struct Persistence {
    db: DBWithThreadMode<MultiThreaded>,
}

impl Persistence {
    pub fn new() -> Result<Self> {
        let mut options = speedb::Options::default();
        options.create_if_missing(true);

        let path = get_storage_path();
        let db = DB::open(&options, path).map_err(Error::from)?;
        Ok(Persistence { db })
    }

    pub fn write(&self, key: &str, value: &[u8]) -> Result<()> {
        self.db.put(key, value).map_err(Error::from)
    }

    const COUNTER_KEY: &'static str = "counter";

    pub fn get_counter(&self) -> Result<u32> {
        let value = self.db.get(Self::COUNTER_KEY).map_err(Error::from)?;
        let value = match value {
            Some(bs) => u32::from_be_bytes(bs.try_into().unwrap()),
            None => 0,
        };
        Ok(value)
    }

    pub fn increment(&self) -> Result<u32> {
        let cur = self.get_counter()?;
        let next = cur.wrapping_add(1);
        _ = self.write(Self::COUNTER_KEY, &next.to_be_bytes())?;
        Ok(next)
    }
}
