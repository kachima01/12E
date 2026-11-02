//! Data shapes shared with Supabase and the Worker API.

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

/// A PostgREST `numeric` value, which may arrive as a number or a string.
pub fn flexible_amount(value: &Value) -> f64 {
    match value {
        Value::Number(number) => number.as_f64().unwrap_or(0.0),
        Value::String(text) => text.trim().parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

pub fn flexible_f64<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f64, D::Error> {
    Ok(flexible_amount(&Value::deserialize(deserializer)?))
}

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub supabase_url: String,
    pub supabase_key: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SessionUser {
    pub id: String,
    #[serde(default)]
    pub email: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub access_token: String,
    pub refresh_token: String,
    /// Unix seconds.
    pub expires_at: f64,
    pub user: SessionUser,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Profile {
    pub id: String,
    pub full_name: String,
    pub role: String,
    pub status: String,
    #[serde(default)]
    pub created_at: String,
}

impl Profile {
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }

    pub fn is_approved(&self) -> bool {
        self.status == "approved"
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Contribution {
    pub id: String,
    pub user_id: String,
    #[serde(deserialize_with = "flexible_f64")]
    pub amount: f64,
    #[serde(default)]
    pub payment_reference: String,
    pub status: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub confirmed_at: Option<String>,
    /// Contributor name, joined client-side from approved profiles.
    #[serde(skip)]
    pub name: String,
}

impl Contribution {
    pub fn effective_date(&self) -> &str {
        self.confirmed_at.as_deref().unwrap_or(&self.created_at)
    }

    pub fn is_confirmed(&self) -> bool {
        self.status == "confirmed"
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct ClassMessage {
    pub id: String,
    pub user_id: String,
    pub message: String,
    pub created_at: String,
    /// Embedded `profiles(full_name)`: an object, or an array on some PostgREST versions.
    #[serde(default)]
    pub profiles: Value,
}

impl ClassMessage {
    pub fn author(&self) -> Option<String> {
        let profile = match &self.profiles {
            Value::Array(items) => items.first()?,
            other => other,
        };
        profile["full_name"].as_str().map(str::to_string).filter(|name| !name.trim().is_empty())
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct AuditEntry {
    pub id: i64,
    #[serde(default)]
    pub actor_id: Option<String>,
    pub action: String,
    pub target_type: String,
    pub target_id: String,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletBalance {
    #[serde(deserialize_with = "flexible_f64")]
    pub available_balance: f64,
    pub currency: String,
    pub fetched_at: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Overview,
    Activity,
    People,
    Messages,
    Settings,
    Admin,
}

impl Page {
    pub fn title(self) -> &'static str {
        match self {
            Page::Overview => "Overview",
            Page::Activity => "Contributions",
            Page::People => "Classmates",
            Page::Messages => "Messages",
            Page::Settings => "Settings",
            Page::Admin => "Admin desk",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthMode {
    Login,
    Signup,
    Reset,
    UpdatePassword,
}
