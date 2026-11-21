CREATE TABLE documents (
    id TEXT PRIMARY KEY NOT NULL,
    path TEXT UNIQUE NOT NULL
);

CREATE TABLE sessions (
    id TEXT PRIMARY KEY NOT NULL,
    started_at INTEGER NOT NULL,
    ended_at INTEGER
);
CREATE UNIQUE INDEX one_open_review ON sessions ((ended_at IS NULL)) WHERE ended_at IS NULL;

CREATE TABLE batches (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    request_key TEXT NOT NULL,
    request TEXT NOT NULL,
    ended INTEGER NOT NULL,
    created INTEGER NOT NULL,
    item_count INTEGER NOT NULL CHECK (item_count > 0),
    lease_token TEXT,
    lease_until INTEGER,
    first_received_at INTEGER,
    lease_received_at INTEGER,
    attempt INTEGER NOT NULL DEFAULT 0,
    UNIQUE (session_id, request_key)
);
CREATE INDEX batches_inbox ON batches (session_id, created);

CREATE TABLE comments (
    id TEXT PRIMARY KEY NOT NULL,
    document_id TEXT NOT NULL REFERENCES documents(id),
    body TEXT NOT NULL,
    anchor TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    resolved INTEGER NOT NULL DEFAULT 0,
    batch_id TEXT REFERENCES batches(id)
);

CREATE TABLE batch_items (
    batch_id TEXT NOT NULL REFERENCES batches(id),
    comment_id TEXT UNIQUE NOT NULL REFERENCES comments(id),
    ordinal INTEGER NOT NULL,
    snapshot TEXT NOT NULL,
    completed_at INTEGER,
    completed_token TEXT,
    PRIMARY KEY (batch_id, comment_id)
);

PRAGMA user_version = 1;
