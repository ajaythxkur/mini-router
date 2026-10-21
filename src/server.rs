use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Arc};

use crate::{
    connector::Connector,
    engine::{RunError, http_client, run},
    loader,
};

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("connector not found: {0}")]
    NotFound(String),
    #[error(transparent)]
    Run(#[from] RunError),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, code, vendor_status) = match &self {
            ApiError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found", None),
            ApiError::Run(RunError::InvalidInput(_)) => {
                (StatusCode::UNPROCESSABLE_ENTITY, "invalid_input", None)
            }
            ApiError::Run(RunError::InvalidSchema(_)) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "connector_misconfigured",
                None,
            ),
            ApiError::Run(RunError::Http(e)) if e.is_timeout() => {
                (StatusCode::GATEWAY_TIMEOUT, "vendor_timeout", None)
            }
            ApiError::Run(RunError::Http(_)) => {
                (StatusCode::BAD_GATEWAY, "vendor_unreachable", None)
            }
            ApiError::Run(RunError::Status { status, .. }) => {
                (StatusCode::BAD_GATEWAY, "vendor_error", Some(*status))
            }
            ApiError::Run(RunError::InvalidResponse(_)) => {
                (StatusCode::BAD_GATEWAY, "vendor_invalid_response", None)
            }
        };
        let mut error = json!({ "code": code, "message": self.to_string() });
        if let Some(vendor_status) = vendor_status {
            error["vendor_status"] = json!(vendor_status);
        }
        (status, Json(json!({ "error": error }))).into_response()
    }
}

#[derive(Clone)]
pub struct AppState {
    pub connectors: Arc<HashMap<String, Connector>>,
    pub client: reqwest::Client,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/connectors/{provider}/{endpoint}", get(inspect))
        .route("/connectors/{provider}/{endpoint}/run", post(run_connector))
        .with_state(state)
}

async fn health() -> &'static str {
    "OK"
}

fn find_connector<'a>(
    state: &'a AppState,
    provider: &str,
    endpoint: &str,
) -> Result<&'a Connector, ApiError> {
    let id = format!("{provider}#{endpoint}");
    state
        .connectors
        .get(&id)
        .ok_or_else(|| ApiError::NotFound(id))
}

async fn inspect(
    State(state): State<AppState>,
    Path((provider, endpoint)): Path<(String, String)>,
) -> Result<Json<Connector>, ApiError> {
    let connector = find_connector(&state, &provider, &endpoint)?;
    Ok(Json(connector.clone()))
}

async fn run_connector(
    State(state): State<AppState>,
    Path((provider, endpoint)): Path<(String, String)>,
    Json(input): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let connector = find_connector(&state, &provider, &endpoint)?;
    let data = run(&state.client, connector, &input).await?;
    Ok(Json(json!({ "connector": &connector.id, "data": data })))
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
