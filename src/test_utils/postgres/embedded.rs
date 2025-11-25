use super::super::SHARED_RUNTIME;
use crate::middleware::{ConfigAndPool, MiddlewarePoolConnection};

#[cfg(feature = "test-utils")]
use postgresql_embedded::PostgreSQL;

/// Represents a running embedded `PostgreSQL` instance.
#[cfg(feature = "test-utils")]
pub struct EmbeddedPostgres {
    pub postgresql: PostgreSQL,
    pub port: u16,
    pub database_url: String,
    /// The actual working configuration with correct credentials
    pub config: deadpool_postgres::Config,
}

/// Set up an embedded `PostgreSQL` instance for testing or benchmarking.
///
/// # Errors
/// Returns an error if the embedded server cannot be set up, started, or if database/user
/// provisioning or the post-start connectivity check fails.
///
/// # Panics
/// Panics if `cfg.dbname` is `None` because the target database name is required.
#[cfg(feature = "test-utils")]
pub fn setup_postgres_embedded(
    cfg: &deadpool_postgres::Config,
) -> Result<EmbeddedPostgres, Box<dyn std::error::Error>> {
    SHARED_RUNTIME.block_on(async {
        let mut postgresql = PostgreSQL::default();

        // Setup PostgreSQL binaries (bundled, so no download conflicts)
        postgresql.setup().await?;

        // Start the PostgreSQL instance
        postgresql.start().await?;

        let port = postgresql.settings().port;
        let host = postgresql.settings().host.clone();
        let embedded_user = postgresql.settings().username.clone();
        let embedded_password = postgresql.settings().password.clone();

        // Create the test database
        let db_name = cfg.dbname.as_ref().unwrap();
        postgresql.create_database(db_name).await?;

        // For backward compatibility, create a user with the credentials from the config
        // if they're different from the embedded defaults
        let (final_user, final_password) = if let (Some(desired_user), Some(desired_password)) =
            (cfg.user.as_ref(), cfg.password.as_ref())
        {
            if desired_user == &embedded_user {
                (embedded_user, embedded_password)
            } else {
                // Connect as the embedded user to create the desired user
                let mut admin_cfg = cfg.clone();
                admin_cfg.port = Some(port);
                admin_cfg.host = Some(host.clone());
                admin_cfg.user = Some(embedded_user.clone());
                admin_cfg.password = Some(embedded_password.clone());
                admin_cfg.dbname = Some("postgres".to_string());

                let admin_pool = ConfigAndPool::new_postgres(admin_cfg).await?;
                let admin_conn = admin_pool.get_connection().await?;

                if let MiddlewarePoolConnection::Postgres {
                    client: pgconn, ..
                } = admin_conn
                {
                    // Create the desired user with the desired password
                    let create_user_sql = format!(
                        "CREATE USER \"{desired_user}\" WITH PASSWORD '{desired_password}' CREATEDB SUPERUSER"
                    );
                    pgconn.execute(&create_user_sql, &[]).await
                        .map_err(|e| format!("Failed to create user {desired_user}: {e}"))?;

                    println!("Created user {desired_user} with desired credentials");
                }

                (desired_user.clone(), desired_password.clone())
            }
        } else {
            (embedded_user, embedded_password)
        };

        let database_url =
            format!("postgres://{final_user}:{final_password}@{host}:{port}/{db_name}");

        println!("PostgreSQL started on port {port}");
        println!("Database URL: {database_url}");

        // Create final config with correct credentials
        let mut final_cfg = cfg.clone();
        final_cfg.port = Some(port);
        final_cfg.host = Some(host);
        final_cfg.user = Some(final_user);
        final_cfg.password = Some(final_password);

        // Quick connection test
        let config_and_pool = ConfigAndPool::new_postgres(final_cfg.clone()).await?;
        let conn = config_and_pool.get_connection().await?;

        if let MiddlewarePoolConnection::Postgres {
            client: pgconn, ..
        } = conn
        {
            // Test with a simple query
            pgconn.execute("SELECT 1", &[]).await?;
            println!("Successfully connected to embedded PostgreSQL database!");
        }

        Ok(EmbeddedPostgres {
            postgresql,
            port,
            database_url,
            config: final_cfg,
        })
    })
}

/// Stop a previously started embedded `PostgreSQL` instance.
#[cfg(feature = "test-utils")]
pub fn stop_postgres_embedded(postgres: EmbeddedPostgres) {
    let EmbeddedPostgres { postgresql, .. } = postgres;
    SHARED_RUNTIME.block_on(async move {
        let _ = postgresql.stop().await;
    });
}

/// Legacy alias for [`setup_postgres_embedded`].
///
/// # Errors
/// Propagates any error returned by [`setup_postgres_embedded`].
///
/// # Panics
/// See [`setup_postgres_embedded`].
#[cfg(feature = "test-utils")]
pub fn setup_postgres_container(
    cfg: &deadpool_postgres::Config,
) -> Result<EmbeddedPostgres, Box<dyn std::error::Error>> {
    setup_postgres_embedded(cfg)
}

/// Stop a previously started embedded `PostgreSQL` instance (legacy name).
#[cfg(feature = "test-utils")]
pub fn stop_postgres_container(postgres: EmbeddedPostgres) {
    stop_postgres_embedded(postgres);
}
