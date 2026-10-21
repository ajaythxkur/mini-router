use axum::{
    Json, Router,
    extract::{Path, State},
    routing::get,
};
use reqwest::StatusCode;
use std::{collections::HashMap, sync::Arc};

use crate::{connector::Connector, engine::http_client, loader};

#[derive(Clone)]
pub struct AppState {
    pub connectors: Arc<HashMap<String, Connector>>,
    pub client: reqwest::Client,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/connectors/{provider}/{endpoint}", get(inspect))
        .with_state(state)
}

async fn health() -> &'static str {
    "OK"
}

async fn inspect(
    State(state): State<AppState>,
    Path((provider, endpoint)): Path<(String, String)>,
) -> Result<Json<Connector>, StatusCode> {
    let id = format!("{provider}#{endpoint}");
    match state.connectors.get(&id) {
        Some(connector) => Ok(Json(connector.clone())),
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn server(addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    let c_vec = loader::load_connectors(std::path::Path::new("connectors"))?;
    let connectors: HashMap<String, Connector> =
        c_vec.into_iter().map(|c| (c.id.clone(), c)).collect();
    let state = AppState {
        connectors: Arc::new(connectors),
        client: http_client()?,
    };
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("listening on http://{addr}");
    axum::serve(listener, router(state)).await?;
    Ok(())
}
