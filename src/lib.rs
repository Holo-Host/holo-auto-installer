// TODO: https://github.com/tokio-rs/tracing/issues/843
#![allow(clippy::unit_arg)]
pub mod config;
pub mod entries;
pub mod websocket;
use anyhow::Result;
pub use websocket::{AdminWebsocket, AppWebsocket};
pub mod get_apps;
use get_apps::get_all_enabled_hosted_happs;
mod install_app;
use install_app::install_holo_hosted_happs;
mod uninstall_apps;
use tracing::info;
use uninstall_apps::uninstall_removed_happs;
mod get_kyc_level;
use get_kyc_level::get_kyc_level;

/// gets all the enabled happs from HHA
/// installs new happs that were enabled or registered by its provider
/// and uninstalles old happs that were disabled or deleted by its provider
pub async fn run(core_happ: &config::Happ, config: &config::Config) -> Result<()> {
    info!("Activating holo hosted apps");
    let is_kyc_level_2 = get_kyc_level().await? == KycLevel::Level2;
    let list_of_happs = get_all_enabled_hosted_happs(core_happ, config).await?;
    install_holo_hosted_happs(&list_of_happs, config, is_kyc_level_2).await?;
    uninstall_removed_happs(&list_of_happs, config, is_kyc_level_2).await?;
    Ok(())
}
