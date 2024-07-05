pub use crate::entries;
use crate::transaction_types::{
    HostingPreferences, InvoiceNote, PendingTransaction, ServiceloggerHappPreferences, POS,
};
use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use holochain_types::dna::{ActionHashB64, AgentPubKey};
use holochain_types::prelude::{
    AppManifest, ExternIO, FunctionName, MembraneProof, SerializedBytes, UnsafeBytes, ZomeName,
};
use holofuel_types::fuel::Fuel;
use hpos_hc_connect::app_connection::CoreAppRoleName;
use hpos_hc_connect::hha_agent::HHAAgent;
use hpos_hc_connect::hha_types::HappAndHost;
use hpos_hc_connect::holo_config::Config;
use hpos_hc_connect::AdminWebsocket;
use isahc::config::RedirectPolicy;
use isahc::{prelude::*, HttpClient};
use itertools::Itertools;
use mr_bundle::Bundle;
use std::collections::HashSet;
use std::{
    collections::HashMap, env, fs, path::PathBuf, process::Command, str::FromStr, sync::Arc,
    time::Duration,
};
use tempfile::TempDir;
use tracing::{debug, error, info, instrument, trace, warn};
use url::Url;

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

/// installs a happs that are mented to be hosted
pub async fn install_holo_hosted_happs(
    config: &Config,
    happs: &[HappBundle],
    is_kyc_level_2: bool,
) -> Result<()> {
    info!("Starting to install....");

    // Hardcoded servicelogger preferences for all the hosted happs installed
    let preferences = entries::HappPreferences {
        max_fuel_before_invoice: Fuel::from_str("1000")?, // MAX_TX_AMT in holofuel is currently hard-coded to 50,000
        max_time_before_invoice: Duration::default(),
        price_compute: Fuel::from_str("0.025")?,
        price_storage: Fuel::from_str("0.025")?,
        price_bandwidth: Fuel::from_str("0.025")?,
    }
    .save()?;

    if happs.is_empty() {
        info!("No happs registered to be enabled for hosting.");
        return Ok(());
    }

    let mut admin_websocket = AdminWebsocket::connect(config.admin_port)
        .await
        .context("failed to connect to holochain's admin interface")?;

    let running_happs = Arc::new(
        admin_websocket
            .list_running_app()
            .await
            .context("failed to get installed hApps")?,
    );

    trace!("running_happs {:?}", running_happs);

    let client = reqwest::Client::new();

    // Iterate through the vec and
    // Call http://localhost/api/v2/apps/hosted/install
    // for each WrappedActionHash to install the hosted_happ
    for HappBundle {
        happ_id,
        bundle_url,
        is_paused,
        is_host_disabled,
        special_installed_app_id,
        exclude_jurisdictions: _,
        jurisdictions: _,
        categories: _,
    } in happs
    {
        // Check if special happ is installed and do nothing if it is installed
        trace!("Trying to install {}", happ_id);
        if special_installed_app_id.is_some()
            && running_happs.contains(&format!("{}::servicelogger", happ_id))
        {
            // We do not need to install bc we never pause this app as we do not want our core-app to be uninstalled ever
            trace!(
                "Special App {:?} already installed",
                special_installed_app_id
            );
        }
        // Check if happ is already installed and disable it if the publisher has paused happ in hha
        // NB: This condition/check will miss hosted holofuel as that happ is never installed under its happ_id
        // This means it will always try and fail to install holofuel again
        // Right now, we don't care
        else if running_happs.contains(&format!("{}", happ_id)) {
            trace!("App {} already installed", happ_id);
            if *is_paused {
                trace!("Pausing {}", happ_id);
                admin_websocket.disable_app(&happ_id.to_string()).await?;
            } else {
                // Check if installed happ is eligible to be enabled for host and enable, if so
                if is_kyc_level_2 {
                    trace!("Enabling {}", happ_id);
                    admin_websocket.enable_app(&happ_id.to_string()).await?;
                } else {
                    trace!(
                        "Not enabling installed {} app due to failed price check for kyc level",
                        happ_id
                    );
                }
            }
        }
        // if the expected happ is disabled by the host, we don't install
        else if is_host_disabled.to_owned() {
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
        }
        // else install the hosted happ read-only instance
        else {
            trace!("Load mem-proofs for {}", happ_id);
            let mem_proof: HashMap<String, MembraneProof> =
                load_mem_proof_file(bundle_url).await.unwrap_or_default();
            trace!(
                "Installing happ-id {} with mem_proof {:?}",
                happ_id,
                mem_proof
            );

            // We'd like to move the logic from `install_hosted_happ` out of `hpos-api` and into this service where it belongs
            let body = entries::InstallHappBody {
                happ_id: happ_id.to_string(),
                preferences: preferences.clone(),
                membrane_proofs: mem_proof.clone(),
            };
            let response = client
                .post("http://localhost/api/v2/apps/hosted/install")
                .json(&body)
                .send()
                .await?;
            info!("Installed happ-id {}", happ_id);
            trace!("Install happ Response {:?}", response);

            // If app was already installed but disabled, the above install will fail, and we just enable it here
            let result = admin_websocket.enable_app(&happ_id.to_string()).await;

            trace!("Enable app result {:?}", result);
        }
    }
    Ok(())
}

