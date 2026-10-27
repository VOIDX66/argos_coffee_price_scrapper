// get_coffee_market_indicator.rs
use anyhow::Result;
use std::sync::Arc;

use crate::domain::entities::coffee_market_indicators::CoffeeMarketIndicators;
use crate::domain::traits::cache_repository::CacheRepository;
use crate::domain::traits::clock::Clock;
use crate::domain::traits::coffee_market_provider::CoffeeMarketProvider;

pub struct GetCoffeeMarketIndicatorsUseCase<P, T>
where
    P: CoffeeMarketProvider,
    T: Clock,
{
    provider: Arc<P>,
    cache: Arc<dyn CacheRepository<CoffeeMarketIndicators>>,
    clock: Arc<T>,
    ttl_seconds: u64,
    refresh_lock: tokio::sync::Mutex<()>,
}

impl<P, T> GetCoffeeMarketIndicatorsUseCase<P, T>
where
    P: CoffeeMarketProvider,
    T: Clock,
{
    pub fn new(
        provider: Arc<P>,
        cache: Arc<dyn CacheRepository<CoffeeMarketIndicators>>,
        clock: Arc<T>,
        ttl_seconds: u64,
    ) -> Self {
        Self {
            provider,
            cache,
            clock,
            ttl_seconds,
            refresh_lock: tokio::sync::Mutex::new(()),
        }
    }

    pub async fn execute(&self) -> Result<CoffeeMarketIndicators> {
        let cache_key = "coffee:market:indicators";
        let today = self.clock.today();

        if let Some(cached) = self.cache.get(cache_key).await? {
            if cached.publication_date == today {
                return Ok(cached);
            }
        }

        let _guard = self.refresh_lock.lock().await;

        let cached = self.cache.get(cache_key).await?;
        if let Some(ref cached) = cached {
            if cached.publication_date == today {
                return Ok(cached.clone());
            }
        }

        let report = match self.provider.get_market_indicators().await {
            Ok(report) => {
                self.cache.set(cache_key, &report, self.ttl_seconds).await?;
                report
            }
            Err(e) => {
                log::warn!("Provider failed: {}. Trying to return cached data.", e);
                if let Some(cached) = cached {
                    cached
                } else {
                    return Err(e);
                }
            }
        };

        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use std::sync::{Arc, Mutex};

    struct MockProvider {
        pub response: CoffeeMarketIndicators,
        pub called: Arc<Mutex<bool>>,
    }

    #[async_trait::async_trait]
    impl CoffeeMarketProvider for MockProvider {
        async fn get_market_indicators(&self) -> Result<CoffeeMarketIndicators> {
            *self.called.lock().unwrap() = true;
            Ok(self.response.clone())
        }
    }

    struct MockCache {
        pub stored: Arc<Mutex<Option<CoffeeMarketIndicators>>>,
    }

    #[async_trait::async_trait]
    impl CacheRepository<CoffeeMarketIndicators> for MockCache {
        async fn get(&self, _key: &str) -> Result<Option<CoffeeMarketIndicators>> {
            Ok(self.stored.lock().unwrap().clone())
        }

        async fn set(&self, _key: &str, value: &CoffeeMarketIndicators, _ttl: u64) -> Result<()> {
            *self.stored.lock().unwrap() = Some(value.clone());
            Ok(())
        }
    }

    struct FixedClock {
        pub date: NaiveDate,
    }

    impl Clock for FixedClock {
        fn today(&self) -> NaiveDate {
            self.date
        }
        fn is_market_day(&self) -> bool {
            true
        }
    }

    struct FailingProvider;

    #[async_trait::async_trait]
    impl CoffeeMarketProvider for FailingProvider {
        async fn get_market_indicators(&self) -> Result<CoffeeMarketIndicators> {
            Err(anyhow::anyhow!("provider error"))
        }
    }

    struct FailingGetCache;

    #[async_trait::async_trait]
    impl CacheRepository<CoffeeMarketIndicators> for FailingGetCache {
        async fn get(&self, _key: &str) -> Result<Option<CoffeeMarketIndicators>> {
            Err(anyhow::anyhow!("cache get error"))
        }

        async fn set(&self, _key: &str, _value: &CoffeeMarketIndicators, _ttl: u64) -> Result<()> {
            Ok(())
        }
    }

    struct FailingSetCache {
        pub stored: Arc<Mutex<Option<CoffeeMarketIndicators>>>,
    }

    #[async_trait::async_trait]
    impl CacheRepository<CoffeeMarketIndicators> for FailingSetCache {
        async fn get(&self, _key: &str) -> Result<Option<CoffeeMarketIndicators>> {
            Ok(self.stored.lock().unwrap().clone())
        }

        async fn set(&self, _key: &str, _value: &CoffeeMarketIndicators, _ttl: u64) -> Result<()> {
            Err(anyhow::anyhow!("cache set error"))
        }
    }

    #[tokio::test]
    async fn should_call_provider_when_cache_is_empty() {
        let today = NaiveDate::from_ymd_opt(2025, 1, 10).unwrap();
        let provider_called = Arc::new(Mutex::new(false));

        let provider = Arc::new(MockProvider {
            response: CoffeeMarketIndicators::new(
                today,
                1_500_000.0,
                120_000.0,
                2.15,
                3900.0,
                1_480_000.0,
            ),
            called: provider_called.clone(),
        });

        let cache = Arc::new(MockCache {
            stored: Arc::new(Mutex::new(None)),
        });

        let clock = Arc::new(FixedClock { date: today });

        let use_case = GetCoffeeMarketIndicatorsUseCase::new(provider, cache, clock, 3600);

        let result = use_case.execute().await.unwrap();

        assert_eq!(result.publication_date, today);
        assert_eq!(*provider_called.lock().unwrap(), true);
    }

    #[tokio::test]
    async fn should_return_cached_value_when_publication_date_is_today() {
        let today = NaiveDate::from_ymd_opt(2025, 1, 10).unwrap();
        let provider_called = Arc::new(Mutex::new(false));

        let provider = Arc::new(MockProvider {
            response: CoffeeMarketIndicators::new(
                today,
                9_999_999.0,
                9_999_999.0,
                9.99,
                9999.0,
                9_999_999.0,
            ),
            called: provider_called.clone(),
        });

        let cached_value =
            CoffeeMarketIndicators::new(today, 1_500_000.0, 120_000.0, 2.15, 3900.0, 1_480_000.0);

        let cache = Arc::new(MockCache {
            stored: Arc::new(Mutex::new(Some(cached_value.clone()))),
        });

        let clock = Arc::new(FixedClock { date: today });
        let use_case = GetCoffeeMarketIndicatorsUseCase::new(provider, cache, clock, 3600);

        let result = use_case.execute().await.unwrap();

        assert_eq!(result.internal_price_cop, 1_500_000.0);
        assert_eq!(*provider_called.lock().unwrap(), false);
    }

    #[tokio::test]
    async fn should_refresh_when_cached_publication_date_is_old() {
        let today = NaiveDate::from_ymd_opt(2025, 1, 10).unwrap();
        let yesterday = NaiveDate::from_ymd_opt(2025, 1, 9).unwrap();

        let provider_called = Arc::new(Mutex::new(false));

        let new_report =
            CoffeeMarketIndicators::new(today, 1_700_000.0, 130_000.0, 2.30, 4000.0, 1_650_000.0);

        let provider = Arc::new(MockProvider {
            response: new_report.clone(),
            called: provider_called.clone(),
        });

        let old_cached_value = CoffeeMarketIndicators::new(
            yesterday,
            1_500_000.0,
            120_000.0,
            2.10,
            3900.0,
            1_480_000.0,
        );

        let cache_storage = Arc::new(Mutex::new(Some(old_cached_value)));

        let cache = Arc::new(MockCache {
            stored: cache_storage.clone(),
        });

        let clock = Arc::new(FixedClock { date: today });
        let use_case = GetCoffeeMarketIndicatorsUseCase::new(provider, cache, clock, 3600);

        let result = use_case.execute().await.unwrap();

        assert_eq!(*provider_called.lock().unwrap(), true);
        assert_eq!(result.publication_date, today);
        assert_eq!(result.internal_price_cop, 1_700_000.0);

        let cached_after = cache_storage.lock().unwrap().clone().unwrap();
        assert_eq!(cached_after.publication_date, today);
    }

    #[tokio::test]
    async fn should_refresh_on_weekend_when_cached_publication_date_is_old() {
        struct WeekendClock {
            pub date: NaiveDate,
        }
        impl Clock for WeekendClock {
            fn today(&self) -> NaiveDate {
                self.date
            }
            fn is_market_day(&self) -> bool {
                false
            }
        }

        let saturday = NaiveDate::from_ymd_opt(2025, 1, 11).unwrap();
        let friday = NaiveDate::from_ymd_opt(2025, 1, 10).unwrap();
        let provider_called = Arc::new(Mutex::new(false));

        let new_report = CoffeeMarketIndicators::new(
            saturday,
            2_100_000.0,
            0.0,
            300.95,
            3130.0,
            0.0,
        );

        let provider = Arc::new(MockProvider {
            response: new_report,
            called: provider_called.clone(),
        });

        let old_cached_value = CoffeeMarketIndicators::new(
            friday,
            2_050_000.0,
            0.0,
            298.00,
            3120.0,
            0.0,
        );

        let cache = Arc::new(MockCache {
            stored: Arc::new(Mutex::new(Some(old_cached_value))),
        });

        let clock = Arc::new(WeekendClock { date: saturday });
        let use_case = GetCoffeeMarketIndicatorsUseCase::new(provider, cache, clock, 3600);

        let result = use_case.execute().await.unwrap();
        assert!(*provider_called.lock().unwrap());
        assert_eq!(result.publication_date, saturday);
        assert_eq!(result.internal_price_cop, 2_100_000.0);
    }

    #[tokio::test]
    async fn should_return_cached_data_when_provider_fails() {
        let today = NaiveDate::from_ymd_opt(2025, 1, 10).unwrap();

        let provider = Arc::new(FailingProvider);

        let cached_value =
            CoffeeMarketIndicators::new(today, 1_500_000.0, 120_000.0, 2.15, 3900.0, 1_480_000.0);

        let cache = Arc::new(MockCache {
            stored: Arc::new(Mutex::new(Some(cached_value.clone()))),
        });

        let clock = Arc::new(FixedClock { date: today });
        let use_case = GetCoffeeMarketIndicatorsUseCase::new(provider, cache, clock, 3600);

        let result = use_case.execute().await.unwrap();

        assert_eq!(result.publication_date, today);
    }

    #[tokio::test]
    async fn should_propagate_error_when_provider_fails_and_no_cache() {
        let today = NaiveDate::from_ymd_opt(2025, 1, 10).unwrap();

        let provider = Arc::new(FailingProvider);
        let cache = Arc::new(MockCache {
            stored: Arc::new(Mutex::new(None)),
        });
        let clock = Arc::new(FixedClock { date: today });

        let use_case = GetCoffeeMarketIndicatorsUseCase::new(provider, cache, clock, 3600);

        let result = use_case.execute().await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn should_propagate_error_when_cache_get_fails() {
        let today = NaiveDate::from_ymd_opt(2025, 1, 10).unwrap();

        let provider = Arc::new(MockProvider {
            response: CoffeeMarketIndicators::new(today, 1.0, 1.0, 1.0, 1.0, 1.0),
            called: Arc::new(Mutex::new(false)),
        });

        let cache = Arc::new(FailingGetCache);
        let clock = Arc::new(FixedClock { date: today });

        let use_case = GetCoffeeMarketIndicatorsUseCase::new(provider, cache, clock, 3600);

        let result = use_case.execute().await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn should_propagate_error_when_cache_set_fails() {
        let today = NaiveDate::from_ymd_opt(2025, 1, 10).unwrap();

        let provider = Arc::new(MockProvider {
            response: CoffeeMarketIndicators::new(today, 1.0, 1.0, 1.0, 1.0, 1.0),
            called: Arc::new(Mutex::new(false)),
        });

        let cache = Arc::new(FailingSetCache {
            stored: Arc::new(Mutex::new(None)),
        });

        let clock = Arc::new(FixedClock { date: today });

        let use_case = GetCoffeeMarketIndicatorsUseCase::new(provider, cache, clock, 3600);

        let result = use_case.execute().await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn should_coalesce_concurrent_cold_start_requests() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::time::Duration;

        struct SlowCountingProvider {
            pub response: CoffeeMarketIndicators,
            pub call_count: Arc<AtomicUsize>,
        }

        #[async_trait::async_trait]
        impl CoffeeMarketProvider for SlowCountingProvider {
            async fn get_market_indicators(&self) -> Result<CoffeeMarketIndicators> {
                self.call_count.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(80)).await;
                Ok(self.response.clone())
            }
        }

        let today = NaiveDate::from_ymd_opt(2026, 10, 26).unwrap();
        let call_count = Arc::new(AtomicUsize::new(0));
        let provider = Arc::new(SlowCountingProvider {
            response: CoffeeMarketIndicators::new(
                today,
                2_450_000.0,
                75_000.0,
                252.35,
                4210.50,
                2_410_000.0,
            ),
            call_count: call_count.clone(),
        });

        let cache = Arc::new(MockCache {
            stored: Arc::new(Mutex::new(None)),
        });
        let clock = Arc::new(FixedClock { date: today });
        let use_case = Arc::new(GetCoffeeMarketIndicatorsUseCase::new(
            provider, cache, clock, 3600,
        ));

        let uc1 = use_case.clone();
        let uc2 = use_case.clone();
        let (r1, r2) = tokio::join!(uc1.execute(), uc2.execute());

        assert_eq!(r1.unwrap().internal_price_cop, 2_450_000.0);
        assert_eq!(r2.unwrap().internal_price_cop, 2_450_000.0);
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }
}
