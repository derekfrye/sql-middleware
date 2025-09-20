-- Turso-adjusted DDL (derived from SQLite test4)
-- TODO: restore DATETIME + DEFAULT CURRENT_TIMESTAMP when turso crate supports it
CREATE TABLE IF NOT EXISTS event (
    event_id INTEGER NOT NULL PRIMARY KEY,
    espn_id INTEGER NOT NULL,
    year INT NOT NULL,
    name TEXT NOT NULL,
    ins_ts TEXT NOT NULL DEFAULT '1970-01-01 00:00:00',

    UNIQUE (espn_id)
);
