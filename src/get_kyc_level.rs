use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::process::{Command, Output};
use tracing::debug;

#[derive(Debug, Deserialize)]
struct HostingCriteria {
    #[allow(dead_code)]
    id: String,
    #[allow(dead_code)]
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
        .args(["--url=http://localhost/holochain-api/", "hosting-criteria"])
        .output()?;

    let output_str = String::from_utf8_lossy(&output.stdout).to_string();

    match serde_json::from_str::<HostingCriteria>(&output_str) {
        Ok(hosting_criteria) => Ok(hosting_criteria.kyc),
        Err(e) => {
            debug!("Failed to deserialize hosting criteria {:?}", e);
            Ok(KycLevel::Error)
        }
    }
}
