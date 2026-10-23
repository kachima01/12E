# 12E Reunion

A private contribution and classmate portal for David Livingstone Secondary School, Class of 2017. It uses React, TypeScript and Vite for the app, Supabase Auth/Postgres for member accounts and data, and Cloudflare Workers for private payment and email integrations.

## Local development

1. Install Node.js 20+ and dependencies:

   ```sh
   npm install
   ```

2. Create a Supabase project. In its SQL editor, run [`schema.sql`](./schema.sql).
3. Copy `.env.example` to `.env.local` and set the Supabase project URL and publishable key.
4. Configure Supabase Auth:
   - Enable email confirmations.
   - Set the site URL and local redirect URL (`http://localhost:5173`).
   - Leave verification and password-reset emails on Supabase Auth's built-in email service. Resend is used only by the Cloudflare Worker for contribution receipts. Supabase's built-in email service has rate limits; if one is reached, wait for it to reset before retrying.
5. Start the Cloudflare Worker and Vite in separate terminals:

   ```sh
   npm run dev:worker
   npm run dev
   ```

   The Vite development server proxies `/api/*` to Wrangler on port 8787. Add the Worker environment variables below to a local `.dev.vars` file (it is ignored by Git).

## First admin

Create an account from the app and verify its email. In the Supabase SQL editor, promote that account:

```sql
update public.profiles
set role = 'admin', status = 'approved'
where id = (select id from auth.users where email = 'YOUR_ADMIN_EMAIL');
```

New sign-ups remain pending until an administrator approves them in **Admin desk**. On an existing database, rerun `schema.sql` to update the profile default and signup trigger; this does not change the status of existing profiles. To send existing non-admin members back for review, first ensure an admin account is configured, then run `update public.profiles set status = 'pending' where role = 'member' and status = 'approved';`. Members can see confirmed contribution totals and classmate names; payer phone numbers are kept in a separate backend-only table.

## Payment integration

The app defaults to ZMW and suggests K500, but members can enter another amount and the Zambian mobile number to charge. The phone input uses a fixed `+260` prefix and accepts nine local digits. The entered amount is credited to the reunion fund; confirm Lipila's fee treatment with the provider before live collection. The member target is K500 per approved classmate, so the overall target is the current approved-classmate count multiplied by K500. Successful payments trigger an in-app realtime confirmation for the payer and a class-wide contribution notification.

The Worker starts a mobile-money collection at `POST /api/payment/checkout`; Lipila sends a prompt to the member's phone, and the contribution is counted only after a signed Lipila callback confirms it. The contribution dialog stays open and polls the authenticated `GET /api/payment/status` endpoint for the callback result; it displays success or failure only after the contribution status changes. Phone entry uses a fixed `+260` prefix and accepts exactly nine local digits. Set `LIPILA_CHECKOUT_URL` to the mobile-money collection endpoint for the matching Lipila environment. The endpoint shown in the [Lipila mobile money collections guide](https://docs.lipila.io/docs/collections/momocollections.html) is `https://api.lipila.dev/api/v1/collections/mobile-money`. Use the test or live API key that matches the environment configured for the merchant account.

```json
{
  "referenceId": "12E-...",
  "amount": 500,
  "narration": "12E reunion contribution",
  "accountNumber": "260971234567",
  "currency": "ZMW",
  "email": "member@example.com",
  "referenceData": "12E-..."
}
```

The Worker sends the API key in the `x-api-key` header and the callback URL in the `callbackUrl` header. Lipila's [collection API guide](https://docs.lipila.io/docs/collections/momocollections.html) documents an initial `Pending` response and a callback with top-level `referenceId`, `amount`, `currency`, `status`, and `type` fields. The handler matches the callback to the provider reference, verifies the amount and currency, credits only a successful collection, and rejects a pending contribution after a signed failed callback. Callback signatures use Lipila's Standard Webhooks-style `webhook-id`, `webhook-timestamp`, and `webhook-signature` headers, including `v1` signatures and multiple signatures during key rotation. The webhook secret must be a base64-encoded 32-byte key (or the `whsec_`-prefixed form); the Worker validates the raw request body and rejects timestamps more than five minutes from the current time. During the 24-hour key-rotation overlap, set the previous secret as the optional `LIPILA_WEBHOOK_SECRET_PREVIOUS` Worker secret; remove it after the overlap ends.

For an existing Supabase project, re-run [`schema.sql`](./schema.sql) to create the `lipila_webhook_events` table and `provider_reference` column used to match Lipila callbacks to contributions. The schema also adds a class message board with approved-member read/post policies and adds the relevant tables to Supabase Realtime.

For an existing Supabase project, re-run [`schema.sql`](./schema.sql) to create the private `contribution_payment_details` table. It migrates any earlier `payer_phone` column data, removes that column from member-visible contribution rows, and keeps payer numbers and provider references accessible only to the backend service key.

