pub use crate::config;
pub use crate::host_zome_calls::HappBundle;
use crate::host_zome_calls::{PendingTransaction, POS};
pub use crate::websocket::AdminWebsocket;
use anyhow::Result;
use chrono::Utc;

pub async fn suspend_unpaid_happs(
    published_happs: &[HappBundle],
    pending_transactions: PendingTransaction,
) -> Result<()> {
    for invoice in &pending_transactions.invoice_pending {
        if let Some(proof_of_service) = &invoice.proof_of_service {
            if let POS::Hosting(_) = proof_of_service {
                if let Some(expiration_date) = invoice.expiration_date {
                    if expiration_date.as_millis() < Utc::now().timestamp_millis() {
                        
                    }
                }
            }
        }
    }

    Ok(())
}
