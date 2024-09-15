use crate::types::PublishedHappDetails;
pub use crate::types::{
    happ::{HostHappPreferences, InstallHappBody},
    hbs::{HostCredentials, KycLevel},
    transaction::InvoiceNote,
    HappBundle,
};
use anyhow::{Context, Result};
use chrono::Utc;
use holochain_conductor_api::AppStatusFilter;
use holochain_types::dna::ActionHashB64;
use holochain_types::prelude::{AppManifest, MembraneProof, SerializedBytes, UnsafeBytes};
use hpos_hc_connect::{
    hha_agent::CoreAppAgent,
    holofuel_types::{PendingTransaction, POS},
    utils::download_file,
    AdminWebsocket,
};
use itertools::Itertools;
use mr_bundle::Bundle;
use std::{
    collections::{HashMap, HashSet},
    env,
    process::Command,
    sync::Arc,
};
use tracing::{debug, error, info, trace, warn};
use url::Url;

/// @TODO: Temporary read-only mem-proofs solution
/// This fn should be replaced by calling the joining-code service and getting the appropriate proof for the agent
pub async fn load_mem_proof_file(bundle_url: &str) -> Result<HashMap<String, MembraneProof>> {
    let url = Url::parse(bundle_url)?;

    let path = download_file(&url).await?;

    let bundle = Bundle::read_from_file(&path).await?;

    let AppManifest::V1(manifest) = bundle.manifest();

    Ok(manifest
        .roles
        .clone()
        .iter()
        .map(|role| {
            (
                role.name.clone(),
                Arc::new(SerializedBytes::from(UnsafeBytes::from(vec![0]))),
            ) // The read only memproof is [0] (or in base64 `AA==`)
        })
        .collect())
}

pub async fn get_all_published_hosted_happs(
    core_app_client: &mut CoreAppAgent,
) -> Result<Vec<HappBundle>> {
    trace!("get_all_published_hosted_happs");

    let happ_bundles = core_app_client.get_happs().await?;

    let happ_bundle_ids = happ_bundles
        .into_iter()
        .map(|happ| {
            trace!(
                "{} with happ-id: {:?} and bundle: {}, is-paused={}",
                happ.name,
                happ.id,
                happ.bundle_url,
                happ.is_paused
            );
            HappBundle {
                happ_id: happ.id,
                bundle_url: happ.bundle_url,
                is_paused: happ.is_paused,
                special_installed_app_id: happ.special_installed_app_id,
                jurisdictions: happ.jurisdictions,
                exclude_jurisdictions: happ.exclude_jurisdictions,
                categories: happ.categories,
                host_settings: happ.host_settings,
                provider_pubkey: happ.provider_pubkey,
                network_seed: happ.uid,
            }
        })
        .collect();

    trace!("got happ bundles");
    Ok(happ_bundle_ids)
}

async fn get_holoport_id() -> Result<String> {
    let password =
        env::var("DEVICE_SEED_DEFAULT_PASSWORD").expect("DEVICE_SEED_DEFAULT_PASSWORD is not set");
    let hpos_config_path = env::var("HPOS_CONFIG_PATH")
        .expect("HPOS_CONFIG_PATH not found. please add the path to the environment variable");

    let holoport_id_output = Command::new("hpos-config-into-base36-id")
        .arg("--config-path")
        .arg(hpos_config_path)
        .arg("--password")
        .arg(password)
        .output()
        .expect("Failed to execute command");
    let holoport_id = String::from_utf8_lossy(&holoport_id_output.stdout);
    Ok(holoport_id.to_string())
}

// There are core infrastructure happs that should never be uninstalled. All uninstallable happs start with "uhCkk" and don't contain ::servicelogger
fn is_hosted_happ(installed_app_id: &str) -> bool {
    installed_app_id.starts_with("uhCkk") && !installed_app_id.contains("::servicelogger")
}

fn is_anonymous_instance(installed_app_id: &str) -> bool {
    installed_app_id.starts_with("uhCkk") && installed_app_id.len() == 53
}

/// Returns true if `installed_app_id` represents an anonymous or identified instance of `happ_id`
fn is_instance_of_happ(happ_id: &str, installed_app_id: &str) -> bool {
    // An `installed_app_id` is one of
    // - A core hApp (e.g. `servicelogger:0_2_1::251e7cc8-9c48-4841-9eb0-435f0bf97373`)
    // - An anonymous instance with installed_app_id == happ_id
    // - An identified instance matching /happ_id::agent_id/
    // - A happ-specific servicelogger instance matching /happ_id::servicelogger/
    happ_id == installed_app_id // anonymous
        || installed_app_id.starts_with(happ_id) && !installed_app_id.ends_with("servicelogger")
}

