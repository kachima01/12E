//! Signed-in layout: shadcn-style sidebar, top bar and page outlet.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::data;
use crate::format::initials;
use crate::pages::contribute::{ContributeDialog, open_contribute};
use crate::pages::{activity, admin, messages, overview, people, settings};
use crate::state::AppState;
use crate::types::Page;
use crate::ui::{Logo, icon};

fn nav_button(
    state: AppState,
    page: Page,
    icon_name: &'static str,
    badge: impl Fn() -> usize + Copy + Send + Sync + 'static,
) -> impl IntoView {
    view! {
        <button
            class="sidebar-item"
            class:active={move || state.page.get() == page}
            aria-current=move || (state.page.get() == page).then_some("page")
            on:click=move |_| {
                state.page.set(page);
                state.mobile_nav.set(false);
            }
        >
            {icon(icon_name)}
            <span class="sidebar-item-label">{page.title()}</span>
            {move || {
                let count = badge();
                (count > 0).then(|| view! { <span class="sidebar-badge">{count}</span> })
            }}
        </button>
    }
}

#[component]
pub fn Portal() -> impl IntoView {
    let state = expect_context::<AppState>();
    let my_id = move || state.profile.with(|p| p.as_ref().map(|p| p.id.clone()).unwrap_or_default());
    let my_pending = move || {
        let id = my_id();
        state.contributions.with(|items| items.iter().filter(|c| c.user_id == id && c.status == "pending").count())
    };
    let all_pending = move || state.contributions.with(|items| items.iter().filter(|c| c.status == "pending").count());
    let is_admin = move || state.profile.with(|p| p.as_ref().is_some_and(|p| p.is_admin()));
    let full_name = move || state.profile.with(|p| p.as_ref().map(|p| p.full_name.clone()).unwrap_or_default());
    let none = || 0usize;

    view! {
        <div class="shell">
            <aside class="sidebar" class:open={move || state.mobile_nav.get()} aria-label="Main navigation">
                <div class="sidebar-header">
                    <button class="sidebar-brand" on:click=move |_| state.page.set(Page::Overview)>
                        <span class="brand-mark"><Logo size=20 /></span>
                        <span class="brand-text">
                            <strong>"12E Reunion"</strong>
                            <span>"Class of 2017"</span>
                        </span>
                    </button>
                    <button
                        class="btn btn-ghost btn-icon sidebar-close"
                        aria-label="Close menu"
                        on:click=move |_| state.mobile_nav.set(false)
                    >
                        {icon("x")}
                    </button>
                </div>
                <nav class="sidebar-content">
                    <div class="sidebar-group">
                        <div class="sidebar-group-label">"Workspace"</div>
                        {nav_button(state, Page::Overview, "layout-dashboard", none)}
                        {nav_button(state, Page::Activity, "credit-card", my_pending)}
                        {nav_button(state, Page::Messages, "message-square", none)}
                        {nav_button(state, Page::People, "users", none)}
                    </div>
                    {move || {
                        is_admin()
                            .then(|| {
                                view! {
                                    <div class="sidebar-group">
                                        <div class="sidebar-group-label">"Administration"</div>
                                        {nav_button(state, Page::Admin, "shield-check", all_pending)}
                                    </div>
                                }
                            })
                    }}
                    <div class="sidebar-group">
                        <div class="sidebar-group-label">"Account"</div>
                        {nav_button(state, Page::Settings, "settings", none)}
                    </div>
                </nav>
                <div class="sidebar-cta">
                    <p class="sidebar-cta-title">"Every classmate counts"</p>
                    <p class="muted small">"K500 per approved member brings 12E back together."</p>
                    <button class="btn btn-primary btn-sm w-full" on:click=move |_| open_contribute(state)>
                        {icon("plus")} "Contribute"
                    </button>
                </div>
                <div class="sidebar-footer">
                    <span class="avatar">{move || initials(&full_name())}</span>
                    <div class="sidebar-user">
                        <strong>{full_name}</strong>
                        <span>{move || if is_admin() { "Class admin" } else { "Member" }}</span>
                    </div>
                    <button
                        class="btn btn-ghost btn-icon"
                        aria-label="Sign out"
                        title="Sign out"
                        on:click=move |_| spawn_local(data::sign_out(state))
                    >
                        {icon("log-out")}
                    </button>
                </div>
            </aside>
            {move || {
                state
                    .mobile_nav
                    .get()
                    .then(|| view! { <div class="scrim" on:click=move |_| state.mobile_nav.set(false)></div> })
            }}

            <div class="inset">
                <header class="topbar">
                    <button
                        class="btn btn-ghost btn-icon mobile-only"
                        aria-label="Open menu"
                        on:click=move |_| state.mobile_nav.set(true)
                    >
                        {icon("menu")}
                    </button>
                    <div class="breadcrumb">
                        <span class="muted desktop-only">"12E Reunion"</span>
                        <span class="muted desktop-only">{icon("chevron-right")}</span>
                        <strong>{move || state.page.get().title()}</strong>
                    </div>
                    <div class="topbar-actions">
                        <button
                            class="btn btn-ghost btn-icon"
                            aria-label="Toggle dark mode"
                            title="Toggle dark mode"
                            on:click=move |_| crate::set_theme(state, !state.dark.get_untracked())
                        >
                            {move || if state.dark.get() { icon("sun").into_any() } else { icon("moon").into_any() }}
                        </button>
                        <button
                            class="btn btn-ghost btn-icon notify-button"
                            aria-label="Notifications"
                            on:click=move |_| {
                                state.toast(if my_pending() > 0 {
                                    "You have a contribution awaiting confirmation."
                                } else {
                                    "You're all caught up!"
                                })
                            }
                        >
                            {icon("bell")}
                            {move || (my_pending() > 0).then(|| view! { <span class="dot"></span> })}
                        </button>
                        <button class="btn btn-primary btn-sm" on:click=move |_| open_contribute(state)>
                            {icon("plus")}
                            <span class="desktop-only">"Contribute"</span>
                        </button>
                    </div>
                </header>

                <main class="main">
                    {move || {
                        let message = state.load_error.get();
                        (!message.is_empty())
                            .then(|| {
                                view! {
                                    <div class="alert alert-destructive" role="alert">
                                        {icon("circle-alert")}
                                        <div class="alert-description">{message}</div>
                                        <button class="btn btn-outline btn-sm" on:click=move |_| spawn_local(data::load_data(state))>
                                            "Retry"
                                        </button>
                                    </div>
                                }
                            })
                    }}
                    {move || match state.page.get() {
                        Page::Overview => view! { <overview::OverviewPage /> }.into_any(),
                        Page::Activity => view! { <activity::ActivityPage /> }.into_any(),
                        Page::People => view! { <people::PeoplePage /> }.into_any(),
                        Page::Messages => view! { <messages::MessagesPage /> }.into_any(),
                        Page::Settings => view! { <settings::SettingsPage /> }.into_any(),
                        Page::Admin if state.is_admin() => view! { <admin::AdminPage /> }.into_any(),
                        Page::Admin => view! { <overview::OverviewPage /> }.into_any(),
                    }}
                </main>
            </div>

            {move || state.contribute.open.get().then(|| view! { <ContributeDialog /> })}
        </div>
    }
}
