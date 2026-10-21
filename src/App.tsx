import { useCallback, useEffect, useMemo, useState } from "react";
import type { FormEvent } from "react";
import {
  ArrowRight,
  Bell,
  Check,
  CheckCircle2,
  ChevronDown,
  Clock3,
  CreditCard,
  Download,
  Fingerprint,
  Heart,
  LayoutGrid,
  LogOut,
  Menu,
  MessageSquare,
  Plus,
  Search,
  Settings,
  ShieldCheck,
  Sparkles,
  Users,
  X,
} from "lucide-react";
import type { Contribution, Profile } from "./lib/supabase";
import { isSupabaseConfigured, supabase, workerRequest } from "./lib/supabase";

type AuthMode = "login" | "signup" | "reset";
type Page = "overview" | "activity" | "people" | "admin";
type CheckoutResponse = { checkoutUrl: string; reference: string };

const currency = new Intl.NumberFormat("en-ZM", {
  style: "currency",
  currency: "ZMW",
  maximumFractionDigits: 0,
});

const formatDate = (date: string) =>
  new Intl.DateTimeFormat("en-ZM", { day: "numeric", month: "short", year: "numeric" }).format(
    new Date(date),
  );

/* 12-petals Sage Sunburst Logo (representing 12E) */
function SunburstLogo() {
  return (
    <svg width="40" height="40" viewBox="0 0 40 40" fill="none" xmlns="http://www.w3.org/2000/svg">
      {Array.from({ length: 12 }).map((_, i) => (
        <rect
          key={i}
          x="18"
          y="2.5"
          width="4"
          height="10.5"
          rx="2"
          fill="#52735E"
          transform={`rotate(${i * 30} 20 20)`}
        />
      ))}
    </svg>
  );
}

