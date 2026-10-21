const json = (body, status = 200) =>
  new Response(JSON.stringify(body), {
    status,
    headers: {
      "content-type": "application/json; charset=utf-8",
      "cache-control": "no-store",
      "x-content-type-options": "nosniff",
    },
  });

function getBearerToken(request) {
  const header = request.headers.get("authorization") || "";
  return header.startsWith("Bearer ") ? header.slice(7) : "";
}

function supabaseHeaders(env, token = env.SUPABASE_SECRET_KEY) {
  return {
    apikey: env.SUPABASE_SECRET_KEY,
    authorization: `Bearer ${token}`,
    "content-type": "application/json",
  };
}

async function supabaseRequest(env, path, options = {}) {
  const response = await fetch(`${env.SUPABASE_URL.replace(/\/$/, "")}${path}`, {
    ...options,
    headers: { ...supabaseHeaders(env), ...options.headers },
  });
  if (!response.ok) {
    const detail = await response.text();
    console.error("Supabase request failed", response.status, detail.slice(0, 500));
    throw new Error("The member service could not complete this request.");
  }
  if (response.status === 204) return null;
  return response.json();
}

async function authenticate(request, env) {
  const accessToken = getBearerToken(request);
  if (!accessToken || !env.SUPABASE_URL || !env.SUPABASE_PUBLISHABLE_KEY || !env.SUPABASE_SECRET_KEY) {
    return null;
  }
  const response = await fetch(`${env.SUPABASE_URL.replace(/\/$/, "")}/auth/v1/user`, {
    headers: {
      apikey: env.SUPABASE_PUBLISHABLE_KEY,
      authorization: `Bearer ${accessToken}`,
    },
  });
  if (!response.ok) return null;
  const user = await response.json();
  return user?.id && user?.email ? { user, accessToken } : null;
}

async function getProfile(env, userId) {
  const rows = await supabaseRequest(
    env,
    `/rest/v1/profiles?id=eq.${encodeURIComponent(userId)}&select=id,full_name,role,status`,
  );
  return rows?.[0] || null;
}

async function getContribution(env, reference) {
  const rows = await supabaseRequest(
    env,
    `/rest/v1/contributions?payment_reference=eq.${encodeURIComponent(reference)}&select=id,user_id,amount,currency,payment_reference,status,receipt_sent_at`,
  );
  return rows?.[0] || null;
}

function randomReference() {
  return `12E-${crypto.randomUUID().replaceAll("-", "").slice(0, 20).toUpperCase()}`;
}

