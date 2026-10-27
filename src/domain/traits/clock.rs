// clock.rs
use chrono::NaiveDate;

pub trait Clock: Send + Sync {
    fn today(&self) -> NaiveDate;
    fn is_market_day(&self) -> bool;
}