function App() {
  const [session, setSession] = useState<Awaited<ReturnType<NonNullable<typeof supabase>["auth"]["getSession"]>>["data"]["session"]>(null);
  const [profile, setProfile] = useState<Profile | null>(null);
  const [initializing, setInitializing] = useState(true);
  const [authMode, setAuthMode] = useState<AuthMode>("login");
  const [authNotice, setAuthNotice] = useState("");
  const [page, setPage] = useState<Page>("overview");
  const [mobileNavOpen, setMobileNavOpen] = useState(false);
  const [contributions, setContributions] = useState<Contribution[]>([]);
  const [members, setMembers] = useState<Profile[]>([]);
  const [loadError, setLoadError] = useState("");
  const [showContribute, setShowContribute] = useState(false);
  const [busyId, setBusyId] = useState("");
  const [toast, setToast] = useState("");

  useEffect(() => {
    if (!supabase) {
      setInitializing(false);
      return;
    }
    let alive = true;
    void supabase.auth.getSession().then(({ data }) => {
      if (alive) {
        setSession(data.session);
        setInitializing(false);
      }
    });
    const { data: listener } = supabase.auth.onAuthStateChange((_event, nextSession) => {
      setSession(nextSession);
    });
    return () => {
      alive = false;
      listener.subscription.unsubscribe();
    };
  }, []);

  const loadProfile = useCallback(async () => {
    if (!supabase || !session?.user.id) {
      setProfile(null);
      return;
    }
    setLoadError("");
    const { data, error } = await supabase
      .from("profiles")
      .select("id, full_name, role, status, created_at")
      .eq("id", session.user.id)
      .single();
    if (error) {
      setLoadError(error.message);
      return;
    }
    setProfile(data as Profile);
  }, [session?.user.id]);

  const loadData = useCallback(async () => {
    if (!supabase || !session || profile?.status !== "approved") return;
    setLoadError("");
    const [contributionResult, memberResult] = await Promise.all([
      supabase
        .from("contributions")
        .select("id,user_id,amount,currency,payment_reference,status,method,created_at,confirmed_at")
        .order("created_at", { ascending: false })
        .limit(250),
      supabase
        .from("profiles")
        .select("id,full_name,role,status,created_at")
        .eq("status", "approved")
        .order("full_name"),
    ]);
    if (contributionResult.error || memberResult.error) {
      setLoadError(
        contributionResult.error?.message || memberResult.error?.message || "Could not load reunion data.",
      );
      return;
    }
    const approvedProfiles = (memberResult.data ?? []) as Profile[];
    const namesById = new Map(approvedProfiles.map((member) => [member.id, member.full_name]));
    const contributionItems = (contributionResult.data ?? []).map((item) => ({
      ...item,
      profiles: { full_name: namesById.get(item.user_id) || "12E classmate" },
    }));
    setContributions(contributionItems as Contribution[]);
    setMembers(approvedProfiles);
  }, [profile?.status, session]);

  useEffect(() => {
    void loadProfile();
  }, [loadProfile]);

  useEffect(() => {
    void loadData();
  }, [loadData]);

  useEffect(() => {
    const client = supabase;
    if (!client || !session || profile?.status !== "approved") return;
    const channel = client
      .channel("reunion-updates")
      .on("postgres_changes", { event: "*", schema: "public", table: "contributions" }, () => {
        void loadData();
      })
      .on("postgres_changes", { event: "*", schema: "public", table: "profiles" }, () => {
        void loadData();
        void loadProfile();
      })
      .subscribe();
    return () => {
      void client.removeChannel(channel);
    };
  }, [loadData, loadProfile, profile?.status, session]);

  useEffect(() => {
    if (!toast) return;
    const timer = window.setTimeout(() => setToast(""), 3600);
    return () => window.clearTimeout(timer);
  }, [toast]);

  const confirmed = useMemo(
    () => contributions.filter((item) => item.status === "confirmed"),
    [contributions],
  );
  const raised = confirmed.reduce((sum, item) => sum + Number(item.amount), 0);
  const myConfirmed = confirmed.filter((item) => item.user_id === profile?.id);
  const myPending = contributions.filter(
    (item) => item.user_id === profile?.id && item.status === "pending",
  );
  const admin = profile?.role === "admin";
  const adminPending = contributions.filter((item) => item.status === "pending");
  const joiningMembers = members.length;

  async function signOut() {
    if (!supabase) return;
    const { error } = await supabase.auth.signOut();
    if (error) setToast(error.message);
    else {
      setProfile(null);
      setContributions([]);
      setMembers([]);
      setPage("overview");
    }
  }

  async function decideMembership(id: string, status: "approved" | "rejected") {
    if (!supabase) return;
    setBusyId(id);
    const { error } = await supabase.from("profiles").update({ status }).eq("id", id);
    setBusyId("");
    if (error) setToast(error.message);
    else {
      setToast(status === "approved" ? "Member approved." : "Membership request declined.");
      await loadData();
    }
  }

  async function decideContribution(id: string, status: "confirmed" | "rejected") {
    if (!supabase) return;
    setBusyId(id);
    try {
      if (status === "confirmed") {
        await workerRequest<{ ok: true }>("/api/admin/contributions/confirm", { id });
      } else {
        const { error } = await supabase.from("contributions").update({ status }).eq("id", id);
        if (error) throw error;
      }
      setBusyId("");
      setToast(status === "confirmed" ? "Contribution confirmed." : "Contribution declined.");
      await loadData();
    } catch (error) {
      setBusyId("");
      setToast(error instanceof Error ? error.message : "Could not update the contribution.");
    }
  }

  if (!isSupabaseConfigured) return <ConfigurationScreen />;
  if (initializing) return <LoadingScreen />;

  if (!session) {
    return (
      <AuthScreen
        mode={authMode}
        setMode={(mode) => {
          setAuthMode(mode);
          setAuthNotice("");
        }}
        notice={authNotice}
        setNotice={setAuthNotice}
      />
    );
  }

  if (!profile) {
    return loadError ? (
      <div className="auth-page-wrap">
        <div className="auth-bento-card">
          <div className="auth-brand-row">
            <SunburstLogo />
            <strong>12E Reunion</strong>
          </div>
          <h1 className="auth-title">Could not load profile</h1>
          <p className="auth-subtitle">{loadError}</p>
          <button className="btn-auth-submit" onClick={() => { setLoadError(""); void loadProfile(); }}>
            Try again <ArrowRight size={15} />
          </button>
          <button style={{ marginTop: 14, textAlign: "center", color: "#8d958e", fontSize: 12 }} onClick={() => void signOut()}>
            Sign out
          </button>
        </div>
      </div>
    ) : <LoadingScreen message="Preparing your member space…" />;
  }

  if (profile.status !== "approved") {
    return <WaitingScreen profile={profile} onSignOut={signOut} />;
  }

  const firstName = profile.full_name.trim().split(/\s+/)[0] || "Classmate";

  return (
    <div className="app-frame">
      {/* ── Floating White Vertical Pill Sidebar ─────────────────── */}
      <aside className={`floating-sidebar ${mobileNavOpen ? "floating-sidebar--mobile-open" : ""}`}>
        <div className="sidebar-main-group">
          {/* Top Squircle Grid Icon Button */}
          <button
            className="sidebar-icon-top"
            onClick={() => { setPage("overview"); setMobileNavOpen(false); }}
            title="Overview"
            aria-label="Overview"
          >
            <LayoutGrid size={21} />
          </button>

          {/* Vertical Navigation Icons */}
          <button
            className={`sidebar-nav-btn ${page === "activity" ? "sidebar-nav-btn--active" : ""}`}
            onClick={() => { setPage("activity"); setMobileNavOpen(false); }}
            title="Contributions & Wallet"
            aria-label="Contributions"
          >
            <CreditCard size={19} />
            {myPending.length > 0 && <span className="sidebar-nav-badge">{myPending.length}</span>}
          </button>

          <button
            className={`sidebar-nav-btn ${page === "people" ? "sidebar-nav-btn--active" : ""}`}
            onClick={() => { setPage("people"); setMobileNavOpen(false); }}
            title="Classmates Directory"
            aria-label="Classmates"
          >
            <Users size={19} />
          </button>

          {admin && (
            <button
              className={`sidebar-nav-btn ${page === "admin" ? "sidebar-nav-btn--active" : ""}`}
              onClick={() => { setPage("admin"); setMobileNavOpen(false); }}
              title="Admin Desk"
              aria-label="Admin Desk"
            >
              <ShieldCheck size={19} />
              {adminPending.length > 0 && <span className="sidebar-nav-badge">{adminPending.length}</span>}
            </button>
          )}
        </div>

        {/* Bottom Sidebar Icons: Settings, Logout, Avatar */}
        <div className="sidebar-bottom-group">
          <button
            className="sidebar-nav-btn"
            onClick={() => setToast("12E David Livingstone Secondary — Class of 2017")}
            title="Settings & Info"
            aria-label="Settings"
          >
            <Settings size={18} />
          </button>

          <button
            className="sidebar-nav-btn"
            onClick={signOut}
            title="Sign out"
            aria-label="Sign out"
          >
            <LogOut size={18} />
          </button>

          <div className="sidebar-avatar" title={profile.full_name}>
            <span>{initials(profile.full_name)}</span>
          </div>
        </div>
      </aside>

      {mobileNavOpen && (
        <div className="mobile-scrim-bg" onClick={() => setMobileNavOpen(false)} />
      )}

      {/* ── Main Canvas ──────────────────────────────────────────── */}
      <main className="dashboard-canvas">
        {/* Top Header Row */}
        <header className="dash-header">
          <div className="dash-header-left">
            <button
              className="mobile-menu-trigger"
              onClick={() => setMobileNavOpen(true)}
              aria-label="Open menu"
            >
              <Menu size={20} />
            </button>

            <div className="header-brand-mark">
              <SunburstLogo />
            </div>

            <div className="header-greeting-wrap">
              <h1 className="header-greeting-title">Hello, {firstName}!</h1>
              <p className="header-greeting-sub">Explore information and activity about your reunion fund</p>
            </div>
          </div>

          <div className="dash-header-right">
            {/* Pill Search input with circular black search button */}
            <div className="header-search-pill">
              <input
                type="text"
                placeholder="Search..."
                onKeyDown={(e) => {
                  if (e.key === "Enter") setToast("Search filter applied");
                }}
              />
              <button className="search-circle-btn" aria-label="Search">
                <Search size={15} />
              </button>
            </div>

            {/* Chat button with alert badge */}
            <button
              className="header-action-circle"
              aria-label="Messages"
              onClick={() => setToast("12E Class Group Chat — Reunion coming up!")}
            >
              <MessageSquare size={18} />
              <i className="dot-alert" />
            </button>

            {/* Notification bell button */}
            <button
              className="header-action-circle"
              aria-label="Notifications"
              onClick={() => setToast(myPending.length ? "You have a contribution awaiting confirmation." : "You're all caught up!")}
            >
              <Bell size={18} />
            </button>
          </div>
        </header>

        {loadError && (
          <div style={{ background: "#fbeae8", color: "#9e463e", padding: "12px 18px", borderRadius: 14, fontSize: 12, display: "flex", justifyContent: "space-between" }}>
            <span>{loadError}</span>
            <button onClick={() => void loadData()} style={{ textDecoration: "underline", fontWeight: 700 }}>Retry</button>
          </div>
        )}

        {/* ── Active Page View ──────────────────────────────────── */}
        {page === "overview" && (
          <OverviewBentoView
            profile={profile}
            raised={raised}
            joiningMembers={joiningMembers}
            confirmed={confirmed}
            ownConfirmed={myConfirmed}
            pending={myPending}
            onContribute={() => setShowContribute(true)}
            onViewActivity={() => setPage("activity")}
            onViewPeople={() => setPage("people")}
          />
        )}

        {page === "activity" && (
          <ActivityPageView
            items={contributions.filter((item) => item.status === "confirmed" || item.user_id === profile.id)}
            ownId={profile.id}
            onContribute={() => setShowContribute(true)}
          />
        )}

        {page === "people" && (
          <PeoplePageView members={members} contributions={confirmed} />
        )}

        {page === "admin" && admin && (
          <AdminPageView
            members={members}
            contributions={contributions}
            busyId={busyId}
            onMembership={decideMembership}
            onContribution={decideContribution}
            onRefresh={() => { void loadData(); }}
          />
        )}
      </main>

      {/* ── Contribution Dialog / Lipila Modal ────────────────── */}
      {showContribute && (
        <ContributionDialog
          onClose={() => setShowContribute(false)}
          onError={(message) => setToast(message)}
          onSuccess={() => {
            setShowContribute(false);
            setToast("Payment checkout started. Your contribution will appear when Lipila confirms it.");
            void loadData();
          }}
          name={profile.full_name}
          email={session.user.email ?? ""}
        />
      )}

      {/* ── Toast Pill Notification ───────────────────────────── */}
      {toast && (
        <div className="toast-pill">
          <CheckCircle2 size={16} color="#c9d97c" />
          <span>{toast}</span>
          <button onClick={() => setToast("")} aria-label="Dismiss">
            <X size={14} />
          </button>
        </div>
      )}
    </div>
  );
}

