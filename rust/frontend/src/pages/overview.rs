//! Overview dashboard: stat cards, fund chart, goal progress, latest contributions.

use leptos::prelude::*;

use crate::format::{MONTHS, first_name, format_date, kwacha, parse_date};
use crate::pages::contribute::open_contribute;
use crate::state::AppState;
use crate::types::{Contribution, Page};
use crate::ui::{avatar, icon};

pub const MEMBER_TARGET: f64 = 500.0;

#[derive(Clone)]
struct MonthBucket {
    label: &'static str,
    year: i32,
    month: i32,
    amount: f64,
}

/// The last six calendar months (oldest first) and the confirmed total before them.
fn monthly_totals(confirmed: &[Contribution]) -> (Vec<MonthBucket>, f64) {
    let today = js_sys::Date::new_0();
    let current = today.get_full_year() as i32 * 12 + today.get_month() as i32;
    let mut buckets: Vec<MonthBucket> = (0..6)
        .map(|index| {
            let total = current - (5 - index);
            MonthBucket { label: MONTHS[total.rem_euclid(12) as usize], year: total.div_euclid(12), month: total.rem_euclid(12), amount: 0.0 }
        })
        .collect();
    let first = buckets[0].year * 12 + buckets[0].month;
    let mut before = 0.0;
    for item in confirmed {
        let Some(date) = parse_date(item.effective_date()) else { continue };
        let key = date.get_full_year() as i32 * 12 + date.get_month() as i32;
        if key < first {
            before += item.amount;
        } else if let Some(bucket) = buckets.iter_mut().find(|b| b.year * 12 + b.month == key) {
            bucket.amount += item.amount;
        }
    }
    (buckets, before)
}

/// Cumulative area chart as an SVG string (shadcn chart styling via CSS variables).
fn chart_svg(buckets: &[MonthBucket], before: f64, scale: f64) -> (String, Vec<(f64, f64, String)>) {
    let mut cumulative = before;
    let points: Vec<(f64, f64, String)> = buckets
        .iter()
        .enumerate()
        .map(|(index, bucket)| {
            cumulative += bucket.amount;
            let x = index as f64 / (buckets.len().max(2) - 1) as f64 * 100.0;
            let y = 92.0 - (cumulative / scale.max(1.0)) * 78.0;
            (x, y, format!("{}: {} confirmed (total {})", bucket.label, kwacha(bucket.amount), kwacha(cumulative)))
        })
        .collect();
    let line: String = points
        .iter()
        .enumerate()
        .map(|(index, (x, y, _))| format!("{}{x:.2},{y:.2}", if index == 0 { "M" } else { " L" }))
        .collect();
    let svg = format!(
        "<svg viewBox='0 0 100 100' preserveAspectRatio='none' aria-hidden='true'>\
           <defs><linearGradient id='fund-fill' x1='0' y1='0' x2='0' y2='1'>\
             <stop offset='5%' stop-color='var(--chart-1)' stop-opacity='0.35'/>\
             <stop offset='95%' stop-color='var(--chart-1)' stop-opacity='0.02'/>\
           </linearGradient></defs>\
           <g class='chart-grid'><line x1='0' x2='100' y1='14' y2='14'/><line x1='0' x2='100' y1='40' y2='40'/><line x1='0' x2='100' y1='66' y2='66'/><line x1='0' x2='100' y1='92' y2='92'/></g>\
           <path d='{line} L100,100 L0,100 Z' fill='url(#fund-fill)'/>\
           <path d='{line}' fill='none' stroke='var(--chart-1)' stroke-width='2' vector-effect='non-scaling-stroke' stroke-linejoin='round' stroke-linecap='round'/>\
         </svg>"
    );
    (svg, points)
}

