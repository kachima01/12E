//! Minimal Supabase Realtime client (Phoenix channel protocol over WebSocket)
//! subscribing to `postgres_changes` on contributions, profiles and messages.

use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::atomic::{AtomicU32, Ordering};

use gloo_timers::callback::{Interval, Timeout};
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde_json::{Value, json};
use wasm_bindgen::{JsCast, closure::Closure};
use web_sys::{CloseEvent, MessageEvent, WebSocket};

use crate::api;
use crate::data;
use crate::format::{enc, kwacha};
use crate::state::AppState;

const HEARTBEAT_MS: u32 = 25_000;
const RECONNECT_MS: u32 = 5_000;

struct Connection {
    socket: WebSocket,
    topic: String,
    _heartbeat: Interval,
    _on_open: Closure<dyn FnMut()>,
    _on_message: Closure<dyn FnMut(MessageEvent)>,
    _on_close: Closure<dyn FnMut(CloseEvent)>,
}

thread_local! {
    static CONNECTION: RefCell<Option<Connection>> = const { RefCell::new(None) };
    static NOTIFIED: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

/// Bumped on every connect/disconnect so stale reconnect timers do nothing.
static GENERATION: AtomicU32 = AtomicU32::new(0);

pub fn connect(state: AppState) {
    disconnect();
    let Some(session) = state.session.get_untracked() else { return };
    let config = api::config();
    let base = config.supabase_url.replacen("https://", "wss://", 1).replacen("http://", "ws://", 1);
    let url = format!("{base}/realtime/v1/websocket?apikey={}&vsn=1.0.0", enc(&config.supabase_key));
    let Ok(socket) = WebSocket::new(&url) else { return };
    let generation = GENERATION.fetch_add(1, Ordering::Relaxed) + 1;
    let topic = format!("realtime:reunion-updates-{}", session.user.id);

    let on_open = {
        let socket = socket.clone();
        let topic = topic.clone();
        Closure::<dyn FnMut()>::new(move || {
            let token = state.session.get_untracked().map(|s| s.access_token).unwrap_or_default();
            let tables = ["contributions", "profiles", "class_messages"]
                .map(|table| json!({ "event": "*", "schema": "public", "table": table }));
            let join = json!({
                "topic": topic,
                "event": "phx_join",
                "payload": {
                    "config": {
                        "broadcast": { "ack": false, "self": false },
                        "presence": { "key": "" },
                        "postgres_changes": tables,
                        "private": false,
                    },
                    "access_token": token,
                },
                "ref": "1",
                "join_ref": "1",
            });
            let _ = socket.send_with_str(&join.to_string());
        })
    };
    let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
        if let Some(text) = event.data().as_string() {
            handle_message(state, &text);
        }
    });
    let on_close = Closure::<dyn FnMut(CloseEvent)>::new(move |_event: CloseEvent| {
        Timeout::new(RECONNECT_MS, move || {
            if GENERATION.load(Ordering::Relaxed) == generation && state.session.get_untracked().is_some() {
                connect(state);
            }
        })
        .forget();
    });
    socket.set_onopen(Some(on_open.as_ref().unchecked_ref()));
    socket.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    socket.set_onclose(Some(on_close.as_ref().unchecked_ref()));

    let heartbeat = {
        let socket = socket.clone();
        Interval::new(HEARTBEAT_MS, move || {
            if socket.ready_state() == WebSocket::OPEN {
                let _ = socket.send_with_str(r#"{"topic":"phoenix","event":"heartbeat","payload":{},"ref":"hb"}"#);
            }
        })
    };
    CONNECTION.with(|slot| {
        *slot.borrow_mut() = Some(Connection {
            socket,
            topic,
            _heartbeat: heartbeat,
            _on_open: on_open,
            _on_message: on_message,
            _on_close: on_close,
        });
    });
}

pub fn disconnect() {
    GENERATION.fetch_add(1, Ordering::Relaxed);
    let previous = CONNECTION.with(|slot| slot.borrow_mut().take());
    if let Some(connection) = previous {
        connection.socket.set_onopen(None);
        connection.socket.set_onmessage(None);
        connection.socket.set_onclose(None);
        let _ = connection.socket.close();
    }
}

/// Give the channel a refreshed JWT so row-level security keeps working.
pub fn update_token(token: &str) {
    CONNECTION.with(|slot| {
        if let Some(connection) = slot.borrow().as_ref()
            && connection.socket.ready_state() == WebSocket::OPEN
        {
            let message = json!({
                "topic": connection.topic,
                "event": "access_token",
                "payload": { "access_token": token },
                "ref": "token",
            });
            let _ = connection.socket.send_with_str(&message.to_string());
        }
    });
}

fn handle_message(state: AppState, text: &str) {
    let Ok(message) = serde_json::from_str::<Value>(text) else { return };
    match message["event"].as_str() {
        Some("postgres_changes") => {
            let data = &message["payload"]["data"];
            match data["table"].as_str() {
                Some("contributions") => {
                    announce_confirmation(state, &data["record"]);
                    spawn_local(async move {
                        data::load_data(state).await;
                        data::load_audit(state).await;
                    });
                }
                Some("profiles") => spawn_local(async move {
                    data::load_profile(state).await;
                    data::load_data(state).await;
                    data::load_admin_profiles(state).await;
                }),
                Some("class_messages") => spawn_local(data::load_messages(state)),
                _ => {}
            }
        }
        Some("phx_reply") if message["payload"]["status"] == "error" => {
            web_sys::console::warn_1(&format!("Realtime subscription error: {}", message["payload"]["response"]).into());
        }
        _ => {}
    }
}

/// Toast when a contribution becomes confirmed (once per contribution).
fn announce_confirmation(state: AppState, record: &Value) {
    let (Some(id), Some("confirmed")) = (record["id"].as_str(), record["status"].as_str()) else { return };
    let was_confirmed =
        state.contributions.with_untracked(|items| items.iter().any(|item| item.id == id && item.is_confirmed()));
    let first_time = NOTIFIED.with(|seen| seen.borrow_mut().insert(id.to_string()));
    if was_confirmed || !first_time {
        return;
    }
    let amount = crate::types::flexible_amount(&record["amount"]);
    let user_id = record["user_id"].as_str().unwrap_or_default();
    if user_id == state.profile_id() {
        state.toast(format!("Your {} contribution is confirmed. Thank you!", kwacha(amount)));
    } else {
        let name = state.members.with_untracked(|members| {
            members.iter().find(|member| member.id == user_id).map(|member| member.full_name.clone())
        });
        state.toast(format!(
            "{} just contributed {} to the reunion fund.",
            name.unwrap_or_else(|| "A 12E classmate".to_string()),
            kwacha(amount)
        ));
    }
}
