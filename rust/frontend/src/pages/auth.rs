//! Sign in, create account, forgot password and set-new-password screens.

use leptos::ev::SubmitEvent;
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;
use crate::data;
use crate::state::AppState;
use crate::types::AuthMode;
use crate::ui::{CenteredCard, Spinner, icon};

fn origin() -> String {
    web_sys::window().and_then(|w| w.location().origin().ok()).unwrap_or_default()
}

fn is_rate_limited(message: &str) -> bool {
    let lower = message.to_lowercase();
    lower.contains("rate limit") && lower.contains("email")
}

#[component]
pub fn AuthScreen() -> impl IntoView {
    let state = expect_context::<AppState>();
    let mode = state.auth_mode;
    let name = RwSignal::new(String::new());
    let email = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let show_password = RwSignal::new(false);
    let working = RwSignal::new(false);
    let error = RwSignal::new(String::new());
    let can_resend = RwSignal::new(false);

    let switch_mode = move |next: AuthMode| {
        mode.set(next);
        state.auth_notice.set(String::new());
        error.set(String::new());
        can_resend.set(false);
    };

    let submit = move |event: SubmitEvent| {
        event.prevent_default();
        if working.get_untracked() {
            return;
        }
        let current = mode.get_untracked();
        let full_name = name.get_untracked().trim().to_string();
        let address = email.get_untracked().trim().to_string();
        let secret = password.get_untracked();
        if current == AuthMode::Signup && !(2..=80).contains(&full_name.chars().count()) {
            error.set("Your name must be between 2 and 80 characters.".to_string());
            return;
        }
        working.set(true);
        error.set(String::new());
        can_resend.set(false);
        state.auth_notice.set(String::new());
        spawn_local(async move {
            let result = match current {
                AuthMode::Signup => match api::sign_up(&full_name, &address, &secret, &origin()).await {
                    Ok(Some(session)) => {
                        data::start_session(state, session);
                        Ok(())
                    }
                    Ok(None) => {
                        state.auth_notice.set(
                            "Account created! Check your inbox to confirm your email, then sign in.".to_string(),
                        );
                        mode.set(AuthMode::Login);
                        Ok(())
                    }
                    Err(message) => Err(message),
                },
                AuthMode::Reset => api::send_password_reset(&address, &origin())
                    .await
                    .map(|_| state.auth_notice.set("Password reset instructions sent to your email.".to_string())),
                AuthMode::UpdatePassword => api::update_password(state, &secret).await.map(|_| {
                    state.auth_notice.set("Your password has been updated.".to_string());
                    state.toast("Your password has been updated.");
                    mode.set(AuthMode::Login);
                }),
                AuthMode::Login => api::sign_in(&address, &secret).await.map(|session| data::start_session(state, session)),
            };
            if let Err(message) = result {
                let limited = is_rate_limited(&message);
                can_resend.set(limited && current == AuthMode::Signup);
                error.set(if limited {
                    "Supabase has temporarily limited verification emails. Wait for its email limit to reset before trying again."
                        .to_string()
                } else {
                    message
                });
            }
            working.set(false);
        });
    };

    let resend = move |_| {
        let address = email.get_untracked().trim().to_string();
        working.set(true);
        spawn_local(async move {
            match api::resend_signup_email(&address).await {
                Ok(()) => {
                    error.set(String::new());
                    can_resend.set(false);
                    state.auth_notice.set("Verification email sent. Check your inbox and spam folder.".to_string());
                }
                Err(message) => error.set(if message.to_lowercase().contains("rate limit") {
                    "Email sending is still rate-limited. Wait for Supabase's email limit to reset, then retry.".to_string()
                } else {
                    message
                }),
            }
            working.set(false);
        });
    };

    let title = move || match mode.get() {
        AuthMode::Signup => "Join the class space",
        AuthMode::Reset => "Reset your password",
        AuthMode::UpdatePassword => "Set a new password",
        AuthMode::Login => "Welcome back to 12E",
    };
    let subtitle = move || match mode.get() {
        AuthMode::Signup => "Connect with classmates and support our reunion fund.",
        AuthMode::Reset => "Enter your email to receive a recovery link.",
        AuthMode::UpdatePassword => "Enter your new password below to regain access.",
        AuthMode::Login => "Sign in to see the class fund, activity and members.",
    };
    let submit_label = move || match mode.get() {
        AuthMode::Signup => "Create account",
        AuthMode::Reset => "Send reset link",
        AuthMode::UpdatePassword => "Update password",
        AuthMode::Login => "Sign in",
    };

    view! {
        <CenteredCard>
            <div class="card-header text-center">
                <h1 class="card-title text-xl">{title}</h1>
                <p class="card-description">{subtitle}</p>
            </div>
            <div class="card-content">
                {move || {
                    let notice = state.auth_notice.get();
                    (!notice.is_empty())
                        .then(|| {
                            view! {
                                <div class="alert alert-success" role="status">
                                    {icon("circle-check")}
                                    <div class="alert-description">{notice}</div>
                                </div>
                            }
                        })
                }}
                <form class="form" on:submit=submit>
                    {move || {
                        (mode.get() == AuthMode::Signup)
                            .then(|| {
                                view! {
                                    <div class="field">
                                        <label class="label" for="auth-name">"Full name"</label>
                                        <input
                                            id="auth-name"
                                            class="input"
                                            type="text"
                                            placeholder="e.g. Chanda Mwansa"
                                            autocomplete="name"
                                            maxlength="80"
                                            required
                                            bind:value=name
                                        />
                                    </div>
                                }
                            })
                    }}
                    {move || {
                        (mode.get() != AuthMode::UpdatePassword)
                            .then(|| {
                                view! {
                                    <div class="field">
                                        <label class="label" for="auth-email">"Email"</label>
                                        <input
                                            id="auth-email"
                                            class="input"
                                            type="email"
                                            placeholder="you@example.com"
                                            autocomplete="email"
                                            required
                                            bind:value=email
                                        />
                                    </div>
                                }
                            })
                    }}
                    {move || {
                        (mode.get() != AuthMode::Reset)
                            .then(|| {
                                view! {
                                    <div class="field">
                                        <div class="field-row">
                                            <label class="label" for="auth-password">
                                                {move || if mode.get() == AuthMode::UpdatePassword { "New password" } else { "Password" }}
                                            </label>
                                            {move || {
                                                (mode.get() == AuthMode::Login)
                                                    .then(|| {
                                                        view! {
                                                            <button type="button" class="link small" on:click=move |_| switch_mode(AuthMode::Reset)>
                                                                "Forgot your password?"
                                                            </button>
                                                        }
                                                    })
                                            }}
                                        </div>
                                        <div class="input-group">
                                            <input
                                                id="auth-password"
                                                class="input"
                                                type=move || if show_password.get() { "text" } else { "password" }
                                                placeholder="At least 8 characters"
                                                autocomplete=move || if mode.get() == AuthMode::Login { "current-password" } else { "new-password" }
                                                minlength="8"
                                                required
                                                bind:value=password
                                            />
                                            <button
                                                type="button"
                                                class="input-addon-button"
                                                aria-label=move || if show_password.get() { "Hide password" } else { "Show password" }
                                                on:click=move |_| show_password.update(|shown| *shown = !*shown)
                                            >
                                                {move || if show_password.get() { icon("eye-off").into_any() } else { icon("eye").into_any() }}
                                            </button>
                                        </div>
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
                    {move || {
                        can_resend
                            .get()
                            .then(|| {
                                view! {
                                    <button
                                        type="button"
                                        class="btn btn-outline w-full"
                                        disabled=move || working.get() || email.with(|e| e.trim().is_empty())
                                        on:click=resend
                                    >
                                        {icon("mail")} "Resend verification email"
                                    </button>
                                }
                            })
                    }}
                    <button class="btn btn-primary w-full" type="submit" disabled=move || working.get()>
                        {move || working.get().then(|| view! { <Spinner /> })}
                        {move || if working.get() { "Please wait…" } else { submit_label() }}
                    </button>
                </form>
                <div class="auth-switch">
                    {move || match mode.get() {
                        AuthMode::Signup => view! {
                            "Already have an account? "
                            <button type="button" class="link" on:click=move |_| switch_mode(AuthMode::Login)>"Sign in"</button>
                        }.into_any(),
                        AuthMode::Reset | AuthMode::UpdatePassword => view! {
                            <button type="button" class="link" on:click=move |_| switch_mode(AuthMode::Login)>"Back to sign in"</button>
                        }.into_any(),
                        AuthMode::Login => view! {
                            "New classmate? "
                            <button type="button" class="link" on:click=move |_| switch_mode(AuthMode::Signup)>"Create an account"</button>
                        }.into_any(),
                    }}
                </div>
            </div>
        </CenteredCard>
    }
}
