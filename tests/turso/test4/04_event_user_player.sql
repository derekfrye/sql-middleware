-- Turso-adjusted DDL (derived from SQLite test4)
-- TODO: restore FOREIGN KEY REFERENCES when turso crate supports them
-- TODO: restore DATETIME types and DEFAULT CURRENT_TIMESTAMP
CREATE TABLE IF NOT EXISTS event_user_player (
    eup_id INTEGER NOT NULL PRIMARY KEY,
    event_id INTEGER NOT NULL,
    user_id INTEGER NOT NULL,
    golfer_id INTEGER NOT NULL,
    last_refresh_ts TEXT,
    ins_ts TEXT NOT NULL DEFAULT '1970-01-01 00:00:00',

    UNIQUE (event_id, user_id, golfer_id)
);
