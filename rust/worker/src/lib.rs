//! 12E Reunion API Worker: Rust port of `src/worker.js`.
//!
//! Routes (all under `/api/`):
//! - `GET  /api/health`                      service readiness
//! - `GET  /api/config`                      public Supabase client configuration
//! - `POST /api/payment/checkout`            start a Lipila mobile-money collection
//! - `GET  /api/payment/status?reference=`   poll a member's own payment
//! - `POST /api/webhooks/lipila`             signed Lipila callback
//! - `POST /api/admin/contributions/confirm` admin confirmation (+ receipt email)
//! - `POST /api/admin/contributions/reject`  admin rejection
//! - `POST /api/admin/members/status`        approve / decline a membership
//! - `GET  /api/admin/wallet`                live Lipila wallet balance
//!
//! Everything else is served from the static frontend assets.

mod webhook;

use serde::{Deserialize, Deserializer};
use serde_json::{Value, json};
use wasm_bindgen::{JsValue, prelude::wasm_bindgen};
use worker::{
    Context, Env, Fetch, Headers, Method, Request, RequestInit, Response, Url, console_error,
    console_warn, event,
};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = crypto, js_name = randomUUID)]
    fn random_uuid() -> String;
}

const CONTRIBUTION_FIELDS: &str = "id,user_id,amount,currency,payment_reference,status,receipt_sent_at";
const MAX_WEBHOOK_BYTES: usize = 1024 * 1024;
const WEBHOOK_TOLERANCE_SECONDS: i64 = 300;

// ───────────────────────────── errors & responses ─────────────────────────────

/// An unexpected failure. It is logged and the client receives a generic message.
#[derive(Debug)]
struct Failure(String);

impl From<worker::Error> for Failure {
    fn from(error: worker::Error) -> Self {
        Failure(error.to_string())
    }
}

impl From<String> for Failure {
    fn from(message: String) -> Self {
        Failure(message)
    }
}

impl From<&str> for Failure {
    fn from(message: &str) -> Self {
        Failure(message.to_string())
    }
}

type Outcome<T> = std::result::Result<T, Failure>;

fn json_response(body: Value, status: u16) -> Outcome<Response> {
    let headers = Headers::new();
    headers.set("content-type", "application/json; charset=utf-8")?;
    headers.set("cache-control", "no-store")?;
    headers.set("x-content-type-options", "nosniff")?;
    Ok(Response::ok(body.to_string())?
        .with_status(status)
        .with_headers(headers))
}

fn error(message: &str, status: u16) -> Outcome<Response> {
    json_response(json!({ "error": message }), status)
}

// ───────────────────────────── small helpers ─────────────────────────────

/// Read a secret or plain variable, ignoring blank values.
fn setting(env: &Env, key: &str) -> Option<String> {
    let value = env
        .secret(key)
        .map(|value| value.to_string())
        .or_else(|_| env.var(key).map(|value| value.to_string()))
        .ok()?;
    let value = value.trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn enc(value: &str) -> String {
    String::from(js_sys::encode_uri_component(value))
}

fn now_iso() -> String {
    String::from(js_sys::Date::new_0().to_iso_string())
}

fn now_seconds() -> i64 {
    (js_sys::Date::now() / 1000.0).floor() as i64
}

fn truncate(text: &str, max: usize) -> &str {
    match text.char_indices().nth(max) {
        Some((index, _)) => &text[..index],
        None => text,
    }
}

/// JavaScript-style `String(value || "")`: non-empty strings and non-zero numbers.
fn truthy_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) if !text.is_empty() => Some(text.clone()),
        Value::Number(number) if number.as_f64() != Some(0.0) => Some(number.to_string()),
        Value::Bool(true) => Some("true".to_string()),
        _ => None,
    }
}

/// JavaScript-style `Number(value)` for numbers and numeric strings.
fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse::<f64>().ok(),
        _ => None,
    }
    .filter(|value| value.is_finite())
}

fn deserialize_amount<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f64, D::Error> {
    let value = Value::deserialize(deserializer)?;
    Ok(number(&value).unwrap_or(0.0))
}

fn same_cents(left: f64, right: f64) -> bool {
    (left * 100.0).round() == (right * 100.0).round()
}

fn has_whole_cents(amount: f64) -> bool {
    ((amount * 100.0).round() - amount * 100.0).abs() < 1e-6
}

fn looks_like_uuid(value: &str) -> bool {
    value.len() == 36 && value.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// `+260` local part (9 digits starting with 7 or 9) from user input.
fn zambian_local_phone(raw: &str) -> Option<String> {
    let digits: String = raw.chars().filter(char::is_ascii_digit).collect();
    let local = digits
        .strip_prefix("260")
        .or_else(|| digits.strip_prefix('0'))
        .unwrap_or(&digits);
    let valid = local.len() == 9 && matches!(local.as_bytes()[0], b'7' | b'9');
    valid.then(|| local.to_string())
}

fn random_reference() -> String {
    let compact: String = random_uuid().chars().filter(|c| *c != '-').take(20).collect();
    format!("12E-{}", compact.to_uppercase())
}

fn escape_html(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            other => escaped.push(other),
        }
    }
    escaped
}

