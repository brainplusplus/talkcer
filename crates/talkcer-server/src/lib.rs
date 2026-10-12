//! Axum HTTP API for Talkcer.
//!
//! Task 1 exposes health. Task 2 exposes session CRUD.
//! Routes bind to `127.0.0.1:54000` by default and do not call AI providers.

mod ai;
mod capture;
mod error;
mod export;
mod health;
mod learning;
mod mock;
mod overlay;
mod remote;
mod sessions;
mod state;
mod timeline;
mod voice;
mod ws;

use std::net::SocketAddr;
use std::path::PathBuf;

use axum::middleware;
use axum::routing::get;
use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

pub use state::AppState;

/// Background audio pipeline pump, started by [`serve`].
pub use capture::spawn_pump;

/// Application version reported by `GET /api/health`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Default listen address from the security defaults.
///
/// One port for everything: the API, the WebSocket, and (when
/// `apps/web/dist` exists) the web app itself. The Vite dev server runs on
/// 54001 and proxies `/api` and `/ws` here.
pub const DEFAULT_BIND: &str = "127.0.0.1:54000";

/// Build the HTTP router.
///
/// The web app in development calls this API directly. CORS is open because
/// the server only binds to loopback unless the operator changes the address.
///
/// When `web_dir` points at a built web bundle (`apps/web/dist`), the server
/// also serves the single-page app on the same port, falling back to
/// `index.html` for client-side routes. A missing directory is ignored, so the
/// API still runs without the bundle.
#[must_use]
pub fn router(state: AppState, web_dir: Option<PathBuf>) -> Router {
    let mut router = Router::new()
        .route("/api/health", get(health::health))
        .route("/ws", get(ws::connect))
        .merge(sessions::routes())
        .merge(timeline::routes())
        .merge(ai::routes())
        .merge(mock::routes())
        .merge(learning::routes())
        .merge(voice::routes())
        .merge(capture::routes())
        .merge(remote::routes())
        .merge(overlay::routes())
        .merge(export::routes());

    if let Some(dir) = web_dir.filter(|path| path.is_dir()) {
        let index = dir.join("index.html");
        router = router.fallback_service(ServeDir::new(&dir).fallback(ServeFile::new(index)));
    }

    let auth_state = state.clone();
    router
        .layer(middleware::from_fn_with_state(auth_state, remote::authorize))
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
/// `web_dir`, when it points at a built web bundle, is served on the same port
/// as the API with an `index.html` fallback for client-side routes.
///
/// # Errors
///
/// Returns an error when the database or the TCP listener cannot be opened.
pub async fn serve(
    database_path: PathBuf,
    bind: SocketAddr,
    web_dir: Option<PathBuf>,
) -> anyhow::Result<()> {
    let state = AppState::connect(&database_path).await?;
    let listener = tokio::net::TcpListener::bind(bind).await?;
    capture::spawn_pump(state.clone());
    tracing::info!(%bind, database = %database_path.display(), "talkcer server listening");
    axum::serve(
        listener,
        router(state, web_dir).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}
