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
    console.error("Supabase request failed", options.operation || path, response.status, detail.slice(0, 500));
    throw new Error(options.operation
      ? `Payment setup could not complete: ${options.operation}.`
      : "The member service could not complete this request.");
  }
  if (response.status === 204) return null;
  return response.json();
}

async function authenticate(request, env) {
  const accessToken = getBearerToken(request);
  if (!accessToken) return null;
  const response = await fetch(`${env.SUPABASE_URL.replace(/\/$/, "")}/auth/v1/user`, {
    headers: {
      apikey: env.SUPABASE_PUBLISHABLE_KEY,
      authorization: `Bearer ${accessToken}`,
    },
  });
  if (!response.ok) {
    console.warn("Checkout authentication was rejected by Supabase", response.status);
    return null;
  }
  const user = await response.json();
  return user?.id && user?.email ? { user, accessToken } : null;
}

async function getProfile(env, userId) {
  const rows = await supabaseRequest(
    env,
    `/rest/v1/profiles?id=eq.${encodeURIComponent(userId)}&select=id,full_name,role,status`,
    { operation: "verify member profile" },
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

async function checkPaymentResource(env, path) {
  try {
    const response = await fetch(`${env.SUPABASE_URL.replace(/\/$/, "")}${path}`, {
      headers: supabaseHeaders(env),
    });
    return response.ok;
  } catch {
    return false;
  }
}

async function hasSupabaseServiceAccess(env) {
  try {
    const response = await fetch(`${env.SUPABASE_URL.replace(/\/$/, "")}/auth/v1/admin/users?per_page=1`, {
      headers: {
        apikey: env.SUPABASE_SECRET_KEY,
        authorization: `Bearer ${env.SUPABASE_SECRET_KEY}`,
      },
    });
    return response.ok;
  } catch {
    return false;
  }
}

async function paymentReadiness(env) {
  const supabaseConfigured = Boolean(env.SUPABASE_URL && env.SUPABASE_PUBLISHABLE_KEY && env.SUPABASE_SECRET_KEY);
  const checkoutConfigured = Boolean(env.LIPILA_CHECKOUT_URL && env.LIPILA_API_KEY && env.LIPILA_WEBHOOK_SECRET);
  let checkoutUrlValid = false;
  try {
    checkoutUrlValid = new URL(env.LIPILA_CHECKOUT_URL).protocol === "https:";
  } catch {
    // Report the invalid URL below without leaking its value.
  }
  const database = supabaseConfigured
    ? {
        serviceKey: await hasSupabaseServiceAccess(env),
        profiles: await checkPaymentResource(env, "/rest/v1/profiles?select=id&limit=1"),
        contributions: await checkPaymentResource(env, "/rest/v1/contributions?select=id&limit=1"),
        paymentDetails: await checkPaymentResource(
          env,
          "/rest/v1/contribution_payment_details?select=contribution_id,provider_reference&limit=1",
        ),
      }
    : { serviceKey: false, profiles: false, contributions: false, paymentDetails: false };
  return {
    ready: supabaseConfigured && checkoutConfigured && checkoutUrlValid && Object.values(database).every(Boolean),
    supabaseConfigured,
    checkoutConfigured,
    checkoutUrlValid,
    database,
  };
}

async function getContributionId(env, reference) {
  const rows = await supabaseRequest(
    env,
    `/rest/v1/contributions?payment_reference=eq.${encodeURIComponent(reference)}&select=id`,
    { operation: "find newly-created contribution" },
  );
  return rows?.[0] || null;
}

async function getContributionByProviderReference(env, providerReference) {
  const details = await supabaseRequest(
    env,
    `/rest/v1/contribution_payment_details?provider_reference=eq.${encodeURIComponent(providerReference)}&select=contribution_id`,
  );
  const contributionId = details?.[0]?.contribution_id;
  if (!contributionId) return null;
  const rows = await supabaseRequest(
    env,
    `/rest/v1/contributions?id=eq.${encodeURIComponent(contributionId)}&select=id,user_id,amount,currency,payment_reference,status,receipt_sent_at`,
  );
  return rows?.[0] || null;
}

function randomReference() {
  return `12E-${crypto.randomUUID().replaceAll("-", "").slice(0, 20).toUpperCase()}`;
}

async function createCheckout(request, env) {
  if (!env.SUPABASE_URL || !env.SUPABASE_PUBLISHABLE_KEY || !env.SUPABASE_SECRET_KEY) {
    console.error("Checkout is missing required Supabase Worker configuration");
    return json({ error: "Payments are not configured yet. Ask the class admin to complete the payment service setup." }, 503);
  }
  const auth = await authenticate(request, env);
  if (!auth) return json({ error: "Please sign in again to continue." }, 401);
  if (!env.LIPILA_CHECKOUT_URL || !env.LIPILA_API_KEY) {
    return json({ error: "Lipila collection is not configured yet. Ask the class admin to finish payment setup." }, 503);
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
  const phoneDigits = String(body.phone || "").replace(/\D/g, "");
  const localPhone = phoneDigits.startsWith("260")
    ? phoneDigits.slice(3)
    : phoneDigits.startsWith("0")
      ? phoneDigits.slice(1)
      : phoneDigits;
  if (!/^[79]\d{8}$/.test(localPhone)) {
    return json({ error: "Enter a valid Zambian mobile money number." }, 400);
  }
  const payerPhone = `+260${localPhone}`;

  const reference = randomReference();
  const createdContributions = await supabaseRequest(env, "/rest/v1/contributions?select=id", {
    method: "POST",
    headers: { prefer: "return=representation" },
    operation: "create contribution",
    body: JSON.stringify({
      user_id: auth.user.id,
      amount,
      currency: "ZMW",
      payment_reference: reference,
      method: "mobile_money",
      status: "pending",
    }),
  });
  // Some PostgREST deployments acknowledge a successful insert with an empty
  // representation despite `return=representation`. The reference is unique,
  // so use it to obtain the id instead of treating that response as a failure.
  const contribution = createdContributions?.[0]?.id
    ? createdContributions[0]
    : await getContributionId(env, reference);
  if (!contribution?.id) {
    throw new Error("Could not create the contribution record.");
  }
  try {
    await supabaseRequest(env, "/rest/v1/contribution_payment_details", {
      method: "POST",
      headers: { prefer: "return=minimal" },
      operation: "save payment details",
      body: JSON.stringify({ contribution_id: contribution.id, payer_phone: payerPhone }),
    });
  } catch (error) {
    await markCheckoutRejected(env, reference);
    throw error;
  }

  const requestUrl = new URL(request.url);
  const collectionPayload = {
    referenceId: reference,
    amount,
    narration: "12E reunion contribution",
    accountNumber: localPhone.startsWith("0") ? `260${localPhone.slice(1)}` : `260${localPhone}`,
    currency: "ZMW",
    email: auth.user.email,
    referenceData: reference,
  };

  let lipilaResponse;
  try {
    lipilaResponse = await fetch(gatewayUrl, {
      method: "POST",
      headers: {
        accept: "application/json",
        "x-api-key": env.LIPILA_API_KEY,
        "content-type": "application/json",
        callbackUrl: `${requestUrl.origin}/api/webhooks/lipila`,
      },
      body: JSON.stringify(collectionPayload),
    });
  } catch (error) {
    console.error("Lipila collection request failed", error);
    await markCheckoutRejected(env, reference);
    return json({ error: "Lipila could not be reached. Please try again." }, 502);
  }

  const providerBody = await lipilaResponse.json().catch(() => null);
  const providerStatus = String(providerBody?.status || "").toLowerCase();
  if (!lipilaResponse.ok || !providerBody || !["pending", "successful"].includes(providerStatus)) {
    console.error("Lipila collection rejected", lipilaResponse.status, providerBody?.message || "No response message");
    await markCheckoutRejected(env, reference);
    return json({ error: providerBody?.message || "Lipila could not start this mobile-money payment." }, 502);
  }

  const providerReference = String(providerBody.referenceId || reference).trim();
  await supabaseRequest(
    env,
    `/rest/v1/contribution_payment_details?contribution_id=eq.${encodeURIComponent(contribution.id)}`,
    {
      method: "PATCH",
      headers: { prefer: "return=minimal" },
      operation: "save Lipila payment reference",
      body: JSON.stringify({ provider_reference: providerReference }),
    },
  );
  return json({
    reference,
    providerReference,
    status: providerStatus,
    message: "Approve the mobile-money prompt on your phone. The contribution is added after Lipila confirms payment.",
  }, 201);
}

async function getPaymentStatus(request, env) {
  const auth = await authenticate(request, env);
  if (!auth) return json({ error: "Please sign in again to check payment status." }, 401);
  const reference = new URL(request.url).searchParams.get("reference")?.trim();
  if (!reference) return json({ error: "Payment reference is required." }, 400);
  const contribution = await getContribution(env, reference);
  if (!contribution || contribution.user_id !== auth.user.id) {
    return json({ error: "Payment was not found." }, 404);
  }
  return json({ status: contribution.status });
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

function decodeWebhookSecret(secret) {
  const encoded = secret.startsWith("whsec_") ? secret.slice(6) : secret;
  let binary;
  try {
    binary = atob(encoded);
  } catch {
    return null;
  }
  if (binary.length !== 32) return null;
  return Uint8Array.from(binary, (character) => character.charCodeAt(0));
}

async function verifyLipilaSignature(secret, webhookId, timestamp, rawBodyBytes, signatureHeader) {
  const keyBytes = decodeWebhookSecret(secret);
  if (!keyBytes) return false;
  const key = await crypto.subtle.importKey(
    "raw",
    keyBytes,
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["verify"],
  );
  const signedBody = new TextEncoder().encode(`${webhookId}.${timestamp}.`);
  const signedPayload = new Uint8Array(signedBody.length + rawBodyBytes.length);
  signedPayload.set(signedBody);
  signedPayload.set(rawBodyBytes, signedBody.length);

  const signatures = signatureHeader.trim().split(/\s+/);
  for (const signature of signatures) {
    const [version, encodedSignature] = signature.split(",", 2);
    if (version !== "v1" || !encodedSignature) continue;
    let signatureBytes;
    try {
      const binary = atob(encodedSignature);
      signatureBytes = Uint8Array.from(binary, (character) => character.charCodeAt(0));
    } catch {
      continue;
    }
    if (await crypto.subtle.verify("HMAC", key, signatureBytes, signedPayload)) return true;
  }
  return false;
}

async function wasWebhookProcessed(env, webhookId) {
  const rows = await supabaseRequest(
    env,
    `/rest/v1/lipila_webhook_events?webhook_id=eq.${encodeURIComponent(webhookId)}&select=webhook_id`,
  );
  return rows.length > 0;
}

async function markWebhookProcessed(env, webhookId, eventType) {
  await supabaseRequest(env, "/rest/v1/lipila_webhook_events", {
    method: "POST",
    headers: { prefer: "resolution=ignore-duplicates,return=minimal" },
    body: JSON.stringify({ webhook_id: webhookId, event_type: eventType }),
  });
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
  const rawBodyBytes = new Uint8Array(await request.arrayBuffer());
  if (rawBodyBytes.length > 1024 * 1024) return json({ error: "Webhook payload is too large." }, 413);
  const webhookId = request.headers.get("webhook-id") || "";
  const timestamp = request.headers.get("webhook-timestamp") || "";
  const signatures = request.headers.get("webhook-signature") || "";
  if (!webhookId || !/^\d+$/.test(timestamp) || !signatures) {
    return json({ error: "Missing or invalid Lipila webhook headers." }, 400);
  }
  const timestampSeconds = Number(timestamp);
  if (
    !Number.isSafeInteger(timestampSeconds) ||
    Math.abs(Math.floor(Date.now() / 1000) - timestampSeconds) > 300
  ) {
    return json({ error: "Webhook timestamp is outside the five-minute tolerance." }, 401);
  }
  const signingSecrets = [env.LIPILA_WEBHOOK_SECRET, env.LIPILA_WEBHOOK_SECRET_PREVIOUS].filter(Boolean);
  let signatureIsValid = false;
  for (const secret of signingSecrets) {
    if (await verifyLipilaSignature(secret, webhookId, timestamp, rawBodyBytes, signatures)) {
      signatureIsValid = true;
      break;
    }
  }
  if (!signatureIsValid) {
    return json({ error: "Invalid webhook signature." }, 401);
  }

  let event;
  try {
    event = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(rawBodyBytes));
  } catch {
    return json({ error: "Invalid JSON payload." }, 400);
  }
  if (await wasWebhookProcessed(env, webhookId)) return json({ ok: true, duplicate: true });

  const eventType = String(event.type || event.typeName || "collection.callback");
  const transactionType = String(event.type || "Collection").toLowerCase();
  if (transactionType !== "collection") {
    await markWebhookProcessed(env, webhookId, eventType);
    return json({ ok: true, ignored: true });
  }
  const status = String(event.status || "").toLowerCase();
  const providerReference = String(event.referenceId || "").trim();
  const internalReference = String(event.identifier || event.referenceData || "").trim();
  if (!providerReference && !internalReference) return json({ error: "Missing Lipila collection reference." }, 400);

  const contribution =
    (internalReference ? await getContribution(env, internalReference) : null) ||
    (providerReference ? await getContributionByProviderReference(env, providerReference) : null) ||
    (providerReference ? await getContribution(env, providerReference) : null);
  if (!contribution) return json({ error: "Contribution not found." }, 404);
  if (status === "failed") {
    await supabaseRequest(
      env,
      `/rest/v1/contributions?id=eq.${encodeURIComponent(contribution.id)}&status=eq.pending`,
      {
        method: "PATCH",
        headers: { prefer: "return=minimal" },
        body: JSON.stringify({ status: "rejected" }),
      },
    );
    await markWebhookProcessed(env, webhookId, eventType);
    return json({ ok: true, failed: true });
  }
  if (status !== "successful") return json({ error: "Unsupported Lipila collection status." }, 400);
  const paidAmount = Number(event.amount);
  if (!Number.isFinite(paidAmount) || Math.round(paidAmount * 100) !== Math.round(Number(contribution.amount) * 100)) {
    return json({ error: "Payment amount does not match the contribution." }, 400);
  }
  if (event.currency && String(event.currency).toUpperCase() !== "ZMW") {
    return json({ error: "Payment currency does not match the contribution." }, 400);
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
  await markWebhookProcessed(env, webhookId, eventType);
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
  if (url.pathname === "/api/payment/readiness" && request.method === "GET") {
    return json(await paymentReadiness(env));
  }
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
      console.error("Checkout setup failed before Lipila collection could be created", error?.message || error, error?.stack || "");
      return json({ error: error instanceof Error ? error.message : "Could not start the mobile-money payment. Please try again." }, 500);
    }
  }
  if (url.pathname === "/api/payment/status" && request.method === "GET") {
    try {
      return await getPaymentStatus(request, env);
    } catch (error) {
      console.error("Could not retrieve Lipila payment status", error);
      return json({ error: "Could not check payment status. Please try again." }, 500);
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

export { verifyLipilaSignature };
