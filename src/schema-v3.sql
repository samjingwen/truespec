CREATE TABLE review_feed (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    kind TEXT NOT NULL CHECK (kind IN ('comment', 'agent_reply')),
    reference_id TEXT NOT NULL UNIQUE
);

INSERT INTO review_feed (kind, reference_id)
SELECT 'comment', id FROM comments ORDER BY rowid;

CREATE TABLE agent_replies (
    id TEXT PRIMARY KEY NOT NULL,
    batch_id TEXT UNIQUE NOT NULL REFERENCES batches(id),
    connector_id TEXT NOT NULL REFERENCES review_connectors(id),
    body TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

PRAGMA user_version = 3;
