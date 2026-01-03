use crate::{MailboxProvider, QueryParameters, QueryResult};
use axum::{Json, Router, extract::State, http::StatusCode, response::IntoResponse, routing::post};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Creates an axum Router that serves mailbox queries from the given provider.
pub fn router<P>(provider: P) -> Router
where
    P: MailboxProvider + Send + 'static,
    P::Error: std::error::Error + Send + Sync + 'static,
{
    let state = Arc::new(Mutex::new(provider));
    Router::new()
        .route("/query", post(query_handler::<P>))
        .with_state(state)
}

async fn query_handler<P>(
    State(state): State<Arc<Mutex<P>>>,
    Json(params): Json<QueryParameters>,
) -> Result<Json<QueryResult>, (StatusCode, impl IntoResponse)>
where
    P: MailboxProvider + Send + 'static,
    P::Error: std::error::Error + Send + Sync + 'static,
{
    let mut provider = state.lock().await;
    let result = provider
        .query(params)
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    Ok(Json(result))
}
