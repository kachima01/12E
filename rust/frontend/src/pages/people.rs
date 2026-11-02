//! Classmate directory with each member's confirmed total.

use std::collections::HashMap;

use leptos::prelude::*;

use crate::format::kwacha;
use crate::pages::overview::MEMBER_TARGET;
use crate::state::AppState;
use crate::types::Profile;
use crate::ui::{avatar, icon};

#[component]
pub fn PeoplePage() -> impl IntoView {
    let state = expect_context::<AppState>();
    let query = RwSignal::new(String::new());
    let totals = Memo::new(move |_| {
        state.contributions.with(|items| {
            let mut totals: HashMap<String, f64> = HashMap::new();
            for item in items.iter().filter(|c| c.is_confirmed()) {
                *totals.entry(item.user_id.clone()).or_default() += item.amount;
            }
            totals
        })
    });
    let visible = move || {
        let needle = query.get().trim().to_lowercase();
        state.members.with(|members| {
            members
                .iter()
                .filter(|m| needle.is_empty() || m.full_name.to_lowercase().contains(&needle))
                .cloned()
                .collect::<Vec<Profile>>()
        })
    };

    view! {
        <div class="page">
            <div class="page-header">
                <div>
                    <h1 class="page-title">"12E classmates"</h1>
                    <p class="page-description">
                        {move || format!("{} classmates in. David Livingstone Secondary School, Class of 2017.", state.members.with(Vec::len))}
                    </p>
                </div>
                <div class="page-actions">
                    <div class="input-icon">
                        {icon("users")}
                        <input class="input" type="search" placeholder="Search classmates…" aria-label="Search classmates" bind:value=query />
                    </div>
                </div>
            </div>
            {move || {
                let members = visible();
                if members.is_empty() {
                    return view! {
                        <div class="card">
                            <div class="empty">
                                <span class="empty-icon">{icon("users")}</span>
                                <strong>"No classmates found"</strong>
                                <p class="muted">"Try a different name."</p>
                            </div>
                        </div>
                    }
                    .into_any();
                }
                view! {
                    <div class="grid-people">
                        {members
                            .into_iter()
                            .map(|member| {
                                let given = totals.with(|t| t.get(&member.id).copied().unwrap_or(0.0));
                                let progress = ((given / MEMBER_TARGET) * 100.0).clamp(0.0, 100.0);
                                view! {
                                    <div class="card person-card">
                                        <div class="person-top">
                                            {avatar(&member.full_name)}
                                            <div class="item-body">
                                                <strong>{member.full_name.clone()}</strong>
                                                <span class="muted small">"Class of 2017"</span>
                                            </div>
                                            {member.is_admin().then(|| view! { <span class="badge badge-secondary">"Admin"</span> })}
                                        </div>
                                        <div class="person-given">
                                            <span class="muted small">"Given"</span>
                                            <strong>{kwacha(given)}</strong>
                                        </div>
                                        <div class="progress progress-sm">
                                            <div class="progress-indicator" style=format!("width:{progress}%")></div>
                                        </div>
                                    </div>
                                }
                            })
                            .collect_view()}
                    </div>
                }
                .into_any()
            }}
        </div>
    }
}
