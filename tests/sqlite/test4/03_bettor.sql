CREATE TABLE IF NOT EXISTS bettor (
    user_id integer NOT NULL PRIMARY KEY,
    name TEXT NOT NULL,
    ins_ts DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
    );
    --alter table golfuser alter column name set data type text;
