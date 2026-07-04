//! Request gate: optional HTTP Basic auth plus the API-key check that protects
//! the `/api/*` surface.

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use base64::Engine;

use super::Shared;

pub(crate) async fn auth_layer(
    State(state): State<Shared>,
    headers: HeaderMap,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    let path = request.uri().path().to_owned();
    let query = request.uri().query().unwrap_or_default().to_owned();

    let Some((user, pass)) = &state.config.auth else {
        if api_key_allowed(&state.config.api_key, &headers, &query, &path) {
            return next.run(request).await;
        }
        return unauthorized("API key required.");
    };

    if let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Basic "))
        .and_then(|b64| base64::engine::general_purpose::STANDARD.decode(b64).ok())
        .and_then(|bytes| String::from_utf8(bytes).ok())
    {
        if let Some((u, p)) = value.split_once(':') {
            if constant_eq(u, user) && constant_eq(p, pass) {
                if api_key_allowed(&state.config.api_key, &headers, &query, &path) {
                    return next.run(request).await;
                }
                return unauthorized("API key required.");
            }
        }
    }

    unauthorized("Authentication required.")
}

fn unauthorized(message: &'static str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Basic realm=\"webcoder\"")],
        message,
    )
        .into_response()
}

fn api_key_allowed(expected: &Option<String>, headers: &HeaderMap, query: &str, path: &str) -> bool {
    if !path.starts_with("/api/") && path != "/api" {
        return true;
    }
    let Some(expected) = expected else {
        return true;
    };
    let header_key = headers.get("x-webcoder-key").and_then(|v| v.to_str().ok());
    let query_key = query.split('&').find_map(|part| {
        let (key, value) = part.split_once('=')?;
        (key == "webcoder_key").then_some(value)
    });
    header_key
        .or(query_key)
        .is_some_and(|value| constant_eq(value, expected))
}

/// Compare in time independent of where the first mismatch is, so a matching
/// prefix can't be discovered via response timing.
fn constant_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}
