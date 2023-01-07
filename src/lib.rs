// TODO: https://github.com/tokio-rs/tracing/issues/843
#![allow(clippy::unit_arg)]
use arbitrary::Arbitrary;
mod config;
pub use config::{Config, Happ, HappsFile, MembraneProofFile, ProofPayload};
mod entries;
pub use entries::{DnaResource, InstallHappBody, Preferences, PresentedHappBundle};
mod websocket;
use anyhow::{anyhow, Context, Result};
use hc_utils::WrappedActionHash;
use holochain_conductor_api::{AppInfo, AppResponse};
use holochain_conductor_api::{CellInfo, ZomeCall};
use holochain_types::prelude::{zome_io::ExternIO, FunctionName, ZomeName};
use holochain_types::prelude::{
    AgentPubKey, AppManifest, CapSecret, MembraneProof, Nonce256Bits, SerializedBytes, Signature,
    Timestamp, UnsafeBytes, ZomeCallUnsigned,
};
use holofuel_types::fuel::Fuel;
use mr_bundle::Bundle;
use std::{
    collections::HashMap,
    env, fs,
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
    time::Duration,
};
use tempfile::TempDir;
use tracing::{debug, info, instrument, warn};
use url::Url;
pub use websocket::{AdminWebsocket, AppWebsocket};
mod agent;
use ed25519_dalek::Signer;

pub async fn activate_holo_hosted_happs(core_happ: &Happ, config: &Config) -> Result<()> {
    println!("activate_holo_hosted_happs");
    let list_of_happs = get_all_enabled_hosted_happs(core_happ).await?;
    install_holo_hosted_happs(&list_of_happs, config).await?;
    uninstall_removed_happs(&list_of_happs, config).await?;
    Ok(())
}

pub struct HappPkg {
    happ_id: WrappedActionHash,
    bundle_url: String,
    is_paused: bool,
    special_installed_app_id: Option<String>,
}

