CREATE TABLE messages (
  id TEXT PRIMARY KEY,
  inbox TEXT NOT NULL,
  sender TEXT NOT NULL,
  subject TEXT,
  received_at INTEGER NOT NULL,
  object_key TEXT NOT NULL
);

CREATE INDEX idx_messages_inbox_received ON messages (inbox, received_at);
CREATE INDEX idx_messages_received ON messages (received_at);
