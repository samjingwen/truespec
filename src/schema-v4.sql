CREATE TABLE agent_replies_v4 (
    id TEXT PRIMARY KEY NOT NULL,
    batch_id TEXT UNIQUE NOT NULL REFERENCES batches(id),
    connector_id TEXT REFERENCES review_connectors(id),
    body TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

INSERT INTO agent_replies_v4 (id, batch_id, connector_id, body, created_at)
SELECT id, batch_id, connector_id, body, created_at FROM agent_replies;

DROP TABLE agent_replies;
ALTER TABLE agent_replies_v4 RENAME TO agent_replies;

PRAGMA user_version = 4;
