//! Class message board.

use leptos::ev::SubmitEvent;
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::data;
use crate::format::format_date_time;
use crate::state::AppState;
use crate::ui::{Spinner, avatar, icon};

const MAX_LENGTH: usize = 500;

#[component]
pub fn MessagesPage() -> impl IntoView {
    let state = expect_context::<AppState>();
    let draft = RwSignal::new(String::new());
    let sending = RwSignal::new(false);
    let error = RwSignal::new(String::new());
    let my_id = move || state.profile.with(|p| p.as_ref().map(|p| p.id.clone()).unwrap_or_default());

    let submit = move |event: SubmitEvent| {
        event.prevent_default();
        let text = draft.get_untracked().trim().to_string();
        let length = text.chars().count();
        if length == 0 || length > MAX_LENGTH {
            error.set("Messages must be between 1 and 500 characters.".to_string());
            return;
        }
        sending.set(true);
        error.set(String::new());
        spawn_local(async move {
            match data::post_message(state, text).await {
                Ok(()) => draft.set(String::new()),
                Err(message) => error.set(message),
            }
            sending.set(false);
        });
    };

    view! {
        <div class="page page-narrow">
            <div class="page-header">
                <div>
                    <h1 class="page-title">"Class messages"</h1>
                    <p class="page-description">"Share a reunion thought or leave a message for the class."</p>
                </div>
            </div>
            <form class="card" on:submit=submit>
                <div class="card-content stack pt">
                    <label class="label" for="class-message">"Write a message"</label>
                    <textarea
                        id="class-message"
                        class="textarea"
                        rows="3"
                        maxlength="500"
                        placeholder="What would you like to tell your classmates?"
                        bind:value=draft
                    ></textarea>
                    {move || {
                        let message = error.get();
                        (!message.is_empty())
                            .then(|| view! { <p class="field-error" role="alert">{message}</p> })
                    }}
                </div>
                <div class="card-footer row">
                    <span class="muted small">{move || format!("{}/{MAX_LENGTH}", draft.with(|d| d.chars().count()))}</span>
                    <button class="btn btn-primary" type="submit" disabled=move || sending.get() || draft.with(|d| d.trim().is_empty())>
                        {move || if sending.get() { view! { <Spinner /> }.into_any() } else { icon("send").into_any() }}
                        {move || if sending.get() { "Posting…" } else { "Post message" }}
                    </button>
                </div>
            </form>
            <div class="card">
                <div class="card-content message-list" aria-live="polite">
                    {move || {
                        let me = my_id();
                        let messages = state.messages.get();
                        if messages.is_empty() {
                            return view! {
                                <div class="empty">
                                    <span class="empty-icon">{icon("message-square")}</span>
                                    <strong>"No messages yet"</strong>
                                    <p class="muted">"Start the conversation!"</p>
                                </div>
                            }
                            .into_any();
                        }
                        messages
                            .into_iter()
                            .map(|item| {
                                let mine = item.user_id == me;
                                let author = item.author().unwrap_or_else(|| "12E classmate".to_string());
                                let label = if mine { "You".to_string() } else { author.clone() };
                                view! {
                                    <article class="message" class:mine=mine>
                                        {avatar(&author)}
                                        <div class="message-body">
                                            <div class="message-meta">
                                                <strong>{label}</strong>
                                                <time datetime=item.created_at.clone()>{format_date_time(&item.created_at)}</time>
                                            </div>
                                            <p>{item.message.clone()}</p>
                                        </div>
                                    </article>
                                }
                            })
                            .collect_view()
                            .into_any()
                    }}
                </div>
            </div>
        </div>
    }
}
