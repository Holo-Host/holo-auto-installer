use holochain_types::prelude::MembraneProof;
use holofuel_types::fuel::Fuel;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::collections::HashSet;
use std::time::Duration;
use tracing::warn;

#[derive(Serialize, Debug, Clone)]
pub struct InstallHappBody {
    pub happ_id: String,
    pub preferences: HostHappPreferences,
    pub membrane_proofs: HashMap<String, MembraneProof>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HostHappPreferences {
    pub max_fuel_before_invoice: Fuel,
    pub max_time_before_invoice: Duration,
    pub price_compute: Fuel,
    pub price_storage: Fuel,
    pub price_bandwidth: Fuel,
    pub invoice_due_in_days: u8,
    pub jurisdiction_prefs: Option<ExclusivePreferences>,
    pub categories_prefs: Option<ExclusivePreferences>,
}
impl HostHappPreferences {
    pub fn is_happ_publisher_in_valid_jurisdiction(
        &self, // host preferences
        maybe_publisher_jurisdiction: &Option<String>,
    ) -> bool {
        let (host_jurisdictions_list, is_exclusion_list) = match self.jurisdiction_prefs.to_owned()
        {
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

        if is_exclusion_list {
            // If the publisher is in a jurisdiction that is in the host's exclusion list, then happ is invalid
            return !host_jurisdictions_list.contains(publisher_jurisdiction);
        }
        // Otherwise, the happ is valid if its publisher is in a jurisdiction that is in the host's inclusion list
        host_jurisdictions_list.contains(publisher_jurisdiction)
    }

    pub fn is_happ_valid_category(
        &self, // host preferences
        happ_categories: &[String],
    ) -> bool {
        let (host_categories_list, is_exclusion_list) = match self.categories_prefs.to_owned() {
            Some(c) => {
                let categories_list: HashSet<String> = c.value.iter().cloned().collect();
                (categories_list, c.is_exclusion)
            }
            None => {
                warn!("Host's category preferences not available");
                return false;
            }
        };

        let happ_category_exists_in_host_preferences = happ_categories
            .iter()
            .any(|category| host_categories_list.contains(category));

        if is_exclusion_list {
            // If the happ contains a category that is in the host's exclusion list, then happ is invalid
            return !happ_category_exists_in_host_preferences;
        }
        // Otherwise, the happ is valid if it contains a category that is in the host's inclusion list
        happ_category_exists_in_host_preferences
    }
}

impl From<hpos_hc_connect::hha_types::HappPreferences> for HostHappPreferences {
    fn from(value: hpos_hc_connect::hha_types::HappPreferences) -> Self {
        HostHappPreferences {
            max_fuel_before_invoice: value.max_fuel_before_invoice,
            max_time_before_invoice: value.max_time_before_invoice,
            price_compute: value.price_compute,
            price_storage: value.price_storage,
            price_bandwidth: value.price_bandwidth,
            invoice_due_in_days: value.invoice_due_in_days,
            jurisdiction_prefs: if value.jurisdiction_prefs.is_some() {
                Some(value.jurisdiction_prefs.unwrap().into())
            } else {
                None
            },
            categories_prefs: if value.categories_prefs.is_some() {
                Some(value.categories_prefs.unwrap().into())
            } else {
                None
            },
        }
    }
}

// NB: This struct is currently only used for categories and jurisdictions
#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct ExclusivePreferences {
    pub value: Vec<String>,
    pub is_exclusion: bool,
}

impl From<hpos_hc_connect::hha_types::ExclusivePreferences> for ExclusivePreferences {
    fn from(value: hpos_hc_connect::hha_types::ExclusivePreferences) -> Self {
        ExclusivePreferences {
            value: value.value,
            is_exclusion: value.is_exclusion,
        }
    }
}
