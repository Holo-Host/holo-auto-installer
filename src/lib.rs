// TODO: https://github.com/tokio-rs/tracing/issues/843
#![allow(clippy::unit_arg)]

mod config;
pub use config::{Config, Happ, HappsFile, MembraneProofFile, ProofPayload};

mod entries;
pub use entries::{DnaResource, InstallHappBody, Preferences, PresentedHappBundle};

mod websocket;
use mr_bundle::Bundle;
pub use websocket::{AdminWebsocket, AppWebsocket};

use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::TempDir;
use tracing::{debug, info, instrument, warn};
use url::Url;

use hc_utils::WrappedHeaderHash;
use holochain::conductor::api::ZomeCall;
use holochain::conductor::api::{AppResponse, InstalledAppInfo};
use holochain_types::prelude::{zome_io::ExternIO, FunctionName, ZomeName};
use holochain_types::prelude::{AppManifest, MembraneProof, UnsafeBytes};

pub async fn activate_holo_hosted_happs(core_happ: &Happ, config: &Config) -> Result<()> {
    let list_of_happs = get_all_enabled_hosted_happs(core_happ).await?;
    install_holo_hosted_happs(&list_of_happs, config).await?;
    check_for_happs_to_be_uninstalled(&list_of_happs, config).await?;
    Ok(())
}

pub async fn install_holo_hosted_happs(
    happs: &[(WrappedHeaderHash, String, bool)],
    config: &Config,
) -> Result<()> {
    info!("Starting to install....");
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

    let active_happs = Arc::new(
        admin_websocket
            .list_enabled_happs()
            .await
            .context("failed to get installed hApps")?,
    );

    let client = reqwest::Client::new();
    // Note: Tmp preferences
    let preferences = Preferences {
        max_fuel_before_invoice: 9999999999.0,
        max_time_before_invoice: vec![86400, 0],
        price_compute: 1.0,
        price_storage: 1.0,
        price_bandwidth: 1.0,
    };
    // iterate through the vec and
    // Call http://localhost/holochain-api/install_hosted_happ
    // for each WrappedHeaderHash to install the hosted_happ
    for (happ_id, bundle_url, is_paused) in happs {
        if active_happs.contains(&format!("{:?}", happ_id)) {
            info!("App {:?} already installed", happ_id);
            if is_paused.to_owned() {
                info!("Pausing {:?}", happ_id);
                admin_websocket
                    .deactivate_app(&happ_id.0.to_string())
                    .await?;
            }
        } else {
            info!("Load mem-proofs for {:?}", happ_id);
            let mem_proof: HashMap<String, MembraneProof> =
                load_mem_proof_file(bundle_url).await.unwrap_or_default();
            info!(
                "Installing happ-id {:?} with mem_proof {:?}",
                happ_id, mem_proof
            );
            let body = InstallHappBody {
                happ_id: happ_id.0.to_string(),
                preferences: preferences.clone(),
                membrane_proofs: mem_proof.clone(),
            };
            let response = client
                .post("http://localhost/holochain-api/install_hosted_happ")
                .json(&body)
                .send()
                .await?;
            info!("Installed happ-id {:?}", happ_id);
            info!("Response {:?}", response);
        }
    }
    Ok(())
}

pub async fn check_for_happs_to_be_uninstalled(
    happs: &[(WrappedHeaderHash, String, bool)],
    config: &Config,
) -> Result<()> {
    info!("Starting to uninstall happs that were removed from the hosted list....");

    let mut admin_websocket = AdminWebsocket::connect(config.admin_port)
        .await
        .context("failed to connect to holochain's admin interface")?;

    let active_apps = admin_websocket
        .list_enabled_happs()
        .await
        .context("failed to get installed hApps")?;

    let ano_happ = filter_for_hosted_happ(active_apps.to_vec());

    let still_active_apps = ano_happ
        .into_iter()
        .filter(|h| !happs.iter().any(|(e, _, _)| &e.0.to_string() == h))
        .collect();

    let happ_to_uninstall = filter_for_hosted_happ_to_uninstall(still_active_apps, active_apps);

    for app in happ_to_uninstall {
        info!("Disabling {}", app);
        admin_websocket.uninstall_app(&app).await?;
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
                role.id.clone(),
                MembraneProof::from(UnsafeBytes::from([0].to_vec())),
            ) // The read only memproof is [0] (or in base64 `AA==`)
        })
        .collect())
}