/// `K1,250.00`, matching `Intl.NumberFormat("en-ZM", { currency: "ZMW" })`.
fn format_kwacha(amount: f64) -> String {
    let cents = (amount.abs() * 100.0).round() as u64;
    let whole = (cents / 100).to_string();
    let mut grouped = String::new();
    for (index, digit) in whole.chars().enumerate() {
        if index > 0 && (whole.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    let sign = if amount < 0.0 { "-" } else { "" };
    format!("{sign}K{grouped}.{:02}", cents % 100)
}

// ───────────────────────────── outbound HTTP ─────────────────────────────

struct HttpResult {
    status: u16,
    text: String,
}

impl HttpResult {
    fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }

    fn json(&self) -> Option<Value> {
        serde_json::from_str(&self.text).ok()
    }
}

async fn http(
    method: Method,
    url: &str,
    headers: &[(&str, &str)],
    body: Option<&Value>,
) -> Outcome<HttpResult> {
    let request_headers = Headers::new();
    for (name, value) in headers {
        request_headers.set(name, value)?;
    }
    let mut init = RequestInit::new();
    init.with_method(method).with_headers(request_headers);
    if let Some(body) = body {
        init.with_body(Some(JsValue::from_str(&body.to_string())));
    }
    let request = Request::new_with_init(url, &init)?;
    let mut response = Fetch::Request(request).send().await?;
    let status = response.status_code();
    let text = response.text().await.unwrap_or_default();
    Ok(HttpResult { status, text })
}

// ───────────────────────────── Supabase ─────────────────────────────

struct Supabase {
    url: String,
    publishable_key: String,
    secret_key: String,
}

#[derive(Default)]
struct Call<'a> {
    /// Bearer token to act as; defaults to the service (secret) key.
    token: Option<&'a str>,
    prefer: Option<&'a str>,
    body: Option<Value>,
    /// Human-readable step name used in error messages.
    operation: Option<&'a str>,
}

#[derive(Clone, Deserialize)]
struct ContributionRow {
    id: String,
    user_id: String,
    #[serde(deserialize_with = "deserialize_amount")]
    amount: f64,
    payment_reference: String,
    status: String,
    #[serde(default)]
    receipt_sent_at: Option<String>,
}

#[derive(Deserialize)]
struct ProfileRow {
    #[serde(default)]
    full_name: String,
    #[serde(default)]
    role: String,
    #[serde(default)]
    status: String,
}

struct AuthUser {
    id: String,
    email: String,
    access_token: String,
}

impl Supabase {
    fn from_env(env: &Env) -> Option<Self> {
        Some(Self {
            url: setting(env, "SUPABASE_URL")?.trim_end_matches('/').to_string(),
            publishable_key: setting(env, "SUPABASE_PUBLISHABLE_KEY")?,
            secret_key: setting(env, "SUPABASE_SECRET_KEY")?,
        })
    }

