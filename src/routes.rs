use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

use crate::config::Config;
use crate::proxy::{non_stream_response, stream_response, AppState, AppStateInner};
use crate::token::TokenManager;

/// API Key 校验（与 Python _verify_api_key 一致）
fn verify_api_key(headers: &axum::http::HeaderMap, expected_key: &str) -> Result<(), StatusCode> {
    let auth = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let key = auth.strip_prefix("Bearer ").unwrap_or(auth).trim();
    if !key.is_empty() && key == expected_key {
        return Ok(());
    }
    let xkey = headers
        .get("X-API-Key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .trim();
    if !xkey.is_empty() && xkey == expected_key {
        return Ok(());
    }
    Err(StatusCode::UNAUTHORIZED)
}

/// 构建路由
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/models", get(list_models))
        .route("/v1/chat/completions", post(chat_completions))
        .layer(
            tower_http::cors::CorsLayer::new()
                .allow_origin(tower_http::cors::Any)
                .allow_headers(tower_http::cors::Any)
                .allow_methods(tower_http::cors::Any)
                .allow_credentials(false),
        )
        .with_state(state)
}

/// GET /health
async fn health(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    verify_api_key(&headers, &state.api_key)?;
    let has_token = {
        let tm = state.token.lock().await;
        !tm.access_token.is_empty()
    };
    let expired = {
        let tm = state.token.lock().await;
        crate::jwt::is_expired(&tm.access_token)
    };
    Ok(Json(serde_json::json!({
        "status": if has_token && !expired { "ok" } else { "degraded" },
        "has_token": has_token,
        "expired": expired,
    })))
}

/// GET /v1/models
async fn list_models(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    verify_api_key(&headers, &state.api_key)?;
    Ok(Json(crate::models::build_models_response()))
}

/// POST /v1/chat/completions
async fn chat_completions(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    body: axum::extract::Json<serde_json::Value>,
) -> Result<axum::response::Response, (StatusCode, String)> {
    if let Err(status) = verify_api_key(&headers, &state.api_key) {
        return Err((status, "Invalid API key".to_string()));
    }

    let body_val = body.0;
    let raw_model = body_val
        .get("model")
        .and_then(|m| m.as_str())
        .unwrap_or("deepseek-v3")
        .to_string();
    let model = crate::models::resolve_model(&raw_model);
    let stream = body_val
        .get("stream")
        .and_then(|s| s.as_bool())
        .unwrap_or(false);

    if raw_model != model {
        tracing::info!("[Model] Mapped: {} → {}", raw_model, model);
    }

    let wb_body = crate::proxy::build_wb_body(body_val.clone(), &model);

    if stream {
        // 流式：mpsc channel + 裸文本 SSE（与 Python 版一致：原样转发上游 data: 行）
        let (tx, rx) = mpsc::channel::<String>(100);
        let state_clone = Arc::clone(&state);
        let model_clone = model.clone();
        let body_clone = wb_body.clone();

        tokio::spawn(async move {
            stream_response(&state_clone, body_clone, model_clone, tx).await;
        });

        let stream = ReceiverStream::new(rx);
        let stream = stream.map(|s| Ok::<_, std::convert::Infallible>(s));
        let body = axum::body::Body::from_stream(stream);
        return Ok(axum::response::Response::builder()
            .header("Content-Type", "text/event-stream")
            .header("Cache-Control", "no-cache")
            .header("X-Accel-Buffering", "no")
            .body(body)
            .expect("failed to build SSE response"));
    }

    // 非流式
    match non_stream_response(&state, wb_body, model).await {
        Ok(json) => Ok(json.into_response()),
        Err((status, msg)) => Err((status, msg)),
    }
}

/// 初始化 AppState（含 token 初始化）
pub async fn init_state(config: Arc<Config>) -> AppState {
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true) // 与 Python verify=False 一致
        .timeout(std::time::Duration::from_secs(config.wb_timeout))
        .connect_timeout(std::time::Duration::from_secs(10))
        .pool_max_idle_per_host(10)
        .build()
        .expect("failed to build reqwest client");

    let api_key = config.proxy_api_key.clone();
    let mut tm = TokenManager::new(config.clone());
    tm.init().await;

    Arc::new(AppStateInner {
        config,
        client,
        token: tokio::sync::Mutex::new(tm),
        api_key,
    })
}
