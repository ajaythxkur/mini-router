use std::{collections::HashSet, time::Duration};

use reqwest::{Client, RequestBuilder};
use serde_json::{Map, Value};
use thiserror::Error;

use crate::connector::{Connector, Method};

#[derive(Debug, Error)]
pub enum RunError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("connector has an invalid input schema: {0}")]
    InvalidSchema(String),
    #[error("request failed")]
    Http(#[from] reqwest::Error),
    #[error("vendor returned HTTP {status}")]
    Status { status: u16, body: String },
    #[error("vendor returned invalid JSON")]
    InvalidResponse(#[source] serde_json::Error),
}

pub fn validate(connector: &Connector, input: &Value) -> Result<(), RunError> {
    let validator = jsonschema::validator_for(&connector.input)
        .map_err(|e| RunError::InvalidSchema(e.to_string()))?;
    let errors: Vec<String> = validator
        .iter_errors(input)
        .map(|error| format!("{}: {}", error.instance_path(), error))
        .collect();

    if errors.is_empty() {
        return Ok(());
    }

    Err(RunError::InvalidInput(errors.join("; ")))
}

pub fn build_request(
    client: &Client,
    connector: &Connector,
    input: &Value,
) -> Result<RequestBuilder, RunError> {
    let Some(object) = input.as_object() else {
        return Err(RunError::InvalidInput("input must be JSON object".into()));
    };
    let (path, used) = fill_path(&connector.request.path, object)?;
    let url = format!("{}{}", connector.request.base_url, path);

    match connector.request.method {
        Method::Post => Ok(client.post(url).json(input)),
        Method::Get => {
            let mut req = client.get(url);
            for (key, value) in object {
                if used.contains(key) {
                    continue;
                }
                if let Some(text) = query_value(value)? {
                    req = req.query(&[(key, text)]);
                }
            }
            Ok(req)
        }
    }
}

fn query_value(value: &Value) -> Result<Option<String>, RunError> {
    let s = match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => return Ok(None),
        Value::Array(_) | Value::Object(_) => {
            return Err(RunError::InvalidInput(
                "nested arrays/objects can't be sent as query parameters".into(),
            ));
        }
    };

    Ok(Some(s))
}

fn fill_path(
    path: &str,
    object: &Map<String, Value>,
) -> Result<(String, HashSet<String>), RunError> {
    let mut filled = path.to_string();
    let mut used = HashSet::new();

    for (key, value) in object {
        let placeholder = format!("{{{key}}}");
        if filled.contains(&placeholder) {
            let Some(text) = query_value(value)? else {
                continue;
            };
            filled = filled.replace(&placeholder, &urlencoding::encode(&text));
            used.insert(key.clone());
        }
    }

    if filled.contains('{') {
        return Err(RunError::InvalidInput(format!(
            "missing path parameter in {path}"
        )));
    }
    Ok((filled, used))
}

pub async fn run(client: &Client, connector: &Connector, input: &Value) -> Result<Value, RunError> {
    validate(connector, input)?;
    let response = build_request(client, connector, input)?.send().await?;
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(RunError::Status {
            status: status.as_u16(),
            body,
        });
    }
    serde_json::from_str(&body).map_err(RunError::InvalidResponse)
}

