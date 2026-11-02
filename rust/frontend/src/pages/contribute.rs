//! Contribution dialog: amount + mobile number, Lipila prompt, live status polling.

use std::sync::atomic::{AtomicU32, Ordering};

use gloo_timers::future::TimeoutFuture;
use leptos::ev::SubmitEvent;
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde_json::json;

use crate::api;
use crate::data;
use crate::format::{enc, kwacha, normalize_phone, now_ms, phone_input};
use crate::state::{AppState, PayPhase};
use crate::ui::{Spinner, icon};

const PRESETS: [u32; 4] = [250, 500, 1000, 2000];
const POLL_INTERVAL_MS: u32 = 2_500;
const POLL_TIMEOUT_MS: f64 = 120_000.0;

pub fn open_contribute(state: AppState) {
    let c = state.contribute;
    c.run_id.update(|run| *run += 1);
    c.amount.set("500".to_string());
    c.phone.set(String::new());
    c.phase.set(PayPhase::Form);
    c.error.set(String::new());
    c.status_warning.set(String::new());
    c.reference.set(String::new());
    c.elapsed.set(0);
    c.open.set(true);
    state.mobile_nav.set(false);
}

fn close(state: AppState) {
    let c = state.contribute;
    if c.phase.get_untracked() == PayPhase::Starting {
        return;
    }
    c.run_id.update(|run| *run += 1);
    c.open.set(false);
}

fn is_busy(phase: PayPhase) -> bool {
    matches!(phase, PayPhase::Starting | PayPhase::Processing)
}

/// Generation of the elapsed-time counter; only the newest counter keeps running.
static TICKER: AtomicU32 = AtomicU32::new(0);

/// Count seconds while the payment is starting or processing.
async fn tick(state: AppState, run: u32) {
    let c = state.contribute;
    let ticker = TICKER.fetch_add(1, Ordering::Relaxed) + 1;
    let started = now_ms();
    c.elapsed.set(0);
    loop {
        TimeoutFuture::new(1_000).await;
        let current = TICKER.load(Ordering::Relaxed) == ticker;
        if !current || c.run_id.get_untracked() != run || !is_busy(c.phase.get_untracked()) {
            return;
        }
        c.elapsed.set(((now_ms() - started) / 1000.0) as u32);
    }
}

async fn wait_for_payment(state: AppState, reference: String, run: u32) {
    let c = state.contribute;
    c.phase.set(PayPhase::Processing);
    spawn_local(tick(state, run));
    let deadline = now_ms() + POLL_TIMEOUT_MS;
    while c.run_id.get_untracked() == run && now_ms() < deadline {
        let path = format!("/api/payment/status?reference={}", enc(&reference));
        match api::worker(state, &path, None).await {
            Ok(result) => {
                if c.run_id.get_untracked() != run {
                    return;
                }
                c.status_warning.set(String::new());
                match result["status"].as_str() {
                    Some("confirmed") => {
                        c.phase.set(PayPhase::Confirmed);
                        spawn_local(data::load_data(state));
                        return;
                    }
                    Some("rejected") => {
                        c.phase.set(PayPhase::Failed);
                        return;
                    }
                    _ => {}
                }
            }
            Err(message) => c.status_warning.set(message),
        }
        TimeoutFuture::new(POLL_INTERVAL_MS).await;
    }
    if c.run_id.get_untracked() == run {
        c.phase.set(PayPhase::Waiting);
    }
}

