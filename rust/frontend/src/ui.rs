//! Small shadcn/ui-style building blocks shared by every page.

use leptos::prelude::*;

use crate::icons;
use crate::state::AppState;

/// A Lucide icon (inline SVG) sized by the `.icon` class.
pub fn icon(name: &'static str) -> impl IntoView {
    view! { <span class="icon" inner_html=icons::svg(name)></span> }
}

/// The 12-petal sunburst used as the 12E mark.
#[component]
pub fn Logo(#[prop(default = 32)] size: u32) -> impl IntoView {
    let petals: String = (0..12)
        .map(|i| format!("<rect x='18' y='2.5' width='4' height='10.5' rx='2' transform='rotate({} 20 20)'/>", i * 30))
        .collect();
    let svg = format!(
        "<svg width='{size}' height='{size}' viewBox='0 0 40 40' fill='currentColor' aria-hidden='true'>{petals}</svg>"
    );
    view! { <span class="logo" inner_html=svg></span> }
}

#[component]
pub fn Spinner() -> impl IntoView {
    view! { <span class="spinner" role="status" aria-label="Loading"></span> }
}

/// Coloured badge for pending / confirmed / rejected / approved states.
pub fn status_badge(status: &str) -> impl IntoView + use<> {
    let variant = match status {
        "confirmed" | "approved" => "badge badge-success",
        "pending" => "badge badge-warning",
        "rejected" => "badge badge-destructive",
        _ => "badge badge-outline",
    };
    let label = match status {
        "confirmed" => "Confirmed",
        "approved" => "Approved",
        "pending" => "Pending",
        "rejected" => "Rejected",
        other => other,
    }
    .to_string();
    view! { <span class=variant>{label}</span> }
}

pub fn avatar(name: &str) -> impl IntoView + use<> {
    let letters = crate::format::initials(name);
    view! { <span class="avatar" aria-hidden="true">{letters}</span> }
}

/// Sonner-style toast in the bottom-right corner.
#[component]
pub fn Toaster() -> impl IntoView {
    let state = expect_context::<AppState>();
    view! {
        <div class="toaster" aria-live="polite">
            {move || {
                state
                    .toast
                    .get()
                    .map(|(_, message)| {
                        view! {
                            <div class="toast" role="status">
                                <span class="toast-icon">{icon("circle-check")}</span>
                                <span class="toast-text">{message}</span>
                                <button
                                    class="btn btn-ghost btn-icon btn-xs"
                                    aria-label="Dismiss"
                                    on:click=move |_| state.toast.set(None)
                                >
                                    {icon("x")}
                                </button>
                            </div>
                        }
                    })
            }}
        </div>
    }
}

/// Centred card layout used by sign-in, waiting and error screens.
#[component]
pub fn CenteredCard(children: Children) -> impl IntoView {
    view! {
        <div class="center-page">
            <div class="center-stack">
                <div class="brand-row">
                    <span class="brand-mark"><Logo size=22 /></span>
                    <div class="brand-text">
                        <strong>"12E Reunion"</strong>
                        <span>"Class of 2017"</span>
                    </div>
                </div>
                <div class="card">{children()}</div>
                <p class="center-footnote">"A private space for David Livingstone Secondary School · Class of 2017"</p>
            </div>
        </div>
    }
}