Members can update their display name and change their password in **Profile settings**. Class messages are visible to approved members. Realtime contribution confirmations appear as in-app notifications while members are connected.

## Vercel deployment

The frontend can be hosted on Vercel while the Cloudflare Worker remains the only payment API. [`vercel.json`](./vercel.json) rewrites same-origin `/api/*` requests to `https://12e.rabsonkachima01.workers.dev/api/*`; the browser does not call a separate backend origin. Push the change to the branch connected to Vercel and wait for its deployment to finish before testing.

Keep all payment, Supabase secret, webhook, and email credentials configured as Cloudflare Worker secrets as described below. Do not add them to Vercel's `VITE_*` browser environment variables. After the Worker secrets are set, check `https://12e.rabsonkachima01.workers.dev/api/health` and the corresponding `/api/health` path on the Vercel domain. Both should return HTTP 200 with `services.supabase`, `services.lipila`, and `services.resend` set to `true` before testing checkout. The current Worker health check returned `false` for all three integrations, so payment is not ready yet; set its secrets and verify again. Test the Vercel health route first, then perform a small live payment only when ready, and confirm the signed callback updates Supabase.

## Cloudflare deployment

For GitHub Actions deployment, add these repository secrets under **Settings → Secrets and variables → Actions**:

- `CLOUDFLARE_API_TOKEN`: the value of an API token with permission to deploy Workers. Cloudflare only shows the token value when it is created; if you no longer have it, create a replacement token.
- `CLOUDFLARE_ACCOUNT_ID`: the Cloudflare account ID for the account hosting this Worker. Find it in the Cloudflare dashboard under the account's overview.

Do not commit these values or paste them into chat. The deployment workflow uses Node.js 22 and checks that both secrets are configured before calling Wrangler.

1. Set the Worker secrets (Wrangler prompts for each value):

   ```sh
   npx wrangler secret put SUPABASE_URL
   npx wrangler secret put SUPABASE_PUBLISHABLE_KEY
   npx wrangler secret put SUPABASE_SECRET_KEY
   npx wrangler secret put RESEND_API_KEY
   npx wrangler secret put RESEND_FROM_EMAIL
   npx wrangler secret put LIPILA_CHECKOUT_URL
   npx wrangler secret put LIPILA_API_KEY
   npx wrangler secret put LIPILA_WEBHOOK_SECRET
   ```

   Keep the Supabase secret key, Lipila API key, webhook secret, and Resend API key in Worker secrets only. Never use them in `VITE_*` variables or browser code. Since the Supabase secret key was shared in chat, rotate it in the Supabase dashboard before setting the Worker secret.
2. Leave Supabase Auth email delivery on its built-in service. The SQL schema enables `profiles`, `contributions`, and `class_messages` in the `supabase_realtime` publication so profile, payment, and message updates can reach connected members.
3. Connect the GitHub repository to Cloudflare Workers, or deploy with:

   ```sh
   npx wrangler login
   npm run deploy
   ```

The free Workers plan serves the built Vite assets and API from one origin. Supabase and Resend have their own free-tier quotas; review current provider limits before inviting the class.

## Why this stack fits 12E

- **API and routing:** the Worker owns health, checkout, and webhook routes; React handles the member-facing pages.
- **Authentication and access control:** Supabase verifies email and manages sessions. PostgreSQL row-level security limits members to approved class data and admins to administrative actions.
- **Webhook and asynchronous payment:** Lipila’s signed webhook is the source of truth for successful payment. The handler checks the signature, reference, and amount before confirming it. Receipt delivery uses an idempotency key; if email delivery fails, the Worker returns a retryable error, so confirm/configure Lipila’s webhook retry behavior before launch.
- **Live updates:** Supabase Realtime uses WebSockets to notify connected members about confirmed contributions and refresh class messages.
- **Edge and availability:** Cloudflare serves the site and API globally; a separate load balancer is not needed at this scale. Supabase and Resend are managed services, but their free-tier quotas and availability still apply, so keep database backups.
- **What we intentionally leave out:** Redis/KV caching is unnecessary for a small class fund and risks stale balances; Kafka or a queue adds operational work without meaningful throughput needs; Docker and Kubernetes are not used because the app runs as a managed Worker rather than a server fleet. Add a queue only if payment traffic or email volume grows enough to justify it.

## Health and operational checks

- `GET /api/health` reports whether required integrations are configured; it never returns secret values.
- Cloudflare Workers observability is enabled in `wrangler.toml`.
- Enable GitHub branch protection and deploy from reviewed changes. Use Cloudflare deployment rollback for a bad release.
- Configure an uptime monitor for `/api/health`, and periodically export/backup the Supabase database from the Supabase dashboard.
- Test a small live payment and its webhook/receipt end-to-end before announcing contributions.
