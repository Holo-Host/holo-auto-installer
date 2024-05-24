// TODO: https://github.com/tokio-rs/tracing/issues/843
#![allow(clippy::unit_arg)]
pub mod config;
pub mod entries;
pub mod websocket;
use std::collections::HashMap;

use anyhow::Result;
use holochain_types::dna::{hash_type::Agent, HoloHash};
pub use websocket::{AdminWebsocket, AppWebsocket};
pub mod host_zome_calls;
pub mod transaction_types;
use host_zome_calls::get_all_published_hosted_happs;
mod install_app;
use install_app::install_holo_hosted_happs;
mod uninstall_apps;
use tracing::{debug, error, info};
use uninstall_apps::uninstall_ineligible_happs;
mod suspend_happs;
use suspend_happs::suspend_unpaid_happs;
mod hbs;
use hbs::{HbsClient, KycLevel};

use crate::host_zome_calls::{
    get_happ_preferences, get_hosting_preferences, get_pending_transactions,
    get_publisher_jurisdiction, CoreAppClient,
};

/// gets all the enabled happs from HHA
/// installs and enables new happs that were registered by a provider and holochain disables those paused by provider in hha
/// then uninstalls happs that are ineligible for host (eg: holo-disabled, unallowed pricing for kyc level)
pub async fn run(core_happ: &config::Happ, config: &config::Config) -> Result<()> {
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

    let mut core_app_client = CoreAppClient::connect(core_happ, config).await?;

    // suspend happs that have overdue payments
    let pending_transactions = get_pending_transactions(&mut core_app_client).await?;
    let suspended_happs = suspend_unpaid_happs(&mut core_app_client, pending_transactions).await?;
    let hosting_preference = get_hosting_preferences(&mut core_app_client).await?;

    let list_of_happs = get_all_published_hosted_happs(&mut core_app_client).await?;
    let mut publisher_jurisdictions: HashMap<HoloHash<Agent>, Option<String>> = HashMap::new();
    let mut happ_jurisdictions: HashMap<String, Option<String>> = HashMap::new();
    // get publisher jurisdiction for each happ
    for happ in list_of_happs.iter() {
        let happ_prefs = get_happ_preferences(&mut core_app_client, happ.happ_id.clone()).await?;
        let publisher_pubkey = happ_prefs.provider_pubkey;
        match publisher_jurisdictions.get(&publisher_pubkey) {
            Some(jurisdiction) => {
                happ_jurisdictions
                    .insert(happ.happ_id.clone().to_string(), (*jurisdiction).clone());
            }
            None => {
                let jurisdiction =
                    get_publisher_jurisdiction(&mut core_app_client, publisher_pubkey.clone())
                        .await?;
                publisher_jurisdictions.insert(publisher_pubkey, jurisdiction.clone());
                happ_jurisdictions.insert(happ.happ_id.clone().to_string(), jurisdiction);
            }
        }
    }
    install_holo_hosted_happs(config, &list_of_happs, is_kyc_level_2).await?;
    uninstall_ineligible_happs(
        config,
        &list_of_happs,
        is_kyc_level_2,
        suspended_happs,
        jurisdiction,
        hosting_preference,
        happ_jurisdictions,
    )
    .await?;
    Ok(())
}