    async fn call(&self, method: Method, path: &str, call: Call<'_>) -> Outcome<Value> {
        let bearer = format!("Bearer {}", call.token.unwrap_or(&self.secret_key));
        let mut headers = vec![
            ("apikey", self.secret_key.as_str()),
            ("authorization", bearer.as_str()),
            ("content-type", "application/json"),
        ];
        if let Some(prefer) = call.prefer {
            headers.push(("prefer", prefer));
        }
        let url = format!("{}{}", self.url, path);
        let result = http(method, &url, &headers, call.body.as_ref()).await?;
        if !result.ok() {
            console_error!(
                "Supabase request failed: {} {} {}",
                call.operation.unwrap_or(path),
                result.status,
                truncate(&result.text, 500)
            );
            return Err(Failure(match call.operation {
                Some(operation) => format!("Payment setup could not complete: {operation}."),
                None => "The member service could not complete this request.".to_string(),
            }));
        }
        // PostgREST may acknowledge a write with an empty body as well as with 204.
        if result.status == 204 || result.text.trim().is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&result.text).map_err(|_| Failure::from("Supabase returned invalid JSON."))
    }

    async fn get(&self, path: &str) -> Outcome<Value> {
        self.call(Method::Get, path, Call::default()).await
    }

    async fn patch(&self, path: &str, body: Value) -> Outcome<()> {
        self.call(
            Method::Patch,
            path,
            Call { prefer: Some("return=minimal"), body: Some(body), ..Call::default() },
        )
        .await
        .map(|_| ())
    }

    async fn first<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Outcome<Option<T>> {
        let rows = self.get(path).await?;
        match rows.as_array().and_then(|rows| rows.first()) {
            Some(row) => serde_json::from_value(row.clone())
                .map(Some)
                .map_err(|error| Failure(format!("Unexpected Supabase row: {error}"))),
            None => Ok(None),
        }
    }

    /// Verify the caller's Supabase access token.
    async fn authenticate(&self, request: &Request) -> Outcome<Option<AuthUser>> {
        let header = request.headers().get("authorization")?.unwrap_or_default();
        let Some(token) = header.strip_prefix("Bearer ").map(str::trim).filter(|t| !t.is_empty()) else {
            return Ok(None);
        };
        let bearer = format!("Bearer {token}");
        let result = http(
            Method::Get,
            &format!("{}/auth/v1/user", self.url),
            &[("apikey", &self.publishable_key), ("authorization", &bearer)],
            None,
        )
        .await?;
        if !result.ok() {
            console_warn!("Authentication was rejected by Supabase: {}", result.status);
            return Ok(None);
        }
        let user = result.json().unwrap_or(Value::Null);
        let id = user["id"].as_str().unwrap_or_default();
        let email = user["email"].as_str().unwrap_or_default();
        if id.is_empty() || email.is_empty() {
            return Ok(None);
        }
        Ok(Some(AuthUser { id: id.to_string(), email: email.to_string(), access_token: token.to_string() }))
    }

    async fn profile(&self, user_id: &str) -> Outcome<Option<ProfileRow>> {
        self.first(&format!(
            "/rest/v1/profiles?id=eq.{}&select=id,full_name,role,status",
            enc(user_id)
        ))
        .await
    }

    async fn contribution_by_reference(&self, reference: &str) -> Outcome<Option<ContributionRow>> {
        self.first(&format!(
            "/rest/v1/contributions?payment_reference=eq.{}&select={CONTRIBUTION_FIELDS}",
            enc(reference)
        ))
        .await
    }

    async fn contribution_by_id(&self, id: &str) -> Outcome<Option<ContributionRow>> {
        self.first(&format!("/rest/v1/contributions?id=eq.{}&select={CONTRIBUTION_FIELDS}", enc(id)))
            .await
    }

    async fn contribution_by_provider_reference(&self, provider_reference: &str) -> Outcome<Option<ContributionRow>> {
        let details = self
            .get(&format!(
                "/rest/v1/contribution_payment_details?provider_reference=eq.{}&select=contribution_id",
                enc(provider_reference)
            ))
            .await?;
        match details[0]["contribution_id"].as_str() {
            Some(id) => self.contribution_by_id(id).await,
            None => Ok(None),
        }
    }

    async fn mark_rejected_if_pending(&self, filter: &str) -> Outcome<()> {
        self.patch(
            &format!("/rest/v1/contributions?{filter}&status=eq.pending"),
            json!({ "status": "rejected" }),
        )
        .await
    }

    /// Best-effort clean-up after a checkout fails part way through.
    async fn reject_checkout_safely(&self, reference: &str) {
        if let Err(Failure(message)) = self
            .mark_rejected_if_pending(&format!("payment_reference=eq.{}", enc(reference)))
            .await
        {
            console_error!("Could not mark failed checkout as rejected: {message}");
        }
    }

    async fn rpc_as_user(&self, function: &str, token: &str, operation: &str, body: Value) -> Outcome<()> {
        self.call(
            Method::Post,
            &format!("/rest/v1/rpc/{function}"),
            Call { token: Some(token), prefer: Some("return=minimal"), body: Some(body), operation: Some(operation) },
        )
        .await
        .map(|_| ())
    }

    async fn auth_email(&self, user_id: &str) -> Outcome<String> {
        let bearer = format!("Bearer {}", self.secret_key);
        let result = http(
            Method::Get,
            &format!("{}/auth/v1/admin/users/{}", self.url, enc(user_id)),
            &[("apikey", &self.secret_key), ("authorization", &bearer)],
            None,
        )
        .await?;
        if !result.ok() {
            console_error!("Could not retrieve receipt recipient: {}", result.status);
            return Err("Could not retrieve the member email.".into());
        }
        result.json().and_then(|user| user["email"].as_str().map(str::to_string)).filter(|email| !email.is_empty())
            .ok_or_else(|| Failure::from("The member account has no email address."))
    }
}

// ───────────────────────────── receipts (Resend) ─────────────────────────────