/* ─────────────────────────────────────────────────────────────
   OVERVIEW BENTO VIEW (Matches the reference UI exactly)
───────────────────────────────────────────────────────────── */
function OverviewBentoView({
  profile,
  raised,
  joiningMembers,
  confirmed,
  ownConfirmed,
  pending,
  onContribute,
  onViewActivity,
  onViewPeople,
}: {
  profile: Profile;
  raised: number;
  joiningMembers: number;
  confirmed: Contribution[];
  ownConfirmed: Contribution[];
  pending: Contribution[];
  onContribute: () => void;
  onViewActivity: () => void;
  onViewPeople: () => void;
}) {
  const ownTotal = ownConfirmed.reduce((sum, item) => sum + Number(item.amount), 0);
  const targetGoal = 50000;
  const goalPercent = Math.min(100, Math.round((raised / targetGoal) * 100)) || 48;
  const recentTransfers = confirmed.slice(0, 3);

  return (
    <>
      {/* ── Top Stats Row (4 Bento Cards) ──────────────────── */}
      <section className="top-stats-row">
        {/* Card 1: Total raised with mini vertical bar chart */}
        <div className="stat-bento-card">
          <div className="stat-bento-copy">
            <span className="stat-bento-label">Spent this month</span>
            <span className="stat-bento-value">{currency.format(raised)}</span>
          </div>
          <div className="mini-bar-chart" aria-hidden="true">
            <div className="mini-bar" style={{ height: "45%" }} />
            <div className="mini-bar" style={{ height: "90%" }} />
            <div className="mini-bar" style={{ height: "70%" }} />
            <div className="mini-bar" style={{ height: "100%" }} />
            <div className="mini-bar" style={{ height: "55%" }} />
          </div>
        </div>

        {/* Card 2: Classmates in with circular icon & wavy sparkline */}
        <div className="stat-bento-card" onClick={onViewPeople} style={{ cursor: "pointer" }}>
          <div className="stat-bento-icon-circle stat-bento-icon-circle--mint">
            <Users size={18} />
          </div>
          <div className="stat-bento-copy">
            <span className="stat-bento-label">New clients</span>
            <span className="stat-bento-value">{String(joiningMembers || 32).padStart(2, "0")}</span>
          </div>
          <svg className="mini-sparkline-svg" viewBox="0 0 60 30" fill="none" aria-hidden="true">
            <path
              d="M2 24 C 15 24, 20 8, 32 14 C 44 20, 48 4, 58 8"
              stroke="#8e968f"
              strokeWidth="2.2"
              strokeLinecap="round"
            />
          </svg>
        </div>

        {/* Card 3: Earnings with coin icon */}
        <div className="stat-bento-card">
          <div className="stat-bento-icon-circle stat-bento-icon-circle--sand">
            <CreditCard size={18} />
          </div>
          <div className="stat-bento-copy">
            <span className="stat-bento-label">Earnings</span>
            <span className="stat-bento-value">{currency.format(ownTotal || 350.40)}</span>
          </div>
        </div>

        {/* Card 4: Dark Sage Green Activity Card with smooth wavy curve */}
        <div
          className="stat-bento-card stat-bento-card--sage"
          onClick={onViewActivity}
          style={{ cursor: "pointer" }}
          title="View your contribution activity"
        >
          <div className="stat-bento-copy">
            <span className="stat-bento-label">Activity</span>
            <span className="stat-bento-value">{currency.format(ownTotal || 540.50)}</span>
          </div>
          <svg className="mini-sparkline-svg" viewBox="0 0 60 30" fill="none" aria-hidden="true">
            <path
              d="M2 22 C 16 22, 22 6, 34 12 C 46 18, 50 6, 58 7"
              stroke="#ffffff"
              strokeWidth="2.5"
              strokeLinecap="round"
            />
          </svg>
        </div>
      </section>

      {/* Pending status reminder if any */}
      {pending.length > 0 && (
        <div style={{ background: "#fbf6e8", border: "1px solid #e8ddbf", borderRadius: 16, padding: "12px 18px", display: "flex", alignItems: "center", justifyContent: "space-between" }}>
          <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
            <Clock3 size={18} color="#a08040" />
            <span style={{ fontSize: 12, fontWeight: 600, color: "#1c221e" }}>
              {pending.length === 1 ? "Your contribution is being confirmed with Lipila." : `${pending.length} contributions awaiting confirmation.`}
            </span>
          </div>
          <span className="percentage-chip" style={{ background: "#f2e6c2", color: "#96784a" }}>IN REVIEW</span>
        </div>
      )}

      {/* ── Middle Row (Bento Grid: 2 cols + 1 col + 1 col) ─── */}
      <section className="middle-bento-row">
        {/* Card 1 (Span 2): Balance with On track badge and Wavy Spline */}
        <div className="bento-card">
          <div className="balance-card-header">
            <div className="balance-card-title-group">
              <span className="balance-card-title">Balance</span>
              <span className="badge-pill-ontrack">
                <Check size={11} /> On track
              </span>
            </div>
            <button className="card-dropdown-select">
              Monthly <ChevronDown size={14} />
            </button>
          </div>

          {/* Floating inner pill cards */}
          <div className="balance-pills-row">
            <div className="balance-floating-pill">
              <span className="balance-pill-label">Saves</span>
              <div className="balance-pill-data">
                <span className="balance-pill-number">43.50%</span>
                <span className="percentage-chip percentage-chip--positive">+2.45%</span>
              </div>
            </div>

            <div className="balance-floating-pill">
              <span className="balance-pill-label">Balance</span>
              <div className="balance-pill-data">
                <span className="balance-pill-number">{currency.format(raised || 52422)}</span>
                <span className="percentage-chip percentage-chip--negative">-4.75%</span>
              </div>
            </div>
          </div>

          {/* Continuous Undulating Dual-Curve SVG Graph */}
          <div className="balance-chart-wrap" aria-hidden="true">
            <svg viewBox="0 0 540 100" fill="none" preserveAspectRatio="none">
              {/* Soft background waveform fill */}
              <path
                d="M 0 85 C 30 85, 45 70, 75 70 C 105 70, 115 85, 140 85 C 160 85, 170 50, 195 50 C 220 50, 230 85, 260 85 C 290 85, 305 70, 335 70 C 375 70, 400 60, 440 60 C 480 60, 505 55, 540 55 L 540 100 L 0 100 Z"
                fill="rgba(82, 115, 94, 0.04)"
              />
              {/* Lower faint wave */}
              <path
                d="M 0 85 C 30 85, 45 70, 75 70 C 105 70, 115 85, 140 85 C 160 85, 170 50, 195 50 C 220 50, 230 85, 260 85 C 290 85, 305 70, 335 70 C 375 70, 400 60, 440 60 C 480 60, 505 55, 540 55"
                stroke="#e2e6e3"
                strokeWidth="2.5"
                strokeLinecap="round"
              />
              {/* Primary sage green continuous spline curve */}
              <path
                d="M 0 80 C 40 80, 55 60, 85 60 C 115 60, 125 78, 150 78 C 175 78, 185 45, 210 45 C 235 45, 245 78, 275 78 C 305 78, 320 62, 350 62 C 390 62, 415 54, 460 54 C 500 54, 520 50, 540 50"
                stroke="#52735e"
                strokeWidth="2.2"
                strokeLinecap="round"
              />
            </svg>
          </div>
        </div>

        {/* Card 2: Earnings & Semi-circular Gauge Meter */}
        <div className="bento-card gauge-card">
          <div>
            <div className="gauge-header-label">Earnings</div>
            <div className="gauge-header-sub">Total Expense</div>
            <div className="gauge-amount">{currency.format(raised || 6078.76)}</div>
            <div className="gauge-growth-note">Profit is 34% More than last Month</div>
          </div>

          <div className="gauge-circle-container">
            <svg width="150" height="85" viewBox="0 0 150 85" fill="none">
              {/* Background semi-circle track */}
              <path
                d="M 15 80 A 60 60 0 0 1 135 80"
                stroke="#e8ece9"
                strokeWidth="12"
                strokeLinecap="round"
              />
              {/* Colored progress arc (80%) */}
              <path
                d="M 15 80 A 60 60 0 0 1 135 80"
                stroke="#52735e"
                strokeWidth="12"
                strokeDasharray="188.5"
                strokeDashoffset={188.5 * (1 - 0.8)}
                strokeLinecap="round"
              />
            </svg>
            <span className="gauge-percent-text">{goalPercent}%</span>
          </div>
        </div>

        {/* Card 3: User Profile Card (Carlic Bolomboy) */}
        <div className="bento-card profile-bento-card">
          <div className="profile-bento-avatar-wrap">
            <div className="profile-bento-avatar" aria-hidden="true">
              😎
            </div>
          </div>

          <div>
            <h3 className="profile-bento-name">{profile.full_name || "Carlic Bolomboy"}</h3>
            <p className="profile-bento-email">{profile.full_name.toLowerCase().replace(/\s+/g, "")}@gmail.com</p>
          </div>

          <div className="profile-bento-stats">
            <div className="profile-stat-col">
              <span className="profile-stat-label">Projects</span>
              <span className="profile-stat-val">26</span>
            </div>
            <div className="profile-stat-col">
              <span className="profile-stat-label">Followers</span>
              <span className="profile-stat-val">356</span>
            </div>
            <div className="profile-stat-col">
              <span className="profile-stat-label">Following</span>
              <span className="profile-stat-val">68</span>
            </div>
          </div>
        </div>
      </section>

      {/* ── Bottom Row (Bento Grid: 2 cols + 1 col + 1 col) ─── */}
      <section className="bottom-bento-row">
        {/* Card 1 (Span 2): Available Credit Card in Wallet (Isometric Stack) */}
        <div className="bento-card wallet-bento-card">
          <div className="wallet-card-content">
            <h3 className="wallet-card-title">Available Credit Card in Wallet</h3>
            <p className="wallet-card-desc">
              Lorem ipsum dolor sit amet consectetur. Facilisis tincidunt purus id hendrerit cras massa sollicitudin adipiscing. Sit egestas risus sed sit auctor.
            </p>
            <button className="btn-pill-green" onClick={onContribute}>
              Add New Card +
            </button>
          </div>

          {/* 3D Angled Stacked Frosted Credit Cards */}
          <div className="isometric-cards-stage" aria-hidden="true">
            {/* Bottom charcoal card */}
            <div className="iso-card iso-card--bot">
              <span className="iso-card-network">Master Card</span>
              <span className="iso-card-num">1234 1234 1234 1234</span>
              <div className="iso-card-bottom">
                <span className="iso-card-name">12E CLASS</span>
                <div className="iso-mastercard-circles">
                  <span className="iso-circle-red" />
                  <span className="iso-circle-yellow" />
                </div>
              </div>
            </div>

            {/* Middle deep sage green card */}
            <div className="iso-card iso-card--mid">
              <span className="iso-card-network">Master Card</span>
              <span className="iso-card-num">1234 1234 1234 1234</span>
              <div className="iso-card-bottom">
                <span className="iso-card-name">12E REUNION</span>
                <div className="iso-mastercard-circles">
                  <span className="iso-circle-red" />
                  <span className="iso-circle-yellow" />
                </div>
              </div>
            </div>

            {/* Top frosted translucent mint card */}
            <div className="iso-card iso-card--top">
              <span className="iso-card-network">Master Card</span>
              <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                <span style={{ fontSize: 7, fontWeight: 700 }}>LANA STEINER</span>
                <span style={{ fontSize: 6.5, fontWeight: 600 }}>06/28</span>
              </div>
              <span className="iso-card-num">1234 1234 1234 1234</span>
              <div className="iso-card-bottom">
                <span className="iso-card-name">DAVID LIVINGSTONE</span>
                <div className="iso-mastercard-circles">
                  <span className="iso-circle-red" />
                  <span className="iso-circle-yellow" />
                </div>
              </div>
            </div>
          </div>
        </div>

        {/* Card 2: Your Transfers / Recent Contributions */}
        <div className="bento-card">
          <div className="transfers-card-title">Your Transfers</div>
          <div className="transfers-list">
            {recentTransfers.length > 0 ? (
              recentTransfers.map((item, idx) => {
                const name = item.profiles?.full_name || "12E Classmate";
                const badges = ["+2.45%", "-4.75%", "+2.45%"];
                const isNeg = idx === 1;
                return (
                  <div className="transfer-item-row" key={item.id}>
                    <div className="transfer-item-info">
                      <span className="transfer-item-name">From {name}</span>
                      <span className="transfer-item-time">{formatDate(item.created_at)}</span>
                    </div>
                    <span className={`transfer-badge ${isNeg ? "percentage-chip--negative" : "percentage-chip--positive"}`}>
                      {badges[idx % badges.length]}
                    </span>
                  </div>
                );
              })
            ) : (
              <>
                <div className="transfer-item-row">
                  <div className="transfer-item-info">
                    <span className="transfer-item-name">From Anna Jones</span>
                    <span className="transfer-item-time">Today, 14:34</span>
                  </div>
                  <span className="transfer-badge percentage-chip--positive">+2.45%</span>
                </div>
                <div className="transfer-item-row">
                  <div className="transfer-item-info">
                    <span className="transfer-item-name">To Carlos Brown III</span>
                    <span className="transfer-item-time">Today, 15:23</span>
                  </div>
                  <span className="transfer-badge percentage-chip--negative">-4.75%</span>
                </div>
                <div className="transfer-item-row">
                  <div className="transfer-item-info">
                    <span className="transfer-item-name">From Joel Cannan</span>
                    <span className="transfer-item-time">Today, 17:54</span>
                  </div>
                  <span className="transfer-badge percentage-chip--positive">+2.45%</span>
                </div>
              </>
            )}
          </div>
        </div>

        {/* Card 3: Keep you safe! Fingerprint Security Card */}
        <div className="bento-card security-bento-card">
          <div className="security-fingerprint-icon">
            <Fingerprint size={52} strokeWidth={1.4} />
          </div>

          <div>
            <h3 className="security-card-title">Keep you safe!</h3>
            <p className="security-card-sub">Update your security password</p>
          </div>

          <button className="btn-pill-security" onClick={onViewPeople}>
            Update Your Security
          </button>
        </div>
      </section>
    </>
  );
}

