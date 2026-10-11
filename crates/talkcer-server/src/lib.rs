//! Axum HTTP API for Talkcer.
//!
//! Task 1 exposes health. Task 2 exposes session CRUD.
//! Routes bind to `127.0.0.1:8787` by default and do not call AI providers.

mod ai;
mod error;
mod health;
mod mock;
mod sessions;
mod state;
mod timeline;
mod ws;

use std::net::SocketAddr;
use std::path::PathBuf;

use axum::routing::get;
use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

pub use state::AppState;

/// Application version reported by `GET /api/health`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Default listen address from the security defaults.
pub const DEFAULT_BIND: &str = "127.0.0.1:8787";

/// Build the HTTP router.
///
/// The web app in development calls this API directly. CORS is open because
/// the server only binds to loopback unless the operator changes the address.
#[must_use]
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health::health))
        .route("/ws", get(ws::connect))
        .merge(sessions::routes())
        .merge(timeline::routes())
        .merge(ai::routes())
        .merge(mock::routes())
        .layer(TraceLayer::new_for_http())
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .with_state(state)
}

/// Open the database, migrate it, and serve until the process is cancelled.
///
/// # Errors
///
/// Returns an error when the database or the TCP listener cannot be opened.
pub async fn serve(database_path: PathBuf, bind: SocketAddr) -> anyhow::Result<()> {
    let state = AppState::connect(&database_path).await?;
    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!(%bind, database = %database_path.display(), "talkcer server listening");
    axum::serve(listener, router(state)).await?;
    Ok(())
}