async fn send_receipt(env: &Env, sb: &Supabase, contribution: &ContributionRow) -> Outcome<()> {
    let (Some(api_key), Some(from)) = (setting(env, "RESEND_API_KEY"), setting(env, "RESEND_FROM_EMAIL")) else {
        return Err("Resend receipt email is not configured.".into());
    };
    let profile: Option<ProfileRow> = sb
        .first(&format!("/rest/v1/profiles?id=eq.{}&select=full_name", enc(&contribution.user_id)))
        .await?;
    let email = sb.auth_email(&contribution.user_id).await?;
    let name = profile.map(|p| p.full_name).filter(|n| !n.trim().is_empty()).unwrap_or_else(|| "12E classmate".to_string());
    let amount = format_kwacha(contribution.amount);
    let reference = &contribution.payment_reference;
    let (safe_name, safe_amount, safe_reference) = (escape_html(&name), escape_html(&amount), escape_html(reference));

    let text = format!(
        "Hi {name}, your contribution of {amount} to the 12E reunion fund is confirmed. Reference: {reference}. Thank you for helping bring the class together."
    );
    let html = format!(
        r#"<div style="font-family:Arial,sans-serif;color:#26372a;max-width:560px;margin:auto;padding:32px"><div style="font-size:23px;font-weight:700;letter-spacing:-2px;background:#304a37;color:#fff;width:48px;height:42px;text-align:center;padding-top:10px;border-radius:12px">12<span style="color:#d5e784">E</span></div><p style="color:#718258;font-size:11px;letter-spacing:1px;font-weight:bold;margin-top:28px">DAVID LIVINGSTONE SECONDARY · CLASS OF 2017</p><h1 style="font-size:27px;margin:10px 0">Thanks for showing up, {safe_name}.</h1><p style="color:#667067;line-height:1.7">Your contribution to the 12E reunion fund has been confirmed.</p><div style="background:#f2f4ec;border-radius:10px;padding:20px;margin:22px 0"><p style="margin:0;color:#77816f;font-size:12px">CONTRIBUTION</p><p style="font-size:26px;font-weight:bold;margin:7px 0 14px">{safe_amount}</p><p style="font-size:12px;margin:0;color:#667067">Reference: {safe_reference}</p></div><p style="color:#667067;line-height:1.7">Every bit brings the class closer. See you at the reunion.</p><p style="color:#9a9f96;font-size:11px;margin-top:30px">12E Reunion Fund · Still our class. Still showing up.</p></div>"#
    );
    let bearer = format!("Bearer {api_key}");
    let idempotency_key = format!("12e-receipt-{}", contribution.id);
    let result = http(
        Method::Post,
        "https://api.resend.com/emails",
        &[("authorization", &bearer), ("content-type", "application/json"), ("idempotency-key", &idempotency_key)],
        Some(&json!({
            "from": from,
            "to": [email],
            "subject": "Your 12E reunion contribution is confirmed",
            "text": text,
            "html": html,
        })),
    )
    .await?;
    if !result.ok() {
        console_error!("Resend email failed: {} {}", result.status, truncate(&result.text, 500));
        return Err("Resend could not deliver the contribution receipt.".into());
    }
    Ok(())
}

/// Send the receipt once and record that it was sent.
async fn deliver_receipt_once(env: &Env, sb: &Supabase, contribution: &ContributionRow) -> Outcome<()> {
    if contribution.receipt_sent_at.is_some() {
        return Ok(());
    }
    send_receipt(env, sb, contribution).await?;
    sb.patch(
        &format!("/rest/v1/contributions?id=eq.{}", enc(&contribution.id)),
        json!({ "receipt_sent_at": now_iso() }),
    )
    .await
}

// ───────────────────────────── payments ─────────────────────────────

