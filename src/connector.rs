use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Connector {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub categories: Vec<String>,
    pub request: Request,
    pub pricing: Pricing,
    pub input: serde_json::Value,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "UPPERCASE")]
pub enum Method {
    Get,
    Post,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Request {
    pub method: Method,
    pub base_url: String,
    pub path: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Pricing {
    PerCall {
        amount: Decimal,
    },
    PerResult {
        amount: Decimal,
        flat_fee: Option<Decimal>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"
        id = "test#ok"
        display_name = "Test"
        description = "A test connector"
        categories = ["testing"]

        [request]
        method = "GET"
        base_url = "https://example.com"
        path = "/"

        [pricing]
        kind = "per_call"
        amount = "0.001"

        [input]
        type = "object"
    "#;

    #[test]
    fn parse_valid_connector() {
        let c: Connector = toml::from_str(VALID).expect("should parse");

        assert_eq!(c.id, "test#ok");
        assert!(matches!(c.request.method, Method::Get));
        assert!(matches!(c.pricing, Pricing::PerCall { .. }));
    }

    #[test]
    fn rejects_invalid_amount() {
        let bad = VALID.replace(r#"amount = "0.001""#, r#"amount = "invalid""#);
        let result: Result<Connector, _> = toml::from_str(&bad);

        assert!(result.is_err());
    }

    #[test]
    fn parses_per_result_pricing() {
        let per_result = VALID.replace(
            r#"kind = "per_call""#,
            "kind = \"per_result\"\nflat_fee = \"0.002\"",
        );

        let c: Connector = toml::from_str(&per_result).expect("should parse");

        match c.pricing {
            Pricing::PerResult { amount, flat_fee } => {
                assert_eq!(amount.to_string(), "0.001");
                assert_eq!(flat_fee.map(|f| f.to_string()), Some("0.002".to_string()));
            }
            other => panic!("expected PerResult: got {other:?}"),
        }
    }

    #[test]
    fn rejects_unknown_method() {
        let bad = VALID.replace("GET", "DELETE");
        let result: Result<Connector, _> = toml::from_str(&bad);

        assert!(result.is_err());
    }
}
