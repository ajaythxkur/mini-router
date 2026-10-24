use rust_decimal::Decimal;
use serde::Serialize;
use serde_json::json;

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
        assert_eq!(receipt.reference, quote.id);
        assert_eq!(quote.amount_usd, receipt.amount_usd);
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
}
