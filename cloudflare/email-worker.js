export default {
  async email(message, env, ctx) {
    if (!env.R2_BUCKET || !env.D1_INDEX) {
      throw new Error("Missing bindings: R2_BUCKET and/or D1_INDEX.");
    }

    const id = crypto.randomUUID();
    const receivedAt = Date.now();
    const expiresAt = receivedAt + 24 * 60 * 60 * 1000;
    const inbox = message.to.slice(0, message.to.lastIndexOf("@"));
    const objectKey = `messages/${id}.eml`;

    const raw = await new Response(message.raw).arrayBuffer();

    await env.R2_BUCKET.put(objectKey, raw, {
      httpMetadata: {
        contentType: "message/rfc822",
      },
      customMetadata: {
        envelopeFrom: message.from,
        envelopeTo: message.to,
        receivedAt: String(receivedAt),
        expiresAt: String(expiresAt),
      },
    });

    await env.D1_INDEX.prepare(`
      INSERT INTO messages (
        id, inbox, sender, subject, received_at, object_key
      )
      VALUES (?, ?, ?, ?, ?, ?)
    `)
      .bind(
        id,
        inbox,
        message.from,
        message.headers.get("subject") ?? "",
        receivedAt,
        objectKey,
      )
      .run();

    // Handling complete; no forwarding or rejection.
  },
};
