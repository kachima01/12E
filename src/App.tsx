import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { FormEvent } from "react";
import {
  ArrowRight,
  Bell,
  Check,
  CheckCircle2,
  Clock3,
  CreditCard,
  Download,
  Eye,
  EyeOff,
  Fingerprint,
  Heart,
  LayoutGrid,
  LogOut,
  Menu,
  MessageSquare,
  Plus,
  Radio,
  Search,
  Settings,
  ShieldCheck,
  Smartphone,
  Sparkles,
  Users,
  X,
} from "lucide-react";
import type { Contribution, Profile } from "./lib/supabase";
import { isSupabaseConfigured, supabase, workerRequest } from "./lib/supabase";

type AuthMode = "login" | "signup" | "reset" | "update_password";
type Page = "overview" | "activity" | "people" | "messages" | "settings" | "admin";
type CheckoutResponse = { reference: string; status: string; message: string };
type ClassMessage = {
  id: string;
  user_id: string;
  message: string;
  created_at: string;
  profiles: { full_name: string } | null;
};
const MEMBER_TARGET = 500;

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
  const [messages, setMessages] = useState<ClassMessage[]>([]);
  const [loadError, setLoadError] = useState("");
  const [showContribute, setShowContribute] = useState(false);
  const [busyId, setBusyId] = useState("");
  const [toast, setToast] = useState("");
  const membersRef = useRef<Profile[]>([]);
  const notifiedContributionIds = useRef(new Set<string>());

  useEffect(() => {
    if (!supabase) {
      setInitializing(false);
      return;
    }
    let alive = true;
    if (window.location.hash.includes("type=recovery")) {
      setAuthMode("update_password");
    }
    void supabase.auth.getSession().then(({ data }) => {
      if (alive) {
        setSession(data.session);
        setInitializing(false);
      }
    });
    const { data: listener } = supabase.auth.onAuthStateChange((event, nextSession) => {
      setSession(nextSession);
      if (event === "PASSWORD_RECOVERY") {
        setAuthMode("update_password");
      }
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
    const userProfile = data as Profile;
    setProfile(userProfile);
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
    membersRef.current = approvedProfiles;
  }, [profile?.status, session]);

  const loadMessages = useCallback(async () => {
    if (!supabase || !session || profile?.status !== "approved") return;
    const { data, error } = await supabase
      .from("class_messages")
      .select("id,user_id,message,created_at,profiles(full_name)")
      .order("created_at", { ascending: false })
      .limit(100);
    if (error) {
      setLoadError(error.message);
      return;
    }
    const messageItems = (data ?? []).map((item) => ({
      ...item,
      profiles: Array.isArray(item.profiles) ? item.profiles[0] ?? null : item.profiles,
    }));
    setMessages(messageItems as ClassMessage[]);
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
      .channel(`reunion-updates-${session.user.id}`)
      .on("postgres_changes", { event: "*", schema: "public", table: "contributions" }, (payload) => {
        const updated = payload.new as Partial<Contribution>;
        const previous = payload.old as Partial<Contribution>;
        if (
          updated.id &&
          updated.status === "confirmed" &&
          previous.status !== "confirmed" &&
          !notifiedContributionIds.current.has(updated.id)
        ) {
          notifiedContributionIds.current.add(updated.id);
          const contributor =
            membersRef.current.find((member) => member.id === updated.user_id)?.full_name ||
            "A 12E classmate";
          const amount = Number(updated.amount);
          setToast(
            updated.user_id === session.user.id
              ? `Your ${currency.format(amount)} contribution is confirmed. Thank you!`
              : `${contributor} just contributed ${currency.format(amount)} to the reunion fund.`,
          );
        }
        void loadData();
      })
      .on("postgres_changes", { event: "*", schema: "public", table: "profiles" }, () => {
        void loadData();
        void loadProfile();
      })
      .on("postgres_changes", { event: "*", schema: "public", table: "class_messages" }, () => {
        void loadMessages();
      })
      .subscribe();
    return () => {
      void client.removeChannel(channel);
    };
  }, [loadData, loadMessages, loadProfile, profile?.status, session]);

  useEffect(() => {
    void loadMessages();
  }, [loadMessages]);

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
  const totalTarget = joiningMembers * MEMBER_TARGET;

  async function signOut() {
    if (!supabase) return;
    const { error } = await supabase.auth.signOut();
    if (error) setToast(error.message);
    else {
      setProfile(null);
      setContributions([]);
      setMembers([]);
      setMessages([]);
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

  if (!session || authMode === "update_password") {
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
    return <WaitingScreen profile={profile} onSignOut={signOut} onRefresh={loadProfile} />;
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
            className={`sidebar-nav-btn ${page === "messages" ? "sidebar-nav-btn--active" : ""}`}
            onClick={() => { setPage("messages"); setMobileNavOpen(false); }}
            title="Class messages"
            aria-label="Class messages"
          >
            <MessageSquare size={19} />
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
            className={`sidebar-nav-btn ${page === "settings" ? "sidebar-nav-btn--active" : ""}`}
            onClick={() => { setPage("settings"); setMobileNavOpen(false); }}
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
              onClick={() => setPage("messages")}
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
            totalTarget={totalTarget}
            confirmed={confirmed}
            contributions={confirmed}
            ownConfirmed={myConfirmed}
            pending={myPending}
            onContribute={() => setShowContribute(true)}
            onViewActivity={() => setPage("activity")}
            onViewPeople={() => setPage("people")}
            onViewSettings={() => setPage("settings")}
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

        {page === "messages" && (
          <MessagesPage
            messages={messages}
            ownId={profile.id}
            onSend={async (message) => {
              if (!supabase) throw new Error("Supabase is not configured.");
              const { error } = await supabase.from("class_messages").insert({
                user_id: profile.id,
                message,
              });
              if (error) throw error;
              await loadMessages();
            }}
          />
        )}

        {page === "settings" && (
          <SettingsPage
            profile={profile}
            onProfileUpdated={async () => {
              await loadProfile();
              setToast("Profile updated.");
            }}
          />
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
          onSuccess={() => { void loadData(); }}
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
  totalTarget,
  confirmed,
  contributions,
  ownConfirmed,
  pending,
  onContribute,
  onViewActivity,
  onViewPeople,
  onViewSettings,
}: {
  profile: Profile;
  raised: number;
  joiningMembers: number;
  totalTarget: number;
  confirmed: Contribution[];
  contributions: Contribution[];
  ownConfirmed: Contribution[];
  pending: Contribution[];
  onContribute: () => void;
  onViewActivity: () => void;
  onViewPeople: () => void;
  onViewSettings: () => void;
}) {
  const ownTotal = ownConfirmed.reduce((sum, item) => sum + Number(item.amount), 0);
  const goalPercent = totalTarget > 0 ? Math.min(100, Math.round((raised / totalTarget) * 100)) : 0;
  const recentTransfers = confirmed.slice(0, 3);
  const monthlyTotals = Array.from({ length: 6 }, (_, index) => {
    const date = new Date();
    date.setDate(1);
    date.setMonth(date.getMonth() - (5 - index));
    return {
      label: new Intl.DateTimeFormat("en-ZM", { month: "short" }).format(date),
      year: date.getFullYear(),
      month: date.getMonth(),
      amount: 0,
    };
  });
  const firstChartMonth = monthlyTotals[0];
  let cumulative = contributions.reduce((total, item) => {
    const date = new Date(item.confirmed_at || item.created_at);
    return date.getFullYear() < firstChartMonth.year ||
      (date.getFullYear() === firstChartMonth.year && date.getMonth() < firstChartMonth.month)
      ? total + Number(item.amount)
      : total;
  }, 0);
  contributions.forEach((item) => {
    const date = new Date(item.confirmed_at || item.created_at);
    const bucket = monthlyTotals.find((value) => value.year === date.getFullYear() && value.month === date.getMonth());
    if (bucket) bucket.amount += Number(item.amount);
  });
  const chartPoints = monthlyTotals.map((bucket, index) => {
    cumulative += bucket.amount;
    return {
      x: (index / Math.max(1, monthlyTotals.length - 1)) * 540,
      y: 88 - (cumulative / Math.max(raised, totalTarget, 1)) * 68,
    };
  });
  const chartPath = chartPoints.map((point, index) => `${index === 0 ? "M" : "L"} ${point.x} ${point.y}`).join(" ");
  const chartArea = `${chartPath} L 540 100 L 0 100 Z`;

  return (
    <>
      {/* ── Top Stats Row (4 Bento Cards) ──────────────────── */}
      <section className="top-stats-row">
        {/* Card 1: Total raised with mini vertical bar chart */}
        <div className="stat-bento-card">
          <div className="stat-bento-copy">
            <span className="stat-bento-label">Total raised</span>
            <span className="stat-bento-value">{currency.format(raised)}</span>
          </div>
          <div className="mini-bar-chart" aria-hidden="true">
            {monthlyTotals.map((bucket) => (
              <div
                className="mini-bar"
                key={`${bucket.year}-${bucket.month}`}
                style={{ height: `${Math.max(8, (bucket.amount / Math.max(...monthlyTotals.map((value) => value.amount), 1)) * 100)}%` }}
              />
            ))}
          </div>
        </div>

        {/* Card 2: Classmates in with circular icon & wavy sparkline */}
        <div className="stat-bento-card" onClick={onViewPeople} style={{ cursor: "pointer" }}>
          <div className="stat-bento-icon-circle stat-bento-icon-circle--mint">
            <Users size={18} />
          </div>
          <div className="stat-bento-copy">
            <span className="stat-bento-label">Classmates joined</span>
            <span className="stat-bento-value">{String(joiningMembers).padStart(2, "0")}</span>
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
            <span className="stat-bento-label">Your contributions</span>
            <span className="stat-bento-value">{currency.format(ownTotal)}</span>
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
            <span className="stat-bento-label">Class target</span>
            <span className="stat-bento-value">{currency.format(totalTarget)}</span>
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
              <span className="balance-card-title">Reunion fund progress</span>
              <span className="badge-pill-ontrack">
                <Check size={11} /> On track
              </span>
            </div>
            <span className="chart-caption">Last 6 months</span>
          </div>

          {/* Floating inner pill cards */}
          <div className="balance-pills-row">
            <div className="balance-floating-pill">
              <span className="balance-pill-label">Target per classmate</span>
              <div className="balance-pill-data">
                <span className="balance-pill-number">{currency.format(MEMBER_TARGET)}</span>
                <span className="percentage-chip percentage-chip--positive">{joiningMembers} members</span>
              </div>
            </div>

            <div className="balance-floating-pill">
              <span className="balance-pill-label">Balance</span>
              <div className="balance-pill-data">
                <span className="balance-pill-number">{currency.format(raised)}</span>
                <span className="percentage-chip percentage-chip--positive">of {currency.format(totalTarget)}</span>
              </div>
            </div>
          </div>

          <div className="balance-chart-wrap" role="img" aria-label={`Contribution progress over the last six months: ${monthlyTotals.map((month) => `${month.label} ${currency.format(month.amount)}`).join(", ")}`}>
            <svg viewBox="0 0 540 100" fill="none" preserveAspectRatio="none">
              <path d={chartArea} fill="rgba(82, 115, 94, 0.08)" />
              <path d={chartPath} stroke="#52735e" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" />
              {chartPoints.map((point, index) => (
                <circle key={index} cx={point.x} cy={point.y} r="3.5" fill="#52735e" />
              ))}
            </svg>
            <div className="chart-month-labels" aria-hidden="true">
              {monthlyTotals.map((month) => <span key={`${month.year}-${month.month}`}>{month.label}</span>)}
            </div>
          </div>
        </div>

        {/* Card 2: Earnings & Semi-circular Gauge Meter */}
        <div className="bento-card gauge-card">
          <div>
            <div className="gauge-header-label">Class target</div>
            <div className="gauge-header-sub">K500 per joined classmate</div>
            <div className="gauge-amount">{currency.format(totalTarget)}</div>
            <div className="gauge-growth-note">{joiningMembers} classmates × {currency.format(MEMBER_TARGET)}</div>
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
                strokeDashoffset={188.5 * (1 - goalPercent / 100)}
                strokeLinecap="round"
              />
            </svg>
            <span className="gauge-percent-text">{goalPercent}%</span>
          </div>
        </div>

        {/* Member profile and personal contribution total */}
        <div className="bento-card profile-bento-card">
          <div className="profile-bento-avatar-wrap">
            <div className="profile-bento-avatar" aria-hidden="true">{initials(profile.full_name)}</div>
          </div>

          <div>
            <h3 className="profile-bento-name">{profile.full_name}</h3>
            <p className="profile-bento-email">12E · Class of 2017</p>
          </div>

          <div className="profile-bento-stats">
            <div className="profile-stat-col">
              <span className="profile-stat-label">Confirmed</span>
              <span className="profile-stat-val">{ownConfirmed.length}</span>
            </div>
            <div className="profile-stat-col">
              <span className="profile-stat-label">Given</span>
              <span className="profile-stat-val">{currency.format(ownTotal)}</span>
            </div>
          </div>
        </div>
      </section>

      {/* ── Bottom Row (Bento Grid: 2 cols + 1 col + 1 col) ─── */}
      <section className="bottom-bento-row">
        {/* Class fund contribution card */}
        <div className="bento-card wallet-bento-card">
          <div className="wallet-card-content">
            <h3 className="wallet-card-title">Every classmate counts.</h3>
            <p className="wallet-card-desc">
              K500 is the class target for each approved member. Add funds using mobile money and help bring 12E back together.
            </p>
            <button className="btn-pill-green" onClick={onContribute}>
              Add funds <Plus size={14} />
            </button>
          </div>

          <MobileMoneyAnimation />
        </div>

        {/* Card 2: Your Transfers / Recent Contributions */}
        <div className="bento-card">
          <div className="transfers-card-title">Latest contributions</div>
          <div className="transfers-list">
            {recentTransfers.length > 0 ? (
              recentTransfers.map((item) => {
                const name = item.profiles?.full_name || "12E Classmate";
                return (
                  <div className="transfer-item-row" key={item.id}>
                    <div className="transfer-item-info">
                      <span className="transfer-item-name">From {name}</span>
                      <span className="transfer-item-time">{formatDate(item.created_at)}</span>
                    </div>
                    <span className="transfer-badge percentage-chip--positive">
                      +{currency.format(Number(item.amount))}
                    </span>
                  </div>
                );
              })
            ) : (
              <p className="transfer-empty">Confirmed contributions will appear here.</p>
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

          <button className="btn-pill-security" onClick={onViewSettings}>
            Update Your Security
          </button>
        </div>
      </section>
    </>
  );
}

function MobileMoneyAnimation() {
  return (
    <div className="mobile-money-visual" aria-label="Mobile money payments supported" role="img">
      <div className="mobile-money-orbit mobile-money-orbit--outer" />
      <div className="mobile-money-orbit mobile-money-orbit--inner" />
      <div className="mobile-money-phone">
        <div className="mobile-money-phone-speaker" />
        <Smartphone size={52} strokeWidth={1.35} />
        <div className="mobile-money-phone-screen">
          <span className="mobile-money-screen-check"><Check size={15} /></span>
          <span>Mobile money</span>
          <strong>Secure prompt</strong>
        </div>
      </div>
      <div className="mobile-money-bubble mobile-money-bubble--left"><Radio size={17} /> MTN</div>
      <div className="mobile-money-bubble mobile-money-bubble--right"><Radio size={17} /> Airtel</div>
      <div className="mobile-money-bubble mobile-money-bubble--bottom">Zamtel</div>
    </div>
  );
}

function MessagesPage({
  messages,
  ownId,
  onSend,
}: {
  messages: ClassMessage[];
  ownId: string;
  onSend: (message: string) => Promise<void>;
}) {
  const [draft, setDraft] = useState("");
  const [sending, setSending] = useState(false);
  const [error, setError] = useState("");

  async function submit(event: FormEvent) {
    event.preventDefault();
    const text = draft.trim();
    if (!text || text.length > 500) {
      setError("Messages must be between 1 and 500 characters.");
      return;
    }
    setSending(true);
    setError("");
    try {
      await onSend(text);
      setDraft("");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Could not post your message.");
    } finally {
      setSending(false);
    }
  }

  return (
    <section className="subpage-container">
      <div className="subpage-header-row">
        <div className="subpage-heading-wrap">
          <h1>Class messages</h1>
          <p>Share a reunion thought or leave a message for the class.</p>
        </div>
      </div>
      <div className="subpage-panel message-board-panel">
        <form className="message-compose" onSubmit={submit}>
          <label className="settings-label" htmlFor="class-message">Write a message</label>
          <textarea
            id="class-message"
            value={draft}
            maxLength={500}
            rows={3}
            placeholder="What would you like to tell your classmates?"
            onChange={(event) => setDraft(event.target.value)}
            required
          />
          <div className="message-compose-footer">
            <span>{draft.length}/500</span>
            <button className="btn-pill-green" type="submit" disabled={sending || !draft.trim()}>
              {sending ? "Posting…" : "Post message"} <ArrowRight size={14} />
            </button>
          </div>
          {error && <p className="settings-error" role="alert">{error}</p>}
        </form>
        <div className="message-list" aria-live="polite">
          {messages.length ? messages.map((item) => {
            const author = item.profiles?.full_name || (item.user_id === ownId ? "You" : "12E classmate");
            return (
              <article className="class-message" key={item.id}>
                <div className="c-avatar-chip">{initials(author)}</div>
                <div className="class-message-content">
                  <div className="class-message-meta">
                    <strong>{item.user_id === ownId ? "You" : author}</strong>
                    <time dateTime={item.created_at}>{formatDate(item.created_at)}</time>
                  </div>
                  <p>{item.message}</p>
                </div>
              </article>
            );
          }) : <p className="message-empty">No messages yet. Start the conversation!</p>}
        </div>
      </div>
    </section>
  );
}

function SettingsPage({
  profile,
  onProfileUpdated,
}: {
  profile: Profile;
  onProfileUpdated: () => Promise<void>;
}) {
  const [name, setName] = useState(profile.full_name);
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [nameBusy, setNameBusy] = useState(false);
  const [passwordBusy, setPasswordBusy] = useState(false);
  const [notice, setNotice] = useState("");
  const [error, setError] = useState("");

  async function saveName(event: FormEvent) {
    event.preventDefault();
    if (!supabase) return;
    const value = name.trim();
    if (value.length < 2 || value.length > 80) {
      setError("Name must be between 2 and 80 characters.");
      return;
    }
    setNameBusy(true);
    setError("");
    setNotice("");
    try {
      const { error: updateError } = await supabase.from("profiles").update({ full_name: value }).eq("id", profile.id);
      if (updateError) throw updateError;
      await onProfileUpdated();
      setNotice("Your profile name has been updated.");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Could not update your profile.");
    } finally {
      setNameBusy(false);
    }
  }

  async function changePassword(event: FormEvent) {
    event.preventDefault();
    if (!supabase) return;
    if (password.length < 8) {
      setError("Use at least 8 characters for your new password.");
      return;
    }
    if (password !== confirmPassword) {
      setError("The password confirmation does not match.");
      return;
    }
    setPasswordBusy(true);
    setError("");
    setNotice("");
    try {
      const { error: updateError } = await supabase.auth.updateUser({ password });
      if (updateError) throw updateError;
      setPassword("");
      setConfirmPassword("");
      setNotice("Your password has been changed.");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Could not update your password.");
    } finally {
      setPasswordBusy(false);
    }
  }

  return (
    <section className="subpage-container">
      <div className="subpage-header-row">
        <div className="subpage-heading-wrap">
          <h1>Profile settings</h1>
          <p>Manage your 12E profile and account security.</p>
        </div>
      </div>
      <div className="settings-grid">
        <form className="subpage-panel settings-panel" onSubmit={saveName}>
          <div className="settings-panel-heading"><Users size={19} /><h2>Profile details</h2></div>
          <label className="settings-label" htmlFor="profile-name">Name shown to classmates</label>
          <input id="profile-name" className="settings-input" value={name} maxLength={80} onChange={(event) => setName(event.target.value)} required />
          <p className="settings-help">Your name is used on the classmate list and message board.</p>
          <button className="btn-pill-green" type="submit" disabled={nameBusy}>{nameBusy ? "Saving…" : "Save profile"}</button>
        </form>
        <form className="subpage-panel settings-panel" onSubmit={changePassword}>
          <div className="settings-panel-heading"><Fingerprint size={20} /><h2>Change password</h2></div>
          <label className="settings-label" htmlFor="new-password">New password</label>
          <input id="new-password" className="settings-input" type="password" autoComplete="new-password" value={password} onChange={(event) => setPassword(event.target.value)} minLength={8} required />
          <label className="settings-label" htmlFor="confirm-new-password">Confirm new password</label>
          <input id="confirm-new-password" className="settings-input" type="password" autoComplete="new-password" value={confirmPassword} onChange={(event) => setConfirmPassword(event.target.value)} minLength={8} required />
          <p className="settings-help">Use at least 8 characters. Your password is managed securely by Supabase Auth.</p>
          <button className="btn-pill-green" type="submit" disabled={passwordBusy}>{passwordBusy ? "Updating…" : "Update password"}</button>
        </form>
      </div>
      {notice && <p className="settings-notice" role="status">{notice}</p>}
      {error && <p className="settings-error" role="alert">{error}</p>}
    </section>
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
  onSuccess,
}: {
  onClose: () => void;
  onSuccess: () => void;
}) {
  const [amount, setAmount] = useState("500");
  const [phone, setPhone] = useState("");
  const [working, setWorking] = useState(false);
  const [error, setError] = useState("");
  const [statusWarning, setStatusWarning] = useState("");
  const [phase, setPhase] = useState<"form" | "starting" | "processing" | "confirmed" | "failed" | "waiting">("form");
  const [paymentReference, setPaymentReference] = useState("");
  const pollingRef = useRef(false);
  const amounts = [250, 500, 1000, 2000];

  async function waitForPayment(reference: string) {
    setPhase("processing");
    const deadline = Date.now() + 120_000;
    while (pollingRef.current && Date.now() < deadline) {
      try {
        const result = await workerRequest<{ status: Contribution["status"] }>(
          `/api/payment/status?reference=${encodeURIComponent(reference)}`,
        );
        if (!pollingRef.current) return;
        setStatusWarning("");
        if (result.status === "confirmed") {
          setPhase("confirmed");
          onSuccess();
          return;
        }
        if (result.status === "rejected") {
          setPhase("failed");
          return;
        }
      } catch (err) {
        setStatusWarning(err instanceof Error ? err.message : "Could not check payment status.");
      }
      await new Promise((resolve) => window.setTimeout(resolve, 2500));
    }
    if (pollingRef.current) setPhase("waiting");
  }

  function closeDialog() {
    pollingRef.current = false;
    onClose();
  }

  async function submit(e: FormEvent) {
    e.preventDefault();
    const num = Number(amount);
    if (!Number.isFinite(num) || num < 1 || num > 100000) {
      setError("Enter an amount between K1 and K100,000.");
      return;
    }
    const normalizedPhone = normalizeZambianPhone(phone);
    if (!normalizedPhone) {
      setError("Enter all 9 digits of your Zambian mobile number.");
      return;
    }
    setWorking(true);
    setError("");
    setStatusWarning("");
    setPhase("starting");
    pollingRef.current = true;
    try {
      const collection = await workerRequest<CheckoutResponse>("/api/payment/checkout", {
        amount: num,
        phone: normalizedPhone,
      });
      if (!collection.reference || !collection.status || !collection.message) {
        throw new Error("Lipila returned an incomplete collection response.");
      }
      setPaymentReference(collection.reference);
      await waitForPayment(collection.reference);
    } catch (err) {
      const msg = err instanceof Error ? err.message : "Could not start payment.";
      setError(msg);
      setPhase("form");
    } finally {
      setWorking(false);
    }
  }

  return (
    <div className="modal-overlay" onMouseDown={(e) => { if (e.target === e.currentTarget && phase !== "starting" && phase !== "processing") closeDialog(); }}>
      <div className="modal-window">
        <button className="modal-close-btn" onClick={closeDialog} aria-label="Close" disabled={phase === "starting"}>
          <X size={16} />
        </button>

        <div style={{ width: 40, height: 40, borderRadius: "50%", background: "#e6eee8", color: "#52735e", display: "grid", placeItems: "center", marginBottom: 12 }}>
          <Heart size={20} />
        </div>

        <h2 className="modal-title">
          {phase === "confirmed" ? "Payment confirmed" : phase === "failed" ? "Payment not completed" : phase === "processing" || phase === "starting" ? "Payment processing" : phase === "waiting" ? "Still waiting for confirmation" : "Contribute to 12E Fund"}
        </h2>
        <p className="modal-sub">
          {phase === "confirmed"
            ? "Lipila confirmed your payment. Your contribution has been added to the reunion fund."
            : phase === "failed"
              ? "Lipila reported that this payment did not go through. No contribution was added."
              : phase === "processing" || phase === "starting"
                ? "Approve the payment request on your mobile-money phone. We’re waiting for Lipila to confirm the result."
                : phase === "waiting"
                  ? "The payment is still pending. Check your phone and wait for the mobile-money prompt or callback."
                  : "Choose an amount that works for you. Every contribution helps bring our class back together."}
        </p>

        {phase === "starting" || phase === "processing" ? (
          <div role="status" aria-live="polite" style={{ background: "#f8f8f5", borderRadius: 14, padding: "24px 16px", textAlign: "center", marginTop: 18 }}>
            <div style={{ width: 34, height: 34, border: "3px solid #dce8df", borderTopColor: "#52735e", borderRadius: "50%", animation: "spin 0.8s linear infinite", margin: "0 auto 14px" }} />
            <strong style={{ color: "#1c221e", fontSize: 14 }}>Amount: {currency.format(Number(amount) || 0)}</strong>
            <p style={{ color: "#8d958e", fontSize: 11, lineHeight: 1.6, margin: "8px 0 0" }}>
              {phase === "starting" ? "Sending the payment request to Lipila…" : "Payment prompt sent. Approve it on your phone."}
            </p>
          </div>
        ) : phase === "confirmed" || phase === "failed" ? (
          <div role="status" aria-live="polite" style={{ background: phase === "confirmed" ? "#e6eee8" : "#fbeae8", borderRadius: 14, padding: "20px 16px", textAlign: "center", marginTop: 18 }}>
            <div style={{ width: 42, height: 42, borderRadius: "50%", background: phase === "confirmed" ? "#52735e" : "#9e463e", color: "white", display: "grid", placeItems: "center", margin: "0 auto 12px" }}>
              {phase === "confirmed" ? <Check size={22} /> : <X size={22} />}
            </div>
            <strong style={{ color: "#1c221e", fontSize: 15 }}>
              {phase === "confirmed" ? `${currency.format(Number(amount) || 0)} added` : "Payment failed"}
            </strong>
          </div>
        ) : phase === "waiting" ? (
          <div role="status" aria-live="polite" style={{ background: "#f8f8f5", borderRadius: 14, padding: "18px 16px", textAlign: "center", marginTop: 18 }}>
            <Clock3 size={25} color="#52735e" />
            <p style={{ color: "#8d958e", fontSize: 11, lineHeight: 1.6, margin: "8px 0 0" }}>
              No final callback received yet. Your contribution remains pending and won’t be counted until confirmed.
            </p>
            {statusWarning && <p style={{ color: "#9e463e", fontSize: 11, lineHeight: 1.5, margin: "8px 0 0" }}>{statusWarning}</p>}
          </div>
        ) : (
        <form onSubmit={submit}>
          <div className="amount-input-box">
            <span className="amount-prefix">K</span>
            <input
              type="number"
              min="1"
              max="100000"
              step="0.01"
              aria-label="Contribution amount in Zambian kwacha"
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

          <div className="auth-field" style={{ marginBottom: 16 }}>
            <label htmlFor="payer-phone">Mobile money number to charge</label>
            <div style={{ display: "flex", alignItems: "center", border: "1px solid #e8e6df", borderRadius: 10, background: "#fff", padding: "0 12px" }}>
              <span aria-hidden="true" style={{ color: "#52735e", fontSize: 13, fontWeight: 700, paddingRight: 9, borderRight: "1px solid #e8e6df" }}>+260</span>
              <input
                id="payer-phone"
                type="tel"
                inputMode="numeric"
                autoComplete="tel-national"
                placeholder="971234567"
                aria-label="Nine-digit Zambian mobile number"
                value={phone}
                maxLength={9}
                pattern="[79][0-9]{8}"
                onChange={(e) => {
                  let digits = e.target.value.replace(/\D/g, "");
                  if (digits.startsWith("260")) digits = digits.slice(3);
                  if (digits.startsWith("0")) digits = digits.slice(1);
                  setPhone(digits.slice(0, 9));
                }}
                style={{ border: 0, outline: 0, boxShadow: "none", flex: 1, minWidth: 0, paddingLeft: 10 }}
                required
              />
            </div>
            <span style={{ color: "#8d958e", fontSize: 10 }}>Enter 9 digits after +260. A leading 0 is ignored.</span>
          </div>

          <div style={{ background: "#e6eee8", borderRadius: 12, padding: "10px 12px", marginBottom: 12, display: "flex", justifyContent: "space-between", alignItems: "center", gap: 8 }}>
            <span style={{ color: "#52735e", fontSize: 10 }}>Added to the reunion fund after payment confirmation</span>
            <strong style={{ color: "#1c221e", whiteSpace: "nowrap" }}>
              {currency.format(Number(amount) || 0)}
            </strong>
          </div>

          {error && <div style={{ color: "#9e463e", fontSize: 11, marginBottom: 10 }}>{error}</div>}
          <button className="btn-auth-submit" style={{ width: "100%" }} type="submit" disabled={working}>
            Send Payment Prompt <ArrowRight size={15} />
          </button>
        </form>
        )}
        {phase === "waiting" && (
          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 10, marginTop: 16 }}>
            <button className="btn-auth-submit" type="button" onClick={() => { pollingRef.current = true; void waitForPayment(paymentReference); }}>
              Check again
            </button>
            <button type="button" onClick={closeDialog} style={{ border: "1px solid #e8e6df", borderRadius: 12, fontWeight: 700, fontSize: 12 }}>
              Close
            </button>
          </div>
        )}
        {(phase === "confirmed" || phase === "failed") && (
          <button className="btn-auth-submit" style={{ width: "100%", marginTop: 16 }} type="button" onClick={closeDialog}>
            Done
          </button>
        )}
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
  const [showPassword, setShowPassword] = useState(false);
  const [working, setWorking] = useState(false);
  const [error, setError] = useState("");
  const [canResendConfirmation, setCanResendConfirmation] = useState(false);

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (!supabase) return;
    setWorking(true);
    setError("");
    setCanResendConfirmation(false);
    setNotice("");
    try {
      if (mode === "signup") {
        const { error: err } = await supabase.auth.signUp({
          email: email.trim(),
          password,
          options: { data: { full_name: name.trim() }, emailRedirectTo: window.location.origin },
        });
        if (err) throw err;
        setNotice("Account created! Check your inbox if you need to confirm your email, or sign in to get started.");
      } else if (mode === "reset") {
        const { error: err } = await supabase.auth.resetPasswordForEmail(email.trim(), { redirectTo: window.location.origin });
        if (err) throw err;
        setNotice("Password reset instructions sent to your email.");
      } else if (mode === "update_password") {
        const { error: err } = await supabase.auth.updateUser({ password });
        if (err) throw err;
        setNotice("Your password has been updated! You can now sign in.");
        setMode("login");
      } else {
        const { error: err } = await supabase.auth.signInWithPassword({ email: email.trim(), password });
        if (err) throw err;
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : "Request failed.";
      const isEmailRateLimit = /email rate limit exceeded|rate limit.*email/i.test(message);
      setCanResendConfirmation(isEmailRateLimit && mode === "signup");
      setError(
        isEmailRateLimit
          ? "Supabase has temporarily limited verification emails. Wait for its email limit to reset before trying again."
          : message,
      );
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
          {mode === "signup"
            ? "Join the class space"
            : mode === "reset"
            ? "Reset your password"
            : mode === "update_password"
            ? "Set a new password"
            : "Welcome back to 12E"}
        </h1>
        <p className="auth-subtitle">
          {mode === "signup"
            ? "Connect with classmates and support our reunion fund."
            : mode === "reset"
            ? "Enter your email to receive a recovery link."
            : mode === "update_password"
            ? "Enter your new password below to regain access."
            : "Sign in to see the class fund, activity, and members."}
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

            {mode !== "update_password" && (
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
            )}

            {mode !== "reset" && (
              <div className="auth-field">
                <div style={{ display: "flex", justifyContent: "space-between" }}>
                  <label>{mode === "update_password" ? "New Password" : "Password"}</label>
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
                <div style={{ position: "relative", display: "flex", alignItems: "center" }}>
                  <input
                    type={showPassword ? "text" : "password"}
                    placeholder="At least 8 characters"
                    value={password}
                    onChange={(e) => setPassword(e.target.value)}
                    style={{ width: "100%", paddingRight: 40 }}
                    minLength={8}
                    required
                  />
                  <button
                    type="button"
                    onClick={() => setShowPassword(!showPassword)}
                    style={{
                      position: "absolute",
                      right: 12,
                      background: "none",
                      border: "none",
                      color: "#8d958e",
                      display: "grid",
                      placeItems: "center",
                      cursor: "pointer",
                      padding: 4,
                    }}
                    aria-label={showPassword ? "Hide password" : "Show password"}
                    title={showPassword ? "Hide password" : "Show password"}
                  >
                    {showPassword ? <EyeOff size={16} /> : <Eye size={16} />}
                  </button>
                </div>
              </div>
            )}
          </div>

          {error && <div style={{ color: "#9e463e", fontSize: 11, marginBottom: 12 }}>{error}</div>}
          {canResendConfirmation && (
            <button
              type="button"
              className="btn-pill-green"
              style={{ width: "100%", marginBottom: 12 }}
              disabled={working || !email.trim()}
              onClick={async () => {
                if (!supabase) return;
                setWorking(true);
                const { error: resendError } = await supabase.auth.resend({
                  type: "signup",
                  email: email.trim(),
                });
                setWorking(false);
                if (resendError) {
                  setError(/rate limit/i.test(resendError.message)
                    ? "Email sending is still rate-limited. Wait for Supabase's email limit to reset, then retry."
                    : resendError.message);
                } else {
                  setError("");
                  setNotice("Verification email sent. Check your inbox and spam folder.");
                  setCanResendConfirmation(false);
                }
              }}
            >
              Resend verification email
            </button>
          )}

          <button className="btn-auth-submit" style={{ width: "100%" }} type="submit" disabled={working}>
            {working
              ? "Please wait…"
              : mode === "signup"
              ? "Create Account"
              : mode === "reset"
              ? "Send Reset Link"
              : mode === "update_password"
              ? "Update Password"
              : "Sign In to 12E"}
          </button>
        </form>

        <div style={{ marginTop: 20, textAlign: "center", fontSize: 11, color: "#8d958e" }}>
          {mode === "signup" ? (
            <>Already have an account? <button style={{ color: "#52735e", fontWeight: 700 }} onClick={() => setMode("login")}>Sign in</button></>
          ) : mode === "reset" || mode === "update_password" ? (
            <button style={{ color: "#52735e", fontWeight: 700 }} onClick={() => setMode("login")}>Back to sign in</button>
          ) : (
            <>New classmate? <button style={{ color: "#52735e", fontWeight: 700 }} onClick={() => setMode("signup")}>Create account</button></>
          )}
        </div>
      </div>
    </div>
  );
}

/* ─────────────────────────────────────────────────────────────
   WAITING / UNDER REVIEW SCREEN
───────────────────────────────────────────────────────────── */
function WaitingScreen({
  profile,
  onSignOut,
  onRefresh,
}: {
  profile: Profile;
  onSignOut: () => void;
  onRefresh: () => void;
}) {
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
        {!rejected && (
          <button className="btn-auth-submit" style={{ marginBottom: 10 }} onClick={onRefresh}>
            Check approval status
          </button>
        )}
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

function normalizeZambianPhone(phone: string) {
  const digits = phone.replace(/[^\d]/g, "");
  const local = digits.startsWith("260") ? digits.slice(3) : digits.startsWith("0") ? digits.slice(1) : digits;
  if (!/^[79]\d{8}$/.test(local)) return null;
  return `+260${local}`;
}

export default App;
