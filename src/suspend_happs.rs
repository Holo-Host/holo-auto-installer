pub use crate::config;
pub use crate::host_zome_calls::HappBundle;
use crate::host_zome_calls::{disable_happ, CoreAppClient, HappAndHost, InvoiceNote, PendingTransaction, POS};
pub use crate::websocket::AdminWebsocket;
use anyhow::Result;
use chrono::Utc;
use serde_yaml;

pub async fn suspend_unpaid_happs(
    core_app_client: &mut CoreAppClient,
    pending_transactions: PendingTransaction,
) -> Result<()> {
    for invoice in &pending_transactions.invoice_pending {
        if let Some(proof_of_service) = &invoice.proof_of_service {
            if let POS::Hosting(_) = proof_of_service {
                if let Some(expiration_date) = invoice.expiration_date {
                    if expiration_date.as_millis() < Utc::now().timestamp_millis() {
                        if let Some(note) = invoice.note.clone() {
                            let invoice_note: Result<InvoiceNote, _> = serde_yaml::from_str(&note);
                            match invoice_note {
                                Ok(note) => {
                                    let hha_id = note.hha_id;
                                    disable_happ(
                                        core_app_client,
                                        HappAndHost {
                                            happ_id: hha_id.clone(),
                                            holoport_id: hha_id,
                                            is_automated: Some(true)
                                        },
                                    ).await?;
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
    }

    Ok(())
}