pub fn http_client() -> Result<Client, reqwest::Error> {
    let client = Client::builder()
        .user_agent(concat!("mini-router/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(20))
        .build()?;
    Ok(client)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn forecast() -> Connector {
        toml::from_str(include_str!("../connectors/open-meteo/forecast.toml")).unwrap()
    }

    fn with_path(path: &str) -> Connector {
        let mut c = forecast();
        c.request.path = path.to_string();
        c.input = json!({ "type": "object" });
        c
    }

    // validate tests
    #[test]
    fn accepts_valid_input() {
        let input = json!({ "latitude": 28.6, "longitude": 77.2, "current": "temperature_2m" });
        assert!(validate(&forecast(), &input).is_ok());
    }

    #[test]
    fn rejects_missing_latitude() {
        let input = json!({ "longitude": 77.2 });
        let err = validate(&forecast(), &input).unwrap_err();
        assert!(
            matches!(err, RunError::InvalidInput(_)),
            "unexpected: {err:?}"
        );
    }

    #[test]
    fn rejects_wrong_type() {
        let input = json!({ "latitude": "abc", "longitude": 77.2, "current": "temperature_2m" });
        let err = validate(&forecast(), &input).unwrap_err();
        assert!(
            matches!(err, RunError::InvalidInput(_)),
            "unexpected: {err:?}"
        );
    }

    #[test]
    fn rejects_out_of_range() {
        let input = json!({ "latitude": 28.6, "longitude": 500 });
        let err = validate(&forecast(), &input).unwrap_err();
        assert!(
            matches!(err, RunError::InvalidInput(_)),
            "unexpected: {err:?}"
        );
    }

    #[test]
    fn rejects_unknown_field() {
        let input = json!({ "latitude": 28.6, "longitude": 77.2, "lattitude": 28.6 });
        let err = validate(&forecast(), &input).unwrap_err();
        let RunError::InvalidInput(msg) = err else {
            panic!("unexpected: {err:?}")
        };
        assert!(msg.contains("lattitude"), "message was: {msg}");
    }

    // build_request tests
    #[test]
    fn get_puts_input_in_query_string() {
        let client = Client::new();
        let input = json!({ "latitude": 28.6, "longitude": 77.2, "current": "temperature_2m" });

        let req = build_request(&client, &forecast(), &input)
            .unwrap()
            .build()
            .unwrap();
        let query = req.url().query().unwrap_or("");

        assert_eq!(req.method(), reqwest::Method::GET);
        assert!(query.contains("latitude=28.6"), "query was: {query}");
        assert!(
            query.contains("current=temperature_2m"),
            "query was: {query}"
        );
    }

    #[test]
    fn string_values_are_not_quoted() {
        let client = Client::new();
        let input = json!({ "latitude": 28.6, "longitude": 77.2, "current": "temperature_2m" });

        let req = build_request(&client, &forecast(), &input)
            .unwrap()
            .build()
            .unwrap();
        let query = req.url().query().unwrap_or("");

        assert!(
            query.contains("current=temperature_2m"),
            "query was: {query}"
        );
        assert!(!query.contains("%22"), "string was JSON-quoted: {query}");
    }

    #[test]
    fn post_sends_json_body() {
        let client = Client::new();
        let input = json!({ "latitude": 28.6, "longitude": 77.2, "current": "temperature_2m" });

        let mut c = forecast();
        c.request.method = Method::Post;

        let req = build_request(&client, &c, &input).unwrap().build().unwrap();

        assert_eq!(req.method(), reqwest::Method::POST);
        assert!(req.body().is_some());
    }

    #[test]
    fn rejects_nested_values() {
        let client = Client::new();
        let input = json!({ "latitude": 28.6, "longitude": 77.2, "current": ["a", "b"] });

        let err = build_request(&client, &forecast(), &input).unwrap_err();
        assert!(
            matches!(err, RunError::InvalidInput(_)),
            "unexpected: {err:?}"
        );
    }

    #[test]
    fn fills_and_encodes_path_params() {
        let c = with_path("/page/summary/{title}");
        let input = json!({ "title": "New York", "lang": "en" });
        let req = build_request(&Client::new(), &c, &input)
            .unwrap()
            .build()
            .unwrap();

        assert!(
            req.url().path().ends_with("/page/summary/New%20York"),
            "url: {}",
            req.url()
        );
        assert_eq!(req.url().query(), Some("lang=en"));
    }

    #[test]
    fn missing_path_param_is_error() {
        let c = with_path("/page/summary/{title}");
        let err = build_request(&Client::new(), &c, &json!({})).unwrap_err();

        assert!(
            matches!(err, RunError::InvalidInput(_)),
            "unexpected err: {err:?}"
        );
    }

    #[test]
    fn encodes_slashes_in_path_params() {
        let c = with_path("/page/summary/{title}");
        let input = json!({ "title": "a/b" });
        let req = build_request(&Client::new(), &c, &input)
            .unwrap()
            .build()
            .unwrap();

        assert!(
            req.url().path().ends_with("/page/summary/a%2Fb"),
            "url: {}",
            req.url()
        );
    }

    #[tokio::test]
    #[ignore = "hits the real Open-Meteo API"]
    async fn live_open_meteo_forecast() {
        let client = Client::new();
        let input = json!({ "latitude": 28.6, "longitude": 77.2, "current": "temperature_2m" });

        let value = run(&client, &forecast(), &input)
            .await
            .expect("live call should succeed");

        assert!(
            value["current"]["temperature_2m"].is_number(),
            "response: {value}"
        );
    }

    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn mock_forecast(response: ResponseTemplate) -> (MockServer, Connector) {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/forecast"))
            .and(query_param("latitude", "28.6"))
            .respond_with(response)
            .mount(&server)
            .await;

        let mut c = forecast();
        c.request.base_url = server.uri();
        (server, c) // return the server too: if it's dropped it shuts down
    }
    #[tokio::test]
    async fn run_returns_vendor_json() {
        let (_server, c) = mock_forecast(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "current": { "temperature_2m": 30.1 }})),
        )
        .await;

        let input = json!({ "latitude": 28.6, "longitude": 77.2 });
        let value = run(&Client::new(), &c, &input).await.unwrap();

        assert_eq!(value["current"]["temperature_2m"], 30.1);
    }

    #[tokio::test]
    async fn run_maps_error_status() {
        let (_server, c) = mock_forecast(ResponseTemplate::new(503).set_body_string("down")).await;

        let input = json!({ "latitude": 28.6, "longitude": 77.2 });
        let err = run(&Client::new(), &c, &input).await.unwrap_err();

        assert!(
            matches!(err, RunError::Status { status: 503, ref body } if body == "down"),
            "unexpected {err:?}"
        );
    }

    #[tokio::test]
    async fn run_rejects_non_json() {
        let (_server, c) =
            mock_forecast(ResponseTemplate::new(200).set_body_string("<html>")).await;
        let input = json!({ "latitude": 28.6, "longitude": 77.2 });
        let err = run(&Client::new(), &c, &input).await.unwrap_err();

        assert!(
            matches!(err, RunError::InvalidResponse(_)),
            "unexpected {err:?}"
        );
    }
}
