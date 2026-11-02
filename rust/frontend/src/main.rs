//! 12E Reunion member portal: Leptos (CSR) port of the React app in `src/App.tsx`.

mod api;
mod data;
mod format;
mod icons;
mod pages;
mod realtime;
mod state;
mod types;
mod ui;

use leptos::mount::mount_to_body;
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsValue;

use crate::format::now_secs;
use crate::state::{AppState, Boot};
use crate::types::{AuthMode, Session};

const THEME_KEY: &str = "reunion12e.theme";

// Trunk builds this crate as a binary, so `main` is the wasm entry point.
fn main() {
    console_error_panic_hook::set_once();
    mount_to_body(App);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Screen {
    Loading,
    ConfigError,
    Auth,
    ProfileLoading,
    ProfileError,
    Waiting,
    Portal,
}

#[component]
fn App() -> impl IntoView {
    let state = AppState::new();
    provide_context(state);
    init_theme(state);
    spawn_local(boot(state));

    // Only re-render the top-level screen when its *kind* changes.
    let screen = Memo::new(move |_| {
        match state.boot.get() {
            Boot::Loading => return Screen::Loading,
            Boot::Failed => return Screen::ConfigError,
            Boot::Ready => {}
        }
        if state.session.with(Option::is_none) || state.auth_mode.get() == AuthMode::UpdatePassword {
            return Screen::Auth;
        }
        match state.profile.with(|p| p.as_ref().map(|p| p.is_approved())) {
            None if state.profile_error.with(String::is_empty) => Screen::ProfileLoading,
            None => Screen::ProfileError,
            Some(false) => Screen::Waiting,
            Some(true) => Screen::Portal,
        }
    });

    // Load member data and live updates when the portal opens; stop them when it closes.
    Effect::new(move |previous: Option<Screen>| {
        let current = screen.get();
        if current == Screen::Portal && previous != Some(Screen::Portal) {
            spawn_local(async move {
                data::load_data(state).await;
                data::load_messages(state).await;
            });
            realtime::connect(state);
        } else if current != Screen::Portal && previous == Some(Screen::Portal) {
            realtime::disconnect();
        }
        current
    });

    view! {
        {move || match screen.get() {
            Screen::Loading => view! { <pages::status::LoadingScreen message="Getting your 12E space ready…" /> }.into_any(),
            Screen::ConfigError => view! { <pages::status::ConfigScreen /> }.into_any(),
            Screen::Auth => view! { <pages::auth::AuthScreen /> }.into_any(),
            Screen::ProfileLoading => view! { <pages::status::LoadingScreen message="Preparing your member space…" /> }.into_any(),
            Screen::ProfileError => view! { <pages::status::ProfileErrorScreen /> }.into_any(),
            Screen::Waiting => view! { <pages::status::WaitingScreen /> }.into_any(),
            Screen::Portal => view! { <pages::shell::Portal /> }.into_any(),
        }}
        <ui::Toaster />
    }
}

/// Load configuration, then restore or establish the member session.
async fn boot(state: AppState) {
    if let Err(error) = api::load_config().await {
        state.boot_error.set(error);
        state.boot.set(Boot::Failed);
        return;
    }

    let session = match session_from_email_link(state).await {
        Some(session) => Some(session),
        None => restore_session().await,
    };
    api::store_session(session.as_ref());
    let signed_in = session.is_some();
    state.session.set(session);
    state.boot.set(Boot::Ready);
    if signed_in {
        data::load_profile(state).await;
    }
    spawn_local(keep_session_fresh(state));
}

/// Supabase email links (confirmation, password recovery) return tokens in the URL hash.
async fn session_from_email_link(state: AppState) -> Option<Session> {
    let window = web_sys::window()?;
    let hash = window.location().hash().ok()?;
    let params: Vec<(String, String)> = hash
        .trim_start_matches('#')
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .map(|(key, value)| {
            let value = value.replace('+', " ");
            let decoded = js_sys::decode_uri_component(&value).map(String::from).unwrap_or(value);
            (key.to_string(), decoded)
        })
        .collect();
    let param = |name: &str| params.iter().find(|(key, _)| key == name).map(|(_, value)| value.clone());
    if param("access_token").is_none() && param("error").is_none() {
        return None;
    }
    // Remove the tokens from the address bar.
    if let Ok(history) = window.history() {
        let location = window.location();
        let clean = format!("{}{}", location.pathname().unwrap_or_default(), location.search().unwrap_or_default());
        let _ = history.replace_state_with_url(&JsValue::NULL, "", Some(&clean));
    }
    if let Some(description) = param("error_description").or_else(|| param("error")) {
        state.auth_notice.set(format!("That email link could not be used: {description}"));
        return None;
    }
    let access_token = param("access_token")?;
    let refresh_token = param("refresh_token").unwrap_or_default();
    let expires_at = param("expires_at")
        .and_then(|v| v.parse::<f64>().ok())
        .or_else(|| param("expires_in").and_then(|v| v.parse::<f64>().ok()).map(|v| now_secs() + v))
        .unwrap_or_else(|| now_secs() + 3600.0);
    let user = api::fetch_user(&access_token).await.ok()?;
    if param("type").as_deref() == Some("recovery") {
        state.auth_mode.set(AuthMode::UpdatePassword);
    }
    Some(Session { access_token, refresh_token, expires_at, user })
}

async fn restore_session() -> Option<Session> {
    let session = api::stored_session()?;
    if session.expires_at - now_secs() > 60.0 {
        return Some(session);
    }
    api::refresh_session(&session.refresh_token).await.ok()
}

/// Refresh the token in the background so realtime and requests keep working.
async fn keep_session_fresh(state: AppState) {
    loop {
        gloo_timers::future::TimeoutFuture::new(60_000).await;
        let expiring = state.session.with_untracked(|s| s.as_ref().map(|s| s.expires_at - now_secs() < 300.0));
        if expiring == Some(true) {
            let _ = api::access_token(state, true).await;
        }
    }
}

fn init_theme(state: AppState) {
    let window = web_sys::window();
    let stored = window
        .as_ref()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item(THEME_KEY).ok().flatten());
    let prefers_dark = window
        .as_ref()
        .and_then(|w| w.match_media("(prefers-color-scheme: dark)").ok().flatten())
        .is_some_and(|query| query.matches());
    state.dark.set(match stored.as_deref() {
        Some("dark") => true,
        Some("light") => false,
        _ => prefers_dark,
    });
    Effect::new(move |_| {
        let dark = state.dark.get();
        if let Some(root) = web_sys::window().and_then(|w| w.document()).and_then(|d| d.document_element()) {
            let _ = root.class_list().toggle_with_force("dark", dark);
        }
    });
}

/// Persist an explicit theme choice.
pub fn set_theme(state: AppState, dark: bool) {
    state.dark.set(dark);
    if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = storage.set_item(THEME_KEY, if dark { "dark" } else { "light" });
    }
}
