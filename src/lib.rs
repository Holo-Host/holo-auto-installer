// TODO: https://github.com/tokio-rs/tracing/issues/843
#![allow(clippy::unit_arg)]
pub mod entries;
use anyhow::Result;
use hpos_hc_connect::hha_agent::HHAAgent;
pub use hpos_hc_connect::AdminWebsocket;
pub mod transaction_types;
mod utils;
use tracing::{debug, error, info};
use utils::{
    get_all_published_hosted_happs, get_pending_transactions, install_holo_hosted_happs,
    suspend_unpaid_happs, uninstall_ineligible_happs,
};
mod hbs;
use hbs::{HbsClient, KycLevel};
use holo_happ_manager::Config;

/// gets all the enabled happs from HHA
/// installs and enables new happs that were registered by a provider and holochain disables those paused by provider in hha
/// then uninstalls happs that are ineligible for host (eg: holo-disabled, unallowed pricing for kyc level)
pub async fn run(config: &Config) -> Result<()> {
    info!("Activating holo hosted apps");
    let hbs_connect = HbsClient::connect()?;
    let hosting_criteria = match hbs_connect.get_hosting_criteria().await {
        Some(v) => v,
        None => {
            error!("Unable to get hosting criteria from HBS. Exiting...");
            return Err(anyhow::anyhow!("Unable to get hosting criteria"));
        }
    };
    let kyc_level = hosting_criteria.kyc;
    debug!("Got kyc level {:?}", &kyc_level);
    let jurisdiction = hosting_criteria.jurisdiction;
    debug!("Got jurisdiction from hbs {:?}", jurisdiction);

    let is_kyc_level_2 = kyc_level == KycLevel::Level2;

    let mut core_app = HHAAgent::spawn(Some(config)).await?;

    // suspend happs that have overdue payments
    let pending_transactions = get_pending_transactions(&mut core_app).await?;
    let suspended_happs = suspend_unpaid_happs(&mut core_app, pending_transactions).await?;

    let list_of_happs = get_all_published_hosted_happs(&mut core_app).await?;
    install_holo_hosted_happs(config, &list_of_happs, is_kyc_level_2).await?;
    uninstall_ineligible_happs(
        config,
        &list_of_happs,
        is_kyc_level_2,
        suspended_happs,
        jurisdiction,
    )
    .await?;
    Ok(())
}