// NB: Suspended happs are all happs that have invoices which remain unpaid at/after the invoice due date
pub fn get_suspended_happs(pending_transactions: PendingTransaction) -> Vec<String> {
    let suspended_happs = pending_transactions
        .invoice_pending
        .iter()
        .filter_map(|invoice| {
            if let Some(POS::Hosting(_)) = &invoice.proof_of_service {
                if let Some(expiration_date) = invoice.expiration_date {
                    if expiration_date.as_millis() < Utc::now().timestamp_millis() {
                        if let Some(note) = invoice.note.clone() {
                            let invoice_note: Result<InvoiceNote, _> = serde_yaml::from_str(&note);
                            match invoice_note {
                                Ok(note) => {
                                    let hha_id = note.hha_id;
                                    return Some(hha_id.clone().to_string());
                                }
                                Err(e) => {
                                    error!("Error parsing invoice note: {:?}", e);
                                    return None;
                                }
                            }
                        }
                    }
                }
            }
            None
        })
        .collect();

    debug!("Created suspend happs list: {:?}", suspended_happs);
    suspended_happs
}

pub async fn should_be_enabled(
    installed_happ_id: &String,
    happ_id: String,
    suspended_happs: Vec<String>,
    host_credentials: HostCredentials, // the kyc and jurisdiction of a host
    host_happ_preferences: HostHappPreferences, // the hosting preferences a host sets
    published_happ_details: HashMap<String, PublishedHappDetails>, // the jurisdiction, categories, and publisher jurisdiction for each happ
) -> bool {
    trace!(
        "Running the `should_be_enabled check` for {}",
        installed_happ_id
    );

    if suspended_happs.contains(&happ_id) {
        trace!("Disabling suspended happ {}", happ_id);
        return false;
    }

    // Iterate over each happ details to run credentials check between the happ, publisher, and host:
    if let Some(happ_registration_details) = published_happ_details.get(&happ_id) {
        // Verify that the publisher's jurisdiction matches the host's jurisdiction preferences
        if !host_happ_preferences.is_happ_publisher_in_valid_jurisdiction(
            &happ_registration_details.publisher_jurisdiction,
        ) {
            warn!(
                "Happ {} will be disabled/uninstalled because publisher is in invalid jurisdiction ",
                installed_happ_id
            );
            // Return false; app should not remain installed/enabled if publisher juridiction is invalid
            return false;
        }

        // Verify that the host's jurisdiction matches the app's jurisdiction list - (ie: ensure that the hApp is allowed to run on the host's current jurisdiction)
        // NB: The host's jurisdiction is taken from mongodb (via hbs)
        if !host_credentials.is_host_in_valid_jurisdiction(
            happ_registration_details.should_exclude_happ_jurisdictions,
            &happ_registration_details.happ_jurisdictions,
        ) {
            warn!(
                "Happ {} will be will be disabled/uninstalled because host is in invalid jurisdiction",
                installed_happ_id
            );
            // Return false; app should not remain installed/enabled if host juridiction is invalid
            return false;
        }

        // Verify that the hApp category is a valid host category.
        if !host_happ_preferences.is_happ_valid_category(&happ_registration_details.happ_categories)
        {
            warn!(
                "Happ {} will be will be disabled/uninstalled because happ category is invalid based on host preferences",
                installed_happ_id
            );
            // Return false; app should not remain installed/enabled if happ category is invalid
            return false;
        };

        // Check whether the expected happ is disabled by the host.
        if happ_registration_details.is_disabled_by_host {
            trace!(
                "Disabling happ in Holochain Conductor {} because host disabled happ it in hha",
                installed_happ_id
            );
            return false;
        }
    }

    // NB: Happ-hosting is only valid (despite price prefs) if the host is >= kyc level 2
    host_credentials.kyc == KycLevel::Level2
}

