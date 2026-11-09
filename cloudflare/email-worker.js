// Default cap on a whole message (headers, body and attachments). Override with
// the MAX_MESSAGE_BYTES Worker variable. Email Routing itself stops at 25 MiB.
const DEFAULT_MAX_MESSAGE_BYTES = 10 * 1024 * 1024;

// RFC 5321 local-part limit; longer recipients are rejected.
const MAX_INBOX_LENGTH = 64;

// Field caps for the D1 index: RFC 5321 path length, RFC 5322 line length.
const MAX_SENDER_LENGTH = 320;
const MAX_SUBJECT_LENGTH = 998;

function maxMessageBytes(env) {
  const configured = Number(env.MAX_MESSAGE_BYTES);
  return Number.isFinite(configured) && configured > 0
    ? configured
    : DEFAULT_MAX_MESSAGE_BYTES;
}

// Rounds up to 0.1 MiB so a message just over the limit never reads as equal to it.
function formatBytes(bytes) {
  return `${Math.ceil((bytes / (1024 * 1024)) * 10) / 10} MiB`;
}

export default {
  async email(message, env, ctx) {
    if (!env.R2_BUCKET || !env.D1_INDEX) {
      throw new Error("Missing bindings: R2_BUCKET and/or D1_INDEX.");
    }

    // Reject before reading the body so oversized mail is never buffered.
    const maxBytes = maxMessageBytes(env);

    if (message.rawSize > maxBytes) {
      console.log(`Rejected ${message.rawSize} byte message to ${message.to}.`);
      message.setReject(
        `Message too large (${formatBytes(message.rawSize)}); the limit is ${formatBytes(maxBytes)}.`,
      );
      return;
    }

    const inbox = message.to.slice(0, message.to.lastIndexOf("@")).toLowerCase();

    if (inbox.length > MAX_INBOX_LENGTH) {
      message.setReject(`Recipient name too long; the limit is ${MAX_INBOX_LENGTH} characters.`);
      return;
    }

    const id = crypto.randomUUID();
    const receivedAt = Date.now();
    const expiresAt = receivedAt + 24 * 60 * 60 * 1000;
    const sender = message.from.slice(0, MAX_SENDER_LENGTH);
    const subject = (message.headers.get("subject") ?? "").slice(0, MAX_SUBJECT_LENGTH);
    const objectKey = `messages/${id}.eml`;

    const raw = await new Response(message.raw).arrayBuffer();

    await env.R2_BUCKET.put(objectKey, raw, {
      httpMetadata: {
        contentType: "message/rfc822",
      },
      customMetadata: {
        envelopeFrom: sender,
        envelopeTo: message.to.slice(0, MAX_SENDER_LENGTH),
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
      .bind(id, inbox, sender, subject, receivedAt, objectKey)
      .run();

    // Handling complete; no forwarding or rejection.
  },
};
