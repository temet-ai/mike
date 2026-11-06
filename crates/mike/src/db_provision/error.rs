use std::io;
use std::path::PathBuf;

use crate::user_config::UserConfigError;

#[derive(Debug, thiserror::Error)]
pub(crate) enum DbError {
    #[error(
        "docker is not installed. Install Docker Desktop, OrbStack or Colima and run `mike db up` again, or run `mike db native` for a Postgres without containers"
    )]
    DockerMissing,
    #[error(
        "the docker daemon does not answer ({detail}). Start Docker Desktop, OrbStack or Colima and run the command again"
    )]
    DockerNotRunning { detail: String },
    #[error(
        "neither `docker compose` nor `docker-compose` works. Install the Docker Compose plugin and run the command again"
    )]
    ComposeMissing,
    #[error(
        "port {port} on 127.0.0.1 is already taken by another program. Choose a free one with `mike db up --port <port>`"
    )]
    PortInUse { port: u16 },
    #[error("`{command}` failed: {detail}")]
    CommandFailed { command: String, detail: String },
    #[error("could not run `{program}`: {source}")]
    Spawn {
        program: String,
        #[source]
        source: io::Error,
    },
    #[error(
        "the database container is not healthy after {waited_secs}s (state: {state}). Last log lines:\n{logs}"
    )]
    NotHealthy {
        waited_secs: u64,
        state: String,
        logs: String,
    },
    #[error(
        "the docker volume `mike_pg_data` already holds a database whose password differs from the one in the mike config file. Restore the original `database_url` in the config file, or delete the volume with `docker volume rm mike_pg_data` (this deletes the stored captures) and run `mike db up` again"
    )]
    PasswordMismatch,
    #[error("the database is running and its URL is saved, but preparing the schema failed: {0}")]
    Migrate(String),
    #[error("PostgreSQL did not answer on {host_port} after {waited_secs}s")]
    NotReady { host_port: String, waited_secs: u64 },
    #[error(transparent)]
    Config(#[from] UserConfigError),
    #[error("cannot write {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}