#[component]
pub fn ContributeDialog() -> impl IntoView {
    let state = expect_context::<AppState>();
    let c = state.contribute;
    let amount_value = move || c.amount.with(|a| a.trim().parse::<f64>().unwrap_or(0.0));

    let keydown = window_event_listener(leptos::ev::keydown, move |event| {
        if event.key() == "Escape" && !is_busy(c.phase.get_untracked()) {
            close(state);
        }
    });
    on_cleanup(move || keydown.remove());

    let submit = move |event: SubmitEvent| {
        event.prevent_default();
        let amount = amount_value();
        if !(1.0..=100_000.0).contains(&amount) {
            c.error.set("Enter an amount between K1 and K100,000.".to_string());
            return;
        }
        let Some(phone) = normalize_phone(&c.phone.get_untracked()) else {
            c.error.set("Enter all 9 digits of your Zambian mobile number.".to_string());
            return;
        };
        c.error.set(String::new());
        c.status_warning.set(String::new());
        c.elapsed.set(0);
        c.phase.set(PayPhase::Starting);
        let run = c.run_id.get_untracked();
        spawn_local(tick(state, run));
        spawn_local(async move {
            let body = json!({ "amount": amount, "phone": phone });
            match api::worker(state, "/api/payment/checkout", Some(body)).await {
                Ok(result) => {
                    let reference = result["reference"].as_str().unwrap_or_default().to_string();
                    if reference.is_empty() {
                        c.error.set("Lipila returned an incomplete collection response.".to_string());
                        c.phase.set(PayPhase::Form);
                        return;
                    }
                    c.reference.set(reference.clone());
                    wait_for_payment(state, reference, run).await;
                }
                Err(message) => {
                    if c.run_id.get_untracked() == run {
                        c.error.set(message);
                        c.phase.set(PayPhase::Form);
                    }
                }
            }
        });
    };

    let title = move || match c.phase.get() {
        PayPhase::Confirmed => "Payment confirmed",
        PayPhase::Failed => "Payment not completed",
        PayPhase::Starting | PayPhase::Processing => "Payment processing",
        PayPhase::Waiting => "Still waiting for confirmation",
        PayPhase::Form => "Contribute to the 12E fund",
    };
    let description = move || match c.phase.get() {
        PayPhase::Confirmed => "Lipila confirmed your payment. Your contribution has been added to the reunion fund.",
        PayPhase::Failed => "Lipila reported that this payment did not go through. No contribution was added.",
        PayPhase::Starting | PayPhase::Processing => {
            "Approve the payment request on your mobile-money phone. We're waiting for Lipila to confirm the result."
        }
        PayPhase::Waiting => "The payment is still pending. Check your phone for the mobile-money prompt.",
        PayPhase::Form => "Choose an amount that works for you. Every contribution helps bring our class back together.",
    };

    view! {
        <div class="dialog-overlay" on:click=move |_| if !is_busy(c.phase.get_untracked()) { close(state) }>
            <div
                class="dialog"
                role="dialog"
                aria-modal="true"
                aria-labelledby="contribute-title"
                on:click=|event| event.stop_propagation()
            >
                <button
                    class="btn btn-ghost btn-icon dialog-close"
                    aria-label="Close"
                    disabled=move || c.phase.get() == PayPhase::Starting
                    on:click=move |_| close(state)
                >
                    {icon("x")}
                </button>
                <div class="dialog-header">
                    <span class="feature-icon">{icon("heart")}</span>
                    <h2 id="contribute-title" class="dialog-title">{title}</h2>
                    <p class="card-description">{description}</p>
                </div>

                {move || match c.phase.get() {
                    PayPhase::Form => view! {
                        <form class="form" on:submit=submit>
                            <div class="field">
                                <label class="label" for="pay-amount">"Amount"</label>
                                <div class="input-group">
                                    <span class="input-addon">"K"</span>
                                    <input id="pay-amount" class="input input-lg" type="number" min="1" max="100000" step="0.01"
                                        aria-label="Contribution amount in Zambian kwacha" required bind:value=c.amount />
                                </div>
                                <div class="toggle-group" role="group" aria-label="Preset amounts">
                                    {PRESETS
                                        .into_iter()
                                        .map(|preset| {
                                            view! {
                                                <button type="button" class="toggle"
                                                    class:active={move || amount_value() == preset as f64}
                                                    on:click=move |_| c.amount.set(preset.to_string())>
                                                    {format!("K{preset}")}
                                                </button>
                                            }
                                        })
                                        .collect_view()}
                                </div>
                            </div>
                            <div class="field">
                                <label class="label" for="pay-phone">"Mobile money number to charge"</label>
                                <div class="input-group">
                                    <span class="input-addon">"+260"</span>
                                    <input id="pay-phone" class="input" type="tel" inputmode="numeric" autocomplete="tel-national"
                                        placeholder="971234567" maxlength="9" pattern="[79][0-9]{8}" required
                                        prop:value=move || c.phone.get()
                                        on:input=move |event| c.phone.set(phone_input(&event_target_value(&event))) />
                                </div>
                                <p class="field-hint">"Enter the 9 digits after +260. A leading 0 is ignored."</p>
                            </div>
                            <div class="summary">
                                <span>"Added to the fund after payment confirmation"</span>
                                <strong>{move || kwacha(amount_value())}</strong>
                            </div>
                            {move || {
                                let message = c.error.get();
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
                            <button class="btn btn-primary btn-lg w-full" type="submit">
                                "Send payment prompt" {icon("arrow-right")}
                            </button>
                            <p class="field-hint text-center">{icon("shield-check")} " Secure mobile-money prompt via Lipila"</p>
                        </form>
                    }
                    .into_any(),
                    PayPhase::Starting | PayPhase::Processing => {
                        let starting = c.phase.get_untracked() == PayPhase::Starting;
                        view! {
                            <div class="payment-progress" role="status" aria-live="polite">
                                <div class="summary">
                                    <span>"Payment amount"</span>
                                    <strong>{kwacha(amount_value())}</strong>
                                </div>
                                <div class="pulse" aria-hidden="true">{icon("smartphone")}</div>
                                <strong class="text-center">{if starting { "Securely contacting Lipila" } else { "Check your mobile-money phone" }}</strong>
                                <p class="muted small text-center">
                                    {if starting {
                                        "We are creating your payment prompt now. Please keep this window open.".to_string()
                                    } else {
                                        format!("A prompt has been sent to +260 {}. Enter your PIN only in the provider prompt.", c.phone.get_untracked())
                                    }}
                                </p>
                                <ol class="steps">
                                    <li class=if starting { "step active" } else { "step done" }>
                                        {if starting { view! { <Spinner /> }.into_any() } else { icon("check").into_any() }}
                                        "Sending secure request"
                                    </li>
                                    <li class=if starting { "step" } else { "step active" }>
                                        {(!starting).then(|| view! { <Spinner /> })}
                                        "Approve on your phone"
                                    </li>
                                    <li class="step">"Confirming contribution"</li>
                                </ol>
                                <div class="row small muted">
                                    <span><span class="live-dot"></span>" Live status check"</span>
                                    <span>{move || format!("{}s elapsed", c.elapsed.get())}</span>
                                </div>
                                {move || {
                                    let warning = c.status_warning.get();
                                    (!warning.is_empty())
                                        .then(|| view! { <p class="field-error">{format!("We could not refresh the status: {warning}")}</p> })
                                }}
                            </div>
                        }
                        .into_any()
                    }
                    PayPhase::Confirmed | PayPhase::Failed => {
                        let ok = c.phase.get_untracked() == PayPhase::Confirmed;
                        view! {
                            <div class="result" class:result-ok=ok class:result-failed=!ok role="status">
                                <span class="result-icon">{if ok { icon("check").into_any() } else { icon("x").into_any() }}</span>
                                <strong>{if ok { format!("{} added", kwacha(amount_value())) } else { "Payment failed".to_string() }}</strong>
                            </div>
                            <button class="btn btn-primary w-full" type="button" on:click=move |_| close(state)>"Done"</button>
                        }
                        .into_any()
                    }
                    PayPhase::Waiting => view! {
                        <div class="result" role="status">
                            <span class="result-icon">{icon("clock")}</span>
                            <p class="muted small">"No final confirmation yet. Your contribution stays pending and isn't counted until it is confirmed."</p>
                            {move || {
                                let warning = c.status_warning.get();
                                (!warning.is_empty()).then(|| view! { <p class="field-error">{warning}</p> })
                            }}
                        </div>
                        <div class="dialog-actions">
                            <button class="btn btn-primary" type="button" on:click=move |_| {
                                let reference = c.reference.get_untracked();
                                let run = c.run_id.get_untracked();
                                spawn_local(wait_for_payment(state, reference, run));
                            }>
                                {icon("refresh-cw")} "Check again"
                            </button>
                            <button class="btn btn-outline" type="button" on:click=move |_| close(state)>"Close"</button>
                        </div>
                    }
                    .into_any(),
                }}
            </div>
        </div>
    }
}
