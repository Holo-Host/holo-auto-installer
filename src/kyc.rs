use anyhow::{Context, Result};
use holochain_types::prelude::{holochain_serial, SerializedBytes};
use hpos_config_core::Config;
use serde::{Deserialize, Serialize};
use std::{env, fs};

#[derive(Debug, Serialize, Deserialize, SerializedBytes, Clone)]
#[serde(rename_all = "camelCase")]
struct KycPayload {
    email: String,
    timestamp: i64,
    pubKey: String,
}

/// Getting KYC level for the host
pub async fn check_kyc_level() -> bool {
    match get_kyc().await {
        Ok(level) => {
            println!("Level found: {}", level);
            if level.contains("level_2") {
                true
            } else {
                false
            }
        }
        Err(_) => false,
    }
}

pub async fn get_kyc() -> Result<String> {
    let (payload, signature) = get_agent_details().await?;
    let client = reqwest::Client::builder().build()?;
    println!("Signing payload {:?}", payload);
    println!("Signing signature {:?}", signature);

    let mut headers = reqwest::header::HeaderMap::new();

    headers.insert("X-Signature", base64::encode(signature).parse()?);
    headers.insert("Content-Type", "application/json".parse()?);

    let json: serde_json::Value = serde_json::to_value(&payload)?;
    let request = client
        .request(
            reqwest::Method::POST,
            "https://hbs.dev.holotest.net/auth/api/v1/holo-client",
        )
        .headers(headers)
        .json(&json);

    let response = request.send().await?;
    let body = response.text().await?;

    println!("Body: {}", body);
    let result: serde_json::Value = serde_json::from_str(&body)?;
    println!("Result: {}", result);
    Ok(result["kyc"].to_string())
}

async fn get_agent_details() -> Result<(KycPayload, holochain_types::prelude::Signature)> {
    let mut agent = hpos_hc_connect::CoreAppAgent::connect().await?;

    let config = get_hpos_config()?;

    match config {
        Config::V2 { settings, .. } => {
            let time = chrono::Utc::now();
            let (_, agent_pub) = agent
                .get_cell(hpos_hc_connect::CoreAppRoleName::HHA)
                .await?;
            let payload = KycPayload {
                email: settings.admin.email,
                pubKey: agent_pub.to_string(),
                timestamp: time.timestamp_millis(),
            };

            let signature = agent
                .sign_raw(
                    SerializedBytes::try_from(payload.clone())
                        .expect("Failed to serialize ReserveProof")
                        .bytes()
                        .to_owned()
                        .into(),
                )
                .await
                .expect("Failed to sign reserve proof payload");
            Ok((payload, signature))
        }
        Config::V1 { .. } => panic!("Unsupported Config version"),
    }
}
/// Reads hpos-config into a struct
pub fn get_hpos_config() -> Result<Config> {
    let config_path = env::var("HPOS_CONFIG_PATH")
        .context("Failed to read HPOS_CONFIG_PATH. Is it set in env?")?;
    read_hpos_config(&config_path)
}

pub fn read_hpos_config(path: &String) -> Result<Config> {
    let config_json = fs::read(path)?;
    let config: Config = serde_json::from_slice(&config_json)?;
    Ok(config)
}