async function createCheckout(request, env) {
  const auth = await authenticate(request, env);
  if (!auth) return json({ error: "Please sign in again to continue." }, 401);
  if (!env.LIPILA_CHECKOUT_URL || !env.LIPILA_API_KEY) {
    return json({ error: "Lipila checkout is not configured yet. Ask the class admin to finish payment setup." }, 503);
  }
  let gatewayUrl;
  try {
    gatewayUrl = new URL(env.LIPILA_CHECKOUT_URL);
  } catch {
    return json({ error: "Lipila checkout configuration is invalid." }, 503);
  }
  if (gatewayUrl.protocol !== "https:") {
    return json({ error: "Lipila checkout must use a secure HTTPS endpoint." }, 503);
  }
  const profile = await getProfile(env, auth.user.id);
  if (!profile || profile.status !== "approved") {
    return json({ error: "Your membership must be approved before you can contribute." }, 403);
  }

  let body;
  try {
    body = await request.json();
  } catch {
    return json({ error: "Invalid request body." }, 400);
  }
  const amount = Number(body.amount);
  if (!Number.isFinite(amount) || amount < 1 || amount > 100000 || Math.round(amount * 100) !== amount * 100) {
    return json({ error: "Enter a valid amount in ZMW." }, 400);
  }

  const reference = randomReference();
  await supabaseRequest(env, "/rest/v1/contributions", {
    method: "POST",
    headers: { prefer: "return=minimal" },
    body: JSON.stringify({
      user_id: auth.user.id,
      amount,
      currency: "ZMW",
      payment_reference: reference,
      method: "mobile_money",
      status: "pending",
    }),
  });

  const requestUrl = new URL(request.url);
  const checkoutPayload = {
    amount,
    currency: "ZMW",
    reference,
    customer: { name: profile.full_name, email: auth.user.email },
    return_url: `${requestUrl.origin}/?payment=return`,
    webhook_url: `${requestUrl.origin}/api/webhooks/lipila`,
  };

  let gatewayResponse;
  try {
    gatewayResponse = await fetch(gatewayUrl, {
      method: "POST",
      headers: {
        authorization: `Bearer ${env.LIPILA_API_KEY}`,
        "content-type": "application/json",
      },
      body: JSON.stringify(checkoutPayload),
    });
  } catch (error) {
    console.error("Lipila checkout request failed", error);
    await markCheckoutRejected(env, reference);
    return json({ error: "Lipila could not be reached. Please try again." }, 502);
  }

  const providerBody = await gatewayResponse.json().catch(() => null);
  const checkoutUrl = providerBody?.checkout_url || providerBody?.checkoutUrl;
  let parsedCheckoutUrl;
  try {
    parsedCheckoutUrl = new URL(checkoutUrl);
  } catch {
    parsedCheckoutUrl = null;
  }
  if (!gatewayResponse.ok || parsedCheckoutUrl?.protocol !== "https:") {
    console.error("Lipila checkout rejected", gatewayResponse.status);
    await markCheckoutRejected(env, reference);
    return json({ error: "Lipila did not return a valid secure checkout link." }, 502);
  }
  return json({ checkoutUrl: parsedCheckoutUrl.toString(), reference }, 201);
}

async function markCheckoutRejected(env, reference) {
  await supabaseRequest(
    env,
    `/rest/v1/contributions?payment_reference=eq.${encodeURIComponent(reference)}&status=eq.pending`,
    {
      method: "PATCH",
      headers: { prefer: "return=minimal" },
      body: JSON.stringify({ status: "rejected" }),
    },
  );
}

function equalSignature(left, right) {
  if (!left || !right || left.length !== right.length) return false;
  let result = 0;
  for (let index = 0; index < left.length; index += 1) {
    result |= left.charCodeAt(index) ^ right.charCodeAt(index);
  }
  return result === 0;
}

