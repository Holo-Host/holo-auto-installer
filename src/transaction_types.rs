use holochain_types::dna::AgentPubKeyB64;
use holochain_types::dna::EntryHashB64;
use holochain_types::prelude::CapSecret;
use holochain_types::prelude::Timestamp;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub enum AcceptedBy {
  ByMe,
  ByCounterParty,
}

#[derive(Deserialize, Debug)]
pub enum TransactionStatus {
  Actionable, // tx that is create by 1st instance and waiting for counterparty to complete the tx
  Pending,    // tx that was created by 1st instance and second instance
  Accepted(AcceptedBy), // tx that was accepted by counterparty but has yet to complete countersigning.
  Completed,
  Declined,
  Expired,
}

#[derive(Deserialize, Debug)]
pub enum TransactionDirection {
  Outgoing, // To(Address),
  Incoming, // From(Address),
}

#[derive(Deserialize, Debug)]
pub enum POS {
  Hosting(CapSecret),
  Redemption(String), // Contains wallet address
}

#[derive(Deserialize, Debug)]
pub enum TransactionType {
    Request, //Invoice
    Offer,   //Promise
}

#[derive(Deserialize, Debug)]
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

#[derive(Deserialize, Debug)]
pub struct PendingTransaction {
  pub invoice_pending: Vec<Transaction>,
  pub promise_pending: Vec<Transaction>,
  pub invoice_declined: Vec<Transaction>,
  pub promise_declined: Vec<Transaction>,
  pub accepted: Vec<Transaction>,
}