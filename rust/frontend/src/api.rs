//! HTTP clients for Supabase Auth (GoTrue), PostgREST and the Worker API.
//! This replaces `@supabase/supabase-js`, using the same REST endpoints.

use std::sync::OnceLock;

use gloo_net::http::{Request, RequestBuilder, Response};
use leptos::prelude::*;
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};

use crate::format::{enc, now_secs};
use crate::state::AppState;
use crate::types::{Config, Session, SessionUser};

static CONFIG: OnceLock<Config> = OnceLock::new();
const SESSION_KEY: &str = "reunion12e.session";
/// Refresh the access token when it expires within this many seconds.
const REFRESH_MARGIN_SECS: f64 = 60.0;

pub fn config() -> &'static Config {
    CONFIG.get().expect("configuration is loaded during start-up")
}

/// Fetch the public Supabase settings exposed by the Worker.
pub async fn load_config() -> Result<(), String> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct RawConfig {
        supabase_url: Option<String>,
        supabase_publishable_key: Option<String>,
    }
    let response = Request::get("/api/config")
        .send()
        .await
        .map_err(|_| "Could not reach the 12E service. Check your connection and reload.".to_string())?;
    if !response.ok() {
        return Err(format!("The 12E service returned HTTP {}.", response.status()));
    }
    let raw: RawConfig = response
        .json()
        .await
        .map_err(|_| "The 12E service returned an invalid configuration.".to_string())?;
    let url = raw.supabase_url.unwrap_or_default().trim().trim_end_matches('/').to_string();
    let key = raw.supabase_publishable_key.unwrap_or_default().trim().to_string();
    if url.is_empty() || key.is_empty() || url.contains("YOUR_PROJECT") {
        return Err("Supabase is not configured on the Worker. Set SUPABASE_URL and SUPABASE_PUBLISHABLE_KEY.".into());
    }
    let _ = CONFIG.set(Config { supabase_url: url, supabase_key: key });
    Ok(())
}

// ───────────────────────────── responses & errors ─────────────────────────────

fn network_error(_: gloo_net::Error) -> String {
    "Network error. Check your connection and try again.".to_string()
}

/// Pull a readable message out of a GoTrue, PostgREST or Worker error body.
fn error_message(text: &str, status: u16) -> String {
    if let Ok(body) = serde_json::from_str::<Value>(text) {
        for key in ["msg", "error_description", "message", "error"] {
            if let Some(message) = body[key].as_str().filter(|m| !m.trim().is_empty()) {
                return message.to_string();
            }
        }
    }
    format!("The request could not be completed (HTTP {status}).")
}

async fn read_json(response: Response) -> Result<Value, String> {
    let status = response.status();
    let ok = response.ok();
    let text = response.text().await.unwrap_or_default();
    if !ok {
        return Err(error_message(&text, status));
    }
    if text.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(&text).map_err(|_| "Received an invalid response.".to_string())
}

async fn send(builder: RequestBuilder, body: Option<&Value>) -> Result<Response, String> {
    match body {
        Some(body) => builder.json(body).map_err(|e| e.to_string())?.send().await,
        None => builder.send().await,
    }
    .map_err(network_error)
}

// ───────────────────────────── session storage ─────────────────────────────

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

pub fn stored_session() -> Option<Session> {
    let raw = storage()?.get_item(SESSION_KEY).ok()??;
    serde_json::from_str(&raw).ok()
}

pub fn store_session(session: Option<&Session>) {
    let Some(storage) = storage() else { return };
    match session.and_then(|s| serde_json::to_string(s).ok()) {
        Some(raw) => {
            let _ = storage.set_item(SESSION_KEY, &raw);
        }
        None => {
            let _ = storage.remove_item(SESSION_KEY);
        }
    }
}

// ───────────────────────────── Supabase Auth ─────────────────────────────

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    #[serde(default)]
    expires_in: Option<f64>,
    #[serde(default)]
    expires_at: Option<f64>,
    user: SessionUser,
}

impl TokenResponse {
    fn into_session(self) -> Session {
        let expires_at = self.expires_at.unwrap_or_else(|| now_secs() + self.expires_in.unwrap_or(3600.0));
        Session { access_token: self.access_token, refresh_token: self.refresh_token, expires_at, user: self.user }
    }
}

fn auth_url(path: &str) -> String {
    format!("{}/auth/v1/{path}", config().supabase_url)
}

fn with_auth_headers(builder: RequestBuilder, token: Option<&str>) -> RequestBuilder {
    let builder = builder.header("apikey", &config().supabase_key);
    match token {
        Some(token) => builder.header("Authorization", &format!("Bearer {token}")),
        None => builder,
    }
}

async fn auth_post(path: &str, body: Value, token: Option<&str>) -> Result<Value, String> {
    let builder = with_auth_headers(Request::post(&auth_url(path)), token);
    read_json(send(builder, Some(&body)).await?).await
}

fn parse_session(value: Value) -> Result<Session, String> {
    serde_json::from_value::<TokenResponse>(value)
        .map(TokenResponse::into_session)
        .map_err(|_| "Supabase returned an unexpected sign-in response.".to_string())
}

