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
    pub membrane_proofs: HashMap<String, MembraneProof>,
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
                warn!("No host jurisdiction for happ set.");
                return true;
            }
        };

        let publisher_jurisdiction = match maybe_publisher_jurisdiction {
            Some(pj) => pj,
            None => {
                warn!("Could not get publisher jurisdiction for happ");
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
                warn!("No category preferences set by host");
                return true;
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

impl From<hpos_hc_connect::hha_types::HappPreferences> for HappPreferences {
    fn from(value: hpos_hc_connect::hha_types::HappPreferences) -> Self {
        HappPreferences {
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
