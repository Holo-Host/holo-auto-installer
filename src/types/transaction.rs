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
    #[serde(flatten)]
    pub invoiced_items: InvoicedItems,
}
