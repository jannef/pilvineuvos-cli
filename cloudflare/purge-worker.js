export default {
  async scheduled(controller, env, ctx) {
    const cutoff = Date.now() - 23 * 60 * 60 * 1000;

    const result = await env.D1_INDEX.prepare(`
      DELETE FROM messages
      WHERE received_at <= ?
    `)
      .bind(cutoff)
      .run();

    console.log(`Deleted ${result.meta.changes} expired message rows.`);
  },
};
