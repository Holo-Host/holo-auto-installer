use holochain_types::dna::ActionHashB64;
use holochain_types::dna::AgentPubKeyB64;
use holochain_types::dna::EntryHashB64;
use holochain_types::prelude::CapSecret;
use holochain_types::prelude::Timestamp;

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub enum AcceptedBy {
    ByMe,
    ByCounterParty,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub enum TransactionStatus {
    Actionable, // tx that is create by 1st instance and waiting for counterparty to complete the tx
    Pending,    // tx that was created by 1st instance and second instance
    Accepted(AcceptedBy), // tx that was accepted by counterparty but has yet to complete countersigning.
    Completed,
    Declined,
    Expired,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub enum TransactionDirection {
    Outgoing, // To(Address),
    Incoming, // From(Address),
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum POS {
    Hosting(CapSecret),
    Redemption(String), // Contains wallet address
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub enum TransactionType {
    Request, //Invoice
    Offer,   //Promise
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct Transaction {
    pub id: EntryHashB64,
    pub amount: String,
    pub fee: String,
    pub created_date: Timestamp,
    pub completed_date: Option<Timestamp>,
    pub transaction_type: TransactionType,
    pub counterparty: AgentPubKeyB64,
    pub direction: TransactionDirection,
    pub status: TransactionStatus,
    pub note: Option<String>,
    pub proof_of_service: Option<POS>,
    pub url: Option<String>,
    pub expiration_date: Option<Timestamp>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct PendingTransaction {
    pub invoice_pending: Vec<Transaction>,
    pub promise_pending: Vec<Transaction>,
    pub invoice_declined: Vec<Transaction>,
    pub promise_declined: Vec<Transaction>,
    pub accepted: Vec<Transaction>,
}

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
