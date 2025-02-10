CREATE TABLE IF NOT EXISTS event (
    event_id SERIAL PRIMARY KEY,
    espn_id INTEGER NOT NULL,
    year INT NOT NULL,
    name TEXT NOT NULL,
    ins_ts TIMESTAMP NOT NULL DEFAULT now(),

    UNIQUE (espn_id)
);