/// Temporary read-only mem-proofs solution
/// should be replaced by calling the joining-code service and getting the appropriate proof for the agent
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

#[instrument(err, skip(url))]
pub(crate) async fn download_file(url: &Url) -> Result<PathBuf> {
    let path = if url.scheme() == "file" {
        let p = PathBuf::from(url.path());
        trace!("Using: {:?}", p);
        p
    } else {
        trace!("downloading");
        let mut url = Url::clone(url);
        url.set_scheme("https")
            .map_err(|_| anyhow!("failed to set scheme to https"))?;
        let client = HttpClient::builder()
            .redirect_policy(RedirectPolicy::Follow)
            .build()
            .context("failed to initiate download request")?;
        let mut response = client
            .get(url.as_str())
            .context("failed to send GET request")?;
        if !response.status().is_success() {
            return Err(anyhow!(
                "response status code {} indicated failure",
                response.status().as_str()
            ));
        }
        let dir = TempDir::new().context("failed to create tempdir")?;
        let url_path = PathBuf::from(url.path());
        let basename = url_path
            .file_name()
            .context("failed to get basename from url")?;
        let path = dir.into_path().join(basename);
        let mut file = fs::File::create(&path).context("failed to create target file")?;
        response
            .copy_to(&mut file)
            .context("failed to write response to file")?;
        trace!("download successful");
        path
    };
    Ok(path)
}

pub async fn get_all_published_hosted_happs(
    core_app_client: &mut HHAAgent,
) -> Result<Vec<HappBundle>> {
    trace!("get_all_published_hosted_happs");

    let happ_bundles: Vec<entries::PresentedHappBundle> = core_app_client
        .app
        .zome_call_typed(
            CoreAppRoleName::HHA.into(),
            ZomeName::from("hha"),
            FunctionName::from("get_happs"),
            (),
        )
        .await?;

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
                is_host_disabled: happ.host_settings.is_host_disabled,
                special_installed_app_id: happ.special_installed_app_id,
                jurisdictions: happ.jurisdictions,
                exclude_jurisdictions: happ.exclude_jurisdictions,
                categories: happ.categories,
            }
        })
        .collect();

    trace!("got happ bundles");
    Ok(happ_bundle_ids)
}

pub async fn get_pending_transactions(
    core_app_client: &mut HHAAgent,
) -> Result<PendingTransaction> {
    let pending_transactions: PendingTransaction = core_app_client
        .app
        .zome_call_typed(
            CoreAppRoleName::Holofuel.into(),
            ZomeName::from("transactor"),
            FunctionName::from("get_pending_transactions"),
            (),
        )
        .await?;

    trace!("got pending transactions");
    Ok(pending_transactions)
}

/// Ineligible Happs = old holo-hosted happs, holo-disabled happs, or happs with invalid pricing for kyc level
/// Handles ineligible happs for 2 cases - identified and anonymous hosted agents:
///  - Identified: Uninstalls & removes identified instances of ineligible happs
///  - Anonymous: Disables anonymous instance of ineligible happs
pub async fn uninstall_ineligible_happs(
    config: &Config,
    published_happs: &[HappBundle],
    is_kyc_level_2: bool,
    suspended_happs: Vec<String>,
    jurisdiction: Option<String>,
    hosting_preferences: HostingPreferences,
    publisher_jurisdictions: HashMap<String, Option<String>>,
) -> Result<()> {
    info!("Checking to uninstall happs that were removed from the hosted list....");

    let mut admin_websocket = AdminWebsocket::connect(config.admin_port)
        .await
        .context("Failed to connect to holochain's admin interface")?;

    let running_happ_ids = admin_websocket
        .list_running_app()
        .await
        .context("Failed to get installed and running hApps")?;

    let unique_running_happ_ids: Vec<&String> = running_happ_ids.iter().unique().collect();

    trace!("Unique_running_happ_ids {:?}", unique_running_happ_ids);

    for happ_id in unique_running_happ_ids {
        if should_be_installed(
            happ_id,
            published_happs,
            is_kyc_level_2,
            suspended_happs.clone(),
            jurisdiction.clone(),
            hosting_preferences.clone(),
            publisher_jurisdictions.clone(),
        )
        .await
        {
            info!(
                "Skipping uninstall of {} as it should remain installed",
                happ_id
            );
            continue;
        }

        if is_anonymous_instance(happ_id) {
            info!("Disabling {}", happ_id);
            admin_websocket.disable_app(happ_id).await?;
        } else {
            info!("Uninstalling {}", happ_id);
            admin_websocket.uninstall_app(happ_id).await?;
        }
    }
    info!("Done uninstalling happs that were removed from the hosted list.");

    Ok(())
}

