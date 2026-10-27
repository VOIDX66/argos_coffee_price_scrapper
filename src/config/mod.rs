//! Configuration Layer for Argos Coffee Price Microservice
//! Loads settings from environment variables with safe defaults (no hardcoded secrets).

pub struct ServiceConfig {
    pub port: u16,
    pub redis_url: Option<String>,
    pub cache_ttl_seconds: u64,
}

impl ServiceConfig {
    pub fn from_env() -> Self {
        let port: u16 = std::env::var("COFFEE_PORT")
            .or_else(|_| std::env::var("COFFEE_SVC_PORT"))
            .or_else(|_| std::env::var("PORT"))
            .unwrap_or_else(|_| "8084".to_string())
            .parse()
            .unwrap_or(8084);

        let redis_url = std::env::var("REDIS_URL")
            .ok()
            .filter(|s| !s.trim().is_empty());

        let cache_ttl_seconds: u64 = std::env::var("COFFEE_CACHE_TTL_SECS")
            .or_else(|_| std::env::var("COFFEE_CACHE_TTL"))
            .unwrap_or_else(|_| "3600".to_string())
            .parse()
            .unwrap_or(3600);

        Self {
            port,
            redis_url,
            cache_ttl_seconds,
        }
    }
}