async fn create_checkout(request: &mut Request, env: &Env) -> Outcome<Response> {
    let Some(sb) = Supabase::from_env(env) else {
        console_error!("Checkout is missing required Supabase Worker configuration");
        return error("Payments are not configured yet. Ask the class admin to complete the payment service setup.", 503);
    };
    let Some(user) = sb.authenticate(request).await? else {
        return error("Please sign in again to continue.", 401);
    };
    let (Some(checkout_url), Some(api_key)) = (setting(env, "LIPILA_CHECKOUT_URL"), setting(env, "LIPILA_API_KEY")) else {
        return error("Lipila collection is not configured yet. Ask the class admin to finish payment setup.", 503);
    };
    let Ok(mut gateway) = Url::parse(&checkout_url) else {
        return error("Lipila checkout configuration is invalid.", 503);
    };
    if gateway.scheme() != "https" {
        return error("Lipila checkout must use a secure HTTPS endpoint.", 503);
    }
    // Accept the bare Lipila API base URL and append the collection endpoint.
    if gateway.path() == "/" {
        gateway.set_path("/api/v1/collections/mobile-money");
    }
    let approved = sb.profile(&user.id).await?.is_some_and(|profile| profile.status == "approved");
    if !approved {
        return error("Your membership must be approved before you can contribute.", 403);
    }

    let Ok(body) = request.json::<Value>().await else {
        return error("Invalid request body.", 400);
    };
    let amount = match number(&body["amount"]) {
        Some(amount) if (1.0..=100_000.0).contains(&amount) && has_whole_cents(amount) => {
            (amount * 100.0).round() / 100.0
        }
        _ => return error("Enter a valid amount in ZMW.", 400),
    };
    let phone_input = body["phone"].as_str().map(str::to_string).or_else(|| truthy_text(&body["phone"])).unwrap_or_default();
    let Some(local_phone) = zambian_local_phone(&phone_input) else {
        return error("Enter a valid Zambian mobile money number.", 400);
    };
    let payer_phone = format!("+260{local_phone}");
    let reference = random_reference();

    let created = sb
        .call(
            Method::Post,
            "/rest/v1/contributions?select=id",
            Call {
                prefer: Some("return=representation"),
                operation: Some("create contribution"),
                body: Some(json!({
                    "user_id": user.id,
                    "amount": amount,
                    "currency": "ZMW",
                    "payment_reference": reference,
                    "method": "mobile_money",
                    "status": "pending",
                })),
                ..Call::default()
            },
        )
        .await?;
    // Some PostgREST deployments return an empty body despite `return=representation`;
    // the reference is unique, so look the id up instead.
    let contribution_id = match created[0]["id"].as_str() {
        Some(id) => id.to_string(),
        None => sb
            .call(
                Method::Get,
                &format!("/rest/v1/contributions?payment_reference=eq.{}&select=id", enc(&reference)),
                Call { operation: Some("find newly-created contribution"), ..Call::default() },
            )
            .await?[0]["id"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| Failure::from("Could not create the contribution record."))?,
    };

    if let Err(failure) = sb
        .call(
            Method::Post,
            "/rest/v1/contribution_payment_details",
            Call {
                prefer: Some("return=minimal"),
                operation: Some("save payment details"),
                body: Some(json!({ "contribution_id": contribution_id, "payer_phone": payer_phone })),
                ..Call::default()
            },
        )
        .await
    {
        sb.reject_checkout_safely(&reference).await;
        return Err(failure);
    }

    let callback_url = format!("{}/api/webhooks/lipila", request.url()?.origin().ascii_serialization());
    let payload = json!({
        "referenceId": reference,
        "amount": amount,
        "narration": "12E reunion contribution",
        "accountNumber": format!("260{local_phone}"),
        "currency": "ZMW",
        "email": user.email,
        "referenceData": reference,
    });
    let lipila = match http(
        Method::Post,
        gateway.as_str(),
        &[
            ("accept", "application/json"),
            ("x-api-key", &api_key),
            ("content-type", "application/json"),
            ("callbackUrl", &callback_url),
        ],
        Some(&payload),
    )
    .await
    {
        Ok(result) => result,
        Err(Failure(message)) => {
            console_error!("Lipila collection request failed: {message}");
            sb.reject_checkout_safely(&reference).await;
            return error("Lipila could not be reached. Please try again.", 502);
        }
    };

    let provider = lipila.json();
    let provider_status = provider
        .as_ref()
        .and_then(|body| body["status"].as_str())
        .unwrap_or_default()
        .to_lowercase();
    if !lipila.ok() || provider.is_none() || !matches!(provider_status.as_str(), "pending" | "successful") {
        let provider_message = provider.as_ref().and_then(|body| body["message"].as_str()).map(str::to_string);
        console_error!(
            "Lipila collection rejected: {} {}",
            lipila.status,
            provider_message.clone().unwrap_or_else(|| truncate(&lipila.text, 500).to_string())
        );
        sb.reject_checkout_safely(&reference).await;
        let message = provider_message.filter(|m| !m.is_empty()).unwrap_or_else(|| match lipila.status {
            400 => "Lipila rejected the payment details. Check the mobile number and amount.".to_string(),
            401 => "Lipila rejected the API key. Confirm that the API key matches the test or live checkout URL.".to_string(),
            403 => "Lipila denied access to mobile-money collections for this account.".to_string(),
            429 => "Lipila is receiving too many requests. Please wait a moment and try again.".to_string(),
            status => format!("Lipila could not start this mobile-money payment (HTTP {status})."),
        });
        return error(&message, 502);
    }

    let provider_reference = provider
        .as_ref()
        .and_then(|body| truthy_text(&body["referenceId"]))
        .unwrap_or_else(|| reference.clone())
        .trim()
        .to_string();
    sb.call(
        Method::Patch,
        &format!("/rest/v1/contribution_payment_details?contribution_id=eq.{}", enc(&contribution_id)),
        Call {
            prefer: Some("return=minimal"),
            operation: Some("save Lipila payment reference"),
            body: Some(json!({ "provider_reference": provider_reference })),
            ..Call::default()
        },
    )
    .await?;

    json_response(
        json!({
            "reference": reference,
            "providerReference": provider_reference,
            "status": provider_status,
            "message": "Approve the mobile-money prompt on your phone. The contribution is added after Lipila confirms payment.",
        }),
        201,
    )
}

