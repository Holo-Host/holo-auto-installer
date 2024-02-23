use crate::host_zome_calls::{
    disable_happ, CoreAppClient, HappAndHost, InvoiceNote, PendingTransaction, POS,
};
use anyhow::Result;
use chrono::Utc;
use holochain_types::dna::HoloHashB64;
use std::env;
use std::process::Command;
use tracing::debug;

pub async fn suspend_unpaid_happs(
    core_app_client: &mut CoreAppClient,
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
    let holoport_id_holo_hash = HoloHashB64::from_b64_str(&holoport_id)?;

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
                                disable_happ(
                                    core_app_client,
                                    HappAndHost {
                                        happ_id: hha_id.clone(),
                                        holoport_id: holoport_id_holo_hash.clone(),
                                        is_automated: Some(true),
                                    },
                                )
                                .await?;
                            }
                            Err(e) => {
                                tracing::error!("Error parsing invoice note: {:?}", e);
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
