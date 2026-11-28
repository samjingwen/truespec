CREATE TABLE workspace_identity (
    singleton INTEGER PRIMARY KEY NOT NULL CHECK (singleton = 1),
    id TEXT UNIQUE NOT NULL
);

PRAGMA user_version = 5;
