use std::net::ToSocketAddrs;
use tiberius::Client;
use tokio::net::TcpStream;
use tokio_util::compat::TokioAsyncWriteCompatExt;

use super::config::MssqlClient;
use crate::middleware::SqlMiddlewareDbError;

/// Helper function to create a new MSSQL connection
///
/// # Errors
/// Returns `SqlMiddlewareDbError::ConnectionError` if the MSSQL connection fails.
pub async fn create_mssql_client(
    server: &str,
    database: &str,
    user: &str,
    password: &str,
    port: Option<u16>,
    instance_name: Option<&str>,
) -> Result<MssqlClient, SqlMiddlewareDbError> {
    let mut config = tiberius::Config::new();

    config.host(server);
    config.database(database);
    config.authentication(tiberius::AuthMethod::sql_server(user, password));

    let port_val = port.unwrap_or(1433);
    config.port(port_val);

    if let Some(instance) = instance_name {
        config.instance_name(instance);
    }

    config.trust_cert(); // For testing/development

    // Create TCP stream using proper socket address
    // Try to resolve the socket address
    let addr_iter = (server, port_val).to_socket_addrs().map_err(|e| {
        SqlMiddlewareDbError::ConnectionError(format!("Failed to resolve server address: {e}"))
    })?;

    // Find the first valid address
    let server_addr = addr_iter.into_iter().next().ok_or_else(|| {
        SqlMiddlewareDbError::ConnectionError(format!("No valid address found for {server}"))
    })?;

    // Connect to the resolved socket address
    let tcp = TcpStream::connect(server_addr)
        .await
        .map_err(|e| SqlMiddlewareDbError::ConnectionError(format!("TCP connection error: {e}")))?;

    // Make compatible with Tiberius
    let tcp = tcp.compat_write();

    // Connect with Tiberius
    Client::connect(config, tcp).await.map_err(|e| {
        SqlMiddlewareDbError::ConnectionError(format!("SQL Server connection error: {e}"))
    })
}