/// Installs all happs that are eligible for hosting
pub async fn install_holo_hosted_happs(
    admin_port: u16,
    happs: &[HappBundle],
    is_kyc_level_2: bool,
) -> Result<()> {
    info!("Starting to install....");

    if happs.is_empty() {
        info!("No happs registered to be enabled for hosting.");
        return Ok(());
    }

    let mut admin_websocket = AdminWebsocket::connect(admin_port)
        .await
        .context("failed to connect to holochain's admin interface")?;

    let enabled_happs = Arc::new(
        admin_websocket
            .list_apps(Some(AppStatusFilter::Enabled))
            .await
            .context("failed to get installed hApps")?,
    );

    let enabled_happ_ids: Vec<String> = enabled_happs
        .iter()
        .map(|h| h.installed_app_id.clone())
        .unique()
        .collect();
    trace!("enabled_happs {:?}", enabled_happ_ids);

    // Iterate through the vec and
    // Call http://localhost/api/v2/apps/hosted/install
    // for each WrappedActionHash to install the hosted_happ
    for HappBundle {
        happ_id,
        bundle_url,
        is_paused,
        special_installed_app_id,
        exclude_jurisdictions: _,
        jurisdictions: _,
        categories: _,
        host_settings,
        ..
    } in happs
    {
        trace!("Trying to install {}", happ_id);

        // Currently, the Hosted HoloFuel and Cloud Console happs should have a `special_installed_app_id`.
        // If happ has a `special_installed_app_id`, the happ relies on the core-app for dna calls.
        // In this case we only need to confirm that the hosted happ has an enabled sl instance.
        // If it does have a runnning SL, we consider the app ready for use and and do nothing
        // ...otherwise, we proceed to install, which leads to the installation of a sl instance for this happ
        if special_installed_app_id.is_some()
            && enabled_happ_ids.contains(&format!("{}::servicelogger", happ_id))
            && host_settings.is_enabled
        {
            // Skip the install/enable step
            // NB: We expect our core-app to already be installed and enabled as we never pause/disable/uninstall it
            trace!(
                "Special App {:?} already installed",
                special_installed_app_id
            );
        }
        // Iterate through all currently enabled apps
        // (NB: The sole exceptions here are Hosted HoloFuel and Cloud Console, as they should always be caught by the prior condition.)
        else if enabled_happ_ids.contains(&format!("{}", happ_id)) && host_settings.is_enabled {
            trace!("App {} already installed", happ_id);
            // Check if this happ was paused by the publisher in hha and disable it in holochain if so
            if *is_paused {
                trace!(
                    "Found paused happ in holo {} - disabling happ on holochain conductor.",
                    happ_id
                );
                admin_websocket.disable_app(&happ_id.to_string()).await?;
            }
        }
        // if the expected happ is disabled by the host, we don't install
        else if host_settings.is_host_disabled.to_owned() {
            trace!(
                "Skipping happ installation due to host's disabled setting for happ {}",
                happ_id
            );
        }
        // if kyc_level is not 2 then happ hosting is not allowed and we don't install
        else if !is_kyc_level_2 {
            trace!(
                "Skipping hosting of happ {} due to host's kyc level ",
                happ_id
            );
        } else {
            // else, install the hosted happ read-only instance
            // (NB: The read-only instance is an instance of the app that installed with the host agent pubkey and a read-only memproof.)
            trace!("Load mem-proofs for {}", happ_id);
            let mem_proof: HashMap<String, MembraneProof> =
                load_mem_proof_file(bundle_url).await.unwrap_or_default();
            trace!(
                "Installing happ-id {} with mem_proof {:?}",
                happ_id,
                mem_proof
            );

            // The installation implementation can be found in`hpos-api` here: https://github.com/Holo-Host/hpos-api-rust/blob/develop/src/handlers/install/mod.rs#L31
            // NB: The `/install_hosted_happ` endpoint will holo-enable the app if it is already installed and enabled on hololchain,
            // ...otherwise it takes the following 5 steps:
            // 1. installs sl for the app,
            // 2. holochain-enables the app's sl,
            // 3. installs the app on holochain (NB: The app installs with the host agent pubkey as this a read-only instance),
            // 4. holochain-enables the app,
            // 5. holo-enables the app
            let body = InstallHappBody {
                happ_id: happ_id.to_string(),
                membrane_proofs: mem_proof.clone(),
            };
            let client = reqwest::Client::new();
            let response = client
                .post("http://localhost/api/v2/apps/hosted/install")
                .json(&body)
                .send()
                .await?;
            trace!("`/v2/apps/hosted/install` happ response {:?}", response);
            info!("Installed and enabled happ-id {}", happ_id);
        }
    }
    Ok(())
}