async fn payment_status(request: &Request, env: &Env) -> Outcome<Response> {
    let Some(sb) = Supabase::from_env(env) else {
        return error("Payments are not configured yet.", 503);
    };
    let Some(user) = sb.authenticate(request).await? else {
        return error("Please sign in again to check payment status.", 401);
    };
    let url = request.url()?;
    let reference = url
        .query_pairs()
        .find(|(key, _)| key == "reference")
        .map(|(_, value)| value.trim().to_string())
        .unwrap_or_default();
    if reference.is_empty() {
        return error("Payment reference is required.", 400);
    }
    match sb.contribution_by_reference(&reference).await? {
        Some(contribution) if contribution.user_id == user.id => {
            json_response(json!({ "status": contribution.status }), 200)
        }
        _ => error("Payment was not found.", 404),
    }
}

async fn lipila_webhook(request: &mut Request, env: &Env) -> Outcome<Response> {
    let Some(primary_secret) = setting(env, "LIPILA_WEBHOOK_SECRET") else {
        return error("Payment webhook is not configured.", 503);
    };
    let Some(sb) = Supabase::from_env(env) else {
        return error("Payment webhook is not configured.", 503);
    };
    let raw_body = request.bytes().await?;
    if raw_body.len() > MAX_WEBHOOK_BYTES {
        return error("Webhook payload is too large.", 413);
    }
    let header = |name: &str| request.headers().get(name).ok().flatten().unwrap_or_default();
    let (webhook_id, timestamp, signatures) =
        (header("webhook-id"), header("webhook-timestamp"), header("webhook-signature"));
    if webhook_id.is_empty()
        || timestamp.is_empty()
        || !timestamp.chars().all(|c| c.is_ascii_digit())
        || signatures.is_empty()
    {
        return error("Missing or invalid Lipila webhook headers.", 400);
    }
    let within_tolerance = timestamp
        .parse::<i64>()
        .is_ok_and(|seconds| (now_seconds() - seconds).abs() <= WEBHOOK_TOLERANCE_SECONDS);
    if !within_tolerance {
        return error("Webhook timestamp is outside the five-minute tolerance.", 401);
    }
    let secrets = [Some(primary_secret), setting(env, "LIPILA_WEBHOOK_SECRET_PREVIOUS")];
    let signature_valid = secrets
        .iter()
        .flatten()
        .any(|secret| webhook::verify_signature(secret, &webhook_id, &timestamp, &raw_body, &signatures));
    if !signature_valid {
        return error("Invalid webhook signature.", 401);
    }

    let Some(event) = std::str::from_utf8(&raw_body).ok().and_then(|text| serde_json::from_str::<Value>(text).ok())
    else {
        return error("Invalid JSON payload.", 400);
    };

    let already_processed = sb
        .get(&format!("/rest/v1/lipila_webhook_events?webhook_id=eq.{}&select=webhook_id", enc(&webhook_id)))
        .await?
        .as_array()
        .is_some_and(|rows| !rows.is_empty());
    if already_processed {
        return json_response(json!({ "ok": true, "duplicate": true }), 200);
    }
    let event_type = truthy_text(&event["type"])
        .or_else(|| truthy_text(&event["typeName"]))
        .unwrap_or_else(|| "collection.callback".to_string());
    let mark_processed = || async {
        sb.call(
            Method::Post,
            "/rest/v1/lipila_webhook_events",
            Call {
                prefer: Some("resolution=ignore-duplicates,return=minimal"),
                body: Some(json!({ "webhook_id": webhook_id, "event_type": event_type })),
                ..Call::default()
            },
        )
        .await
        .map(|_| ())
    };

    let transaction_type = truthy_text(&event["type"]).unwrap_or_else(|| "Collection".to_string()).to_lowercase();
    if transaction_type != "collection" {
        mark_processed().await?;
        return json_response(json!({ "ok": true, "ignored": true }), 200);
    }
    let status = truthy_text(&event["status"]).unwrap_or_default().to_lowercase();
    let provider_reference = truthy_text(&event["referenceId"]).unwrap_or_default().trim().to_string();
    let internal_reference = truthy_text(&event["identifier"])
        .or_else(|| truthy_text(&event["referenceData"]))
        .unwrap_or_default()
        .trim()
        .to_string();
    if provider_reference.is_empty() && internal_reference.is_empty() {
        return error("Missing Lipila collection reference.", 400);
    }

    let mut contribution = None;
    if !internal_reference.is_empty() {
        contribution = sb.contribution_by_reference(&internal_reference).await?;
    }
    if contribution.is_none() && !provider_reference.is_empty() {
        contribution = sb.contribution_by_provider_reference(&provider_reference).await?;
    }
    if contribution.is_none() && !provider_reference.is_empty() {
        contribution = sb.contribution_by_reference(&provider_reference).await?;
    }
    let Some(contribution) = contribution else {
        return error("Contribution not found.", 404);
    };

    if status == "failed" {
        sb.mark_rejected_if_pending(&format!("id=eq.{}", enc(&contribution.id))).await?;
        mark_processed().await?;
        return json_response(json!({ "ok": true, "failed": true }), 200);
    }
    if status != "successful" {
        return error("Unsupported Lipila collection status.", 400);
    }
    if !number(&event["amount"]).is_some_and(|paid| same_cents(paid, contribution.amount)) {
        return error("Payment amount does not match the contribution.", 400);
    }
    if truthy_text(&event["currency"]).is_some_and(|currency| currency.to_uppercase() != "ZMW") {
        return error("Payment currency does not match the contribution.", 400);
    }
    if contribution.status != "confirmed" {
        sb.patch(
            &format!("/rest/v1/contributions?id=eq.{}&status=eq.pending", enc(&contribution.id)),
            json!({ "status": "confirmed", "confirmed_at": now_iso() }),
        )
        .await?;
    }
    if let Err(Failure(message)) = deliver_receipt_once(env, &sb, &contribution).await {
        // Not marked as processed, so Lipila's retry will attempt the receipt again.
        console_error!("Receipt delivery is pending retry: {message}");
        return error("Payment was recorded but the receipt could not be sent yet.", 503);
    }
    mark_processed().await?;
    json_response(json!({ "ok": true }), 200)
}

