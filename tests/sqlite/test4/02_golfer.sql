CREATE TABLE IF NOT EXISTS golfer (
    -- drop table player cascade
    golfer_id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    espn_id integer NOT NULL UNIQUE,
    name TEXT NOT NULL UNIQUE, -- i don't think its critical this is unique, program doesn't require it i don't think, but doing this just for extra data safety
    ins_ts DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
    );



/*
SELECT espn_id,
    COUNT(*)
FROM player
GROUP BY espn_id
HAVING COUNT(*) > 1;
*/
