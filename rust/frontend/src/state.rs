//! Application-wide reactive state. Every signal here is created once by the
//! root component, so async tasks can safely read and write it at any time.

use std::sync::atomic::{AtomicU32, Ordering};

use leptos::prelude::*;

use crate::types::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Boot {
    Loading,
    Failed,
    Ready,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PayPhase {
    Form,
    Starting,
    Processing,
    Confirmed,
    Failed,
    Waiting,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdminTab {
    Members,
    Payments,
    Wallet,
    Audit,
}

/// State for the contribution dialog (kept global so polling survives re-renders).
#[derive(Clone, Copy)]
pub struct ContributeState {
    pub open: RwSignal<bool>,
    pub amount: RwSignal<String>,
    pub phone: RwSignal<String>,
    pub phase: RwSignal<PayPhase>,
    pub error: RwSignal<String>,
    pub status_warning: RwSignal<String>,
    pub reference: RwSignal<String>,
    pub elapsed: RwSignal<u32>,
    /// Incremented to cancel an in-flight polling loop.
    pub run_id: RwSignal<u32>,
}

#[derive(Clone, Copy)]
pub struct AdminState {
    pub tab: RwSignal<AdminTab>,
    pub all_profiles: RwSignal<Vec<Profile>>,
    pub audit: RwSignal<Vec<AuditEntry>>,
    pub audit_loading: RwSignal<bool>,
    pub error: RwSignal<String>,
    pub wallet: RwSignal<Option<WalletBalance>>,
    pub wallet_loading: RwSignal<bool>,
    pub wallet_error: RwSignal<String>,
}

#[derive(Clone, Copy)]
pub struct AppState {
    pub boot: RwSignal<Boot>,
    pub boot_error: RwSignal<String>,
    pub session: RwSignal<Option<Session>>,
    pub auth_mode: RwSignal<AuthMode>,
    pub auth_notice: RwSignal<String>,
    pub profile: RwSignal<Option<Profile>>,
    pub profile_error: RwSignal<String>,
    pub load_error: RwSignal<String>,
    pub contributions: RwSignal<Vec<Contribution>>,
    pub members: RwSignal<Vec<Profile>>,
    pub messages: RwSignal<Vec<ClassMessage>>,
    pub page: RwSignal<Page>,
    pub mobile_nav: RwSignal<bool>,
    pub toast: RwSignal<Option<(u32, String)>>,
    pub busy_id: RwSignal<String>,
    pub dark: RwSignal<bool>,
    pub contribute: ContributeState,
    pub admin: AdminState,
}

static NEXT_TOAST: AtomicU32 = AtomicU32::new(1);

impl AppState {
    pub fn new() -> Self {
        Self {
            boot: RwSignal::new(Boot::Loading),
            boot_error: RwSignal::new(String::new()),
            session: RwSignal::new(None),
            auth_mode: RwSignal::new(AuthMode::Login),
            auth_notice: RwSignal::new(String::new()),
            profile: RwSignal::new(None),
            profile_error: RwSignal::new(String::new()),
            load_error: RwSignal::new(String::new()),
            contributions: RwSignal::new(Vec::new()),
            members: RwSignal::new(Vec::new()),
            messages: RwSignal::new(Vec::new()),
            page: RwSignal::new(Page::Overview),
            mobile_nav: RwSignal::new(false),
            toast: RwSignal::new(None),
            busy_id: RwSignal::new(String::new()),
            dark: RwSignal::new(false),
            contribute: ContributeState {
                open: RwSignal::new(false),
                amount: RwSignal::new("500".to_string()),
                phone: RwSignal::new(String::new()),
                phase: RwSignal::new(PayPhase::Form),
                error: RwSignal::new(String::new()),
                status_warning: RwSignal::new(String::new()),
                reference: RwSignal::new(String::new()),
                elapsed: RwSignal::new(0),
                run_id: RwSignal::new(0),
            },
            admin: AdminState {
                tab: RwSignal::new(AdminTab::Members),
                all_profiles: RwSignal::new(Vec::new()),
                audit: RwSignal::new(Vec::new()),
                audit_loading: RwSignal::new(false),
                error: RwSignal::new(String::new()),
                wallet: RwSignal::new(None),
                wallet_loading: RwSignal::new(false),
                wallet_error: RwSignal::new(String::new()),
            },
        }
    }

    /// Show a short notification that dismisses itself.
    pub fn toast(&self, message: impl Into<String>) {
        let id = NEXT_TOAST.fetch_add(1, Ordering::Relaxed);
        self.toast.set(Some((id, message.into())));
        let toast = self.toast;
        gloo_timers::callback::Timeout::new(3_600, move || {
            if toast.get_untracked().is_some_and(|(current, _)| current == id) {
                toast.set(None);
            }
        })
        .forget();
    }

    pub fn profile_id(&self) -> String {
        self.profile.with_untracked(|profile| profile.as_ref().map(|p| p.id.clone()).unwrap_or_default())
    }

    pub fn is_approved(&self) -> bool {
        self.profile.with_untracked(|profile| profile.as_ref().is_some_and(Profile::is_approved))
    }

    pub fn is_admin(&self) -> bool {
        self.profile
            .with_untracked(|profile| profile.as_ref().is_some_and(|p| p.is_admin() && p.is_approved()))
    }

    /// Clear everything tied to the signed-in member.
    pub fn reset_member_data(&self) {
        self.profile.set(None);
        self.profile_error.set(String::new());
        self.load_error.set(String::new());
        self.contributions.set(Vec::new());
        self.members.set(Vec::new());
        self.messages.set(Vec::new());
        self.page.set(Page::Overview);
        self.contribute.open.set(false);
        self.contribute.run_id.update(|run| *run += 1);
        self.admin.all_profiles.set(Vec::new());
        self.admin.audit.set(Vec::new());
        self.admin.wallet.set(None);
    }
}
