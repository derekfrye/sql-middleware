CREATE TABLE IF NOT EXISTS event_user_player (
    -- drop table event_user_player cascade
    eup_id INTEGER NOT NULL PRIMARY KEY,
    event_id INTEGER NOT NULL REFERENCES event(event_id),
    user_id INTEGER NOT NULL REFERENCES golfuser(user_id),
    player_id INTEGER NOT NULL REFERENCES player(player_id),
    last_refresh_ts DATETIME,
    ins_ts DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
    );



-- delete from event_user_player where event_id = 3
-- SELECT *
-- FROM event_user_player;
