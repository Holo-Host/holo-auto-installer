// TODO: https://github.com/tokio-rs/tracing/issues/843
#![allow(clippy::unit_arg)]
pub mod config;
pub mod entries;
pub mod websocket;
use anyhow::Result;
pub use websocket::{AdminWebsocket, AppWebsocket};
pub mod host_zome_calls;
pub mod transaction_types;
use host_zome_calls::get_all_published_hosted_happs;
mod install_app;
use install_app::install_holo_hosted_happs;
mod uninstall_apps;
use tracing::{debug, info};
use uninstall_apps::uninstall_ineligible_happs;
mod get_kyc_level;
use get_kyc_level::{get_kyc_level, KycLevel};
mod suspend_happs;
use suspend_happs::suspend_unpaid_happs;

use crate::host_zome_calls::{get_pending_transactions, CoreAppClient};

/// gets all the enabled happs from HHA
/// installs and enables new happs that were registered by a provider and holochain disables those paused by provider in hha
/// then uninstalls happs that are ineligible for host (eg: holo-disabled, unallowed pricing for kyc level)
pub async fn run(core_happ: &config::Happ, config: &config::Config) -> Result<()> {
    info!("Activating holo hosted apps");
    let kyc_level = get_kyc_level().await?;
    debug!("Got kyc level {:?}", &kyc_level);
    let is_kyc_level_2 = kyc_level == KycLevel::Level2;

    let mut core_app_client = CoreAppClient::connect(core_happ, config).await?;

    // suspend happs that have overdue payments
    let pending_transactions = get_pending_transactions(&mut core_app_client).await?;
    let suspended_happs = suspend_unpaid_happs(&mut core_app_client, pending_transactions).await?;

    let list_of_happs = get_all_published_hosted_happs(&mut core_app_client).await?;
    install_holo_hosted_happs(config, &list_of_happs, is_kyc_level_2).await?;
    uninstall_ineligible_happs(config, &list_of_happs, is_kyc_level_2, suspended_happs).await?;
    Ok(())
}
