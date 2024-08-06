// TODO: https://github.com/tokio-rs/tracing/issues/843
#![allow(clippy::unit_arg)]
pub mod types;
mod utils;

pub use crate::types::happ::HappPreferences;
pub use hpos_hc_connect::AdminWebsocket;

use anyhow::Result;
use holochain_types::dna::{hash_type::Agent, HoloHash};
use hpos_hc_connect::{hha_agent::CoreAppAgent, holo_config::Config};
use std::collections::HashMap;
use tracing::{debug, error, info};
use types::hbs::{HbsClient, KycLevel};
use types::PublishedHappDetails;
use utils::{
    get_all_published_hosted_happs, get_suspended_happs, handle_ineligible_happs,
    install_holo_hosted_happs,
};

/// 1. Gets all the holo-enabled happs from HHA
/// 2. Suspends happs with overdue payments
/// 3. Installs and enables (enables in holochain and holo) all new happs that were registered by a provider and holochain-disables those paused by provider in hha
/// 4. Uninstalls happs that are ineligible for host (eg: holo-disabled, unallowed pricing for kyc level, incongruent price settings with publisher/happ)
pub async fn run(config: &Config) -> Result<()> {
    info!("Activating holo hosted apps");
    let hbs_connect = HbsClient::connect()?;
    let host_credentials = match hbs_connect.get_host_hosting_criteria().await {
        Some(v) => v,
        None => {
            error!("Unable to get hosting criteria from HBS. Exiting...");
            return Err(anyhow::anyhow!("Unable to get hosting criteria"));
        }
    };
    debug!("Got host credentials from hbs {:?}", host_credentials);

    let mut core_app = CoreAppAgent::spawn(Some(config)).await?;

    // Suspend happs that have overdue payments
    let pending_transactions = core_app.get_pending_transactions().await?;
    let suspended_happs = get_suspended_happs(pending_transactions);

    let published_happs = get_all_published_hosted_happs(&mut core_app).await?;

    // Get happ jurisdictions AND publisher jurisdiction for each happ
    let mut published_happ_details: HashMap<String, PublishedHappDetails> = HashMap::new();
    let mut publisher_jurisdictions: HashMap<HoloHash<Agent>, Option<String>> = HashMap::new();

    for happ in published_happs.iter() {
        let happ_prefs = core_app.get_happ_preferences(happ.happ_id.clone()).await?;
        let publisher_pubkey = happ_prefs.provider_pubkey;

        // If already have publisher pubkey stored in `publisher_jurisdictions` map, then grab the jurisdiction value and set value in `published_happ_details` map
        // otherwise, make a call to hha to fetch the publisher jurisdiction and set in both the `published_happ_details` map and `publisher_jurisdictions` map
        match publisher_jurisdictions.get(&publisher_pubkey) {
            Some(jurisdiction) => {
                published_happ_details.insert(
                    happ.happ_id.clone().to_string(),
                    PublishedHappDetails {
                        publisher_jurisdiction: (*jurisdiction).clone(),
                        happ_jurisdictions: happ.jurisdictions.clone(),
                        should_exclude_happ_jurisdictions: happ.exclude_jurisdictions,
                        happ_categories: happ.categories.clone(),
                        is_disabled_by_host: happ.is_host_disabled,
                    },
                );
            }
            None => {
                let jurisdiction = core_app
                    .get_publisher_jurisdiction(publisher_pubkey.clone())
                    .await?;
                publisher_jurisdictions.insert(publisher_pubkey, jurisdiction.clone());
                published_happ_details.insert(
                    happ.happ_id.clone().to_string(),
                    PublishedHappDetails {
                        publisher_jurisdiction: jurisdiction,
                        happ_jurisdictions: happ.jurisdictions.clone(),
                        should_exclude_happ_jurisdictions: happ.exclude_jurisdictions,
                        happ_categories: happ.categories.clone(),
                        is_disabled_by_host: happ.is_host_disabled,
                    },
                );
            }
        }
    }

    let host_happ_preferences = core_app.get_host_preferences().await?.into();

    let is_host_kyc_level_2 = host_credentials.clone().kyc == KycLevel::Level2;

    install_holo_hosted_happs(config.admin_port, &published_happs, is_host_kyc_level_2).await?;

    handle_ineligible_happs(
        &mut core_app,
        config.admin_port,
        suspended_happs,
        host_credentials,
        host_happ_preferences,
        published_happ_details,
    )
    .await?;
    Ok(())
}