/// Handles ineligible happs for 2 cases - identified and anonymous hosted agents:
///  - Identified: Uninstalls & removes identified instances of ineligible happs
///  - Anonymous: Disables anonymous instance of ineligible happs
/// Ineligible Happs = old holo-hosted happs, holo-disabled happs, suspended happs, or happs with one of the following:
///  - 1. an invalid pricing for kyc level, 2. invalid pricing preference, 3. invalid uptime, or 4. invalid jurisdiction
pub async fn handle_ineligible_happs(
    core_app_client: &mut CoreAppAgent,
    admin_port: u16,
    suspended_happs: Vec<String>,
    host_credentials: HostCredentials,
    host_happ_preferences: HostHappPreferences,
    published_happ_details: HashMap<String, PublishedHappDetails>,
) -> Result<()> {
    info!("Checking to uninstall happs that were removed from the hosted list....");

    let mut happs_to_holo_disable = HashSet::new();

    let mut admin_websocket = AdminWebsocket::connect(admin_port)
        .await
        .context("Failed to connect to holochain's admin interface")?;

    let enabled_happs = admin_websocket
        .list_apps(Some(AppStatusFilter::Enabled))
        .await
        .context("Failed to get installed and enabled hApps")?;

    let enabled_happ_ids: Vec<&String> = enabled_happs
        .iter()
        .map(|h| &h.installed_app_id)
        .unique()
        .collect();
    trace!("enabled_happ_ids {:?}", enabled_happ_ids);

    let published_happ_ids: Vec<String> = published_happ_details.clone().into_keys().collect();
    trace!("published_happ_ids {:?}", published_happ_ids);

    for enabled_happ_id in enabled_happ_ids {
        // Deteremine if the enabled happ is an instance of a published happ
        let maybe_hosted_instance_happ_id = published_happ_ids
            .clone()
            .into_iter()
            .find(|published_happ_id| is_instance_of_happ(published_happ_id, enabled_happ_id));

        let should_happ_remain_enabled = match maybe_hosted_instance_happ_id {
            Some(happ_id) => {
                trace!("Found hosted happ instance {:?}", &happ_id);

                let should_remain_enabled = should_be_enabled(
                    &enabled_happ_id.to_string(),
                    happ_id.clone(),
                    suspended_happs.clone(),
                    host_credentials.clone(),
                    host_happ_preferences.clone(),
                    published_happ_details.clone(),
                )
                .await;

                if !should_remain_enabled {
                    happs_to_holo_disable.insert(happ_id);
                }

                should_remain_enabled
            }
            None => {
                // Filter out the infrastructure apps (ie: the core apps)
                if !is_hosted_happ(enabled_happ_id) {
                    trace!("Keeping infrastructure happ {}", enabled_happ_id);
                    true
                } else {
                    // The enabled happ is not a hosted instance of the happ nor a core app, so it shouldn't remain installed/enabled
                    false
                }
            }
        };

        if should_happ_remain_enabled {
            // If the happ should remain disabled, we leave the happ status unchanged and continue to next happ
            info!(
                "Skipping disabling/uninstalling of {} as it should remain enabled",
                enabled_happ_id
            );
            continue;
        } else {
            // If apps should no longer remain enabled, we need to take two steps:
            // Step 1: disable or uninstall app from Holochain Conductor (depending on instance type)
            if is_anonymous_instance(enabled_happ_id) {
                // Anonymous apps are only disabled, never uninstalled, as they are currently use a readonly instance of the host's instance of the app
                info!("Holochain-disabling {}", enabled_happ_id);
                admin_websocket.disable_app(enabled_happ_id).await?;
            } else {
                info!("Uninstalling {} from Holochain Conductor", enabled_happ_id);
                admin_websocket
                    .uninstall_app(enabled_happ_id, false)
                    .await?;
            }
        }
    }

    // Step 2: disable hosted happ in hha (holo hosting)
    for happ_id in happs_to_holo_disable {
        info!("Holo-disabling {}", happ_id);
        let holoport_id = get_holoport_id().await?;
        let happ_id_hash = ActionHashB64::from_b64_str(&happ_id)?;
        core_app_client
            .holo_disable_happ(&happ_id_hash, &holoport_id)
            .await?;
    }

    info!("Done disabling/uninstalling all ineligible happs");
    Ok(())
}
