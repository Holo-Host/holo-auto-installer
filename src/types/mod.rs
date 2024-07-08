pub mod happ;
pub mod transaction;
use holochain_types::dna::ActionHashB64;
use holochain_types::prelude::{holochain_serial, SerializedBytes};
use serde::{Deserialize, Serialize};

pub struct HappBundle {
    pub happ_id: ActionHashB64,
    pub bundle_url: String,
    pub is_paused: bool,
    pub is_host_disabled: bool,
    pub special_installed_app_id: Option<String>,
    pub jurisdictions: Vec<String>,
    pub exclude_jurisdictions: bool,
    pub categories: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, SerializedBytes)]
pub struct PublishedHappDetails {
    pub publisher_jurisdiction: Option<String>,
    pub happ_jurisdictions: Vec<String>,
    pub should_exclude_happ_jurisdictions: bool,
    pub happ_categories: Vec<String>,
    pub is_disabled_by_host: bool,
}
