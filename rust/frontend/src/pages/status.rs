//! Full-screen states: loading, configuration error, profile error, membership review.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::data;
use crate::state::AppState;
use crate::ui::{CenteredCard, Logo, Spinner, icon};

#[component]
pub fn LoadingScreen(#[prop(into)] message: String) -> impl IntoView {
    view! {
        <div class="center-page">
            <div class="loading-stack">
                <span class="brand-mark brand-mark-lg"><Logo size=28 /></span>
                <Spinner />
                <p class="muted">{message}</p>
            </div>
        </div>
    }
}

#[component]
pub fn ConfigScreen() -> impl IntoView {
    let state = expect_context::<AppState>();
    view! {
        <CenteredCard>
            <div class="card-header">
                <h1 class="card-title">"Setup needed"</h1>
                <p class="card-description">{move || state.boot_error.get()}</p>
            </div>
            <div class="card-content stack">
                <pre class="code-block">"SUPABASE_URL=https://YOUR_PROJECT.supabase.co\nSUPABASE_PUBLISHABLE_KEY=sb_publishable_…"</pre>
                <button class="btn btn-primary w-full" on:click=|_| {
                    if let Some(window) = web_sys::window() {
                        let _ = window.location().reload();
                    }
                }>
                    {icon("refresh-cw")} "Reload"
                </button>
            </div>
        </CenteredCard>
    }
}

#[component]
pub fn ProfileErrorScreen() -> impl IntoView {
    let state = expect_context::<AppState>();
    view! {
        <CenteredCard>
            <div class="card-header">
                <h1 class="card-title">"Could not load your profile"</h1>
                <p class="card-description">{move || state.profile_error.get()}</p>
            </div>
            <div class="card-content stack">
                <button class="btn btn-primary w-full" on:click=move |_| {
                    state.profile_error.set(String::new());
                    spawn_local(data::load_profile(state));
                }>
                    "Try again" {icon("arrow-right")}
                </button>
                <button class="btn btn-ghost w-full" on:click=move |_| spawn_local(data::sign_out(state))>
                    "Sign out"
                </button>
            </div>
        </CenteredCard>
    }
}

#[component]
pub fn WaitingScreen() -> impl IntoView {
    let state = expect_context::<AppState>();
    let rejected = move || state.profile.with(|p| p.as_ref().is_some_and(|p| p.status == "rejected"));
    let name = move || state.profile.with(|p| p.as_ref().map(|p| p.full_name.clone()).unwrap_or_default());
    let checking = RwSignal::new(false);
    view! {
        <CenteredCard>
            <div class="card-header">
                <h1 class="card-title">{move || if rejected() { "Membership update" } else { "Membership in review" }}</h1>
                <p class="card-description">
                    {move || {
                        if rejected() {
                            "Your request was not approved. Reach out to a class admin if this was a mistake."
                        } else {
                            "Your email is verified! A 12E class administrator will approve your profile shortly."
                        }
                    }}
                </p>
            </div>
            <div class="card-content stack">
                <div class="item">
                    <span class="item-media">{icon("user")}</span>
                    <div class="item-body">
                        <strong>{name}</strong>
                        <span class="muted small">"12E · Class of 2017"</span>
                    </div>
                    {move || {
                        if rejected() {
                            view! { <span class="badge badge-destructive">"Not approved"</span> }.into_any()
                        } else {
                            view! { <span class="badge badge-warning">"Pending approval"</span> }.into_any()
                        }
                    }}
                </div>
                {move || {
                    (!rejected())
                        .then(|| {
                            view! {
                                <button
                                    class="btn btn-primary w-full"
                                    disabled=move || checking.get()
                                    on:click=move |_| {
                                        checking.set(true);
                                        spawn_local(async move {
                                            data::load_profile(state).await;
                                            checking.set(false);
                                        });
                                    }
                                >
                                    {move || if checking.get() { "Checking…" } else { "Check approval status" }}
                                </button>
                            }
                        })
                }}
                <button class="btn btn-outline w-full" on:click=move |_| spawn_local(data::sign_out(state))>
                    {icon("log-out")} "Sign out"
                </button>
            </div>
        </CenteredCard>
    }
}
