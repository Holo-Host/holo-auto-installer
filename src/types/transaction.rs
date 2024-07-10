use holochain_types::dna::ActionHashB64;
use holochain_types::prelude::Timestamp;

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct InvoicedItems {
    pub quantity: String, // we're using serde_yaml to convert the struct into a string
    pub prices: String,   // we're using serde_yaml to convert the struct into a string
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct InvoiceNote {
    pub hha_id: ActionHashB64,
    pub invoice_period_start: Timestamp,
    pub invoice_period_end: Timestamp,
    // This can be commented back in when the chc can support larger entries [#78](https://github.com/Holo-Host/servicelogger-rsm/pull/78)
    // activity_logs_range: Vec<ActionHashB64>,
    // disk_usage_logs_range: Vec<ActionHashB64>,
    #[serde(flatten)]
    pub invoiced_items: InvoicedItems,
}
