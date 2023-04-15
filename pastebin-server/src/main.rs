use axum::{
    error_handling::HandleErrorLayer,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::{
    net::SocketAddr,
    sync::{Arc, RwLock},
    time::Duration,
};
use tower::{BoxError, ServiceBuilder};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

const CLIPBOARD_SIZE: usize = 10;

type SharedClipboard = Arc<RwLock<Clipboard>>;

#[derive(Debug, Deserialize, Serialize, Clone)]
struct Entry {
    data: String,
}

#[derive(Debug, Clone)]
struct Clipboard {
    queue: Vec<Entry>,
    capacity: usize,
}

impl Clipboard {
    fn add(&mut self, entry: Entry) {
        if self.queue.len() == self.capacity {
            self.queue.remove(0);
        }
        self.queue.push(entry);
    }

    fn get_entries(&self) -> Vec<Entry> {
        self.queue.clone()
    }
}

impl Default for Clipboard {
    fn default() -> Self {
        Clipboard {
            queue: vec![],
            capacity: CLIPBOARD_SIZE,
        }
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "pastebin_server=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let clipboard = SharedClipboard::default();

    let app = Router::new()
        .route("/paste", post(add_entry))
        .route("/copy", get(get_entries))
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
        .with_state(clipboard);

    let addr = SocketAddr::from(([192, 168, 0, 10], 3000));
    tracing::debug!("listening on {}", addr);

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn add_entry(
    State(clipboard): State<SharedClipboard>,
    Json(entry): Json<Entry>,
) -> impl IntoResponse {
    clipboard.write().unwrap().add(entry);
    tracing::debug!("added clipboard entry");
    StatusCode::OK
}

async fn get_entries(State(clipboard): State<SharedClipboard>) -> impl IntoResponse {
    tracing::debug!("fetching clipboard");
    let entries = clipboard.read().unwrap().get_entries();
    (StatusCode::OK, Json(entries))
}