#[component]
pub fn OverviewPage() -> impl IntoView {
    let state = expect_context::<AppState>();
    let greeting = move || state.profile.with(|p| first_name(p.as_ref().map(|p| p.full_name.as_str()).unwrap_or("")));
    let my_id = move || state.profile.with(|p| p.as_ref().map(|p| p.id.clone()).unwrap_or_default());

    let confirmed = Memo::new(move |_| {
        let mut items: Vec<Contribution> =
            state.contributions.with(|all| all.iter().filter(|c| c.is_confirmed()).cloned().collect());
        items.sort_by(|a, b| b.effective_date().cmp(a.effective_date()));
        items
    });
    let raised = Memo::new(move |_| confirmed.with(|items| items.iter().map(|c| c.amount).sum::<f64>()));
    let member_count = move || state.members.with(Vec::len);
    let target = move || member_count() as f64 * MEMBER_TARGET;
    let own_total = move || {
        let id = my_id();
        confirmed.with(|items| items.iter().filter(|c| c.user_id == id).map(|c| c.amount).sum::<f64>())
    };
    let own_count = move || {
        let id = my_id();
        confirmed.with(|items| items.iter().filter(|c| c.user_id == id).count())
    };
    let pending = move || {
        let id = my_id();
        state.contributions.with(|items| items.iter().filter(|c| c.user_id == id && c.status == "pending").count())
    };
    let percent = move || {
        let goal = target();
        if goal > 0.0 { ((raised.get() / goal) * 100.0).round().min(100.0) as u32 } else { 0 }
    };
    let this_month = move || confirmed.with(|items| monthly_totals(items).0.last().map(|b| b.amount).unwrap_or(0.0));

    view! {
        <div class="page">
            <div class="page-header">
                <div>
                    <h1 class="page-title">{move || format!("Hello, {}!", greeting())}</h1>
                    <p class="page-description">"Here's how the 12E reunion fund is coming along."</p>
                </div>
                <div class="page-actions">
                    <button class="btn btn-outline" on:click=move |_| state.page.set(Page::Activity)>
                        {icon("credit-card")} "View activity"
                    </button>
                    <button class="btn btn-primary" on:click=move |_| open_contribute(state)>
                        {icon("plus")} "Contribute"
                    </button>
                </div>
            </div>

            <section class="grid-cards">
                <div class="card stat-card">
                    <div class="stat-head">
                        <span class="card-description">"Total raised"</span>
                        <span class="badge badge-outline">{icon("trending-up")} {move || format!("{}%", percent())}</span>
                    </div>
                    <div class="stat-value">{move || kwacha(raised.get())}</div>
                    <div class="stat-foot">
                        <strong>{move || format!("{} this month", kwacha(this_month()))}</strong>
                        <span class="muted">"Confirmed contributions only"</span>
                    </div>
                </div>
                <div class="card stat-card">
                    <div class="stat-head">
                        <span class="card-description">"Class target"</span>
                        <span class="badge badge-outline">{icon("target")} "K500 each"</span>
                    </div>
                    <div class="stat-value">{move || kwacha(target())}</div>
                    <div class="stat-foot">
                        <strong>{move || format!("{} classmates × K500", member_count())}</strong>
                        <span class="muted">"Grows as members are approved"</span>
                    </div>
                </div>
                <button class="card stat-card card-button" on:click=move |_| state.page.set(Page::People)>
                    <div class="stat-head">
                        <span class="card-description">"Classmates joined"</span>
                        <span class="badge badge-outline">{icon("users")} "12E"</span>
                    </div>
                    <div class="stat-value">{move || format!("{:02}", member_count())}</div>
                    <div class="stat-foot">
                        <strong>"Approved members"</strong>
                        <span class="muted">"Open the classmate directory"</span>
                    </div>
                </button>
                <div class="card stat-card">
                    <div class="stat-head">
                        <span class="card-description">"Your contributions"</span>
                        <span class="badge badge-outline">{icon("heart")} {move || own_count()}</span>
                    </div>
                    <div class="stat-value">{move || kwacha(own_total())}</div>
                    <div class="stat-foot">
                        <strong>{move || if own_total() >= MEMBER_TARGET { "Target reached. Thank you!" } else { "Thank you for showing up" }}</strong>
                        <span class="muted">{move || {
                            let left = (MEMBER_TARGET - own_total()).max(0.0);
                            if left > 0.0 { format!("{} to your K500 target", kwacha(left)) } else { "Every extra kwacha helps".to_string() }
                        }}</span>
                    </div>
                </div>
            </section>

            {move || {
                let count = pending();
                (count > 0)
                    .then(|| {
                        view! {
                            <div class="alert alert-warning" role="status">
                                {icon("clock")}
                                <div>
                                    <div class="alert-title">
                                        {if count == 1 {
                                            "Your contribution is being confirmed with Lipila.".to_string()
                                        } else {
                                            format!("{count} contributions are awaiting confirmation.")
                                        }}
                                    </div>
                                    <div class="alert-description">"It is added to the fund as soon as the payment is confirmed."</div>
                                </div>
                            </div>
                        }
                    })
            }}

            <section class="grid-main">
                <div class="card span-2">
                    <div class="card-header row">
                        <div>
                            <h2 class="card-title">"Reunion fund progress"</h2>
                            <p class="card-description">"Cumulative confirmed contributions · last 6 months"</p>
                        </div>
                        <span class="badge badge-success">
                            {icon("check")}
                            {move || match percent() {
                                100.. => "Goal reached",
                                1..=99 => "In progress",
                                _ => "Getting started",
                            }}
                        </span>
                    </div>
                    <div class="card-content">
                        {move || {
                            let (buckets, before) = confirmed.with(|items| monthly_totals(items));
                            let scale = raised.get().max(target());
                            let (svg, points) = chart_svg(&buckets, before, scale);
                            let summary = buckets
                                .iter()
                                .map(|b| format!("{} {}", b.label, kwacha(b.amount)))
                                .collect::<Vec<_>>()
                                .join(", ");
                            view! {
                                <div class="chart" role="img" aria-label=format!("Monthly confirmed totals: {summary}")>
                                    <div class="chart-plot">
                                        <div class="chart-svg" inner_html=svg></div>
                                        {points
                                            .into_iter()
                                            .map(|(x, y, title)| {
                                                view! {
                                                    <span class="chart-dot" style=format!("left:{x}%;top:{y}%") title=title></span>
                                                }
                                            })
                                            .collect_view()}
                                    </div>
                                    <div class="chart-labels">
                                        {buckets.into_iter().map(|b| view! { <span>{b.label}</span> }).collect_view()}
                                    </div>
                                </div>
                            }
                        }}
                    </div>
                </div>

                <div class="card">
                    <div class="card-header">
                        <h2 class="card-title">"Class goal"</h2>
                        <p class="card-description">"K500 per approved classmate"</p>
                    </div>
                    <div class="card-content stack">
                        <div class="goal-figure">
                            <span class="goal-percent">{move || format!("{}%", percent())}</span>
                            <span class="muted small">"of the class target"</span>
                        </div>
                        <div class="progress" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow=move || percent().to_string()>
                            <div class="progress-indicator" style=move || format!("width:{}%", percent())></div>
                        </div>
                        <dl class="kv">
                            <div><dt>"Raised"</dt><dd>{move || kwacha(raised.get())}</dd></div>
                            <div><dt>"Target"</dt><dd>{move || kwacha(target())}</dd></div>
                            <div><dt>"Remaining"</dt><dd>{move || kwacha((target() - raised.get()).max(0.0))}</dd></div>
                        </dl>
                    </div>
                </div>
            </section>

            <section class="grid-main">
                <div class="card span-2">
                    <div class="card-header row">
                        <div>
                            <h2 class="card-title">"Latest contributions"</h2>
                            <p class="card-description">"Celebrating every step forward."</p>
                        </div>
                        <button class="btn btn-ghost btn-sm" on:click=move |_| state.page.set(Page::Activity)>
                            "View all" {icon("arrow-right")}
                        </button>
                    </div>
                    <div class="card-content">
                        {move || {
                            let recent: Vec<Contribution> = confirmed.with(|items| items.iter().take(5).cloned().collect());
                            if recent.is_empty() {
                                view! {
                                    <div class="empty">
                                        <span class="empty-icon">{icon("sparkles")}</span>
                                        <strong>"No contributions yet"</strong>
                                        <p class="muted">"Be the first to help start the reunion fund."</p>
                                        <button class="btn btn-outline btn-sm" on:click=move |_| open_contribute(state)>"Make a contribution"</button>
                                    </div>
                                }
                                .into_any()
                            } else {
                                view! {
                                    <ul class="item-list">
                                        {recent
                                            .into_iter()
                                            .map(|item| {
                                                view! {
                                                    <li class="item">
                                                        {avatar(&item.name)}
                                                        <div class="item-body">
                                                            <strong>{item.name.clone()}</strong>
                                                            <span class="muted small">{format_date(item.effective_date())}</span>
                                                        </div>
                                                        <span class="amount-positive">{format!("+{}", kwacha(item.amount))}</span>
                                                    </li>
                                                }
                                            })
                                            .collect_view()}
                                    </ul>
                                }
                                .into_any()
                            }
                        }}
                    </div>
                </div>

                <div class="stack">
                    <div class="card card-accent">
                        <div class="card-header">
                            <span class="feature-icon">{icon("smartphone")}</span>
                            <h2 class="card-title">"Add funds by mobile money"</h2>
                            <p class="card-description">"MTN, Airtel and Zamtel. Approve the secure prompt on your phone."</p>
                        </div>
                        <div class="card-footer">
                            <button class="btn btn-primary w-full" on:click=move |_| open_contribute(state)>
                                "Add funds securely" {icon("arrow-right")}
                            </button>
                        </div>
                    </div>
                    <div class="card">
                        <div class="card-header">
                            <span class="feature-icon">{icon("fingerprint")}</span>
                            <h2 class="card-title">"Keep your account safe"</h2>
                            <p class="card-description">"Update your password and profile details."</p>
                        </div>
                        <div class="card-footer">
                            <button class="btn btn-outline w-full" on:click=move |_| state.page.set(Page::Settings)>
                                {icon("settings")} "Account settings"
                            </button>
                        </div>
                    </div>
                </div>
            </section>
        </div>
    }
}