pub async fn sign_in(email: &str, password: &str) -> Result<Session, String> {
    parse_session(auth_post("token?grant_type=password", json!({ "email": email, "password": password }), None).await?)
}

/// Returns a session when the project auto-confirms new accounts.
pub async fn sign_up(name: &str, email: &str, password: &str, redirect_to: &str) -> Result<Option<Session>, String> {
    let body = json!({ "email": email, "password": password, "data": { "full_name": name } });
    let response = auth_post(&format!("signup?redirect_to={}", enc(redirect_to)), body, None).await?;
    Ok(parse_session(response).ok())
}

pub async fn send_password_reset(email: &str, redirect_to: &str) -> Result<(), String> {
    auth_post(&format!("recover?redirect_to={}", enc(redirect_to)), json!({ "email": email }), None).await.map(|_| ())
}

pub async fn resend_signup_email(email: &str) -> Result<(), String> {
    auth_post("resend", json!({ "type": "signup", "email": email }), None).await.map(|_| ())
}

pub async fn refresh_session(refresh_token: &str) -> Result<Session, String> {
    parse_session(auth_post("token?grant_type=refresh_token", json!({ "refresh_token": refresh_token }), None).await?)
}

pub async fn sign_out_remote(access_token: &str) {
    let _ = auth_post("logout", json!({}), Some(access_token)).await;
}

/// Look up the user that owns an access token (used for email-link sign-in).
pub async fn fetch_user(access_token: &str) -> Result<SessionUser, String> {
    let builder = with_auth_headers(Request::get(&auth_url("user")), Some(access_token));
    let value = read_json(send(builder, None).await?).await?;
    serde_json::from_value(value).map_err(|_| "Supabase returned an unexpected user.".to_string())
}

pub async fn update_password(state: AppState, password: &str) -> Result<(), String> {
    let token = access_token(state, false).await?;
    let builder = with_auth_headers(Request::put(&auth_url("user")), Some(&token));
    read_json(send(builder, Some(&json!({ "password": password }))).await?).await.map(|_| ())
}

/// A valid access token, refreshing it when it is about to expire (or when forced).
pub async fn access_token(state: AppState, force_refresh: bool) -> Result<String, String> {
    let session = state.session.get_untracked().ok_or_else(|| "Please sign in again to continue.".to_string())?;
    if !force_refresh && session.expires_at - now_secs() > REFRESH_MARGIN_SECS {
        return Ok(session.access_token);
    }
    match refresh_session(&session.refresh_token).await {
        Ok(next) => {
            store_session(Some(&next));
            let token = next.access_token.clone();
            // Ignore the result if the member signed out while refreshing.
            if state.session.get_untracked().is_some() {
                state.session.set(Some(next));
                crate::realtime::update_token(&token);
            }
            Ok(token)
        }
        Err(_) => Err("Your session has expired. Please sign in again.".to_string()),
    }
}

// ───────────────────────────── PostgREST & Worker ─────────────────────────────

/// Call PostgREST as the signed-in member (row-level security applies).
pub async fn rest(
    state: AppState,
    method: &str,
    path: &str,
    body: Option<Value>,
    prefer: Option<&str>,
) -> Result<Value, String> {
    let url = format!("{}/rest/v1/{path}", config().supabase_url);
    let mut retried = false;
    loop {
        let token = access_token(state, retried).await?;
        let builder = match method {
            "POST" => Request::post(&url),
            "PATCH" => Request::patch(&url),
            _ => Request::get(&url),
        };
        let mut builder = with_auth_headers(builder, Some(&token));
        if let Some(prefer) = prefer {
            builder = builder.header("Prefer", prefer);
        }
        let response = send(builder, body.as_ref()).await?;
        if response.status() == 401 && !retried {
            retried = true;
            continue;
        }
        return read_json(response).await;
    }
}

pub async fn rest_get<T: DeserializeOwned>(state: AppState, path: &str) -> Result<T, String> {
    let value = rest(state, "GET", path, None, None).await?;
    serde_json::from_value(value).map_err(|error| format!("Unexpected data from Supabase: {error}"))
}

/// Fetch every row of a PostgREST query, 1000 at a time.
pub async fn rest_get_all<T: DeserializeOwned>(state: AppState, path: &str) -> Result<Vec<T>, String> {
    const PAGE: usize = 1000;
    let mut rows = Vec::new();
    loop {
        let page: Vec<T> = rest_get(state, &format!("{path}&limit={PAGE}&offset={}", rows.len())).await?;
        let count = page.len();
        rows.extend(page);
        if count < PAGE {
            return Ok(rows);
        }
    }
}

/// Call the Worker API with the member's access token (GET without body, POST with).
pub async fn worker(state: AppState, path: &str, body: Option<Value>) -> Result<Value, String> {
    let mut retried = false;
    loop {
        let token = access_token(state, retried).await?;
        let builder = if body.is_some() { Request::post(path) } else { Request::get(path) };
        let builder = builder.header("Authorization", &format!("Bearer {token}"));
        let response = send(builder, body.as_ref()).await?;
        // A displayed session can hold a token that has only just expired.
        if response.status() == 401 && !retried {
            retried = true;
            continue;
        }
        return read_json(response).await;
    }
}