#[instrument(err)]
pub async fn get_all_enabled_hosted_happs(
    core_happ: &Happ,
) -> Result<Vec<(WrappedHeaderHash, String, bool)>> {
    let mut app_websocket = AppWebsocket::connect(42233)
        .await
        .context("failed to connect to holochain's app interface")?;
    match app_websocket.get_app_info(core_happ.id()).await {
        Some(InstalledAppInfo {
            // This works on the assumption that the core happs has HHA in the first position of the vec
            cell_data,
            ..
        }) => {
            let zome_call_payload = ZomeCall {
                cell_id: cell_data[0].as_id().clone(),
                zome_name: ZomeName::from("hha"),
                fn_name: FunctionName::from("get_happs"),
                payload: ExternIO::encode(())?,
                cap_secret: None,
                provenance: cell_data[0].clone().into_id().into_dna_and_agent().1,
            };
            let response = app_websocket.zome_call(zome_call_payload).await?;
            match response {
                // This is the happs list that is returned from the hha DNA
                // https://github.com/Holo-Host/holo-hosting-app-rsm/blob/develop/zomes/hha/src/lib.rs#L54
                // return Vec of happ_list.happ_id
                AppResponse::ZomeCall(r) => {
                    info!("ZomeCall Response - Hosted happs List {:?}", r);
                    let happ_bundles: Vec<PresentedHappBundle> =
                        rmp_serde::from_slice(r.as_bytes())?;
                    let happ_bundle_ids = happ_bundles
                        .into_iter()
                        .map(|happ| (happ.id, happ.bundle_url, happ.is_paused))
                        .collect();
                    Ok(happ_bundle_ids)
                }
                _ => Err(anyhow!("unexpected response: {:?}", response)),
            }
        }
        None => Err(anyhow!("HHA is not installed")),
    }
}

#[instrument(err, fields(path = %path.as_ref().display()))]
pub fn load_happ_file(path: impl AsRef<Path>) -> Result<HappsFile> {
    use std::fs::File;

    let file = File::open(path).context("failed to open file")?;
    let happ_file =
        serde_yaml::from_reader(&file).context("failed to deserialize YAML as HappsFile")?;
    debug!(?happ_file);
    Ok(happ_file)
}

#[instrument(err, skip(url))]
pub(crate) async fn download_file(url: &Url) -> Result<PathBuf> {
    use isahc::config::RedirectPolicy;
    use isahc::prelude::*;

    let path = if url.scheme() == "file" {
        let p = PathBuf::from(url.path());
        debug!("Using: {:?}", p);
        p
    } else {
        debug!("downloading");
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
        debug!("download successful");
        path
    };
    Ok(path)
}

// Returns true if app should be kept active in holochain
fn _keep_app_active(installed_app_id: &str, happs_to_keep: Vec<String>) -> bool {
    happs_to_keep.contains(&installed_app_id.to_string()) || installed_app_id.contains("uhCkk")
}

fn filter_for_hosted_happ_to_uninstall(
    hosted_app_to_uninstall: Vec<String>,
    active_apps: Vec<String>,
) -> Vec<String> {
    active_apps
        .into_iter()
        .filter(|app| {
            hosted_app_to_uninstall.iter().any(|h| app.starts_with(h))
                && !app.ends_with("servicelogger")
        })
        .collect()
}

fn filter_for_hosted_happ(active_apps: Vec<String>) -> Vec<String> {
    active_apps
        .into_iter()
        .filter(|app| app.starts_with("uhCkk") && (app.len() == 53))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_keep_app_active() {
        let happs_to_keep = vec!["elemental-chat:2".to_string(), "hha:1".to_string()];
        let app_1 = "elemental-chat:1";
        let app_2 = "elemental-chat:2";
        let app_3 = "uhCkkcF0X1dpwHFeIPI6-7rzM6ma9IgyiqD-othxgENSkL1So1Slt::servicelogger";
        let app_4 = "other-app";

        assert_eq!(_keep_app_active(app_1, happs_to_keep.clone()), false);
        assert_eq!(_keep_app_active(app_2, happs_to_keep.clone()), true); // because it is in config
        assert_eq!(_keep_app_active(app_3, happs_to_keep.clone()), true); // because it is hosted
        assert_eq!(_keep_app_active(app_4, happs_to_keep.clone()), false);
    }
}
