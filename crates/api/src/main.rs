mod config;
mod error;
mod routes;
mod state;

use anyhow::{Context, Result};
use axum::extract::{DefaultBodyLimit, Request, State};
use axum::http::{HeaderName, HeaderValue, Method, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tower_governor::GovernorLayer;
use tower_governor::governor::GovernorConfigBuilder;
use tower_governor::key_extractor::SmartIpKeyExtractor;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

use crate::config::Config;
use crate::error::ApiError;
use crate::state::AppState;

const API_KEY_HEADER: &str = "x-api-key";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .json()
        .init();

    let cfg = Config::from_env()?;
    let state = Arc::new(AppState::init(&cfg).await?);
    state::spawn_hash_index_refresh(Arc::clone(&state));

    let app = router(Arc::clone(&state), &cfg)?;
    let listener = tokio::net::TcpListener::bind(&cfg.bind_addr)
        .await
        .with_context(|| format!("binding {}", cfg.bind_addr))?;
    tracing::info!(addr = %cfg.bind_addr, "api listening");
    // Connect info is the rate limiter's fallback key when no proxy header is present.
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c().await.ok();
        })
        .await?;
    Ok(())
}

fn router(state: Arc<AppState>, cfg: &Config) -> Result<Router> {
    // 2 requests/s per client IP with bursts of 10. viz-sv proxies every request, so it must
    // forward the browser's address in X-Forwarded-For; the API key keeps others from spoofing it.
    let governor = Arc::new(
        GovernorConfigBuilder::default()
            .per_millisecond(500)
            .burst_size(10)
            .key_extractor(SmartIpKeyExtractor)
            .finish()
            .context("invalid rate limit config")?,
    );
    let limiter = governor.limiter().clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            limiter.retain_recent();
        }
    });
    let rate_limit = GovernorLayer::new(governor).error_handler(|e| match e {
        tower_governor::GovernorError::TooManyRequests { .. } => ApiError::RateLimited.into_response(),
        other => ApiError::internal(other).into_response(),
    });

    let search = Router::new()
        .route("/api/search/image", post(routes::search::search_image))
        .route("/api/search/video", post(routes::search::search_video))
        .route("/api/search/text", post(routes::search::search_text))
        .layer(DefaultBodyLimit::max(cfg.max_upload_bytes))
        .route_layer(rate_limit)
        // Outermost, so unauthenticated requests are rejected before they touch the limiter.
        .route_layer(middleware::from_fn_with_state(Arc::clone(&state), require_api_key));

    let cors = CorsLayer::new()
        .allow_origin(cfg.allowed_origin.parse::<HeaderValue>().context("API_ALLOWED_ORIGIN is not a valid origin")?)
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([header::CONTENT_TYPE, HeaderName::from_static(API_KEY_HEADER)]);

    Ok(Router::new()
        .merge(search)
        .route("/healthz", get(routes::health::healthz))
        .layer(CompressionLayer::new())
        .layer(TimeoutLayer::with_status_code(StatusCode::GATEWAY_TIMEOUT, REQUEST_TIMEOUT))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state))
}

/// Only viz-sv's server (which holds BACKEND_API_KEY privately) may call the search routes.
async fn require_api_key(State(state): State<Arc<AppState>>, request: Request, next: Next) -> Response {
    let provided = request.headers().get(API_KEY_HEADER).map(HeaderValue::as_bytes).unwrap_or_default();
    if constant_time_eq(provided, state.api_key.as_bytes()) {
        next.run(request).await
    } else {
        ApiError::Unauthorized.into_response()
    }
}

/// Comparison time doesn't depend on where the inputs differ, so the key can't be guessed byte by byte.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |diff, (x, y)| diff | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::constant_time_eq;

    #[test]
    fn compares_keys() {
        assert!(constant_time_eq(b"secret", b"secret"));
        assert!(!constant_time_eq(b"secret", b"secreT"));
        assert!(!constant_time_eq(b"secret", b"secret2"));
        assert!(!constant_time_eq(b"", b"secret"));
    }
}
