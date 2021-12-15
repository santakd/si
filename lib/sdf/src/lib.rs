#![warn(
    clippy::unwrap_in_result,
    clippy::unwrap_used,
    clippy::panic,
    clippy::missing_panics_doc,
    clippy::panic_in_result_fn
)]
#![allow(
    clippy::missing_errors_doc,
    clippy::module_inception,
    clippy::module_name_repetitions
)]

mod server;
pub use server::{
    build_service, service, Config, ConfigError, ConfigFile, IncomingStream, JwtSigningKey,
    MigrationMode, Server, StandardConfig, StandardConfigFile,
};