// ───────────────────────────── admin ─────────────────────────────

/// Authenticate an approved admin, or produce the response that denies access.
async fn admin_gate(request: &Request, env: &Env) -> Outcome<Result<(Supabase, AuthUser), Response>> {
    let Some(sb) = Supabase::from_env(env) else {
        return Ok(Err(error("The member service is not configured.", 503)?));
    };
    let Some(user) = sb.authenticate(request).await? else {
        return Ok(Err(error("Please sign in again.", 401)?));
    };
    let is_admin = sb
        .profile(&user.id)
        .await?
        .is_some_and(|profile| profile.role == "admin" && profile.status == "approved");
    if !is_admin {
        return Ok(Err(error("Admin access is required.", 403)?));
    }
    Ok(Ok((sb, user)))
}

async fn confirm_contribution(request: &mut Request, env: &Env) -> Outcome<Response> {
    let (sb, admin) = match admin_gate(request, env).await? {
        Ok(gate) => gate,
        Err(response) => return Ok(response),
    };
    let Ok(body) = request.json::<Value>().await else {
        return error("Invalid request body.", 400);
    };
    let id = body["id"].as_str().unwrap_or_default().to_string();
    if !looks_like_uuid(&id) {
        return error("Invalid contribution.", 400);
    }
    let Some(contribution) = sb.contribution_by_id(&id).await? else {
        return error("Contribution not found.", 404);
    };
    if contribution.status == "rejected" {
        return error("Rejected contributions cannot be confirmed.", 409);
    }
    if contribution.status != "confirmed" {
        sb.rpc_as_user(
            "admin_set_contribution_status",
            &admin.access_token,
            "record admin contribution confirmation",
            json!({ "p_actor_id": admin.id, "p_contribution_id": id, "p_status": "confirmed" }),
        )
        .await?;
    }
    if let Err(Failure(message)) = deliver_receipt_once(env, &sb, &contribution).await {
        console_error!("Admin-confirmed receipt delivery failed: {message}");
        return json_response(
            json!({
                "ok": true,
                "receiptSent": false,
                "message": "The contribution was confirmed, but email receipt delivery needs configuration.",
            }),
            200,
        );
    }
    json_response(json!({ "ok": true, "receiptSent": true }), 200)
}

async fn reject_contribution(request: &mut Request, env: &Env) -> Outcome<Response> {
    let (sb, admin) = match admin_gate(request, env).await? {
        Ok(gate) => gate,
        Err(response) => return Ok(response),
    };
    let Ok(body) = request.json::<Value>().await else {
        return error("Invalid request body.", 400);
    };
    let id = body["id"].as_str().unwrap_or_default().to_string();
    if !looks_like_uuid(&id) {
        return error("Invalid contribution.", 400);
    }
    sb.rpc_as_user(
        "admin_set_contribution_status",
        &admin.access_token,
        "record admin contribution rejection",
        json!({ "p_actor_id": admin.id, "p_contribution_id": id, "p_status": "rejected" }),
    )
    .await?;
    json_response(json!({ "ok": true }), 200)
}

async fn set_member_status(request: &mut Request, env: &Env) -> Outcome<Response> {
    let (sb, admin) = match admin_gate(request, env).await? {
        Ok(gate) => gate,
        Err(response) => return Ok(response),
    };
    let Ok(body) = request.json::<Value>().await else {
        return error("Invalid request body.", 400);
    };
    let id = body["id"].as_str().unwrap_or_default().to_string();
    let status = body["status"].as_str().unwrap_or_default().to_string();
    if !looks_like_uuid(&id) || !matches!(status.as_str(), "approved" | "rejected") {
        return error("Invalid membership decision.", 400);
    }
    sb.rpc_as_user(
        "admin_set_member_status",
        &admin.access_token,
        "record admin membership decision",
        json!({ "p_actor_id": admin.id, "p_member_id": id, "p_status": status }),
    )
    .await?;
    json_response(json!({ "ok": true }), 200)
}

