CREATE TABLE IF NOT EXISTS eup_statistic (
    eup_stat_id SERIAL PRIMARY KEY,
    event_espn_id INT NOT NULL REFERENCES event(espn_id),
    golfer_espn_id INT NOT NULL REFERENCES golfer(espn_id),
    eup_id INT NOT NULL REFERENCES event_user_player(eup_id),
    grp INT NOT NULL,

    -- pub rounds: Vec<IntStat>,
    -- pub round_scores: Vec<IntStat>,
    -- pub tee_times: Vec<StringStat>,
    -- pub holes_completed_by_round: Vec<IntStat>,
    -- pub line_scores: Vec<LineScore>,
    -- pub success_fail: ResultStatus,
    -- pub total_score: i32,

    rounds JSONB NOT NULL,
    round_scores JSONB NOT NULL,
    tee_times JSONB NOT NULL,
    holes_completed_by_round JSONB NOT NULL,
    line_scores JSONB NOT NULL,
    total_score INT NOT NULL,
    upd_ts TIMESTAMP NOT NULL DEFAULT now(),
    ins_ts TIMESTAMP NOT NULL DEFAULT now(),

    UNIQUE (golfer_espn_id, eup_id)
);
