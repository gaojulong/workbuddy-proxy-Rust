//! WorkBuddy Proxy 入口
//! 仅负责：加载配置、初始化日志、启动 axum 服务

use std::sync::Arc;

use tracing_subscriber::EnvFilter;
use workbuddy_proxy_rust::config::Config;

#[tokio::main]
async fn main() {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let config = Arc::new(Config::load());
    let wb_version = config.final_wb_version();
    let proxy_port = config.proxy_port;

    tracing::info!("Starting WorkBuddy proxy on port {}", proxy_port);
    tracing::info!("WB version: {}", wb_version);
    tracing::info!("API key: {}", config.proxy_api_key);
    tracing::info!("Upstream: {}", config.wb_api_base);

    // 初始化状态（token 等）
    let state = workbuddy_proxy_rust::routes::init_state(config).await;

    // 构建路由
    let app = workbuddy_proxy_rust::routes::build_router(state);

    // 启动服务
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", proxy_port))
        .await
        .expect("failed to bind port");

    tracing::info!("Listening on 0.0.0.0:{}", proxy_port);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");
}

/// 优雅关闭（Ctrl+C / SIGTERM）
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("Shutting down...");
}
