CREATE TABLE messages (
  id TEXT PRIMARY KEY,
  inbox TEXT NOT NULL,
  sender TEXT NOT NULL,
  subject TEXT,
  received_at INTEGER NOT NULL,
  object_key TEXT NOT NULL
);
