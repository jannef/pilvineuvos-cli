# Cloudflare side

Server-side scripts behind `https://inbox.pilvineuvos.com`.

| File | Purpose |
|---|---|
| `email-worker.js` | Email Routing worker: stores the raw message in R2 (`messages/{uuid}.eml`) and indexes it in D1 under the recipient local part |
| `api-worker.js` | Read-only HTTP API used by `pnmail` (`/api/inboxes/{inbox}`, `/api/messages/{id}`) |
| `purge-worker.js` | Scheduled (hourly cron) worker that deletes index rows older than 23h from `D1_INDEX` |
| `schema.sql` | D1 schema for the `messages` and `api_keys` tables |

Bindings (all workers use `D1_INDEX`; the email and API workers also use `R2_BUCKET`):

- `D1_INDEX` — D1 database with a `messages` table (see `schema.sql`; `received_at` is epoch ms)
- `R2_BUCKET` — R2 bucket holding the raw `.eml` objects (24h TTL)

Flow: mail to `<inbox>@pilvineuvos.com` → `email-worker.js` → R2 + D1 → read via `api-worker.js`.

Index rows older than 23h are purged hourly; the API additionally hides anything older than 24h.

## Ingress limits

`email-worker.js` bounces mail back to the sender (SMTP reject) when:

- the whole message is larger than **10 MB** (10,000,000 bytes) — override with a
  `MAX_MESSAGE_BYTES` Worker variable (in bytes); Email Routing's own ceiling is 25 MiB
- the recipient name (before `@`) is longer than 64 characters (RFC 5321)

Oversized mail is rejected before its body is read. Stored index fields are capped: subject at
998 characters, sender at 320.

## API keys

Both API endpoints require `Authorization: Bearer <key>`, where `<key>` is a row in `api_keys`
(compared in lowercase). Incoming mail is not affected.

Create a key (replace `<DB_NAME>` with the D1 database name):

```sh
KEY=$(uuidgen | tr A-Z a-z)
npx wrangler d1 execute <DB_NAME> --remote --command \
  "INSERT INTO api_keys (key, name, created_at) VALUES ('$KEY', 'janne-laptop', unixepoch() * 1000)"
echo "$KEY"
```

Revoke one:

```sh
npx wrangler d1 execute <DB_NAME> --remote --command "DELETE FROM api_keys WHERE name = 'janne-laptop'"
```

When enabling auth on an existing deployment, create the `api_keys` table and at least one key
**before** deploying the new `api-worker.js`; otherwise every request fails until the table exists.
