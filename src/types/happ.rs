use anyhow::{Context, Result};
use holochain_types::dna::AgentPubKey;
use holochain_types::prelude::ActionHashB64;
use holochain_types::prelude::AgentPubKeyB64;
use holochain_types::prelude::MembraneProof;
use holofuel_types::fuel::Fuel;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashSet;
use std::fs::File;
use std::time::Duration;
use std::{collections::HashMap, env};
use tracing::{trace, warn};

#[derive(Deserialize, Debug, Clone)]
pub struct PublisherPricingPref {
    pub cpu: Fuel,
    pub storage: Fuel,
    pub bandwidth: Fuel,
}

#[derive(Deserialize, Debug, Clone)]
pub struct DnaResource {
    pub hash: String, // hash of the dna, not a stored dht address
    pub src_url: String,
    pub nick: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HostSettings {
    pub is_enabled: bool,
    pub is_host_disabled: bool,
    pub is_auto_disabled: bool,
}

#[derive(Deserialize, Debug, Clone)]
pub struct PresentedHappBundle {
    pub id: ActionHashB64,
    pub provider_pubkey: AgentPubKeyB64,
    pub is_draft: bool,
    pub is_paused: bool,
    pub uid: Option<String>,
    pub bundle_url: String,
    pub name: String,
    pub categories: Vec<String>,
    pub jurisdictions: Vec<String>,
    pub exclude_jurisdictions: bool,
    pub special_installed_app_id: Option<String>,
    pub host_settings: HostSettings,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HappPreferences {
    pub max_fuel_before_invoice: Fuel,
    pub max_time_before_invoice: Duration,
    pub price_compute: Fuel,
    pub price_storage: Fuel,
    pub price_bandwidth: Fuel,
    pub invoice_due_in_days: u8,
    pub jurisdiction_prefs: Option<ExclusivePreferences>,
    pub categories_prefs: Option<ExclusivePreferences>,
}
impl HappPreferences {
    /// Save preferences to a file under {SL_PREFS_PATH}
    /// which allows hpos-api to read current values
    pub fn save(self) -> Result<Self> {
        if let Ok(path) = env::var("SL_PREFS_PATH") {
            trace!("Writing default servicelogger prefs to {}", &path);
            // create or overwrite to a file
            let file = File::create(&path)?;
            serde_yaml::to_writer(file, &self).context(format!(
                "Failed writing service logger preferences to file {}",
                path
            ))?;
        };
        Ok(self)
    }

    pub fn is_happ_publisher_in_valid_jurisdiction(
        &self, // host preferences
        maybe_publisher_jurisdiction: &Option<String>,
    ) -> bool {
        let (jurisdictions_list, is_exclusive_list) = match self.jurisdiction_prefs.to_owned() {
            Some(c) => {
                let jurisdictions_list: HashSet<String> = c.value.iter().cloned().collect();
                (jurisdictions_list, c.is_exclusion)
            }
            None => {
                warn!("Could not get publisher jurisdiction for happ.");
                return false;
            }
        };

        let publisher_jurisdiction = match maybe_publisher_jurisdiction {
            Some(pj) => pj,
            None => {
                warn!("Could not get publisher jurisdiction for happ.");
                return false;
            }
        };

        let host_preferences_contain_happ_jurisdiction =
            jurisdictions_list.contains(publisher_jurisdiction);

        if host_preferences_contain_happ_jurisdiction && is_exclusive_list {
            // if the happ contains a jurisdiction that is in an exlusive list, then happ is invalid
            return false;
        }
        if !host_preferences_contain_happ_jurisdiction && !is_exclusive_list {
            // if the happ doesn't a jurisdiction that is in an inclusive list, then happ is invalid
            return false;
        }

        true
    }

    pub fn is_happ_valid_category(
        &self, // host preferences
        happ_categories: &[String],
    ) -> bool {
        let (categories_list, is_exclusive_list) = match self.categories_prefs.to_owned() {
            Some(c) => {
                let categories_list: HashSet<String> = c.value.iter().cloned().collect();
                (categories_list, c.is_exclusion)
            }
            None => {
                warn!("Host's category preferences not available");
                return false;
            }
        };

        let host_preferences_contain_happ_category = happ_categories
            .iter()
            .any(|category| categories_list.contains(category));

        if host_preferences_contain_happ_category && is_exclusive_list {
            // if the happ contains a category that is in an exlusive list, then happ is invalid
            return false;
        }
        if !host_preferences_contain_happ_category && !is_exclusive_list {
            // if the happ doesn't a category that is in an inclusive list, then happ is invalid
            return false;
        }

        true
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct ServiceloggerHappPreferences {
    pub provider_pubkey: AgentPubKey,
    pub max_fuel_before_invoice: Fuel,
    pub price_compute: Fuel,
    pub price_storage: Fuel,
    pub price_bandwidth: Fuel,
    pub max_time_before_invoice: Duration,
    pub invoice_due_in_days: u8, // how many days after an invoice is created it it due
}

#[derive(Serialize, Debug, Clone)]
pub struct InstallHappBody {
    pub happ_id: String,
    pub preferences: HappPreferences,
    pub membrane_proofs: HashMap<String, MembraneProof>,
}

// NB: This struct is currently only used for categories and jurisdictions
#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct ExclusivePreferences {
    pub value: Vec<String>,
    pub is_exclusion: bool,
}

pub struct PublisherJurisdiction {
    pub happ_id: ActionHashB64,
    pub jurisdiction: Option<String>,
}
