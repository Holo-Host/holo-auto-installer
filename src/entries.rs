use hc_utils::{WrappedActionHash, WrappedAgentPubKey};
use holochain_types::prelude::MembraneProof;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use holofuel_types::fuel::Fuel;

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

#[derive(Serialize, Debug, Clone)]
pub struct InstallHappBody {
    pub happ_id: String,
    pub preferences: Preferences,
    pub membrane_proofs: HashMap<String, MembraneProof>,
}