pub async fn install_holo_hosted_happs(happs: &[HappPkg], config: &Config) -> Result<()> {
    info!("Starting to install....");

    // Hardcoded servicelogger preferences for all the hosted happs installed
    let preferences = Preferences {
        max_fuel_before_invoice: Fuel::from_str("1000")?, // MAX_TX_AMT in holofuel is currently hard-coded to 50,000
        max_time_before_invoice: vec![86400, 0],
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

    let active_happs = Arc::new(
        admin_websocket
            .list_running_app()
            .await
            .context("failed to get installed hApps")?,
    );

    let client = reqwest::Client::new();

    // iterate through the vec and
    // Call http://localhost/holochain-api/install_hosted_happ
    // for each WrappedActionHash to install the hosted_happ
    for HappPkg {
        happ_id,
        bundle_url,
        is_paused,
        special_installed_app_id,
    } in happs
    {
        // if special happ is installed and do nothing if it is installed
        if special_installed_app_id.is_some()
            && active_happs.contains(&format!("{:?}::servicelogger", happ_id))
        {
            info!(
                "Special App {:?} already installed",
                special_installed_app_id
            );
            // We do not pause here because we do not want our core-app to be uninstalled ever
        }
        // Check if happ is already installed and deactivate it if happ is paused in hha
        else if active_happs.contains(&format!("{:?}", happ_id)) {
            info!("App {:?} already installed", happ_id);
            if *is_paused {
                info!("Pausing {:?}", happ_id);
                admin_websocket
                    .deactivate_app(&happ_id.0.to_string())
                    .await?;
            }
        }
        // else installed the hosted happ read-only instance
        else {
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

pub async fn uninstall_removed_happs(happs: &[HappPkg], config: &Config) -> Result<()> {
    info!("Checking to uninstall happs that were removed from the hosted list....");

    let mut admin_websocket = AdminWebsocket::connect(config.admin_port)
        .await
        .context("failed to connect to holochain's admin interface")?;

    let active_apps = admin_websocket
        .list_running_app()
        .await
        .context("failed to get installed hApps")?;

    let ano_happ = filter_for_hosted_happ(active_apps.to_vec());

    let happ_ids_to_uninstall = ano_happ
        .into_iter()
        .filter(|h| {
            !happs
                .iter()
                .any(|HappPkg { happ_id, .. }| &happ_id.0.to_string() == h)
        })
        .collect();

    let happ_to_uninstall = filter_for_hosted_happ_to_uninstall(happ_ids_to_uninstall, active_apps);

    for app in happ_to_uninstall {
        info!("Disabling {}", app);
        admin_websocket.uninstall_app(&app).await?;
    }
    info!("Done uninstall happs that were removed from the hosted list.");

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

#[instrument(err)]
pub async fn get_all_enabled_hosted_happs(core_happ: &Happ) -> Result<Vec<HappPkg>> {
    println!("get_all_enabled_hosted_happs");
    let mut app_websocket = AppWebsocket::connect(42233)
        .await
        .context("failed to connect to holochain's app interface")?;
    println!("get app info for {:?}", core_happ.id());
    match app_websocket.get_app_info(core_happ.id()).await {
        Some(AppInfo {
            // This works on the assumption that the core happs has HHA in the first position of the vec
            cell_info,
            ..
        }) => {
            println!("got app info");

            let cell = match &cell_info.get("core-app").unwrap()[0] {
                CellInfo::Provisioned(c) => c.clone(),
                _ => return Err(anyhow!("core-app cell not found")),
            };
            println!("got cell {:?}", cell);
            let signing_keypair = agent::HostAgent::get().await?;
            let cap_secret =
                if &env::var("FORCE_RANDOM_AGENT_KEY").unwrap_or_else(|_| "0".to_string()) == "1" {
                    let mut buf = arbitrary::Unstructured::new(&[0, 1, 6, 14, 26, 0]);
                    Some(CapSecret::arbitrary(&mut buf).unwrap())
                } else {
                    None
                };
            let (nonce, expires_at) = fresh_nonce()?;
            let zome_call_unsigned = ZomeCallUnsigned {
                cell_id: cell.cell_id.clone(),
                zome_name: ZomeName::from("hha"),
                fn_name: FunctionName::from("get_happs"),
                payload: ExternIO::encode(())?,
                cap_secret,
                provenance: AgentPubKey::from_raw_32(
                    signing_keypair.key.public.to_bytes().to_vec(),
                ),
                nonce,
                expires_at,
            };
            let signature = signing_keypair
                .key
                .sign(&zome_call_unsigned.data_to_sign().unwrap());

            let response = app_websocket
                .zome_call(ZomeCall {
                    cell_id: zome_call_unsigned.cell_id,
                    zome_name: zome_call_unsigned.zome_name,
                    fn_name: zome_call_unsigned.fn_name,
                    payload: zome_call_unsigned.payload,
                    cap_secret: zome_call_unsigned.cap_secret,
                    provenance: zome_call_unsigned.provenance,
                    nonce: zome_call_unsigned.nonce,
                    expires_at: zome_call_unsigned.expires_at,
                    signature: Signature::from(signature.to_bytes()),
                })
                .await?;
            match response {
                // This is the happs list that is returned from the hha DNA
                // https://github.com/Holo-Host/holo-hosting-app-rsm/blob/develop/zomes/hha/src/lib.rs#L54
                // return Vec of happ_list.happ_id
                AppResponse::ZomeCalled(r) => {
                    println!("zome call response {:?}", r);
                    let happ_bundles: Vec<PresentedHappBundle> =
                        rmp_serde::from_slice(r.as_bytes())?;
                    let happ_bundle_ids = happ_bundles
                        .into_iter()
                        .map(|happ| {
                            info!(
                                "{} with happ-id: {:?} and bundle: {}, is-paused={}",
                                happ.name, happ.id, happ.bundle_url, happ.is_paused
                            );
                            HappPkg {
                                happ_id: happ.id,
                                bundle_url: happ.bundle_url,
                                is_paused: happ.is_paused,
                                special_installed_app_id: happ.special_installed_app_id,
                            }
                        })
                        .collect();
                    println!("got happ bundles");
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

/// Takes a list of hApp IDs and returns a list of `installed_app_id`s corresponding with the anonymous and identified instances of those hApps.
fn filter_for_hosted_happ_to_uninstall(
    happ_ids: Vec<String>,
    active_installed_app_ids: Vec<String>,
) -> Vec<String> {
    active_installed_app_ids
        .into_iter()
        .filter(|installed_app_id| {
            happ_ids
                .iter()
                .any(|happ_id| is_instance_of_happ(happ_id, installed_app_id))
        })
        .collect()
}
/// Returns true if `installed_app_id` represents an anonymous or identified instance of `happ_id`
fn is_instance_of_happ(happ_id: &str, installed_app_id: &str) -> bool {
    // An `installed_app_id` is one of
    // - A core hApp (e.g. `servicelogger:0_2_1::251e7cc8-9c48-4841-9eb0-435f0bf97373`)
    // - An anonymous instance with installed_app_id == happ_id
    // - An identified instance matching /happ_id::agent_id/
    // - A happ-specific servicelogger instance matching /happ_id::servicelogger/
    !installed_app_id.ends_with("servicelogger") && installed_app_id.starts_with(happ_id)
}

fn filter_for_hosted_happ(active_apps: Vec<String>) -> Vec<String> {
    active_apps
        .into_iter()
        .filter(|app| is_anonymous(app))
        .collect()
}

fn is_anonymous(app: &str) -> bool {
    app.starts_with("uhCkk") && app.len() == 53
}

pub fn fresh_nonce() -> Result<(Nonce256Bits, Timestamp)> {
    let mut bytes = [0; 32];
    getrandom::getrandom(&mut bytes)?;
    let nonce = Nonce256Bits::from(bytes);
    // Rather arbitrary but we expire nonces after 5 mins.
    let expires: Timestamp = (Timestamp::now() + Duration::from_secs(60 * 5))?;
    Ok((nonce, expires))
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
