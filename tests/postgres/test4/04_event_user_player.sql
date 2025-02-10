CREATE TABLE IF NOT EXISTS event_user_player (
    eup_id SERIAL PRIMARY KEY,
    event_id INTEGER NOT NULL REFERENCES event(event_id),
    user_id INTEGER NOT NULL REFERENCES bettor(user_id),
    golfer_id INTEGER NOT NULL REFERENCES golfer(golfer_id),
    last_refresh_ts TIMESTAMP,
    ins_ts TIMESTAMP NOT NULL DEFAULT now(),

    UNIQUE (event_id, user_id, golfer_id)
);



-- delete from event_user_player where event_id = 3
-- SELECT *
-- FROM event_user_player;
