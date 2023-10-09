pub use crate::config;
pub use crate::entries;
pub use crate::get_apps;
pub use crate::AdminWebsocket;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::process::{Command, Output};

#[derive(Debug, Deserialize)]
struct HostingCriteria {
    id: String,
    jurisdiction: String,
    kyc: KycLevel,
}
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub enum KycLevel {
    #[serde(rename = "holo_kyc_1")]
    Level1,
    #[serde(rename = "holo_kyc_2")]
    Level2,
    #[serde(rename = "error")]
    Error,
}

pub async fn get_kyc_level() -> Result<KycLevel> {
    let output: Output = Command::new("/run/current-system/sw/bin/hpos-holochain-client")
        .args(&["--url=http://localhost/holochain-api/", "hosting-criteria"])
        .output()?;

    let output_str = String::from_utf8_lossy(&output.stdout).to_string();

    let hosting_criteria: HostingCriteria = serde_json::from_str(&output_str)?;

    Ok(hosting_criteria.kyc)
}
