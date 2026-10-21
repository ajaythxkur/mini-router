use std::sync::Arc;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use mini_router::{
    loader,
    server::{AppState, router},
};
use serde_json::Value;
use tower::ServiceExt;

fn test_app() -> Router {
    let connectors = loader::load_connectors(std::path::Path::new("connectors"))
        .unwrap()
        .into_iter()
        .map(|c| (c.id.clone(), c))
        .collect();
    router(AppState {
        connectors: Arc::new(connectors),
        client: reqwest::Client::new(),
    })
}

async fn send(req: Request<Body>) -> (StatusCode, Value) {
    let res = test_app().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn unknown_connector_is_404_json() {
    let req = Request::get("/connectors/nope/nothing")
        .body(Body::empty())
        .unwrap();
    let (status, body) = send(req).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "not_found")
}

#[tokio::test]
async fn invalid_input_is_422() {
    let req = Request::post("/connectors/open-meteo/forecast/run")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"latitude":200, "longitude":0}"#))
        .unwrap();

    let (status, body) = send(req).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "invalid_input");
}

#[tokio::test]
async fn discover_ranks_weather_first() {
    let req = Request::get("/discover?q=weather")
        .body(Body::empty())
        .unwrap();

    let (status, body) = send(req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body[0]["id"], "open-meteo#forecast");
}

#[tokio::test]
async fn discover_without_q_lists_all() {
    let req = Request::get("/discover").body(Body::empty()).unwrap();

    let (status, body) = send(req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 12);
}
