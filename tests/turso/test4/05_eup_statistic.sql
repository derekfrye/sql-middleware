-- Turso-adjusted DDL (derived from SQLite test4)
-- TODO: restore JSON affinities and FOREIGN KEY REFERENCES; restore DATETIME defaults
CREATE TABLE IF NOT EXISTS eup_statistic (
    eup_stat_id INTEGER NOT NULL PRIMARY KEY,
    event_espn_id INT NOT NULL,
    golfer_espn_id INT NOT NULL,
    eup_id INT NOT NULL,
    grp INT NOT NULL,

    rounds TEXT NOT NULL,
    round_scores TEXT NOT NULL,
    tee_times TEXT NOT NULL,
    holes_completed_by_round TEXT NOT NULL,
    line_scores TEXT NOT NULL,
    total_score INT NOT NULL,
    upd_ts TEXT NOT NULL,
    ins_ts TEXT NOT NULL,

    UNIQUE (golfer_espn_id, eup_id)
);

