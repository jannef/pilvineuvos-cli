// Accepts `Authorization: Bearer <key>` where <key> is a row in api_keys.
async function isAuthorized(request, env) {
  const header = request.headers.get("Authorization") ?? "";
  const match = header.match(/^Bearer\s+(\S+)$/i);

  if (!match) {
    return false;
  }

  const row = await env.D1_INDEX.prepare(`
    SELECT 1
    FROM api_keys
    WHERE key = ?
  `)
    .bind(match[1].toLowerCase())
    .first();

  return row !== null;
}

export default {
  async fetch(request, env, ctx) {
    if (!(await isAuthorized(request, env))) {
      return new Response("Unauthorized", {
        status: 401,
        headers: { "WWW-Authenticate": 'Bearer realm="pilvineuvos"' },
      });
    }

    const url = new URL(request.url);
    const cutoff = Date.now() - 24 * 60 * 60 * 1000;

    // GET /api/inboxes/some-inbox-name
    const inboxMatch = url.pathname.match(/^\/api\/inboxes\/([^/]+)$/);

    if (inboxMatch) {
      const inbox = decodeURIComponent(inboxMatch[1]);

      const { results } = await env.D1_INDEX.prepare(`
        SELECT id, sender, subject, received_at
        FROM messages
        WHERE inbox = ? AND received_at > ?
        ORDER BY received_at DESC
        LIMIT 100
      `)
        .bind(inbox, cutoff)
        .all();

      return Response.json(results);
    }

    // GET /api/messages/message-id
    const messageMatch = url.pathname.match(/^\/api\/messages\/([^/]+)$/);

    if (messageMatch) {
      const message = await env.D1_INDEX.prepare(`
        SELECT object_key
        FROM messages
        WHERE id = ? AND received_at > ?
      `)
        .bind(decodeURIComponent(messageMatch[1]), cutoff)
        .first();

      if (!message) {
        return new Response("Not found", { status: 404 });
      }

      const object = await env.R2_BUCKET.get(message.object_key);

      if (!object) {
        return new Response("Not found", { status: 404 });
      }

      return new Response(object.body, {
        headers: {
          "Content-Type": "message/rfc822",
          "Content-Disposition": 'attachment; filename="message.eml"',
          "Cache-Control": "no-store",
        },
      });
    }

    return new Response("Not found", { status: 404 });
  },
};
