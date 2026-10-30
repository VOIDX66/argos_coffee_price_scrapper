use chrono::{Datelike, FixedOffset, NaiveDate, Utc, Weekday};

use crate::domain::traits::clock::Clock;

/// Colombia Standard Time offset (America/Bogota is UTC-5 year-round).
const COLOMBIA_UTC_OFFSET_SECS: i32 = -5 * 3600;

pub struct SystemClock;

impl SystemClock {
    fn now_colombia() -> chrono::DateTime<FixedOffset> {
        let offset = FixedOffset::east_opt(COLOMBIA_UTC_OFFSET_SECS)
            .or_else(|| FixedOffset::east_opt(0))
            .unwrap_or(FixedOffset::east_opt(0).unwrap_or_else(|| unreachable!()));
        Utc::now().with_timezone(&offset)
    }
}

impl Clock for SystemClock {
    fn today(&self) -> NaiveDate {
        Self::now_colombia().date_naive()
    }

    fn is_market_day(&self) -> bool {
        !matches!(
            Self::now_colombia().weekday(),
            Weekday::Sat | Weekday::Sun
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_clock_uses_colombia_timezone() {
        let clock = SystemClock;
        let today = clock.today();
        let expected_colombia = Utc::now()
            .with_timezone(&FixedOffset::east_opt(-5 * 3600).unwrap())
            .date_naive();
        assert_eq!(today, expected_colombia);
    }
}