async function hmacHex(secret, body) {
  const key = await crypto.subtle.importKey(
    "raw",
    new TextEncoder().encode(secret),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const signature = await crypto.subtle.sign("HMAC", key, new TextEncoder().encode(body));
  return Array.from(new Uint8Array(signature), (byte) => byte.toString(16).padStart(2, "0")).join("");
}

async function findAuthEmail(env, userId) {
  const response = await fetch(
    `${env.SUPABASE_URL.replace(/\/$/, "")}/auth/v1/admin/users/${encodeURIComponent(userId)}`,
    { headers: supabaseHeaders(env) },
  );
  if (!response.ok) {
    console.error("Could not retrieve receipt recipient", response.status);
    throw new Error("Could not retrieve the member email.");
  }
  const user = await response.json();
  if (!user.email) throw new Error("The member account has no email address.");
  return user.email;
}

function escapeHtml(value) {
  return String(value).replace(/[&<>"']/g, (character) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  })[character]);
}

async function sendReceipt(env, contribution) {
  if (!env.RESEND_API_KEY || !env.RESEND_FROM_EMAIL) {
    throw new Error("Resend receipt email is not configured.");
  }
  const [profile] = await supabaseRequest(
    env,
    `/rest/v1/profiles?id=eq.${encodeURIComponent(contribution.user_id)}&select=full_name`,
  );
  const email = await findAuthEmail(env, contribution.user_id);
  const name = profile?.full_name || "12E classmate";
  const amount = new Intl.NumberFormat("en-ZM", {
    style: "currency",
    currency: "ZMW",
    maximumFractionDigits: 2,
  }).format(Number(contribution.amount));
  const safeName = escapeHtml(name);
  const safeAmount = escapeHtml(amount);
  const reference = escapeHtml(contribution.payment_reference);
  const response = await fetch("https://api.resend.com/emails", {
    method: "POST",
    headers: {
      authorization: `Bearer ${env.RESEND_API_KEY}`,
      "content-type": "application/json",
      "idempotency-key": `12e-receipt-${contribution.id}`,
    },
    body: JSON.stringify({
      from: env.RESEND_FROM_EMAIL,
      to: [email],
      subject: "Your 12E reunion contribution is confirmed",
      text: `Hi ${name}, your contribution of ${amount} to the 12E reunion fund is confirmed. Reference: ${contribution.payment_reference}. Thank you for helping bring the class together.`,
      html: `<div style="font-family:Arial,sans-serif;color:#26372a;max-width:560px;margin:auto;padding:32px"><div style="font-size:23px;font-weight:700;letter-spacing:-2px;background:#304a37;color:#fff;width:48px;height:42px;text-align:center;padding-top:10px;border-radius:12px">12<span style="color:#d5e784">E</span></div><p style="color:#718258;font-size:11px;letter-spacing:1px;font-weight:bold;margin-top:28px">DAVID LIVINGSTONE SECONDARY · CLASS OF 2017</p><h1 style="font-size:27px;margin:10px 0">Thanks for showing up, ${safeName}.</h1><p style="color:#667067;line-height:1.7">Your contribution to the 12E reunion fund has been confirmed.</p><div style="background:#f2f4ec;border-radius:10px;padding:20px;margin:22px 0"><p style="margin:0;color:#77816f;font-size:12px">CONTRIBUTION</p><p style="font-size:26px;font-weight:bold;margin:7px 0 14px">${safeAmount}</p><p style="font-size:12px;margin:0;color:#667067">Reference: ${reference}</p></div><p style="color:#667067;line-height:1.7">Every bit brings the class closer. See you at the reunion.</p><p style="color:#9a9f96;font-size:11px;margin-top:30px">12E Reunion Fund · Still our class. Still showing up.</p></div>`,
    }),
  });
  if (!response.ok) {
    const detail = await response.text();
    console.error("Resend email failed", response.status, detail.slice(0, 500));
    throw new Error("Resend could not deliver the contribution receipt.");
  }
}

async function confirmContribution(env, contribution) {
  if (!contribution.receipt_sent_at) {
    await sendReceipt(env, contribution);
    await supabaseRequest(
      env,
      `/rest/v1/contributions?id=eq.${encodeURIComponent(contribution.id)}`,
      {
        method: "PATCH",
        headers: { prefer: "return=minimal" },
        body: JSON.stringify({ receipt_sent_at: new Date().toISOString() }),
      },
    );
  }
}

async function handlePaymentWebhook(request, env) {
  if (!env.LIPILA_WEBHOOK_SECRET) return json({ error: "Payment webhook is not configured." }, 503);
  const rawBody = await request.text();
  const expected = await hmacHex(env.LIPILA_WEBHOOK_SECRET, rawBody);
  if (!equalSignature((request.headers.get("x-lipila-signature") || "").toLowerCase(), expected)) {
    return json({ error: "Invalid webhook signature." }, 401);
  }

  let event;
  try {
    event = JSON.parse(rawBody);
  } catch {
    return json({ error: "Invalid JSON payload." }, 400);
  }
  if (String(event.status || "").toLowerCase() !== "success") return json({ ok: true, ignored: true });
  const reference = String(event.reference || "").trim();
  if (!reference) return json({ error: "Missing payment reference." }, 400);

  const contribution = await getContribution(env, reference);
  if (!contribution) return json({ error: "Contribution not found." }, 404);
  const paidAmount = Number(event.amount);
  if (!Number.isFinite(paidAmount) || Math.round(paidAmount * 100) !== Math.round(Number(contribution.amount) * 100)) {
    return json({ error: "Payment amount does not match the contribution." }, 400);
  }
  if (contribution.status !== "confirmed") {
    await supabaseRequest(
      env,
      `/rest/v1/contributions?id=eq.${encodeURIComponent(contribution.id)}&status=eq.pending`,
      {
        method: "PATCH",
        headers: { prefer: "return=minimal" },
        body: JSON.stringify({ status: "confirmed", confirmed_at: new Date().toISOString() }),
      },
    );
  }
  try {
    await confirmContribution(env, contribution);
  } catch (error) {
    console.error("Receipt delivery is pending retry", error);
    return json({ error: "Payment was recorded but the receipt could not be sent yet." }, 503);
  }
  return json({ ok: true });
}

async function confirmAsAdmin(request, env) {
  const auth = await authenticate(request, env);
  if (!auth) return json({ error: "Please sign in again." }, 401);
  const profile = await getProfile(env, auth.user.id);
  if (!profile || profile.role !== "admin" || profile.status !== "approved") {
    return json({ error: "Admin access is required." }, 403);
  }
  let body;
  try {
    body = await request.json();
  } catch {
    return json({ error: "Invalid request body." }, 400);
  }
  const id = String(body.id || "");
  if (!/^[0-9a-f-]{36}$/i.test(id)) return json({ error: "Invalid contribution." }, 400);
  const [contribution] = await supabaseRequest(
    env,
    `/rest/v1/contributions?id=eq.${encodeURIComponent(id)}&select=id,user_id,amount,currency,payment_reference,status,receipt_sent_at`,
  );
  if (!contribution) return json({ error: "Contribution not found." }, 404);
  if (contribution.status === "rejected") return json({ error: "Rejected contributions cannot be confirmed." }, 409);
  if (contribution.status !== "confirmed") {
    await supabaseRequest(
      env,
      `/rest/v1/contributions?id=eq.${encodeURIComponent(id)}&status=eq.pending`,
      {
        method: "PATCH",
        headers: { prefer: "return=minimal" },
        body: JSON.stringify({ status: "confirmed", confirmed_at: new Date().toISOString() }),
      },
    );
  }
  const confirmed = { ...contribution, status: "confirmed", receipt_sent_at: contribution.receipt_sent_at };
  try {
    await confirmContribution(env, confirmed);
  } catch (error) {
    console.error("Admin-confirmed receipt delivery failed", error);
    return json({ error: "The contribution was confirmed, but its email receipt could not be sent." }, 502);
  }
  return json({ ok: true });
}

async function routeApi(request, env) {
  const url = new URL(request.url);
  if (url.pathname === "/api/health" && request.method === "GET") {
    return json({
      ok: true,
      services: {
        supabase: Boolean(env.SUPABASE_URL && env.SUPABASE_PUBLISHABLE_KEY && env.SUPABASE_SECRET_KEY),
        lipila: Boolean(env.LIPILA_CHECKOUT_URL && env.LIPILA_API_KEY && env.LIPILA_WEBHOOK_SECRET),
        resend: Boolean(env.RESEND_API_KEY && env.RESEND_FROM_EMAIL),
      },
      time: new Date().toISOString(),
    });
  }
  if (url.pathname === "/api/payment/checkout" && request.method === "POST") {
    try {
      return await createCheckout(request, env);
    } catch (error) {
      console.error("Could not create Lipila checkout", error);
      return json({ error: "Could not start checkout. Please try again." }, 500);
    }
  }
  if (url.pathname === "/api/webhooks/lipila" && request.method === "POST") {
    try {
      return await handlePaymentWebhook(request, env);
    } catch (error) {
      console.error("Lipila webhook processing failed", error);
      return json({ error: "Payment confirmation is being retried." }, 500);
    }
  }
  if (url.pathname === "/api/admin/contributions/confirm" && request.method === "POST") {
    try {
      return await confirmAsAdmin(request, env);
    } catch (error) {
      console.error("Admin contribution confirmation failed", error);
      return json({ error: "Could not confirm the contribution." }, 500);
    }
  }
  return json({ error: "Not found." }, 404);
}

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    if (url.pathname.startsWith("/api/")) {
      return routeApi(request, env);
    }
    return env.ASSETS.fetch(request);
  },
};
