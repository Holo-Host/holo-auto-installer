pub use crate::types::happ::{HappPreferences, PresentedHappBundle, ServiceloggerHappPreferences};
use crate::types::transaction::PendingTransaction;
use anyhow::Result;
use holochain_types::dna::{ActionHashB64, AgentPubKey};
use holochain_types::prelude::{ExternIO, FunctionName, ZomeName};
use hpos_hc_connect::app_connection::CoreAppRoleName;
use hpos_hc_connect::hha_agent::HHAAgent;
use hpos_hc_connect::hha_types::HappAndHost;
use tracing::trace;

pub async fn get_host_preferences(core_app_client: &mut HHAAgent) -> Result<HappPreferences> {
    let host_happ_preferences: HappPreferences = core_app_client
        .app
        .zome_call_typed(
            CoreAppRoleName::HHA.into(),
            ZomeName::from("hha"),
            FunctionName::from("get_default_happ_preferences"),
            (),
        )
        .await?;

    trace!("got host happ settings preferences");
    Ok(host_happ_preferences)
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

pub async fn get_happs(core_app_client: &mut HHAAgent) -> Result<Vec<PresentedHappBundle>> {
    let happ_bundles: Vec<PresentedHappBundle> = core_app_client
        .app
        .zome_call_typed(
            CoreAppRoleName::HHA.into(),
            ZomeName::from("hha"),
            FunctionName::from("get_happs"),
            (),
        )
        .await?;
    trace!("got happs");
    Ok(happ_bundles)
}

pub async fn holo_enable_happ(
    core_app_client: &mut HHAAgent,
    happ_id: &ActionHashB64,
    holoport_id: &String,
) -> Result<()> {
    let ok_result: () = core_app_client
        .app
        .zome_call_typed(
            CoreAppRoleName::HHA.into(),
            ZomeName::from("hha"),
            FunctionName::from("enable_happ"),
            ExternIO::encode(HappAndHost {
                happ_id: happ_id.to_owned(),
                holoport_id: holoport_id.to_owned(),
            })?,
        )
        .await?;

    Ok(ok_result)
}

pub async fn holo_disable_happ(
    core_app_client: &mut HHAAgent,
    happ_id: &ActionHashB64,
    holoport_id: &String,
) -> Result<()> {
    let ok_result: () = core_app_client
        .app
        .zome_call_typed(
            CoreAppRoleName::HHA.into(),
            ZomeName::from("hha"),
            FunctionName::from("disable_happ"),
            ExternIO::encode(HappAndHost {
                happ_id: happ_id.to_owned(),
                holoport_id: holoport_id.to_owned(),
            })?,
        )
        .await?;

    Ok(ok_result)
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
