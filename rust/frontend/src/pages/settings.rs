//! Profile name and password settings.

use leptos::ev::SubmitEvent;
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;
use crate::data;
use crate::state::AppState;
use crate::ui::{Spinner, icon};

#[component]
pub fn SettingsPage() -> impl IntoView {
    let state = expect_context::<AppState>();
    let initial_name = state.profile.with_untracked(|p| p.as_ref().map(|p| p.full_name.clone()).unwrap_or_default());
    let email = state.session.with_untracked(|s| s.as_ref().and_then(|s| s.user.email.clone()).unwrap_or_default());
    let name = RwSignal::new(initial_name);
    let password = RwSignal::new(String::new());
    let confirm = RwSignal::new(String::new());
    let name_busy = RwSignal::new(false);
    let password_busy = RwSignal::new(false);
    let notice = RwSignal::new(String::new());
    let error = RwSignal::new(String::new());

    let save_name = move |event: SubmitEvent| {
        event.prevent_default();
        let value = name.get_untracked().trim().to_string();
        if !(2..=80).contains(&value.chars().count()) {
            error.set("Name must be between 2 and 80 characters.".to_string());
            return;
        }
        name_busy.set(true);
        error.set(String::new());
        notice.set(String::new());
        spawn_local(async move {
            match data::update_name(state, value).await {
                Ok(()) => {
                    notice.set("Your profile name has been updated.".to_string());
                    state.toast("Profile updated.");
                }
                Err(message) => error.set(message),
            }
            name_busy.set(false);
        });
    };

    let change_password = move |event: SubmitEvent| {
        event.prevent_default();
        let next = password.get_untracked();
        if next.chars().count() < 8 {
            error.set("Use at least 8 characters for your new password.".to_string());
            return;
        }
        if next != confirm.get_untracked() {
            error.set("The password confirmation does not match.".to_string());
            return;
        }
        password_busy.set(true);
        error.set(String::new());
        notice.set(String::new());
        spawn_local(async move {
            match api::update_password(state, &next).await {
                Ok(()) => {
                    password.set(String::new());
                    confirm.set(String::new());
                    notice.set("Your password has been changed.".to_string());
                }
                Err(message) => error.set(message),
            }
            password_busy.set(false);
        });
    };

    view! {
        <div class="page page-narrow">
            <div class="page-header">
                <div>
                    <h1 class="page-title">"Settings"</h1>
                    <p class="page-description">"Manage your 12E profile and account security."</p>
                </div>
            </div>
            {move || {
                let message = notice.get();
                (!message.is_empty())
                    .then(|| {
                        view! {
                            <div class="alert alert-success" role="status">
                                {icon("circle-check")}
                                <div class="alert-description">{message}</div>
                            </div>
                        }
                    })
            }}
            {move || {
                let message = error.get();
                (!message.is_empty())
                    .then(|| {
                        view! {
                            <div class="alert alert-destructive" role="alert">
                                {icon("circle-alert")}
                                <div class="alert-description">{message}</div>
                            </div>
                        }
                    })
            }}
            <form class="card" on:submit=save_name>
                <div class="card-header">
                    <h2 class="card-title">"Profile"</h2>
                    <p class="card-description">"Your name appears on the classmate list and message board."</p>
                </div>
                <div class="card-content form">
                    <div class="field">
                        <label class="label" for="profile-name">"Name shown to classmates"</label>
                        <input id="profile-name" class="input" maxlength="80" required bind:value=name />
                    </div>
                    <div class="field">
                        <label class="label" for="profile-email">"Email"</label>
                        <input id="profile-email" class="input" value=email disabled />
                    </div>
                </div>
                <div class="card-footer row-end">
                    <button class="btn btn-primary" type="submit" disabled=move || name_busy.get()>
                        {move || name_busy.get().then(|| view! { <Spinner /> })}
                        {move || if name_busy.get() { "Saving…" } else { "Save profile" }}
                    </button>
                </div>
            </form>
            <form class="card" on:submit=change_password>
                <div class="card-header">
                    <h2 class="card-title">"Password"</h2>
                    <p class="card-description">"Use at least 8 characters. Passwords are managed securely by Supabase Auth."</p>
                </div>
                <div class="card-content form">
                    <div class="field">
                        <label class="label" for="new-password">"New password"</label>
                        <input id="new-password" class="input" type="password" autocomplete="new-password" minlength="8" required bind:value=password />
                    </div>
                    <div class="field">
                        <label class="label" for="confirm-password">"Confirm new password"</label>
                        <input id="confirm-password" class="input" type="password" autocomplete="new-password" minlength="8" required bind:value=confirm />
                    </div>
                </div>
                <div class="card-footer row-end">
                    <button class="btn btn-primary" type="submit" disabled=move || password_busy.get()>
                        {move || password_busy.get().then(|| view! { <Spinner /> })}
                        {move || if password_busy.get() { "Updating…" } else { "Update password" }}
                    </button>
                </div>
            </form>
        </div>
    }
}
