pub use crate::config;
pub use crate::entries;
pub use crate::transaction_types::*;
pub use crate::websocket::{AdminWebsocket, AppWebsocket};
use anyhow::{anyhow, Context, Result};
use holochain_conductor_api::{AppInfo, AppResponse, CellInfo, ProvisionedCell, ZomeCall};
use holochain_keystore::MetaLairClient;
use holochain_types::prelude::{
    ActionHashB64, ExternIO, FunctionName, Nonce256Bits, Timestamp, ZomeCallUnsigned, ZomeName,
};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde::Serialize;
use std::time::Duration;
use tracing::trace;

pub struct HappBundle {
    pub happ_id: ActionHashB64,
    pub bundle_url: String,
    pub is_paused: bool,
    pub is_host_disabled: bool,
    pub special_installed_app_id: Option<String>,
    pub jurisdictions: Vec<String>,
    pub exclude_jurisdictions: bool,
}

#[derive(Clone)]
pub struct CoreAppClient {
    pub app_ws: AppWebsocket,
    pub core_happ_cell: ProvisionedCell,
    pub holofuel_cell: ProvisionedCell,
    pub keystore: MetaLairClient,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HappAndHost {
    pub happ_id: ActionHashB64,
    pub holoport_id: String,
    pub is_automated: Option<bool>,
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

                let core_happ_cell: holochain_conductor_api::ProvisionedCell =
                    match &cell_info.get("core-app").unwrap()[0] {
                        CellInfo::Provisioned(c) => c.clone(),
                        _ => return Err(anyhow!("core-app cell not found")),
                    };
                trace!("got core happ cell {:?}", core_happ_cell);
                let holofuel_cell: holochain_conductor_api::ProvisionedCell =
                    match &cell_info.get("holofuel").unwrap()[0] {
                        CellInfo::Provisioned(c) => c.clone(),
                        _ => return Err(anyhow!("holofuel cell not found")),
                    };
                trace!("got holofuel cell {:?}", holofuel_cell);
                Ok(CoreAppClient {
                    app_ws,
                    core_happ_cell,
                    holofuel_cell,
                    keystore,
                })
            }
            None => Err(anyhow!("HHA is not installed")),
        }
    }

    pub async fn zome_call<T, R>(
        &mut self,
        cell: ProvisionedCell,
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
            cell_id: cell.cell_id.clone(),
            zome_name,
            fn_name,
            payload: ExternIO::encode(payload)?,
            cap_secret: None,
            provenance: cell.cell_id.agent_pubkey().clone(),
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

pub async fn get_all_published_hosted_happs(
    core_app_client: &mut CoreAppClient,
) -> Result<Vec<HappBundle>> {
    trace!("get_all_published_hosted_happs");

    let core_happ_cell = core_app_client.clone().core_happ_cell;
    let happ_bundles: Vec<entries::PresentedHappBundle> = core_app_client
        .zome_call(
            core_happ_cell,
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
            }
        })
        .collect();

    trace!("got happ bundles");
    Ok(happ_bundle_ids)
}

pub async fn get_pending_transactions(
    core_app_client: &mut CoreAppClient,
) -> Result<PendingTransaction> {
    let holofuel_cell = core_app_client.clone().holofuel_cell;
    let pending_transactions: PendingTransaction = core_app_client
        .zome_call(
            holofuel_cell,
            ZomeName::from("transactor"),
            FunctionName::from("get_pending_transactions"),
            (),
        )
        .await?;

    trace!("got pending transactions");
    Ok(pending_transactions)
}

pub async fn disable_happ(core_app_client: &mut CoreAppClient, payload: HappAndHost) -> Result<()> {
    let core_happ_cell = core_app_client.clone().core_happ_cell;
    core_app_client
        .zome_call(
            core_happ_cell,
            ZomeName::from("hha"),
            FunctionName::from("disable_happ"),
            payload,
        )
        .await?;

    trace!("disabled happ");
    Ok(())
}

pub async fn get_hosting_preferences(core_app_client: &mut CoreAppClient) -> Result<HostingPreferences> {
    let core_happ_cell = core_app_client.clone().core_happ_cell;
    let hosting_preferences: HostingPreferences = core_app_client
        .zome_call(
            core_happ_cell,
            ZomeName::from("hha"),
            FunctionName::from("get_default_happ_preferences"),
            (),
        )
        .await?;

    trace("got hosting preferences");
    Ok(hosting_preferences)
}
