# Cloudflare side

Server-side scripts behind `https://inbox.pilvineuvos.com`.

| File | Purpose |
|---|---|
| `api-worker.js` | Read-only HTTP API used by `pnmail` (`/api/inboxes/{inbox}`, `/api/messages/{id}`) |

Bindings used by the API worker:

- `D1_INDEX` — D1 database with a `messages` table (`id`, `inbox`, `sender`, `subject`, `received_at` in epoch ms, `object_key`)
- `R2_BUCKET` — R2 bucket holding the raw `.eml` objects (24h TTL)

Index rows older than 23h are purged hourly; the API additionally hides anything older than 24h.
