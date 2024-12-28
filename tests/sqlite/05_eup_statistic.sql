CREATE TABLE IF NOT EXISTS eup_statistic (
    -- drop table eup_statistic
    eup_stat_id INTEGER NOT NULL PRIMARY KEY,
    eup_id INT NOT NULL REFERENCES event_user_player(eup_id),
    round INT NOT NULL,
    statistic_type TEXT NOT NULL,
    intval INT,
    timeval DATETIME,
    last_refresh_ts DATETIME,
    ins_ts DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
    );
