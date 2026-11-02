//! Loading member data and performing member/admin actions.

use std::collections::HashMap;

use leptos::prelude::*;
use leptos::task::spawn_local;
use serde_json::json;

use crate::api;
use crate::format::enc;
use crate::state::AppState;
use crate::types::*;

const PROFILE_FIELDS: &str = "id,full_name,role,status,created_at";
const CONTRIBUTION_FIELDS: &str = "id,user_id,amount,currency,payment_reference,status,method,created_at,confirmed_at";

/// Begin a signed-in session (after sign-in, sign-up or an email link).
pub fn start_session(state: AppState, session: Session) {
    api::store_session(Some(&session));
    state.reset_member_data();
    state.session.set(Some(session));
    spawn_local(load_profile(state));
}

pub async fn sign_out(state: AppState) {
    let token = state.session.get_untracked().map(|s| s.access_token);
    crate::realtime::disconnect();
    api::store_session(None);
    state.session.set(None);
    state.reset_member_data();
    state.auth_mode.set(AuthMode::Login);
    if let Some(token) = token {
        api::sign_out_remote(&token).await;
    }
}

pub async fn load_profile(state: AppState) {
    let Some(session) = state.session.get_untracked() else {
        state.profile.set(None);
        return;
    };
    state.profile_error.set(String::new());
    let path = format!("profiles?id=eq.{}&select={PROFILE_FIELDS}", enc(&session.user.id));
    match api::rest_get::<Vec<Profile>>(state, &path).await {
        Ok(rows) => match rows.into_iter().next() {
            Some(profile) => state.profile.set(Some(profile)),
            None => state
                .profile_error
                .set("Your member profile could not be found. Ask a class admin for help.".to_string()),
        },
        Err(error) => state.profile_error.set(error),
    }
}

/// Contributions and approved classmates (approved members only).
pub async fn load_data(state: AppState) {
    if state.session.get_untracked().is_none() || !state.is_approved() {
        return;
    }
    state.load_error.set(String::new());
    let contributions = api::rest_get_all::<Contribution>(
        state,
        &format!("contributions?select={CONTRIBUTION_FIELDS}&order=created_at.desc,id.desc"),
    )
    .await;
    let members = api::rest_get_all::<Profile>(
        state,
        &format!("profiles?select={PROFILE_FIELDS}&status=eq.approved&order=full_name.asc,id.asc"),
    )
    .await;
    match (contributions, members) {
        (Ok(mut contributions), Ok(members)) => {
            let names: HashMap<&str, &str> =
                members.iter().map(|member| (member.id.as_str(), member.full_name.as_str())).collect();
            for item in &mut contributions {
                item.name = names.get(item.user_id.as_str()).unwrap_or(&"12E classmate").to_string();
            }
            state.contributions.set(contributions);
            state.members.set(members);
        }
        (Err(error), _) | (_, Err(error)) => state.load_error.set(error),
    }
}

pub async fn load_messages(state: AppState) {
    if state.session.get_untracked().is_none() || !state.is_approved() {
        return;
    }
    let path = "class_messages?select=id,user_id,message,created_at,profiles(full_name)&order=created_at.desc&limit=100";
    match api::rest_get::<Vec<ClassMessage>>(state, path).await {
        Ok(messages) => state.messages.set(messages),
        Err(error) => state.load_error.set(error),
    }
}

pub async fn post_message(state: AppState, message: String) -> Result<(), String> {
    let body = json!({ "user_id": state.profile_id(), "message": message });
    api::rest(state, "POST", "class_messages", Some(body), Some("return=minimal")).await?;
    load_messages(state).await;
    Ok(())
}

pub async fn update_name(state: AppState, name: String) -> Result<(), String> {
    let path = format!("profiles?id=eq.{}", enc(&state.profile_id()));
    api::rest(state, "PATCH", &path, Some(json!({ "full_name": name })), Some("return=minimal")).await?;
    load_profile(state).await;
    load_data(state).await;
    Ok(())
}

// ───────────────────────────── admin ─────────────────────────────

pub async fn load_admin_profiles(state: AppState) {
    if !state.is_admin() {
        return;
    }
    match api::rest_get_all::<Profile>(state, &format!("profiles?select={PROFILE_FIELDS}&order=created_at.desc,id.asc")).await
    {
        Ok(profiles) => state.admin.all_profiles.set(profiles),
        Err(error) => state.admin.error.set(error),
    }
}

pub async fn load_audit(state: AppState) {
    if !state.is_admin() {
        return;
    }
    state.admin.audit_loading.set(true);
    let path = "admin_audit_log?select=id,actor_id,action,target_type,target_id,details,created_at&order=created_at.desc&limit=100";
    match api::rest_get::<Vec<AuditEntry>>(state, path).await {
        Ok(entries) => state.admin.audit.set(entries),
        Err(error) => state.admin.error.set(error),
    }
    state.admin.audit_loading.set(false);
}

pub async fn load_wallet(state: AppState) {
    if !state.is_admin() {
        return;
    }
    let admin = state.admin;
    admin.wallet_loading.set(true);
    admin.wallet_error.set(String::new());
    match api::worker(state, "/api/admin/wallet", None).await {
        Ok(value) => match serde_json::from_value::<WalletBalance>(value) {
            Ok(wallet) => admin.wallet.set(Some(wallet)),
            Err(_) => admin.wallet_error.set("Lipila returned an unexpected wallet balance.".to_string()),
        },
        Err(error) => admin.wallet_error.set(error),
    }
    admin.wallet_loading.set(false);
}

pub async fn refresh_admin(state: AppState) {
    load_data(state).await;
    load_admin_profiles(state).await;
    load_audit(state).await;
}

pub async fn decide_membership(state: AppState, id: String, approve: bool) {
    state.busy_id.set(id.clone());
    let status = if approve { "approved" } else { "rejected" };
    match api::worker(state, "/api/admin/members/status", Some(json!({ "id": id, "status": status }))).await {
        Ok(_) => {
            state.toast(if approve { "Member approved." } else { "Membership request declined." });
            refresh_admin(state).await;
        }
        Err(error) => state.toast(error),
    }
    state.busy_id.set(String::new());
}

pub async fn decide_contribution(state: AppState, id: String, confirm: bool) {
    state.busy_id.set(id.clone());
    let path = if confirm { "/api/admin/contributions/confirm" } else { "/api/admin/contributions/reject" };
    match api::worker(state, path, Some(json!({ "id": id }))).await {
        Ok(result) => {
            let message = match (confirm, result["receiptSent"].as_bool()) {
                (true, Some(false)) => "Contribution confirmed. Email receipt delivery needs configuration.",
                (true, _) => "Contribution confirmed.",
                (false, _) => "Contribution declined.",
            };
            state.toast(message);
            refresh_admin(state).await;
        }
        Err(error) => state.toast(error),
    }
    state.busy_id.set(String::new());
}
