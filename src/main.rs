pub mod application;
pub mod config;
pub mod domain;
pub mod infrastructure;
pub mod interfaces;

use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{info, warn};

use application::use_cases::get_coffee_market_indicators::GetCoffeeMarketIndicatorsUseCase;
use config::ServiceConfig;
use domain::entities::coffee_market_indicators::CoffeeMarketIndicators;
use domain::traits::cache_repository::CacheRepository;
use infrastructure::cache::file_history_repository::FileHistoryRepository;
use infrastructure::cache::memory_cache::MemoryCache;
use infrastructure::cache::redis_cache::RedisCache;
use infrastructure::providers::scraper_market_provider::ScraperCoffeeMarketProvider;
use infrastructure::time::system_clock::SystemClock;
use interfaces::http::routes::build_router;

use domain::traits::clock::Clock;
use domain::traits::coffee_market_provider::CoffeeMarketProvider;

pub fn spawn_cache_warmup<P, T>(
    use_case: Arc<GetCoffeeMarketIndicatorsUseCase<P, T>>,
) -> tokio::task::JoinHandle<()>
where
    P: CoffeeMarketProvider + 'static,
    T: Clock + 'static,
{
    tokio::spawn(async move {
        info!("Pre-warming coffee market indicators cache in background...");
        match use_case.execute().await {
            Ok(indicators) => {
                info!(
                    "Coffee market indicators cache pre-warmed (publication_date={})",
                    indicators.publication_date
                );
            }
            Err(e) => {
                warn!("Background cache pre-warm failed: {e}");
            }
        }
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let cfg = ServiceConfig::from_env();
    let provider = Arc::new(ScraperCoffeeMarketProvider::new());

    let cache: Arc<dyn CacheRepository<CoffeeMarketIndicators>> = match cfg.redis_url {
        Some(ref url) => match RedisCache::new(url).await {
            Ok(redis) => {
                info!("Connected to Redis cache backend");
                Arc::new(redis)
            }
            Err(e) => {
                warn!("Redis connection failed ({e}), falling back to in-memory TTL cache");
                Arc::new(MemoryCache::new())
            }
        },
        None => {
            info!("Using in-memory TTL cache for coffee market indicators");
            Arc::new(MemoryCache::new())
        }
    };

    let clock = Arc::new(SystemClock);
    let history = Arc::new(FileHistoryRepository::default_persistent());
    let use_case = Arc::new(
        GetCoffeeMarketIndicatorsUseCase::new(provider, cache, clock, cfg.cache_ttl_seconds)
            .with_history(history),
    );

    spawn_cache_warmup(use_case.clone());

    let app = build_router(use_case);


    let addr = SocketAddr::from(([0, 0, 0, 0], cfg.port));
    info!(
        "☕ Argos Coffee Market Microservice (argos_coffee_price_scrapper) running on http://{}",
        addr
    );

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    struct MockProvider {
        should_fail: bool,
    }

    #[async_trait::async_trait]
    impl CoffeeMarketProvider for MockProvider {
        async fn get_market_indicators(&self) -> anyhow::Result<CoffeeMarketIndicators> {
            if self.should_fail {
                Err(anyhow::anyhow!("simulated scrape failure"))
            } else {
                Ok(CoffeeMarketIndicators::new(
                    NaiveDate::from_ymd_opt(2026, 10, 26).unwrap(),
                    2_450_000.0,
                    75_000.0,
                    252.35,
                    4210.50,
                    2_410_000.0,
                ))
            }
        }
    }

    struct FixedClock;
    impl Clock for FixedClock {
        fn today(&self) -> NaiveDate {
            NaiveDate::from_ymd_opt(2026, 10, 26).unwrap()
        }
        fn is_market_day(&self) -> bool {
            true
        }
    }

    #[tokio::test]
    async fn test_spawn_cache_warmup_populates_cache() {
        let cache = Arc::new(MemoryCache::new());
        let use_case = Arc::new(GetCoffeeMarketIndicatorsUseCase::new(
            Arc::new(MockProvider { should_fail: false }),
            cache.clone(),
            Arc::new(FixedClock),
            3600,
        ));

        let handle = spawn_cache_warmup(use_case);
        handle.await.unwrap();

        let cached = cache.get("coffee:market:indicators").await.unwrap();
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().internal_price_cop, 2_450_000.0);
    }

    #[tokio::test]
    async fn test_spawn_cache_warmup_handles_provider_failure_gracefully() {
        let cache = Arc::new(MemoryCache::new());
        let use_case = Arc::new(GetCoffeeMarketIndicatorsUseCase::new(
            Arc::new(MockProvider { should_fail: true }),
            cache.clone(),
            Arc::new(FixedClock),
            3600,
        ));

        let handle = spawn_cache_warmup(use_case);
        handle.await.unwrap();

        let cached = cache.get("coffee:market:indicators").await.unwrap();
        assert!(cached.is_none());
    }
}

