CREATE TABLE IF NOT EXISTS golfstatistic (
    -- drop table golfstatistic
    stat_id INTEGER NOT NULL PRIMARY KEY,
    statistic_type TEXT NOT NULL,
    intval INTEGER,
    timeval DATETIME,
    ins_ts DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
    );