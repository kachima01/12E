//! Contribution history: confirmed class contributions plus the member's own.

use leptos::prelude::*;

use crate::format::{format_date, kwacha};
use crate::pages::contribute::open_contribute;
use crate::state::AppState;
use crate::types::Contribution;
use crate::ui::{avatar, icon, status_badge};

#[component]
pub fn ActivityPage() -> impl IntoView {
    let state = expect_context::<AppState>();
    let filter = RwSignal::new("all");
    let my_id = move || state.profile.with(|p| p.as_ref().map(|p| p.id.clone()).unwrap_or_default());
    let rows = move || {
        let id = my_id();
        let only_mine = filter.get() == "mine";
        state.contributions.with(|items| {
            items
                .iter()
                .filter(|c| c.is_confirmed() || c.user_id == id)
                .filter(|c| !only_mine || c.user_id == id)
                .cloned()
                .collect::<Vec<Contribution>>()
        })
    };

    view! {
        <div class="page">
            <div class="page-header">
                <div>
                    <h1 class="page-title">"Contribution history"</h1>
                    <p class="page-description">"Every confirmed contribution moves our class closer to reunion day."</p>
                </div>
                <div class="page-actions">
                    <button class="btn btn-primary" on:click=move |_| open_contribute(state)>
                        {icon("plus")} "Make a contribution"
                    </button>
                </div>
            </div>
            <div class="card">
                <div class="card-header row">
                    <div>
                        <h2 class="card-title">"Activity"</h2>
                        <p class="card-description">"Your pending and rejected payments are visible only to you."</p>
                    </div>
                    <div class="tabs-list" role="tablist">
                        <button class="tabs-trigger" role="tab" class:active={move || filter.get() == "all"} on:click=move |_| filter.set("all")>"Everyone"</button>
                        <button class="tabs-trigger" role="tab" class:active={move || filter.get() == "mine"} on:click=move |_| filter.set("mine")>"Mine"</button>
                    </div>
                </div>
                <div class="card-content">
                    {move || {
                        let id = my_id();
                        let items = rows();
                        if items.is_empty() {
                            return view! {
                                <div class="empty">
                                    <span class="empty-icon">{icon("sparkles")}</span>
                                    <strong>"No contributions yet"</strong>
                                    <p class="muted">"Be the first to get our reunion fund started!"</p>
                                </div>
                            }
                            .into_any();
                        }
                        view! {
                            <div class="table-wrap">
                                <table class="table">
                                    <thead>
                                        <tr>
                                            <th>"Classmate"</th>
                                            <th>"Date"</th>
                                            <th class="desktop-cell">"Reference"</th>
                                            <th>"Status"</th>
                                            <th class="text-right">"Amount"</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {items
                                            .into_iter()
                                            .map(|item| {
                                                let mine = item.user_id == id;
                                                let name = if mine { "You".to_string() } else { item.name.clone() };
                                                view! {
                                                    <tr>
                                                        <td>
                                                            <div class="cell-person">
                                                                {avatar(&item.name)}
                                                                <span>{name}</span>
                                                            </div>
                                                        </td>
                                                        <td class="muted">{format_date(&item.created_at)}</td>
                                                        <td class="muted mono desktop-cell">{item.payment_reference.clone()}</td>
                                                        <td>{status_badge(&item.status)}</td>
                                                        <td class="text-right strong">{kwacha(item.amount)}</td>
                                                    </tr>
                                                }
                                            })
                                            .collect_view()}
                                    </tbody>
                                </table>
                            </div>
                        }
                        .into_any()
                    }}
                </div>
            </div>
        </div>
    }
}
