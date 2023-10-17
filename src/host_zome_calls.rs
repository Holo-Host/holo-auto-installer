pub use crate::config;
pub use crate::entries;
pub use crate::websocket::{AdminWebsocket, AppWebsocket};
use anyhow::{anyhow, Context, Result};
use holochain_conductor_api::{AppInfo, AppResponse, CellInfo, ProvisionedCell, ZomeCall};
use holochain_keystore::MetaLairClient;
use holochain_types::prelude::{
    ActionHashB64, ExternIO, FunctionName, Nonce256Bits, Timestamp, ZomeCallUnsigned, ZomeName,
};
use holofuel_types::fuel::Fuel;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde::Serialize;
use std::time::Duration;
use tracing::trace;

pub struct HappBundle {
    pub happ_id: ActionHashB64,
    pub bundle_url: String,
    pub is_paused: bool,
    pub special_installed_app_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct HappPreferences {
    pub max_fuel_before_invoice: f64,
    pub price_compute: Fuel,
    pub price_storage: Fuel,
    pub price_bandwidth: Fuel,
    pub max_time_before_invoice: Duration,
}

pub struct CoreAppClient {
    pub app_ws: AppWebsocket,
    pub cell: ProvisionedCell,
    pub keystore: MetaLairClient,
}

impl CoreAppClient {
    pub async fn connect(
        core_happ: &config::Happ,
        config: &config::Config,
    ) -> Result<CoreAppClient> {
        // connect to lair
        let passphrase = sodoken::BufRead::from(config::default_password()?.as_bytes().to_vec());
        let keystore = holochain_keystore::lair_keystore::spawn_lair_keystore(
            url2::url2!("{}", config.lair_url),
            passphrase,
        )
        .await?;

        let mut app_ws = AppWebsocket::connect(42233)
            .await
            .context("failed to connect to holochain's app interface")?;

        trace!("get app info for {}", core_happ.id());
        match app_ws.get_app_info(core_happ.id()).await {
            Some(AppInfo {
                // This works on the assumption that the core happs has HHA in the first position of the vec
                cell_info,
                ..
            }) => {
                trace!("got app info");

                let cell: holochain_conductor_api::ProvisionedCell =
                    match &cell_info.get("core-app").unwrap()[0] {
                        CellInfo::Provisioned(c) => c.clone(),
                        _ => return Err(anyhow!("core-app cell not found")),
                    };
                trace!("got cell {:?}", cell);
                Ok(CoreAppClient {
                    app_ws,
                    cell,
                    keystore,
                })
            }
            None => Err(anyhow!("HHA is not installed")),
        }
    }

    pub async fn zome_call<T, R>(
        &mut self,
        zome_name: ZomeName,
        fn_name: FunctionName,
        payload: T,
    ) -> Result<R>
    where
        T: Serialize + std::fmt::Debug,
        R: DeserializeOwned,
    {
        let (nonce, expires_at) = fresh_nonce()?;
        let zome_call_unsigned = ZomeCallUnsigned {
            cell_id: self.cell.cell_id.clone(),
            zome_name,
            fn_name,
            payload: ExternIO::encode(payload)?,
            cap_secret: None,
            provenance: self.cell.cell_id.agent_pubkey().clone(),
            nonce,
            expires_at,
        };
        let signed_zome_call =
            ZomeCall::try_from_unsigned_zome_call(&self.keystore, zome_call_unsigned).await?;

        let response = self.app_ws.zome_call(signed_zome_call).await?;

        match response {
            // This is the happs list that is returned from the hha DNA
            // https://github.com/Holo-Host/holo-hosting-app-rsm/blob/develop/zomes/hha/src/lib.rs#L54
            // return Vec of happ_list.happ_id
            AppResponse::ZomeCalled(r) => {
                let response: R = rmp_serde::from_slice(r.as_bytes())?;
                Ok(response)
            }
            _ => Err(anyhow!("unexpected response: {:?}", response)),
        }
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

pub async fn get_all_enabled_hosted_happs(
    core_app_client: &mut CoreAppClient,
) -> Result<Vec<HappBundle>> {
    trace!("get_all_enabled_hosted_happs");

    let happ_bundles: Vec<entries::PresentedHappBundle> = core_app_client
        .zome_call(ZomeName::from("hha"), FunctionName::from("get_happs"), ())
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
                special_installed_app_id: happ.special_installed_app_id,
            }
        })
        .collect();

    trace!("got happ bundles");
    Ok(happ_bundle_ids)
}

pub async fn is_happ_free(happ_id: &String, core_app_client: &mut CoreAppClient) -> Result<bool> {
    trace!("is_happ_free");

    let happ_preferences: HappPreferences = core_app_client
        .zome_call(
            ZomeName::from("hha"),
            FunctionName::from("get_happ_preferences"),
            happ_id,
        )
        .await?;

    let zero_fuel = Fuel::new(0);

    trace!("happ_preferences {:?}", happ_preferences);

    Ok(happ_preferences.price_compute == zero_fuel
        && happ_preferences.price_bandwidth == zero_fuel
        && happ_preferences.price_storage == zero_fuel)
}
