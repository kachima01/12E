//! Admin desk: membership requests, payment verification, wallet balance, audit log.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::data;
use crate::format::{format_date, format_date_time, kwacha, kwacha_cents};
use crate::state::{AdminTab, AppState};
use crate::types::{AuditEntry, Contribution, Profile};
use crate::ui::{Spinner, avatar, icon};

fn audit_label(action: &str) -> &'static str {
    match action {
        "membership.approve" => "Approved member",
        "membership.reject" => "Declined membership",
        "contribution.confirm" => "Confirmed contribution",
        "contribution.reject" => "Rejected contribution",
        _ => "Admin action",
    }
}

#[component]
pub fn AdminPage() -> impl IntoView {
    let state = expect_context::<AppState>();
    let admin = state.admin;

    // Reload the full member list and audit log whenever class data changes.
    Effect::new(move |_| {
        state.members.track();
        state.contributions.track();
        spawn_local(async move {
            data::load_admin_profiles(state).await;
            data::load_audit(state).await;
        });
    });
    spawn_local(data::load_wallet(state));

    let awaiting_members = move || {
        admin.all_profiles.with(|p| p.iter().filter(|m| m.status == "pending").cloned().collect::<Vec<Profile>>())
    };
    let awaiting_payments = move || {
        state.contributions.with(|c| c.iter().filter(|c| c.status == "pending").cloned().collect::<Vec<Contribution>>())
    };
    let confirmed_count = move || state.contributions.with(|c| c.iter().filter(|c| c.is_confirmed()).count());
    let confirmed_total =
        move || state.contributions.with(|c| c.iter().filter(|c| c.is_confirmed()).map(|c| c.amount).sum::<f64>());
    let busy = move |id: &str| state.busy_id.with(|busy| busy == id);

    let tab_button = move |tab: AdminTab, label: &'static str, icon_name: &'static str, count: Option<fn(AppState) -> usize>| {
        view! {
            <button class="tabs-trigger" role="tab" class:active={move || admin.tab.get() == tab} on:click=move |_| admin.tab.set(tab)>
                {icon(icon_name)}
                <span>{label}</span>
                {move || count.map(|count| {
                    let n = count(state);
                    view! { <span class="tabs-count">{n}</span> }
                })}
            </button>
        }
    };

    view! {
        <div class="page">
            <div class="page-header">
                <div>
                    <h1 class="page-title">"Admin desk"</h1>
                    <p class="page-description">"Review class registrations and verify reunion fund contributions."</p>
                </div>
                <div class="page-actions">
                    <button class="btn btn-outline" on:click=move |_| {
                        spawn_local(async move {
                            data::refresh_admin(state).await;
                            data::load_wallet(state).await;
                        });
                    }>
                        {icon("refresh-cw")} "Refresh"
                    </button>
                </div>
            </div>

            <section class="grid-cards">
                <div class="card stat-card compact">
                    <span class="card-description">"Members to review"</span>
                    <div class="stat-value">{move || awaiting_members().len()}</div>
                </div>
                <div class="card stat-card compact">
                    <span class="card-description">"Payments to review"</span>
                    <div class="stat-value">{move || awaiting_payments().len()}</div>
                </div>
                <div class="card stat-card compact">
                    <span class="card-description">"Confirmed payments"</span>
                    <div class="stat-value">{confirmed_count}</div>
                </div>
                <div class="card stat-card compact">
                    <span class="card-description">"Class total"</span>
                    <div class="stat-value">{move || kwacha(confirmed_total())}</div>
                </div>
            </section>

            <div class="card">
                <div class="card-header">
                    <div class="tabs-list tabs-scroll" role="tablist">
                        {tab_button(AdminTab::Members, "Membership requests", "users", Some(|s: AppState| s.admin.all_profiles.with(|p| p.iter().filter(|m| m.status == "pending").count())))}
                        {tab_button(AdminTab::Payments, "Payment verification", "credit-card", Some(|s: AppState| s.contributions.with(|c| c.iter().filter(|c| c.status == "pending").count())))}
                        {tab_button(AdminTab::Wallet, "Wallet balance", "wallet", None)}
                        {tab_button(AdminTab::Audit, "Security & audit", "scroll-text", None)}
                    </div>
                </div>
                <div class="card-content">
                    {move || {
                        let message = admin.error.get();
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
                    {move || match admin.tab.get() {
                        AdminTab::Members => {
                            let members = awaiting_members();
                            if members.is_empty() {
                                return view! { <p class="muted empty-line">"No pending membership requests."</p> }.into_any();
                            }
                            view! {
                                <ul class="item-list">
                                    {members
                                        .into_iter()
                                        .map(|member| {
                                            let approve_id = member.id.clone();
                                            let decline_id = member.id.clone();
                                            let busy_id = member.id.clone();
                                            let busy_id2 = member.id.clone();
                                            view! {
                                                <li class="item">
                                                    {avatar(&member.full_name)}
                                                    <div class="item-body">
                                                        <strong>{member.full_name.clone()}</strong>
                                                        <span class="muted small">{format!("Requested {}", format_date(&member.created_at))}</span>
                                                    </div>
                                                    <div class="item-actions">
                                                        <button class="btn btn-primary btn-sm" disabled=move || busy(&busy_id)
                                                            on:click=move |_| spawn_local(data::decide_membership(state, approve_id.clone(), true))>
                                                            {icon("check")} "Approve"
                                                        </button>
                                                        <button class="btn btn-destructive-outline btn-sm" disabled=move || busy(&busy_id2)
                                                            on:click=move |_| spawn_local(data::decide_membership(state, decline_id.clone(), false))>
                                                            "Decline"
                                                        </button>
                                                    </div>
                                                </li>
                                            }
                                        })
                                        .collect_view()}
                                </ul>
                            }
                            .into_any()
                        }
                        AdminTab::Payments => {
                            let payments = awaiting_payments();
                            if payments.is_empty() {
                                return view! { <p class="muted empty-line">"No pending payments to review."</p> }.into_any();
                            }
                            view! {
                                <div class="table-wrap">
                                    <table class="table">
                                        <thead>
                                            <tr>
                                                <th>"Classmate"</th>
                                                <th class="desktop-cell">"Reference"</th>
                                                <th>"Date"</th>
                                                <th class="text-right">"Amount"</th>
                                                <th class="text-right">"Decision"</th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {payments
                                                .into_iter()
                                                .map(|item| {
                                                    let confirm_id = item.id.clone();
                                                    let reject_id = item.id.clone();
                                                    let busy_id = item.id.clone();
                                                    let busy_id2 = item.id.clone();
                                                    view! {
                                                        <tr>
                                                            <td><div class="cell-person">{avatar(&item.name)}<span>{item.name.clone()}</span></div></td>
                                                            <td class="muted mono desktop-cell">{item.payment_reference.clone()}</td>
                                                            <td class="muted">{format_date(&item.created_at)}</td>
                                                            <td class="text-right strong">{kwacha(item.amount)}</td>
                                                            <td class="text-right">
                                                                <div class="item-actions end">
                                                                    <button class="btn btn-primary btn-sm" disabled=move || busy(&busy_id)
                                                                        on:click=move |_| spawn_local(data::decide_contribution(state, confirm_id.clone(), true))>
                                                                        "Confirm"
                                                                    </button>
                                                                    <button class="btn btn-destructive-outline btn-sm" disabled=move || busy(&busy_id2)
                                                                        on:click=move |_| spawn_local(data::decide_contribution(state, reject_id.clone(), false))>
                                                                        "Decline"
                                                                    </button>
                                                                </div>
                                                            </td>
                                                        </tr>
                                                    }
                                                })
                                                .collect_view()}
                                        </tbody>
                                    </table>
                                </div>
                            }
                            .into_any()
                        }
                        AdminTab::Wallet => view! { <WalletPanel /> }.into_any(),
                        AdminTab::Audit => view! { <AuditPanel /> }.into_any(),
                    }}
                </div>
            </div>
        </div>
    }
}

#[component]
fn WalletPanel() -> impl IntoView {
    let state = expect_context::<AppState>();
    let admin = state.admin;
    let confirmed_total =
        move || state.contributions.with(|c| c.iter().filter(|c| c.is_confirmed()).map(|c| c.amount).sum::<f64>());
    view! {
        <div class="wallet">
            <div class="wallet-head">
                <span class="feature-icon">{icon("wallet")}</span>
                <div>
                    <h3 class="card-title">"Lipila wallet"</h3>
                    <p class="card-description">"Live merchant balance"</p>
                </div>
                <button class="btn btn-outline btn-icon" aria-label="Refresh balance" disabled=move || admin.wallet_loading.get()
                    on:click=move |_| spawn_local(data::load_wallet(state))>
                    <span class:spin={move || admin.wallet_loading.get()}>{icon("refresh-cw")}</span>
                </button>
            </div>
            {move || {
                let error = admin.wallet_error.get();
                if !error.is_empty() {
                    return view! {
                        <div class="alert alert-destructive" role="alert">
                            {icon("circle-alert")}
                            <div class="alert-description">{error}</div>
                        </div>
                    }
                    .into_any();
                }
                match admin.wallet.get() {
                    None => view! { <div class="loading-line"><Spinner /> <span class="muted">"Fetching balance from Lipila…"</span></div> }.into_any(),
                    Some(wallet) => view! {
                        <div class="wallet-balance">
                            <span class="muted">{wallet.currency.clone()}</span>
                            <strong>{kwacha_cents(wallet.available_balance).trim_start_matches('K').to_string()}</strong>
                        </div>
                        <dl class="kv kv-grid">
                            <div><dt>"Status"</dt><dd><span class="badge badge-success"><span class="live-dot"></span>"Live"</span></dd></div>
                            <div><dt>"Last updated"</dt><dd>{format_date_time(&wallet.fetched_at)}</dd></div>
                            <div><dt>"Confirmed contributions"</dt><dd>{move || format!("{} payments", state.contributions.with(|c| c.iter().filter(|c| c.is_confirmed()).count()))}</dd></div>
                            <div><dt>"In-app total (confirmed)"</dt><dd>{move || kwacha(confirmed_total())}</dd></div>
                        </dl>
                    }
                    .into_any(),
                }
            }}
            <p class="muted small">"Fetched live from the Lipila merchant API. Your API key never leaves the server."</p>
        </div>
    }
}

#[component]
fn AuditPanel() -> impl IntoView {
    let state = expect_context::<AppState>();
    let admin = state.admin;
    let name_of = move |id: &str| admin.all_profiles.with(|p| p.iter().find(|m| m.id == id).map(|m| m.full_name.clone()));
    let describe = move |entry: &AuditEntry| {
        let actor = entry.actor_id.as_deref().and_then(name_of).unwrap_or_else(|| "Former admin".to_string());
        let target = if entry.target_type == "profile" {
            name_of(&entry.target_id).unwrap_or_else(|| "Member record".to_string())
        } else {
            state
                .contributions
                .with(|c| c.iter().find(|c| c.id == entry.target_id).map(|c| format!("{} · {}", c.name, kwacha(c.amount))))
                .unwrap_or_else(|| "Contribution".to_string())
        };
        format!("{target} · by {actor}")
    };
    view! {
        <div class="stack">
            <div class="alert">
                {icon("shield-check")}
                <div>
                    <div class="alert-title">"Admin activity"</div>
                    <div class="alert-description">"Membership and payment decisions are recorded with the admin identity and timestamp."</div>
                </div>
            </div>
            {move || {
                if admin.audit_loading.get() && admin.audit.with(Vec::is_empty) {
                    return view! { <div class="loading-line"><Spinner /> <span class="muted">"Loading audit history…"</span></div> }.into_any();
                }
                let entries = admin.audit.get();
                if entries.is_empty() {
                    return view! { <p class="muted empty-line">"No admin actions recorded yet. New membership and payment decisions will appear here."</p> }.into_any();
                }
                view! {
                    <ul class="item-list">
                        {entries
                            .into_iter()
                            .map(|entry| {
                                let summary = describe(&entry);
                                view! {
                                    <li class="item">
                                        <span class="item-media">{icon("check")}</span>
                                        <div class="item-body">
                                            <strong>{audit_label(&entry.action)}</strong>
                                            <span class="muted small">{summary}</span>
                                        </div>
                                        <time class="muted small" datetime=entry.created_at.clone()>{format_date(&entry.created_at)}</time>
                                    </li>
                                }
                            })
                            .collect_view()}
                    </ul>
                }
                .into_any()
            }}
            <p class="muted small">"Showing the latest 100 entries. This history is read-only; changes are written by protected database functions."</p>
        </div>
    }
}
