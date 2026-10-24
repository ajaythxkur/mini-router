use rust_decimal::Decimal;
use serde::Serialize;
use serde_json::json;
use uuid::Uuid;

use crate::connector::{Connector, Pricing};

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Quote {
    pub id: String,
    pub connector_id: String,
    pub amount_usd: Decimal,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Receipt {
    pub provider: String,
    pub reference: String,
    pub amount_usd: Decimal,
}

#[derive(Debug, thiserror::Error)]
pub enum PaymentError {
    #[error("malformed payment proof: {0}")]
    InvalidProof(String),
    #[error("payment not found")]
    NotFound,
    #[error("underpaid: expected {expected} USD, paid {paid} USD")]
    Underpaid { expected: Decimal, paid: Decimal },
    #[error("wrong recipient: {0}")]
    WrongRecipient(String),
    #[error("provider unreachable: {0}")]
    ProviderUnreachable(String),
}

pub trait PaymentProvider: Send + Sync {
    fn name(&self) -> &str;
    fn instructions(&self, quote: &Quote) -> serde_json::Value;
    fn verify(
        &self,
        quote: &Quote,
        proof: &str,
    ) -> impl Future<Output = Result<Receipt, PaymentError>> + Send;
}

pub struct TestProvider;

impl PaymentProvider for TestProvider {
    fn name(&self) -> &str {
        "test"
    }

    fn instructions(&self, quote: &Quote) -> serde_json::Value {
        json!({ "provider": "test", "proof": format!("test-paid:{}", quote.id)})
    }

    async fn verify(&self, quote: &Quote, proof: &str) -> Result<Receipt, PaymentError> {
        let Some(quote_id) = proof.strip_prefix("test-paid:") else {
            return Err(PaymentError::InvalidProof(proof.into()));
        };

        if quote_id != quote.id {
            return Err(PaymentError::NotFound);
        }

        Ok(Receipt {
            provider: "test".into(),
            reference: proof.into(),
            amount_usd: quote.amount_usd,
        })
    }
}

impl Quote {
    /// Upfront price for one run. `None` if the price is only known after running.
    pub fn for_connector(connector: &Connector) -> Option<Quote> {
        match connector.pricing {
            Pricing::PerCall { amount } => Some(Quote {
                id: Uuid::new_v4().to_string(),
                connector_id: connector.id.clone(),
                amount_usd: amount,
            }),
            Pricing::PerResult { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quote() -> Quote {
        Quote {
            id: "q1".into(),
            connector_id: "open-meteo#forecast".into(),
            amount_usd: Decimal::new(1, 3), // 0.001
        }
    }

    #[tokio::test]
    async fn accepts_matching_proof() {
        let quote = quote();
        let receipt = TestProvider.verify(&quote, "test-paid:q1").await.unwrap();
        assert_eq!(receipt.reference, "test-paid:q1");
        assert_eq!(receipt.amount_usd, quote.amount_usd);
    }

    #[tokio::test]
    async fn rejects_proof_for_other_quote() {
        let err = TestProvider
            .verify(&quote(), "test-paid:q2")
            .await
            .unwrap_err();
        assert!(matches!(err, PaymentError::NotFound), "unexpected: {err}");
    }

    #[tokio::test]
    async fn rejects_garbage_proof() {
        let err = TestProvider.verify(&quote(), "garbage").await.unwrap_err();
        assert!(
            matches!(err, PaymentError::InvalidProof(ref proof) if proof == "garbage"),
            "unexpected: {err}"
        );
    }

    fn forecast() -> Connector {
        toml::from_str(include_str!("../../../connectors/open-meteo/forecast.toml")).unwrap()
    }

    #[test]
    fn quotes_per_call_connector() {
        let quote = Quote::for_connector(&forecast()).unwrap();
        assert_eq!(quote.connector_id, "open-meteo#forecast");
        assert_eq!(quote.amount_usd, Decimal::new(1, 3));
    }

    #[test]
    fn quote_ids_are_unique() {
        let q1 = Quote::for_connector(&forecast()).unwrap();
        let q2 = Quote::for_connector(&forecast()).unwrap();
        assert_ne!(q1.id, q2.id);
    }

    #[test]
    fn per_result_has_no_upfront_quote() {
        let mut c = forecast();
        c.pricing = Pricing::PerResult {
            amount: Decimal::new(1, 3),
            flat_fee: None,
        };
        assert!(Quote::for_connector(&c).is_none());
    }
}
