pub use crate::config;
pub use crate::entries;
use crate::host_zome_calls::CoreAppClient;
pub use crate::host_zome_calls::{is_happ_free, HappBundle};
pub use crate::AdminWebsocket;
use anyhow::{anyhow, Context, Result};
use holochain_types::prelude::{AppManifest, MembraneProof, SerializedBytes, UnsafeBytes};
use holofuel_types::fuel::Fuel;
use isahc::config::RedirectPolicy;
use isahc::prelude::*;
use isahc::HttpClient;
use mr_bundle::Bundle;
use std::time::Duration;
use std::{collections::HashMap, fs, path::PathBuf, str::FromStr, sync::Arc};
use tempfile::TempDir;
use tracing::{info, instrument, trace, warn};
use url::Url;

/// installs a happs that are mented to be hosted
pub async fn install_holo_hosted_happs(
    core_app_client: &mut CoreAppClient,
    config: &config::Config,
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

    if let Err(error) = admin_websocket.attach_app_interface(config.happ_port).await {
        warn!(port = ?config.happ_port, ?error, "failed to start app interface, maybe it's already up?");
    }

    let running_happs = Arc::new(
        admin_websocket
            .list_running_app()
            .await
            .context("failed to get installed hApps")?,
    );

    trace!("running_happs {:?}", running_happs);

    let client = reqwest::Client::new();

    // Iterate through the vec and
    // Call http://localhost/holochain-api/install_hosted_happ
    // for each WrappedActionHash to install the hosted_happ
    for HappBundle {
        happ_id,
        bundle_url,
        is_paused,
        is_host_disabled,
        special_installed_app_id,
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
                // NB: This check only compares price settings with kyc level for now
                if is_kyc_level_2 || is_happ_free(&happ_id.to_string(), core_app_client).await? {
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
        // if kyc_level is not 2 and the happ is not free, we don't install
        else if !is_kyc_level_2 && !is_happ_free(&happ_id.to_string(), core_app_client).await? {
            trace!("Skipping paid happ due to kyc level {}", happ_id);
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

            // We'd like to move the logic from `install_hosted_happ` out of `hpos-holochain-api` and into this service where it belongs
            let body = entries::InstallHappBody {
                happ_id: happ_id.to_string(),
                preferences: preferences.clone(),
                membrane_proofs: mem_proof.clone(),
            };
            let response = client
                .post("http://localhost/holochain-api/install_hosted_happ")
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

    let bundle = Bundle::read_from_file(&path).await.unwrap();

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
