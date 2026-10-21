import { createClient } from "@supabase/supabase-js";

const supabaseUrl = import.meta.env.VITE_SUPABASE_URL;
const supabasePublishableKey =
  import.meta.env.VITE_SUPABASE_PUBLISHABLE_KEY || import.meta.env.VITE_SUPABASE_ANON_KEY;

export const isSupabaseConfigured = Boolean(
  supabaseUrl && supabasePublishableKey && !supabaseUrl.includes("YOUR_PROJECT"),
);

export const supabase = isSupabaseConfigured
  ? createClient(supabaseUrl, supabasePublishableKey, {
      auth: {
        autoRefreshToken: true,
        persistSession: true,
        detectSessionInUrl: true,
      },
    })
  : null;

export type Profile = {
  id: string;
  full_name: string;
  role: "member" | "admin";
  status: "pending" | "approved" | "rejected";
  created_at: string;
};

export type Contribution = {
  id: string;
  user_id: string;
  amount: number;
  currency: "ZMW";
  payment_reference: string;
  status: "pending" | "confirmed" | "rejected";
  method: "mobile_money";
  created_at: string;
  confirmed_at: string | null;
  profiles?: { full_name: string } | null;
};

export async function getWorkerToken() {
  if (!supabase) throw new Error("Supabase is not configured.");
  const { data, error } = await supabase.auth.getSession();
  if (error) throw error;
  return data.session?.access_token ?? null;
}

export async function workerRequest<T>(path: string, body?: unknown): Promise<T> {
  const token = await getWorkerToken();
  const response = await fetch(path, {
    method: body ? "POST" : "GET",
    headers: {
      ...(body ? { "content-type": "application/json" } : {}),
      ...(token ? { authorization: `Bearer ${token}` } : {}),
    },
    body: body ? JSON.stringify(body) : undefined,
  });
  const result = (await response.json().catch(() => ({}))) as {
    error?: string;
    message?: string;
  } & T;
  if (!response.ok) {
    throw new Error(result.message || result.error || "The request could not be completed.");
  }
  return result;
}
