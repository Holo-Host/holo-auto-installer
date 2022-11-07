use anyhow::{Context, Result};
use hc_utils::{WrappedActionHash, WrappedAgentPubKey};
use holochain_types::prelude::MembraneProof;
use holofuel_types::fuel::Fuel;
use serde::Deserialize;
use serde::Serialize;
use std::fs::File;
use std::{collections::HashMap, env};

#[derive(Deserialize, Debug, Clone)]
pub struct DnaResource {
    pub hash: String, // hash of the dna, not a stored dht address
    pub src_url: String,
    pub nick: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct PresentedHappBundle {
    pub id: WrappedActionHash,
    pub provider_pubkey: WrappedAgentPubKey,
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
