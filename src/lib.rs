// TODO: https://github.com/tokio-rs/tracing/issues/843
#![allow(clippy::unit_arg)]
pub mod config;
pub mod entries;
pub mod websocket;
use anyhow::Result;
pub use websocket::{AdminWebsocket, AppWebsocket};
pub mod host_zome_calls;
use host_zome_calls::get_all_enabled_hosted_happs;
mod install_app;
use install_app::install_holo_hosted_happs;
mod uninstall_apps;
use tracing::{debug, info};
use uninstall_apps::uninstall_removed_happs;
mod get_kyc_level;
use get_kyc_level::{get_kyc_level, KycLevel};

use crate::host_zome_calls::CoreAppClient;

/// gets all the enabled happs from HHA
/// installs new happs that were enabled or registered by its provider
/// and uninstalles old happs that were disabled or deleted by its provider
pub async fn run(core_happ: &config::Happ, config: &config::Config) -> Result<()> {
    info!("Activating holo hosted apps");
    let kyc_level = get_kyc_level().await?;
    debug!("Got kyc level {:?}", &kyc_level);
    let is_kyc_level_2 = kyc_level == KycLevel::Level2;

    let mut core_app_client = CoreAppClient::connect(core_happ, config).await?;
    let list_of_happs = get_all_enabled_hosted_happs(&mut core_app_client).await?;
    install_holo_hosted_happs(&mut core_app_client, config, &list_of_happs, is_kyc_level_2).await?;
    uninstall_removed_happs(&mut core_app_client, config, &list_of_happs, is_kyc_level_2).await?;
    Ok(())
}
