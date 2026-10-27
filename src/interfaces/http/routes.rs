//! Presentation Adapter — Axum HTTP Routes for Argos Coffee Market Microservice
//! Exposes `/health`, `/info`, `/coffee`, `/price` (TOON) and `/json` (JSON).

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde_json::json;
use std::sync::Arc;
use tracing::{error, info};

use crate::application::use_cases::get_coffee_market_indicators::GetCoffeeMarketIndicatorsUseCase;
use crate::domain::entities::coffee_market_indicators::CoffeeMarketIndicators;
use crate::domain::traits::clock::Clock;
use crate::domain::traits::coffee_market_provider::CoffeeMarketProvider;

pub struct CoffeeAppState<P, T>
where
    P: CoffeeMarketProvider + 'static,
    T: Clock + 'static,
{
    pub use_case: Arc<GetCoffeeMarketIndicatorsUseCase<P, T>>,
}

impl<P, T> Clone for CoffeeAppState<P, T>
where
    P: CoffeeMarketProvider + 'static,
    T: Clock + 'static,
{
    fn clone(&self) -> Self {
        Self {
            use_case: self.use_case.clone(),
        }
    }
}

pub fn build_router<P, T>(use_case: Arc<GetCoffeeMarketIndicatorsUseCase<P, T>>) -> Router
where
    P: CoffeeMarketProvider + 'static,
    T: Clock + 'static,
{
    let state = CoffeeAppState { use_case };

    Router::new()
        .route("/health", get(health_handler))
        .route("/info", get(info_handler))
        .route("/coffee", get(coffee_toon_handler::<P, T>))
        .route("/price", get(coffee_toon_handler::<P, T>))
        .route("/history", get(coffee_history_toon_handler::<P, T>))
        .route("/history/json", get(coffee_history_json_handler::<P, T>))
        .route("/json", get(coffee_json_handler::<P, T>))
        .with_state(state)
}

async fn health_handler() -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "service": "argos_coffee_price_scrapper",
        "alias": "svc-coffee"
    }))
}

async fn info_handler() -> Response {
    let info = json!({
        "service": "argos_coffee_price_scrapper",
        "protocol": "TOON",
        "source": "Federación Nacional de Cafeteros de Colombia",
        "capabilities": [
            "internal_price_cop",
            "pasilla_cop",
            "ny_price_usd",
            "exchange_rate_cop_usd",
            "mecic_cop",
            "history"
        ]
    });

    let toon = toon_codec::to_toon(&info).unwrap_or_default();
    ([("content-type", "text/plain; charset=utf-8")], toon).into_response()
}

async fn coffee_toon_handler<P, T>(State(state): State<CoffeeAppState<P, T>>) -> Response
where
    P: CoffeeMarketProvider + 'static,
    T: Clock + 'static,
{
    info!("Fetching coffee market indicators (TOON)");
    match state.use_case.execute().await {
        Ok(indicators) => {
            let payload = indicators_to_json(&indicators);
            match toon_codec::to_toon(&payload) {
                Ok(toon) => ([("content-type", "text/plain; charset=utf-8")], toon).into_response(),
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("error:TOON encoding failed: {e}"),
                )
                    .into_response(),
            }
        }
        Err(e) => {
            error!("Failed to fetch coffee market indicators: {e}");
            (
                StatusCode::BAD_GATEWAY,
                format!("error:No se pudieron obtener los indicadores del café: {e}"),
            )
                .into_response()
        }
    }
}

async fn coffee_history_toon_handler<P, T>(State(state): State<CoffeeAppState<P, T>>) -> Response
where
    P: CoffeeMarketProvider + 'static,
    T: Clock + 'static,
{
    info!("Fetching coffee historical indicators (TOON)");
    match state.use_case.get_history(30).await {
        Ok(records) => {
            let toon = CoffeeMarketIndicators::format_history_toon(&records);
            ([("content-type", "text/plain; charset=utf-8")], toon).into_response()
        }
        Err(e) => {
            error!("Failed to fetch coffee historical indicators: {e}");
            (
                StatusCode::BAD_GATEWAY,
                format!("error:No se pudo obtener el historial del café: {e}"),
            )
                .into_response()
        }
    }
}

async fn coffee_history_json_handler<P, T>(State(state): State<CoffeeAppState<P, T>>) -> Response
where
    P: CoffeeMarketProvider + 'static,
    T: Clock + 'static,
{
    match state.use_case.get_history(30).await {
        Ok(records) => Json(json!({
            "service": "argos_coffee_price_scrapper",
            "count": records.len(),
            "records": records
        }))
        .into_response(),
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({
                "error": format!("No se pudo obtener el historial del café: {e}")
            })),
        )
            .into_response(),
    }
}

async fn coffee_json_handler<P, T>(State(state): State<CoffeeAppState<P, T>>) -> Response
where
    P: CoffeeMarketProvider + 'static,
    T: Clock + 'static,
{
    match state.use_case.execute().await {
        Ok(indicators) => Json(indicators_to_json(&indicators)).into_response(),
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({
                "error": format!("No se pudieron obtener los indicadores del café: {e}")
            })),
        )
            .into_response(),
    }
}

pub fn indicators_to_json(ind: &CoffeeMarketIndicators) -> serde_json::Value {
    json!({
        "service": "argos_coffee_price_scrapper",
        "publication_date": ind.publication_date.to_string(),
        "internal_price_cop": ind.internal_price_cop,
        "pasilla_cop": ind.pasilla_cop,
        "ny_price_usd": ind.ny_price_usd,
        "exchange_rate_cop_usd": ind.exchange_rate_cop_usd,
        "mecic_cop": ind.mecic_cop
    })
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::cache::memory_cache::MemoryCache;
    use axum::body::Body;
    use axum::http::Request;
    use chrono::NaiveDate;
    use tower::ServiceExt;

    struct DummyProvider;

    #[async_trait::async_trait]
    impl CoffeeMarketProvider for DummyProvider {
        async fn get_market_indicators(&self) -> anyhow::Result<CoffeeMarketIndicators> {
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

    struct DummyClock;
    impl Clock for DummyClock {
        fn today(&self) -> NaiveDate {
            NaiveDate::from_ymd_opt(2026, 10, 26).unwrap()
        }
        fn is_market_day(&self) -> bool {
            true
        }
    }

    #[tokio::test]
    async fn test_http_endpoints_health_info_coffee_and_price() {
        let use_case = Arc::new(GetCoffeeMarketIndicatorsUseCase::new(
            Arc::new(DummyProvider),
            Arc::new(MemoryCache::new()),
            Arc::new(DummyClock),
            3600,
        ));

        let app = build_router(use_case);

        // 1. /health
        let res = app
            .clone()
            .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // 2. /info
        let res = app
            .clone()
            .oneshot(Request::builder().uri("/info").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // 3. /coffee (TOON)
        let res = app
            .clone()
            .oneshot(Request::builder().uri("/coffee").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let toon_str = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(toon_str.contains("internal_price_cop:2450000"));
        assert!(toon_str.contains("publication_date:2026-10-26"));

        // 4. /price (alias for /coffee)
        let res = app
            .clone()
            .oneshot(Request::builder().uri("/price").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // 5. /history (TOON tabular summary + daily records)
        let res = app
            .clone()
            .oneshot(Request::builder().uri("/history").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let hist_toon = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(hist_toon.contains("resumen_historico{"));
        assert!(hist_toon.contains("historial_diario[1]{"));

        // 6. /history/json
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/history/json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }
}
