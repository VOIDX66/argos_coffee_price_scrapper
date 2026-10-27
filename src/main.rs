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
use infrastructure::cache::memory_cache::MemoryCache;
use infrastructure::cache::redis_cache::RedisCache;
use infrastructure::providers::scraper_market_provider::ScraperCoffeeMarketProvider;
use infrastructure::time::system_clock::SystemClock;
use interfaces::http::routes::build_router;

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
    let use_case = Arc::new(GetCoffeeMarketIndicatorsUseCase::new(
        provider,
        cache,
        clock,
        cfg.cache_ttl_seconds,
    ));

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
