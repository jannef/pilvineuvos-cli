# Cloudflare side

Server-side scripts behind `https://inbox.pilvineuvos.com`.

| File | Purpose |
|---|---|
| `email-worker.js` | Email Routing worker: stores the raw message in R2 (`messages/{uuid}.eml`) and indexes it in D1 under the recipient local part |
| `api-worker.js` | Read-only HTTP API used by `pnmail` (`/api/inboxes/{inbox}`, `/api/messages/{id}`) |
| `purge-worker.js` | Scheduled (hourly cron) worker that deletes index rows older than 23h from `D1_INDEX` |

Bindings (all workers use `D1_INDEX`; the email and API workers also use `R2_BUCKET`):

- `D1_INDEX` — D1 database with a `messages` table (`id`, `inbox`, `sender`, `subject`, `received_at` in epoch ms, `object_key`)
- `R2_BUCKET` — R2 bucket holding the raw `.eml` objects (24h TTL)

Flow: mail to `<inbox>@pilvineuvos.com` → `email-worker.js` → R2 + D1 → read via `api-worker.js`.

Index rows older than 23h are purged hourly; the API additionally hides anything older than 24h.
