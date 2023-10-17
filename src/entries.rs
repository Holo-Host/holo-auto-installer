use anyhow::{Context, Result};
use holochain_types::prelude::ActionHashB64;
use holochain_types::prelude::AgentPubKeyB64;
use holochain_types::prelude::MembraneProof;
use holofuel_types::fuel::Fuel;
use serde::Deserialize;
use serde::Serialize;
use std::fs::File;
use std::{collections::HashMap, env};
use tracing::trace;

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

#[derive(Deserialize, Debug, Clone)]
pub struct PresentedHappBundle {
    pub id: ActionHashB64,
    pub provider_pubkey: AgentPubKeyB64,
    pub is_draft: bool,
    pub is_paused: bool,
    pub uid: Option<String>,
    pub bundle_url: String,
    pub name: String,
    pub special_installed_app_id: Option<String>,
}

#[derive(Serialize, Debug, Clone)]
pub struct Preferences {
    pub max_fuel_before_invoice: Fuel,
    pub max_time_before_invoice: Vec<u64>,
    pub price_compute: Fuel,
    pub price_storage: Fuel,
    pub price_bandwidth: Fuel,
}
impl Preferences {
    /// Save preferences to a file under {SL_PREFS_PATH}
    /// which allows hpos-holochain-api to read current values
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
}

#[derive(Serialize, Debug, Clone)]
pub struct InstallHappBody {
    pub happ_id: String,
    pub preferences: Preferences,
    pub membrane_proofs: HashMap<String, MembraneProof>,
}
