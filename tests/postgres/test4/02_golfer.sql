CREATE TABLE IF NOT EXISTS golfer (
    golfer_id SERIAL PRIMARY KEY,
    espn_id INTEGER NOT NULL UNIQUE,
    name TEXT NOT NULL UNIQUE, -- This uniqueness constraint provides additional data safety but is not required by the program.
    ins_ts TIMESTAMP NOT NULL DEFAULT now()
);




/*
SELECT espn_id,
    COUNT(*)
FROM player
GROUP BY espn_id
HAVING COUNT(*) > 1;
*/