//! Remote inference configuration and calls.
//!
//! The API key lives in the platform credential store (or
//! `POIEWEAVE_INFERENCE_API_KEY` for headless use), never in SQLite. Inference is
//! disabled until a signed-in local operator saves an endpoint + model + key, and
//! every configuration change and call is written to the audit log. The provider
//! contract is OpenAI-compatible chat completions.

use std::time::Instant;

use serde::Serialize;

use crate::auth::{self, SessionState};
use crate::db::Db;
use crate::http::HttpTransport;
use crate::secrets::SecretStore;

pub const SECRET_SERVICE: &str = "com.poieweave.desktop.inference";
pub const SECRET_ACCOUNT: &str = "api-key";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InferenceStatus {
    pub configured: bool,
    pub endpoint: Option<String>,
    pub model: Option<String>,
    pub has_key: bool,
    pub updated_at_ms: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InferenceResult {
    pub text: String,
    pub model: String,
    pub endpoint: String,
    pub latency_ms: i64,
}

pub fn status(db: &Db) -> Result<InferenceStatus, String> {
    let config = db.inference_config()?;
    Ok(match config {
        Some(config) => InferenceStatus {
            configured: true,
            endpoint: Some(config.endpoint),
            model: Some(config.model),
            has_key: config.has_key,
            updated_at_ms: Some(config.updated_at_ms),
        },
        None => InferenceStatus { configured: false, endpoint: None, model: None, has_key: false, updated_at_ms: None },
    })
}

fn validate_endpoint(endpoint: &str) -> Result<String, String> {
    let endpoint = endpoint.trim();
    if endpoint.is_empty() || endpoint.len() > 500 {
        return Err("endpoint must be 1–500 characters".into());
    }
    if endpoint.chars().any(char::is_whitespace) || endpoint.contains('@') {
        return Err("endpoint must be a single URL without credentials".into());
    }
    let lower = endpoint.to_ascii_lowercase();
    if lower.starts_with("https://") {
        return Ok(endpoint.to_string());
    }
    if lower.starts_with("http://127.0.0.1:") || lower.starts_with("http://localhost:") {
        return Ok(endpoint.to_string());
    }
    Err("endpoint must use HTTPS (HTTP is allowed only for localhost)".into())
}

fn completion_url(endpoint: &str) -> String {
    let base = endpoint.trim_end_matches('/');
    if base.ends_with("/chat/completions") {
        base.to_string()
    } else if base.ends_with("/v1") {
        format!("{base}/chat/completions")
    } else {
        format!("{base}/v1/chat/completions")
    }
}

pub fn configure(
    db: &Db,
    secrets: &dyn SecretStore,
    session: &SessionState,
    endpoint: &str,
    model: &str,
    api_key: &str,
) -> Result<InferenceStatus, String> {
    let operator = auth::require(session)?;
    let endpoint = validate_endpoint(endpoint)?;
    let model = model.trim();
    if model.is_empty() || model.len() > 200 {
        return Err("model must be 1–200 characters".into());
    }
    let key = api_key.trim();
    if key.is_empty() || key.len() > 4096 {
        return Err("API key must be 1–4096 characters".into());
    }
    secrets.set(SECRET_SERVICE, SECRET_ACCOUNT, key)?;
    let at = auth::now_ms();
    if let Err(error) = db.save_inference_config(&endpoint, model, true, at) {
        let _ = secrets.delete(SECRET_SERVICE, SECRET_ACCOUNT);
        return Err(error);
    }
    db.append_audit(at, &operator.display_name, "inference.configure", &format!("{endpoint} · {model}"))?;
    status(db)
}

pub fn forget(db: &Db, secrets: &dyn SecretStore, session: &SessionState) -> Result<InferenceStatus, String> {
    let operator = auth::require(session)?;
    secrets.delete(SECRET_SERVICE, SECRET_ACCOUNT)?;
    db.clear_inference_config()?;
    db.append_audit(auth::now_ms(), &operator.display_name, "inference.forget", "configuration removed")?;
    status(db)
}

pub fn complete(
    db: &Db,
    secrets: &dyn SecretStore,
    session: &SessionState,
    transport: &dyn HttpTransport,
    prompt: &str,
) -> Result<InferenceResult, String> {
    let operator = auth::require(session)?;
    let config = db.inference_config()?.ok_or_else(|| "remote inference is not configured".to_string())?;
    if !config.has_key {
        return Err("no API key is configured".into());
    }
    let key = secrets
        .get(SECRET_SERVICE, SECRET_ACCOUNT)?
        .ok_or_else(|| "API key is missing from the credential store".to_string())?;
    let prompt = prompt.trim();
    if prompt.is_empty() || prompt.len() > 4000 {
        return Err("prompt must be 1–4000 characters".into());
    }
    let url = completion_url(&config.endpoint);
    let at = auth::now_ms();
    db.append_audit(
        at,
        &operator.display_name,
        "inference.request",
        &format!("{} · {} · {} chars", config.model, config.endpoint, prompt.chars().count()),
    )?;
    let body = serde_json::json!({
        "model": config.model,
        "messages": [{ "role": "user", "content": prompt }],
        "temperature": 0,
        "max_tokens": 256
    });
    let started = Instant::now();
    let response = transport.post_json(&url, &key, &body.to_string());
    let latency_ms = started.elapsed().as_millis() as i64;
    let response = match response {
        Ok(response) => response,
        Err(error) => {
            db.append_audit(
                auth::now_ms(),
                &operator.display_name,
                "inference.failed",
                &format!("transport · {error}"),
            )?;
            return Err(error);
        }
    };
    if !(200..300).contains(&response.status) {
        let detail = format!(
            "provider returned HTTP {}: {}",
            response.status,
            response.body.chars().take(400).collect::<String>()
        );
        db.append_audit(
            auth::now_ms(),
            &operator.display_name,
            "inference.failed",
            &format!("HTTP {}", response.status),
        )?;
        return Err(detail);
    }
    let parsed: serde_json::Value =
        serde_json::from_str(&response.body).map_err(|error| format!("provider response is not JSON: {error}"))?;
    let text = parsed
        .pointer("/choices/0/message/content")
        .and_then(|value| value.as_str())
        .or_else(|| parsed.pointer("/choices/0/text").and_then(|value| value.as_str()))
        .ok_or_else(|| "provider response has no choices[0].message.content".to_string())?
        .to_string();
    db.append_audit(
        auth::now_ms(),
        &operator.display_name,
        "inference.success",
        &format!("{} · {} chars", config.model, text.chars().count()),
    )?;
    Ok(InferenceResult { text, model: config.model, endpoint: config.endpoint, latency_ms })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{Session, SessionState};
    use crate::http::HttpResponse;
    use crate::secrets::MemorySecretStore;
    use std::sync::Mutex;

    struct MockTransport {
        request: Mutex<Option<(String, String)>>,
    }

    impl HttpTransport for MockTransport {
        fn post_json(&self, url: &str, api_key: &str, body: &str) -> Result<HttpResponse, String> {
            *self.request.lock().unwrap() = Some((url.to_string(), body.to_string()));
            assert_eq!(api_key, "test-key");
            Ok(HttpResponse { status: 200, body: r#"{"choices":[{"message":{"content":"pong"}}]}"#.to_string() })
        }
    }

    fn signed_in_session() -> SessionState {
        SessionState::new(Some(Session { display_name: "Ada".into(), expires_at_ms: auth::now_ms() + 60_000 }))
    }

    #[test]
    fn inference_requires_auth_and_valid_endpoint() {
        let db = Db::open_in_memory().unwrap();
        let secrets = MemorySecretStore::new();
        let session = SessionState::new(None);
        assert!(configure(&db, &secrets, &session, "https://example.test/v1", "m", "k").is_err());
        let session = signed_in_session();
        assert!(configure(&db, &secrets, &session, "http://example.test/v1", "m", "k").is_err());
        assert!(configure(&db, &secrets, &session, "https://example.test/v1", "m", "k").is_ok());
        assert!(status(&db).unwrap().configured);
    }

    #[test]
    fn completion_sends_the_configured_model_and_audits() {
        let db = Db::open_in_memory().unwrap();
        let secrets = MemorySecretStore::new();
        let session = signed_in_session();
        configure(&db, &secrets, &session, "https://example.test/v1", "test-model", "test-key").unwrap();
        let transport = MockTransport { request: Mutex::new(None) };
        let result = complete(&db, &secrets, &session, &transport, "ping").unwrap();
        assert_eq!(result.text, "pong");
        assert_eq!(result.model, "test-model");
        let request = transport.request.lock().unwrap().clone().unwrap();
        assert_eq!(request.0, "https://example.test/v1/chat/completions");
        assert!(request.1.contains("\"model\":\"test-model\""));
        assert!(db.audit(20).unwrap().iter().any(|row| row.event == "inference.success"));
        assert!(forget(&db, &secrets, &session).unwrap().configured == false);
    }
}
