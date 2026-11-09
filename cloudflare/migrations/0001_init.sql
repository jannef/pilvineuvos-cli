-- IF NOT EXISTS so this also applies cleanly to the database created before migrations existed.
CREATE TABLE IF NOT EXISTS messages (
  id TEXT PRIMARY KEY,
  inbox TEXT NOT NULL,
  sender TEXT NOT NULL,
  subject TEXT,
  received_at INTEGER NOT NULL,
  object_key TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_messages_inbox_received ON messages (inbox, received_at);
CREATE INDEX IF NOT EXISTS idx_messages_received ON messages (received_at);

CREATE TABLE IF NOT EXISTS api_keys (
  key TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  created_at INTEGER NOT NULL
);