async fn wallet_balance(request: &Request, env: &Env) -> Outcome<Response> {
    if let Err(response) = admin_gate(request, env).await? {
        return Ok(response);
    }
    let (Some(checkout_url), Some(api_key)) = (setting(env, "LIPILA_CHECKOUT_URL"), setting(env, "LIPILA_API_KEY")) else {
        return error("Lipila is not configured.", 503);
    };
    let Ok(base) = Url::parse(&checkout_url) else {
        return error("Lipila checkout URL is invalid.", 503);
    };
    let balance_url = format!("{}/api/v1/wallet/balance", base.origin().ascii_serialization());
    let result = match http(Method::Get, &balance_url, &[("accept", "application/json"), ("x-api-key", &api_key)], None)
        .await
    {
        Ok(result) => result,
        Err(Failure(message)) => {
            console_error!("Lipila wallet balance request failed: {message}");
            return error("Could not reach Lipila. Please try again.", 502);
        }
    };
    if !result.ok() {
        console_error!("Lipila wallet balance rejected: {} {}", result.status, truncate(&result.text, 500));
        let message = if result.status == 401 {
            "Lipila rejected the API key. Verify the key matches your environment.".to_string()
        } else {
            format!("Could not retrieve wallet balance (HTTP {}).", result.status)
        };
        return error(&message, 502);
    }
    let body = result.json().ok_or_else(|| Failure::from("Lipila returned an invalid wallet response."))?;
    let available = number(&body["availableBalance"]).or_else(|| number(&body["available_balance"])).unwrap_or(0.0);
    let currency = body["currency"].as_str().filter(|c| !c.is_empty()).unwrap_or("ZMW");
    json_response(json!({ "availableBalance": available, "currency": currency, "fetchedAt": now_iso() }), 200)
}

// ───────────────────────────── health & config ─────────────────────────────

fn health(env: &Env) -> Outcome<Response> {
    let all = |keys: &[&str]| keys.iter().all(|key| setting(env, key).is_some());
    json_response(
        json!({
            "ok": true,
            "services": {
                "supabase": all(&["SUPABASE_URL", "SUPABASE_PUBLISHABLE_KEY", "SUPABASE_SECRET_KEY"]),
                "lipila": all(&["LIPILA_CHECKOUT_URL", "LIPILA_API_KEY", "LIPILA_WEBHOOK_SECRET"]),
                "resend": all(&["RESEND_API_KEY", "RESEND_FROM_EMAIL"]),
            },
            "time": now_iso(),
        }),
        200,
    )
}

fn public_config(env: &Env) -> Outcome<Response> {
    json_response(
        json!({
            "supabaseUrl": setting(env, "SUPABASE_URL"),
            "supabasePublishableKey": setting(env, "SUPABASE_PUBLISHABLE_KEY"),
        }),
        200,
    )
}

// ───────────────────────────── router ─────────────────────────────

#[event(fetch)]
pub async fn fetch(mut request: Request, env: Env, _context: Context) -> worker::Result<Response> {
    let path = request.path();
    if !path.starts_with("/api/") {
        return env.assets("ASSETS")?.fetch_request(request).await;
    }

    let (outcome, fallback) = match (request.method(), path.as_str()) {
        (Method::Get, "/api/health") => (health(&env), "Health check failed."),
        (Method::Get, "/api/config") => (public_config(&env), "Configuration is unavailable."),
        (Method::Post, "/api/payment/checkout") => (
            create_checkout(&mut request, &env).await,
            "We could not prepare your payment. Please try again.",
        ),
        (Method::Get, "/api/payment/status") => (
            payment_status(&request, &env).await,
            "Could not check payment status. Please try again.",
        ),
        (Method::Post, "/api/webhooks/lipila") => (
            lipila_webhook(&mut request, &env).await,
            "Payment confirmation is being retried.",
        ),
        (Method::Post, "/api/admin/contributions/confirm") => (
            confirm_contribution(&mut request, &env).await,
            "Could not confirm the contribution.",
        ),
        (Method::Post, "/api/admin/contributions/reject") => (
            reject_contribution(&mut request, &env).await,
            "Could not reject the contribution.",
        ),
        (Method::Post, "/api/admin/members/status") => (
            set_member_status(&mut request, &env).await,
            "Could not update the membership request.",
        ),
        (Method::Get, "/api/admin/wallet") => (
            wallet_balance(&request, &env).await,
            "Could not retrieve wallet balance.",
        ),
        _ => (error("Not found.", 404), "Not found."),
    };

    match outcome {
        Ok(response) => Ok(response),
        Err(Failure(message)) => {
            console_error!("{path} failed: {message}");
            error(fallback, 500).map_err(|Failure(message)| worker::Error::RustError(message))
        }
    }
}