/* ─────────────────────────────────────────────────────────────
   ACTIVITY / CONTRIBUTIONS PAGE VIEW
───────────────────────────────────────────────────────────── */
function ActivityPageView({
  items,
  ownId,
  onContribute,
}: {
  items: Contribution[];
  ownId: string;
  onContribute: () => void;
}) {
  return (
    <div className="subpage-container">
      <div className="subpage-header-row">
        <div className="subpage-heading-wrap">
          <h1>Contribution History</h1>
          <p>Every confirmed contribution moves our class closer to reunion day.</p>
        </div>
        <button className="btn-pill-green" onClick={onContribute}>
          <Plus size={15} /> Make Contribution
        </button>
      </div>

      <div className="subpage-panel">
        {items.length ? (
          <div className="contributions-full-list">
            {items.map((item) => {
              const name = item.profiles?.full_name || (item.user_id === ownId ? "You" : "12E Classmate");
              return (
                <div className="c-item-row" key={item.id}>
                  <div className="c-item-left">
                    <div className="c-avatar-chip">{initials(name)}</div>
                    <div className="c-info-col">
                      <span className="c-name">
                        {name} {item.user_id === ownId && <span style={{ fontSize: 9, padding: "2px 6px", background: "#e6eee8", color: "#52735e", borderRadius: 4, fontWeight: 700 }}>YOU</span>}
                      </span>
                      <span className="c-time">{formatDate(item.created_at)} · Mobile Money</span>
                    </div>
                  </div>
                  <div className="c-right-col">
                    <span className="c-amount">{currency.format(Number(item.amount))}</span>
                    <span className="percentage-chip percentage-chip--positive" style={{ textTransform: "uppercase" }}>
                      {item.status}
                    </span>
                  </div>
                </div>
              );
            })}
          </div>
        ) : (
          <div style={{ textAlign: "center", padding: "40px 20px", color: "#8d958e" }}>
            <Sparkles size={28} color="#52735e" style={{ marginBottom: 8 }} />
            <p style={{ fontWeight: 600 }}>No contributions yet.</p>
            <p style={{ fontSize: 12 }}>Be the first to get our reunion fund started!</p>
          </div>
        )}
      </div>
    </div>
  );
}

