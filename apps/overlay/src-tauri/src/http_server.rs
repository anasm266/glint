//! Local HTTP listener for hook events on 127.0.0.1:47611.

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde_json::json;
use std::net::SocketAddr;
use tauri::AppHandle;

use crate::session;
use crate::state::SharedState;

pub const PORT: u16 = 47611;

#[derive(Clone)]
struct Ctx {
    app: AppHandle,
    state: SharedState,
}

pub fn spawn(app: AppHandle, state: SharedState) {
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(2)
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                tracing::error!("failed to build tokio runtime: {e}");
                return;
            }
        };
        rt.block_on(async move {
            let ctx = Ctx { app, state };
            let router = Router::new()
                .route("/health", get(health))
                .route("/event", post(event))
                .with_state(ctx);

            let addr: SocketAddr = ([127, 0, 0, 1], PORT).into();
            tracing::info!("hook listener on http://{addr}");

            match tokio::net::TcpListener::bind(addr).await {
                Ok(listener) => {
                    if let Err(e) = axum::serve(listener, router).await {
                        tracing::error!("axum serve error: {e}");
                    }
                }
                Err(e) => {
                    tracing::error!("failed to bind {addr}: {e}");
                }
            }
        });
    });
}

async fn health() -> impl IntoResponse {
    Json(json!({ "ok": true, "version": env!("CARGO_PKG_VERSION") }))
}

async fn event(
    State(ctx): State<Ctx>,
    Json(raw): Json<session::RawEvent>,
) -> impl IntoResponse {
    tracing::debug!("event: {} payload={}", raw.event, raw.payload);

    let touched = ctx
        .state
        .with_session_state(|map, routing| session::apply(map, routing, raw));
    if touched.is_some() {
        ctx.state.emit_snapshot(&ctx.app);
    }
    (StatusCode::OK, Json(json!({ "ok": true })))
}
