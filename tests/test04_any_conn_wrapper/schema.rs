use sql_middleware::SqlMiddlewareDbError;
use sql_middleware::middleware::{DatabaseType, MiddlewarePoolConnection};

pub(super) async fn apply_schema(
    conn: &mut MiddlewarePoolConnection,
    db_type: &DatabaseType,
) -> Result<(), SqlMiddlewareDbError> {
    let ddl = match db_type {
        DatabaseType::Postgres => vec![
            include_str!("../../tests/postgres/test4/00_event.sql"),
            include_str!("../../tests/postgres/test4/02_golfer.sql"),
            include_str!("../../tests/postgres/test4/03_bettor.sql"),
            include_str!("../../tests/postgres/test4/04_event_user_player.sql"),
            include_str!("../../tests/postgres/test4/05_eup_statistic.sql"),
        ],
        DatabaseType::Sqlite => vec![
            include_str!("../../tests/sqlite/test4/00_event.sql"),
            include_str!("../../tests/sqlite/test4/02_golfer.sql"),
            include_str!("../../tests/sqlite/test4/03_bettor.sql"),
            include_str!("../../tests/sqlite/test4/04_event_user_player.sql"),
            include_str!("../../tests/sqlite/test4/05_eup_statistic.sql"),
        ],
        #[cfg(feature = "mssql")]
        DatabaseType::Mssql => mssql_ddl(),
        #[cfg(feature = "turso")]
        DatabaseType::Turso => vec![
            include_str!("../../tests/turso/test4/00_event.sql"),
            include_str!("../../tests/turso/test4/02_golfer.sql"),
            include_str!("../../tests/turso/test4/03_bettor.sql"),
        ],
    };
    apply_ddl(conn, db_type, &ddl).await
}

async fn apply_ddl(
    conn: &mut MiddlewarePoolConnection,
    db_type: &DatabaseType,
    ddl: &[&str],
) -> Result<(), SqlMiddlewareDbError> {
    #[cfg(feature = "turso")]
    if db_type == &DatabaseType::Turso {
        for (idx, stmt) in ddl.iter().enumerate() {
            conn.execute_batch(stmt).await.map_err(|error| {
                let part = idx + 1;
                SqlMiddlewareDbError::ExecutionError(format!(
                    "Turso DDL failure on part {part}: {error}"
                ))
            })?;
        }
        return Ok(());
    }

    let ddl_query = ddl.join("\n");
    conn.execute_batch(&ddl_query).await
}

#[cfg(feature = "mssql")]
fn mssql_ddl() -> Vec<&'static str> {
    vec![
        "IF OBJECT_ID('dbo.event', 'U') IS NULL
BEGIN
    CREATE TABLE dbo.event (
        event_id INT IDENTITY(1,1) PRIMARY KEY,
        espn_id INT NOT NULL UNIQUE,
        year INT NOT NULL,
        name NVARCHAR(255) NOT NULL,
        ins_ts DATETIME2 NOT NULL DEFAULT SYSUTCDATETIME()
    );
END;",
        "IF OBJECT_ID('dbo.golfer', 'U') IS NULL
BEGIN
    CREATE TABLE dbo.golfer (
        golfer_id INT IDENTITY(1,1) PRIMARY KEY,
        espn_id INT NOT NULL UNIQUE,
        name NVARCHAR(255) NOT NULL UNIQUE,
        ins_ts DATETIME2 NOT NULL DEFAULT SYSUTCDATETIME()
    );
END;",
        "IF OBJECT_ID('dbo.bettor', 'U') IS NULL
BEGIN
    CREATE TABLE dbo.bettor (
        user_id INT IDENTITY(1,1) PRIMARY KEY,
        name NVARCHAR(255) NOT NULL,
        ins_ts DATETIME2 NOT NULL DEFAULT SYSUTCDATETIME()
    );
END;",
        "IF OBJECT_ID('dbo.event_user_player', 'U') IS NULL
BEGIN
    CREATE TABLE dbo.event_user_player (
        eup_id INT IDENTITY(1,1) PRIMARY KEY,
        event_id INT NOT NULL REFERENCES dbo.event(event_id),
        user_id INT NOT NULL REFERENCES dbo.bettor(user_id),
        golfer_id INT NOT NULL REFERENCES dbo.golfer(golfer_id),
        last_refresh_ts DATETIME2 NULL,
        ins_ts DATETIME2 NOT NULL DEFAULT SYSUTCDATETIME(),
        CONSTRAINT uq_event_user_player UNIQUE (event_id, user_id, golfer_id)
    );
END;",
        "IF OBJECT_ID('dbo.eup_statistic', 'U') IS NULL
BEGIN
    CREATE TABLE dbo.eup_statistic (
        eup_stat_id INT IDENTITY(1,1) PRIMARY KEY,
        event_espn_id INT NOT NULL REFERENCES dbo.event(espn_id),
        golfer_espn_id INT NOT NULL REFERENCES dbo.golfer(espn_id),
        eup_id INT NOT NULL REFERENCES dbo.event_user_player(eup_id),
        grp INT NOT NULL,
        rounds NVARCHAR(MAX) NOT NULL,
        round_scores NVARCHAR(MAX) NOT NULL,
        tee_times NVARCHAR(MAX) NOT NULL,
        holes_completed_by_round NVARCHAR(MAX) NOT NULL,
        line_scores NVARCHAR(MAX) NOT NULL,
        total_score INT NOT NULL,
        upd_ts DATETIME2 NOT NULL DEFAULT SYSUTCDATETIME(),
        ins_ts DATETIME2 NOT NULL DEFAULT SYSUTCDATETIME(),
        CONSTRAINT uq_eup_stat UNIQUE (golfer_espn_id, eup_id)
    );
END;",
    ]
}
