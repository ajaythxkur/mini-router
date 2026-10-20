use reqwest::{Client, RequestBuilder};
use serde_json::Value;
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
    let url = format!("{}{}", connector.request.base_url, connector.request.path);
    match connector.request.method {
        Method::Post => Ok(client.post(url).json(input)),
        Method::Get => {
            let Some(object) = input.as_object() else {
                return Err(RunError::InvalidInput("input must be JSON object".into()));
            };
            let mut req = client.get(url);
            for (key, value) in object {
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn forecast() -> Connector {
        toml::from_str(include_str!("../connectors/open-meteo/forecast.toml")).unwrap()
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
}
