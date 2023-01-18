pub use crate::config;
pub use crate::entries;
pub use crate::websocket::{AdminWebsocket, AppWebsocket};
use anyhow::{anyhow, Context, Result};
use hc_utils::WrappedActionHash;
use holochain_conductor_api::{AppInfo, AppResponse};
use holochain_conductor_api::{CellInfo, ZomeCall};
use holochain_types::prelude::{zome_io::ExternIO, FunctionName, ZomeName};
use holochain_types::prelude::{Nonce256Bits, Timestamp, ZomeCallUnsigned};
use std::time::Duration;
use tracing::{info, instrument};

pub struct HappBundle {
    pub happ_id: WrappedActionHash,
    pub bundle_url: String,
    pub is_paused: bool,
    pub special_installed_app_id: Option<String>,
}

#[instrument(err)]
pub async fn get_all_enabled_hosted_happs(
    core_happ: &config::Happ,
    config: &config::Config,
) -> Result<Vec<HappBundle>> {
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

            // connect to lair
            let passphrase =
                sodoken::BufRead::from(config::default_password()?.as_bytes().to_vec());
            let keystore = holochain_keystore::lair_keystore::spawn_lair_keystore(
                url2::url2!("{}", config.lair_url),
                passphrase,
            )
            .await?;

            let (nonce, expires_at) = fresh_nonce()?;
            let zome_call_unsigned = ZomeCallUnsigned {
                cell_id: cell.cell_id.clone(),
                zome_name: ZomeName::from("hha"),
                fn_name: FunctionName::from("get_happs"),
                payload: ExternIO::encode(())?,
                cap_secret: None,
                provenance: cell.cell_id.agent_pubkey().clone(),
                nonce,
                expires_at,
            };
            let signed_zome_call =
                ZomeCall::try_from_unsigned_zome_call(&keystore, zome_call_unsigned).await?;

            let response = app_websocket.zome_call(signed_zome_call).await?;

            match response {
                // This is the happs list that is returned from the hha DNA
                // https://github.com/Holo-Host/holo-hosting-app-rsm/blob/develop/zomes/hha/src/lib.rs#L54
                // return Vec of happ_list.happ_id
                AppResponse::ZomeCalled(r) => {
                    println!("zome call response {:?}", r);
                    let happ_bundles: Vec<entries::PresentedHappBundle> =
                        rmp_serde::from_slice(r.as_bytes())?;
                    let happ_bundle_ids = happ_bundles
                        .into_iter()
                        .map(|happ| {
                            info!(
                                "{} with happ-id: {:?} and bundle: {}, is-paused={}",
                                happ.name, happ.id, happ.bundle_url, happ.is_paused
                            );
                            HappBundle {
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

/// generates nonce for zome calls
pub fn fresh_nonce() -> Result<(Nonce256Bits, Timestamp)> {
    let mut bytes = [0; 32];
    getrandom::getrandom(&mut bytes)?;
    let nonce = Nonce256Bits::from(bytes);
    // Rather arbitrary but we expire nonces after 5 mins.
    let expires: Timestamp = (Timestamp::now() + Duration::from_secs(60 * 5))?;
    Ok((nonce, expires))
}
