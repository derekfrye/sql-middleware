use crate::middleware::{ConfigAndPool, MiddlewarePoolConnection, PostgresOptions};
use postgresql_embedded::PostgreSQL;

const POSTGRES_BENCH_DDL: &str = "CREATE TABLE IF NOT EXISTS test (
    recid SERIAL PRIMARY KEY,
    a int, b text, c timestamp not null default now(),
    d real, e boolean, f bytea, g jsonb,
    h text, i text, j text, k text, l text, m text, n text, o text, p text
)";

pub(super) struct InstanceSettings {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
}

pub(super) fn build_base_config(
    db_user: &str,
    db_pass: &str,
    db_name: &str,
) -> deadpool_postgres::Config {
    let mut cfg = deadpool_postgres::Config::new();
    cfg.dbname = Some(db_name.to_string());
    cfg.user = Some(db_user.to_string());
    cfg.password = Some(db_pass.to_string());
    cfg
}

pub(super) fn build_final_config(
    base_cfg: &deadpool_postgres::Config,
    settings: &InstanceSettings,
    user: &str,
    password: &str,
) -> deadpool_postgres::Config {
    let mut final_cfg = base_cfg.clone();
    final_cfg.port = Some(settings.port);
    final_cfg.host = Some(settings.host.clone());
    final_cfg.user = Some(user.to_string());
    final_cfg.password = Some(password.to_string());
    final_cfg
}

pub(super) async fn start_postgres_instance(
    db_name: &str,
) -> Result<(PostgreSQL, InstanceSettings), Box<dyn std::error::Error>> {
    let mut postgresql = PostgreSQL::default();
    postgresql.setup().await?;
    postgresql.start().await?;

    let settings = postgresql.settings();
    let details = InstanceSettings {
        host: settings.host.clone(),
        port: settings.port,
        username: settings.username.clone(),
        password: settings.password.clone(),
    };

    postgresql.create_database(db_name).await?;
    Ok((postgresql, details))
}

pub(super) async fn ensure_target_user(
    db_user: &str,
    db_pass: &str,
    base_cfg: &deadpool_postgres::Config,
    settings: &InstanceSettings,
) -> Result<(String, String), Box<dyn std::error::Error>> {
    if db_user == settings.username {
        return Ok((settings.username.clone(), settings.password.clone()));
    }

    let mut admin_cfg = base_cfg.clone();
    admin_cfg.port = Some(settings.port);
    admin_cfg.host = Some(settings.host.clone());
    admin_cfg.user = Some(settings.username.clone());
    admin_cfg.password = Some(settings.password.clone());
    admin_cfg.dbname = Some(String::from("postgres"));

    let admin_pool = ConfigAndPool::new_postgres(PostgresOptions::new(admin_cfg)).await?;
    let admin_conn = admin_pool.get_connection().await?;

    if let MiddlewarePoolConnection::Postgres { client: pgconn, .. } = admin_conn {
        let create_user_sql =
            format!("CREATE USER \"{db_user}\" WITH PASSWORD '{db_pass}' CREATEDB SUPERUSER");
        pgconn
            .execute(&create_user_sql, &[])
            .await
            .map_err(|error| format!("Failed to create user {db_user}: {error}"))?;
    }

    Ok((db_user.to_string(), db_pass.to_string()))
}

pub(super) async fn initialise_benchmark_schema(
    config_and_pool: &ConfigAndPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let conn = config_and_pool.get_connection().await?;

    if let MiddlewarePoolConnection::Postgres {
        client: mut pgconn, ..
    } = conn
    {
        let tx = pgconn.transaction().await?;
        tx.batch_execute(POSTGRES_BENCH_DDL).await?;
        tx.commit().await?;
        Ok(())
    } else {
        Err("Expected PostgreSQL connection".into())
    }
}