// There are core infrastructure happs that should never be uninstalled. All uninstallable happs start with "uhCkk" and don't contain ::servicelogger
fn is_hosted_happ(app: &str) -> bool {
    app.starts_with("uhCkk") && !app.contains("::servicelogger")
}

fn is_anonymous_instance(happ_id: &str) -> bool {
    happ_id.starts_with("uhCkk") && happ_id.len() == 53
}

/// Returns true if `installed_app_id` represents an anonymous or identified instance of `happ_id`
fn is_instance_of_happ(happ_id: &str, installed_app_id: &str) -> bool {
    // An `installed_app_id` is one of
    // - A core hApp (e.g. `servicelogger:0_2_1::251e7cc8-9c48-4841-9eb0-435f0bf97373`)
    // - An anonymous instance with installed_app_id == happ_id
    // - An identified instance matching /happ_id::agent_id/
    // - A happ-specific servicelogger instance matching /happ_id::servicelogger/
    installed_app_id.starts_with(happ_id) && !installed_app_id.ends_with("servicelogger")
}

pub async fn should_be_installed(
    running_happ_id: &String,
    published_happs: &[HappBundle],
    is_kyc_level_2: bool,
    suspended_happs: Vec<String>,
    jurisdiction: Option<String>,
    hosting_preferences: HostingPreferences,
    publisher_jurisdictions: HashMap<String, Option<String>>,
) -> bool {
    trace!("`should_be_installed check` for {}", running_happ_id);
    // This should be the first check since the core-app should never be uninstalled currently
    if !is_hosted_happ(running_happ_id) {
        trace!("Keeping infrastructure happ {}", running_happ_id);
        return true;
    }

    // checks if published happ is still running
    let published_happ = published_happs
        .iter()
        .find(|&happ| happ.happ_id.to_string() == *running_happ_id);

    if suspended_happs.contains(running_happ_id) {
        trace!("Disabling suspended happ {}", running_happ_id);
        return false;
    }

    if let Some(jurisdiction_preferences) = hosting_preferences.jurisdiction_prefs {
        let publisher_jurisdiction = publisher_jurisdictions.get(running_happ_id);
        match publisher_jurisdiction {
            Some(jurisdiction) => match jurisdiction {
                Some(jurisdiction) => {
                    let mut is_jurisdiction_in_list = false;
                    if jurisdiction_preferences
                        .value
                        .iter()
                        .any(|host_jurisdiction| *host_jurisdiction == *jurisdiction)
                    {
                        is_jurisdiction_in_list = true;
                    }
                    if jurisdiction_preferences.is_exclusion && is_jurisdiction_in_list {
                        return false;
                    }
                    if !jurisdiction_preferences.is_exclusion && !is_jurisdiction_in_list {
                        return false;
                    }
                }
                _ => {
                    warn!("could not get publisher jurisdiction");
                    warn!("happ {} won't be installed", running_happ_id);
                    return false;
                }
            },
            _ => {
                warn!("could not get publisher jurisdiction");
                warn!("happ {} won't be installed", running_happ_id);
                return false;
            }
        }
    }

    // verify the hApp is allowed to run on this jurisdiction.
    // jurisdiction is taken from mongodb and compared against hApps jurisdictions
    match jurisdiction {
        Some(jurisdiction) => {
            if let Some(happ) = published_happ {
                let mut is_jurisdiction_in_list = false;
                if let Some(_happ_jurisdiction) = happ
                    .jurisdictions
                    .iter()
                    .find(|&happ_jurisdiction| *happ_jurisdiction == jurisdiction)
                {
                    is_jurisdiction_in_list = true;
                }
                if happ.exclude_jurisdictions && is_jurisdiction_in_list {
                    return false;
                }
                if !happ.exclude_jurisdictions && !is_jurisdiction_in_list {
                    return false;
                }
            }
        }
        None => {
            warn!("jurisdiction not available for holoport");
            warn!("happ {} won't be installed", running_happ_id);
            return false;
        }
    }

    if let Some(categories_preferences) = hosting_preferences.categories_prefs {
        // verify the happ matches the hosting categories preferences
        if let Some(happ) = published_happ {
            let categories_list: HashSet<String> =
                categories_preferences.value.iter().cloned().collect();

            let contains_category = happ
                .categories
                .iter()
                .any(|category| categories_list.contains(category));

            if contains_category && categories_preferences.is_exclusion {
                return false;
            }
            if !contains_category && !categories_preferences.is_exclusion {
                return false;
            }
        }
    }

    // The running happ is an instance of an expected happ
    let expected_happ = published_happs.iter().find(|published_happ| {
        is_instance_of_happ(&published_happ.happ_id.to_string(), running_happ_id)
    });

    trace!(
        "Found expected_happ {:?}",
        &expected_happ.map(|eh| &eh.happ_id)
    );

    if let Some(expected_happ) = expected_happ {
        // if the expected happ is disabled by the host, happ shouldn't be installed
        if expected_happ.is_host_disabled {
            trace!(
                "Disabling happ {} because host was disabled it in hha",
                expected_happ.happ_id
            );
            return false;
        }

        // happ hosting is only valid (despite price prefs) if the host is >= kyc level 2
        is_kyc_level_2
    } else {
        // The running happ is not an instance of any expected happ, so shouldn't be installed
        false
    }
}

