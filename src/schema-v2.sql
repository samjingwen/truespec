CREATE TABLE review_connectors (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT UNIQUE NOT NULL REFERENCES sessions(id),
    kind TEXT NOT NULL,
    destination TEXT NOT NULL,
    label TEXT NOT NULL
);

ALTER TABLE batches ADD COLUMN connector_id TEXT REFERENCES review_connectors(id);
ALTER TABLE batches ADD COLUMN connector_receipt TEXT;
ALTER TABLE batches ADD COLUMN retry_at INTEGER;

PRAGMA user_version = 2;
