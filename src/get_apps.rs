pub use crate::config;
pub use crate::entries;
pub use crate::websocket::{AdminWebsocket, AppWebsocket};
use anyhow::Result;
use holochain_types::prelude::ActionHashB64;
use holochain_types::prelude::{ExternIO, FunctionName, ZomeName};
use hpos_hc_connect::CoreAppAgent;
use tracing::trace;

pub struct HappBundle {
    pub happ_id: ActionHashB64,
    pub bundle_url: String,
    pub is_paused: bool,
    pub special_installed_app_id: Option<String>,
}

///
///
pub async fn get_all_enabled_hosted_happs() -> Result<Vec<HappBundle>> {
    let mut agent = CoreAppAgent::connect().await?;

    trace!("get_all_enabled_hosted_happs");
    let result = agent
        .zome_call(
            hpos_hc_connect::CoreAppRoleName::HHA,
            ZomeName::from("hha"),
            FunctionName::from("get_happs"),
            ExternIO::encode(())?,
        )
        .await?;
    trace!("results: {:?}", result);
    let happ_bundles: Vec<entries::PresentedHappBundle> = rmp_serde::from_slice(result.as_bytes())?;

    trace!("happ_bundles: {:?}", happ_bundles);
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
            }
        })
        .collect();
    trace!("got happ bundles");

    Ok(happ_bundle_ids)
}