pub async fn suspend_unpaid_happs(
    core_app_client: &mut HHAAgent,
    pending_transactions: PendingTransaction,
) -> Result<Vec<String>> {
    let mut suspended_happs: Vec<String> = Vec::new();

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

    for invoice in &pending_transactions.invoice_pending {
        if let Some(POS::Hosting(_)) = &invoice.proof_of_service {
            if let Some(expiration_date) = invoice.expiration_date {
                if expiration_date.as_millis() < Utc::now().timestamp_millis() {
                    if let Some(note) = invoice.note.clone() {
                        let invoice_note: Result<InvoiceNote, _> = serde_yaml::from_str(&note);
                        match invoice_note {
                            Ok(note) => {
                                let hha_id = note.hha_id;
                                suspended_happs.push(hha_id.clone().to_string());
                                core_app_client
                                    .app
                                    .zome_call_typed(
                                        CoreAppRoleName::HHA.into(),
                                        ZomeName::from("hha"),
                                        FunctionName::from("disable_happ"),
                                        ExternIO::encode(HappAndHost {
                                            happ_id: hha_id.clone(),
                                            holoport_id: holoport_id.to_string(),
                                        })?,
                                    )
                                    .await?;
                            }
                            Err(e) => {
                                error!("Error parsing invoice note: {:?}", e);
                            }
                        }
                    }
                }
            }
        }
    }

    debug!("suspend happs completed: {:?}", suspended_happs);
    Ok(suspended_happs)
}

pub async fn get_hosting_preferences(core_app_client: &mut HHAAgent) -> Result<HostingPreferences> {
    let hosting_preferences: HostingPreferences = core_app_client
        .app
        .zome_call_typed(
            CoreAppRoleName::HHA.into(),
            ZomeName::from("hha"),
            FunctionName::from("get_default_happ_preferences"),
            (),
        )
        .await?;

    trace!("got hosting preferences");
    Ok(hosting_preferences)
}

pub async fn get_happ_preferences(
    core_app_client: &mut HHAAgent,
    happ_id: ActionHashB64,
) -> Result<ServiceloggerHappPreferences> {
    let happ_preference: ServiceloggerHappPreferences = core_app_client
        .app
        .zome_call_typed(
            CoreAppRoleName::HHA.into(),
            ZomeName::from("hha"),
            FunctionName::from("get_happ_preferences"),
            happ_id,
        )
        .await?;

    trace!("got happ preferences");
    Ok(happ_preference)
}

pub async fn get_publisher_jurisdiction(
    core_app_client: &mut HHAAgent,
    pubkey: AgentPubKey,
) -> Result<Option<String>> {
    let publisher_jurisdiction: Option<String> = core_app_client
        .app
        .zome_call_typed(
            CoreAppRoleName::HHA.into(),
            ZomeName::from("hha"),
            FunctionName::from("get_publisher_jurisdiction"),
            pubkey,
        )
        .await?;

    trace!("got publisher jurisdiction");
    Ok(publisher_jurisdiction)
}
