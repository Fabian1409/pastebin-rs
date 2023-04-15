use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, RwLock},
    time::Duration,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use axum::{
    error_handling::HandleErrorLayer,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tower::{BoxError, ServiceBuilder};
use tower_http::trace::TraceLayer;
use uuid::Uuid;

#[derive(Debug, Deserialize, Serialize, Clone)]
struct Entry {
    id: Uuid,
    data: String,
}

#[derive(Debug, Deserialize)]
struct NewEntry {
    data: String,
}

type Database = Arc<RwLock<HashMap<Uuid, Entry>>>;

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "pastebin_server=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let db = Database::default();

    let app = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        .route("/paste", post(add_entry))
        .route("/copy/:id", get(get_entry))
        .layer(
            ServiceBuilder::new()
                .layer(HandleErrorLayer::new(|error: BoxError| async move {
                    if error.is::<tower::timeout::error::Elapsed>() {
                        Ok(StatusCode::REQUEST_TIMEOUT)
                    } else {
                        Err((
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("Unhandled internal error: {}", error),
                        ))
                    }
                }))
                .timeout(Duration::from_secs(10))
                .layer(TraceLayer::new_for_http())
                .into_inner(),
        )
        .with_state(db);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::debug!("listening on {}", addr);

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn add_entry(State(db): State<Database>, Json(input): Json<NewEntry>) -> impl IntoResponse {
    let id = Uuid::new_v4();
    let entry = Entry {
        id,
        data: input.data,
    };

    db.write().unwrap().insert(id, entry);
    tracing::debug!("created clipboard entry with id={}", id);

    (StatusCode::OK, Json(id))
}

async fn get_entry(
    State(db): State<Database>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    tracing::debug!("fetching clipboard entry with id={}", id);
    if let Some(entry) = db.read().unwrap().get(&id) {
        Ok(Json(entry.clone()))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}