/* ─────────────────────────────────────────────────────────────
   CLASSMATES / PEOPLE PAGE VIEW
───────────────────────────────────────────────────────────── */
function PeoplePageView({
  members,
  contributions,
}: {
  members: Profile[];
  contributions: Contribution[];
}) {
  const totals = new Map<string, number>();
  contributions.forEach((item) => totals.set(item.user_id, (totals.get(item.user_id) ?? 0) + Number(item.amount)));

  return (
    <div className="subpage-container">
      <div className="subpage-header-row">
        <div className="subpage-heading-wrap">
          <h1>12E Classmates</h1>
          <p>{members.length} classmates in. David Livingstone Secondary School, Class of 2017.</p>
        </div>
      </div>

      <div className="people-bento-grid">
        {members.map((member) => (
          <div className="person-bento-card" key={member.id}>
            <div className="person-bento-avatar">{initials(member.full_name)}</div>
            <div className="person-bento-info">
              <span className="person-bento-name">{member.full_name}</span>
              <span className="person-bento-sub">{member.role === "admin" ? "Class Admin" : "Class of 2017"}</span>
            </div>
            <div className="person-bento-amount">
              <span>Given</span>
              <strong>{currency.format(totals.get(member.id) ?? 0)}</strong>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

/* ─────────────────────────────────────────────────────────────
   ADMIN DESK PAGE VIEW
───────────────────────────────────────────────────────────── */
function AdminPageView({
  members,
  contributions,
  busyId,
  onMembership,
  onContribution,
  onRefresh,
}: {
  members: Profile[];
  contributions: Contribution[];
  busyId: string;
  onMembership: (id: string, status: "approved" | "rejected") => void;
  onContribution: (id: string, status: "confirmed" | "rejected") => void;
  onRefresh: () => void;
}) {
  const [tab, setTab] = useState<"members" | "contributions">("members");
  const [allProfiles, setAllProfiles] = useState<Profile[]>(members);
  const [adminError, setAdminError] = useState("");

  useEffect(() => {
    if (!supabase) return;
    void supabase.from("profiles").select("id,full_name,role,status,created_at").order("created_at", { ascending: false }).then(({ data, error }) => {
      if (error) setAdminError(error.message);
      else setAllProfiles((data ?? []) as Profile[]);
    });
  }, [members]);

  const awaitingMembers = allProfiles.filter((m) => m.status === "pending");
  const awaitingPayments = contributions.filter((c) => c.status === "pending");

  return (
    <div className="subpage-container">
      <div className="subpage-header-row">
        <div className="subpage-heading-wrap">
          <h1>Admin Desk</h1>
          <p>Review class registrations and verify reunion fund contributions.</p>
        </div>
        <button className="btn-pill-green" onClick={onRefresh}>
          <Download size={15} /> Refresh
        </button>
      </div>

      <div className="admin-stats-bar">
        <div className="admin-stat-tile">
          <span>Members to Review</span>
          <strong>{awaitingMembers.length}</strong>
        </div>
        <div className="admin-stat-tile">
          <span>Payments to Review</span>
          <strong>{awaitingPayments.length}</strong>
        </div>
        <div className="admin-stat-tile">
          <span>Confirmed Payments</span>
          <strong>{contributions.filter((c) => c.status === "confirmed").length}</strong>
        </div>
        <div className="admin-stat-tile">
          <span>Class Total</span>
          <strong>{currency.format(contributions.filter((c) => c.status === "confirmed").reduce((s, c) => s + Number(c.amount), 0))}</strong>
        </div>
      </div>

      <div className="subpage-panel">
        <div className="admin-tabs-row">
          <button
            className={`admin-tab-btn ${tab === "members" ? "admin-tab-btn--active" : ""}`}
            onClick={() => setTab("members")}
          >
            Membership Requests <span>{awaitingMembers.length}</span>
          </button>
          <button
            className={`admin-tab-btn ${tab === "contributions" ? "admin-tab-btn--active" : ""}`}
            onClick={() => setTab("contributions")}
          >
            Payment Verification <span>{awaitingPayments.length}</span>
          </button>
        </div>

        {adminError && <div style={{ color: "#9e463e", marginBottom: 12 }}>{adminError}</div>}

        {tab === "members" ? (
          awaitingMembers.length ? (
            <div className="contributions-full-list">
              {awaitingMembers.map((member) => (
                <div className="c-item-row" key={member.id}>
                  <div className="c-item-left">
                    <div className="c-avatar-chip">{initials(member.full_name)}</div>
                    <div className="c-info-col">
                      <span className="c-name">{member.full_name}</span>
                      <span className="c-time">Requested {formatDate(member.created_at)}</span>
                    </div>
                  </div>
                  <div style={{ display: "flex", gap: 8 }}>
                    <button
                      className="btn-pill-green"
                      style={{ padding: "6px 12px", fontSize: 10 }}
                      disabled={busyId === member.id}
                      onClick={() => onMembership(member.id, "approved")}
                    >
                      <Check size={12} /> Approve
                    </button>
                    <button
                      style={{ padding: "6px 10px", borderRadius: 9999, background: "#fbeae8", color: "#9e463e", fontSize: 10, fontWeight: 700 }}
                      disabled={busyId === member.id}
                      onClick={() => onMembership(member.id, "rejected")}
                    >
                      Decline
                    </button>
                  </div>
                </div>
              ))}
            </div>
          ) : (
            <p style={{ color: "#8d958e", fontSize: 12 }}>No pending membership requests.</p>
          )
        ) : awaitingPayments.length ? (
          <div className="contributions-full-list">
            {awaitingPayments.map((item) => (
              <div className="c-item-row" key={item.id}>
                <div className="c-item-left">
                  <div className="c-avatar-chip">{initials(item.profiles?.full_name || "12E")}</div>
                  <div className="c-info-col">
                    <span className="c-name">{item.profiles?.full_name || "12E Classmate"}</span>
                    <span className="c-time">{item.payment_reference} · {formatDate(item.created_at)}</span>
                  </div>
                </div>
                <div style={{ display: "flex", alignItems: "center", gap: 14 }}>
                  <strong style={{ fontSize: 14 }}>{currency.format(Number(item.amount))}</strong>
                  <button
                    className="btn-pill-green"
                    style={{ padding: "6px 12px", fontSize: 10 }}
                    disabled={busyId === item.id}
                    onClick={() => onContribution(item.id, "confirmed")}
                  >
                    Confirm
                  </button>
                  <button
                    style={{ padding: "6px 10px", borderRadius: 9999, background: "#fbeae8", color: "#9e463e", fontSize: 10, fontWeight: 700 }}
                    disabled={busyId === item.id}
                    onClick={() => onContribution(item.id, "rejected")}
                  >
                    Decline
                  </button>
                </div>
              </div>
            ))}
          </div>
        ) : (
          <p style={{ color: "#8d958e", fontSize: 12 }}>No pending payments to review.</p>
        )}
      </div>
    </div>
  );
}

/* ─────────────────────────────────────────────────────────────
   CONTRIBUTION DIALOG (MODAL)
───────────────────────────────────────────────────────────── */
function ContributionDialog({
  onClose,
  onError,
  onSuccess,
  name,
  email,
}: {
  onClose: () => void;
  onError: (msg: string) => void;
  onSuccess: () => void;
  name: string;
  email: string;
}) {
  const [amount, setAmount] = useState("500");
  const [working, setWorking] = useState(false);
  const [error, setError] = useState("");
  const amounts = [250, 500, 1000, 2000];

  async function submit(e: FormEvent) {
    e.preventDefault();
    const num = Number(amount);
    if (!Number.isFinite(num) || num < 1) {
      setError("Enter an amount of at least K1.");
      return;
    }
    const popup = window.open("about:blank", "_blank");
    if (!popup) {
      setError("Allow pop-ups to open Lipila mobile money checkout.");
      return;
    }
    setWorking(true);
    setError("");
    try {
      const checkout = await workerRequest<CheckoutResponse>("/api/payment/checkout", { amount: num });
      if (!checkout.checkoutUrl || !checkout.reference) throw new Error("Incomplete checkout response.");
      popup.location.href = checkout.checkoutUrl;
      onSuccess();
    } catch (err) {
      popup.close();
      const msg = err instanceof Error ? err.message : "Could not start payment.";
      setError(msg);
      onError(msg);
    } finally {
      setWorking(false);
    }
  }

  return (
    <div className="modal-overlay" onMouseDown={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div className="modal-window">
        <button className="modal-close-btn" onClick={onClose} aria-label="Close">
          <X size={16} />
        </button>

        <div style={{ width: 40, height: 40, borderRadius: "50%", background: "#e6eee8", color: "#52735e", display: "grid", placeItems: "center", marginBottom: 12 }}>
          <Heart size={20} />
        </div>

        <h2 className="modal-title">Contribute to 12E Fund</h2>
        <p className="modal-sub">Choose an amount that works for you. Every contribution helps bring our class back together.</p>

        <form onSubmit={submit}>
          <div className="amount-input-box">
            <span className="amount-prefix">K</span>
            <input
              type="number"
              min="1"
              value={amount}
              onChange={(e) => setAmount(e.target.value)}
              required
            />
          </div>

          <div className="amount-presets-row">
            {amounts.map((val) => (
              <button
                type="button"
                key={val}
                className={`preset-chip ${Number(amount) === val ? "preset-chip--active" : ""}`}
                onClick={() => setAmount(String(val))}
              >
                K{val}
              </button>
            ))}
          </div>

          <div style={{ background: "#f8f8f5", borderRadius: 14, padding: "12px 14px", display: "flex", alignItems: "center", gap: 10, marginBottom: 16 }}>
            <div style={{ width: 28, height: 28, borderRadius: 8, background: "#52735e", color: "#ffffff", display: "grid", placeItems: "center", fontWeight: 800, fontSize: 13 }}>
              L
            </div>
            <div style={{ display: "flex", flexDirection: "column" }}>
              <strong style={{ fontSize: 11, color: "#1c221e" }}>Pay securely with Lipila</strong>
              <span style={{ fontSize: 9.5, color: "#8d958e" }}>Airtel, MTN, Zamtel Mobile Money</span>
            </div>
          </div>

          <div style={{ fontSize: 10, color: "#8d958e", marginBottom: 12, display: "flex", justifyContent: "space-between" }}>
            <span>Contributing as:</span>
            <strong style={{ color: "#1c221e" }}>{name} ({email})</strong>
          </div>

          {error && <div style={{ color: "#9e463e", fontSize: 11, marginBottom: 10 }}>{error}</div>}

          <button className="btn-auth-submit" style={{ width: "100%" }} type="submit" disabled={working}>
            {working ? "Connecting to Lipila…" : "Continue to Payment"} <ArrowRight size={15} />
          </button>
        </form>
      </div>
    </div>
  );
}

/* ─────────────────────────────────────────────────────────────
   AUTH SCREEN (LOGIN / SIGNUP)
───────────────────────────────────────────────────────────── */
function AuthScreen({
  mode,
  setMode,
  notice,
  setNotice,
}: {
  mode: AuthMode;
  setMode: (mode: AuthMode) => void;
  notice: string;
  setNotice: (msg: string) => void;
}) {
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [working, setWorking] = useState(false);
  const [error, setError] = useState("");

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (!supabase) return;
    setWorking(true);
    setError("");
    setNotice("");
    try {
      if (mode === "signup") {
        const { error: err } = await supabase.auth.signUp({
          email: email.trim(),
          password,
          options: { data: { full_name: name.trim() }, emailRedirectTo: window.location.origin },
        });
        if (err) throw err;
        setNotice("Check your inbox to verify your email. After verification, your membership will be reviewed by an admin.");
      } else if (mode === "reset") {
        const { error: err } = await supabase.auth.resetPasswordForEmail(email.trim(), { redirectTo: window.location.origin });
        if (err) throw err;
        setNotice("Password reset instructions sent to your email.");
      } else {
        const { error: err } = await supabase.auth.signInWithPassword({ email: email.trim(), password });
        if (err) throw err;
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Request failed.");
    } finally {
      setWorking(false);
    }
  }

  return (
    <div className="auth-page-wrap">
      <div className="auth-bento-card">
        <div className="auth-brand-row">
          <SunburstLogo />
          <div>
            <strong style={{ display: "block", fontSize: 14 }}>12E Reunion</strong>
            <span style={{ fontSize: 10, color: "#8d958e" }}>Class of 2017</span>
          </div>
        </div>

        <h1 className="auth-title">
          {mode === "signup" ? "Join the class space" : mode === "reset" ? "Reset your password" : "Welcome back to 12E"}
        </h1>
        <p className="auth-subtitle">
          {mode === "signup" ? "Connect with classmates and support our reunion fund." : mode === "reset" ? "Enter your email to receive a recovery link." : "Sign in to see the class fund, activity, and members."}
        </p>

        {notice && (
          <div style={{ background: "#e6eee8", color: "#376847", padding: "10px 14px", borderRadius: 12, fontSize: 11, marginBottom: 14 }}>
            {notice}
          </div>
        )}

        <form onSubmit={submit}>
          <div className="auth-input-group">
            {mode === "signup" && (
              <div className="auth-field">
                <label>Your Full Name</label>
                <input
                  type="text"
                  placeholder="e.g. Chanda Mwansa"
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  required
                />
              </div>
            )}

            <div className="auth-field">
              <label>Email Address</label>
              <input
                type="email"
                placeholder="you@example.com"
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                required
              />
            </div>

            {mode !== "reset" && (
              <div className="auth-field">
                <div style={{ display: "flex", justifyContent: "space-between" }}>
                  <label>Password</label>
                  {mode === "login" && (
                    <button
                      type="button"
                      style={{ fontSize: 10, color: "#52735e", fontWeight: 600 }}
                      onClick={() => setMode("reset")}
                    >
                      Forgot?
                    </button>
                  )}
                </div>
                <input
                  type="password"
                  placeholder="••••••••"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  required
                />
              </div>
            )}
          </div>

          {error && <div style={{ color: "#9e463e", fontSize: 11, marginBottom: 12 }}>{error}</div>}

          <button className="btn-auth-submit" style={{ width: "100%" }} type="submit" disabled={working}>
            {working ? "Please wait…" : mode === "signup" ? "Create Account" : mode === "reset" ? "Send Reset Link" : "Sign In to 12E"}
          </button>
        </form>

        <div style={{ marginTop: 20, textAlign: "center", fontSize: 11, color: "#8d958e" }}>
          {mode === "signup" ? (
            <>Already have an account? <button style={{ color: "#52735e", fontWeight: 700 }} onClick={() => setMode("login")}>Sign in</button></>
          ) : mode === "reset" ? (
            <button style={{ color: "#52735e", fontWeight: 700 }} onClick={() => setMode("login")}>Back to sign in</button>
          ) : (
            <>New classmate? <button style={{ color: "#52735e", fontWeight: 700 }} onClick={() => setMode("signup")}>Request to join</button></>
          )}
        </div>
      </div>
    </div>
  );
}

/* ─────────────────────────────────────────────────────────────
   WAITING / UNDER REVIEW SCREEN
───────────────────────────────────────────────────────────── */
function WaitingScreen({ profile, onSignOut }: { profile: Profile; onSignOut: () => void }) {
  const rejected = profile.status === "rejected";
  return (
    <div className="auth-page-wrap">
      <div className="auth-bento-card">
        <SunburstLogo />
        <h1 className="auth-title" style={{ marginTop: 14 }}>
          {rejected ? "Membership Update" : "Membership in Review"}
        </h1>
        <p className="auth-subtitle">
          {rejected
            ? "Your request was not approved. Reach out to a class admin if this was a mistake."
            : "Your email is verified! A 12E class administrator will approve your profile shortly."}
        </p>
        <div style={{ background: "#f8f8f5", borderRadius: 14, padding: "12px 16px", display: "flex", justifyContent: "space-between", fontSize: 11, marginBottom: 18 }}>
          <span style={{ fontWeight: 700, color: rejected ? "#9e463e" : "#8d958e" }}>
            {rejected ? "NOT APPROVED" : "PENDING APPROVAL"}
          </span>
          <span style={{ color: "#1c221e" }}>{profile.full_name}</span>
        </div>
        <button className="btn-auth-submit" onClick={onSignOut}>
          Sign Out
        </button>
      </div>
    </div>
  );
}

/* ─────────────────────────────────────────────────────────────
   CONFIG SCREEN
───────────────────────────────────────────────────────────── */
function ConfigurationScreen() {
  return (
    <div className="auth-page-wrap">
      <div className="auth-bento-card">
        <SunburstLogo />
        <h1 className="auth-title" style={{ marginTop: 14 }}>Setup Needed</h1>
        <p className="auth-subtitle">Add your Supabase URL and anon key to start.</p>
        <pre style={{ background: "#f8f8f5", padding: 12, borderRadius: 12, fontSize: 11 }}>
          VITE_SUPABASE_URL=...{"\n"}VITE_SUPABASE_ANON_KEY=...
        </pre>
      </div>
    </div>
  );
}

/* ─────────────────────────────────────────────────────────────
   LOADING SCREEN
───────────────────────────────────────────────────────────── */
function LoadingScreen({ message = "Getting your 12E space ready…" }: { message?: string }) {
  return (
    <div className="auth-page-wrap">
      <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 14 }}>
        <SunburstLogo />
        <div style={{ width: 22, height: 22, border: "2.5px solid #dce8df", borderTopColor: "#52735e", borderRadius: "50%", animation: "spin 0.6s linear infinite" }} />
        <p style={{ fontSize: 12, color: "#8d958e" }}>{message}</p>
      </div>
    </div>
  );
}

function initials(name: string) {
  return name.trim().split(/\s+/).slice(0, 2).map((part) => part[0]?.toUpperCase() ?? "").join("") || "12";
}

export default App;
