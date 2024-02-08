use anyhow::Context;
use anyhow::Result;
use base64::prelude::*;
use holochain_types::prelude::{holochain_serial, SerializedBytes, Signature};
use hpos_hc_connect::{hpos_agent::get_hpos_config, CoreAppAgent};
use serde::{Deserialize, Serialize};

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
}
pub struct HbsClient {
    pub client: reqwest::Client,
}
impl HbsClient {
    pub fn connect() -> Result<Self> {
        let client = reqwest::Client::builder().build()?;
        Ok(Self { client })
    }
    // return kyc level and assumes default as level 1
    pub async fn get_kyc_level(&self) -> KycLevel {
        match self.get_access_token().await {
            Ok(v) => v.kyc,
            Err(e) => {
                tracing::warn!("Unable to get kyc: {:?}", e);
                tracing::warn!("returning default kyc level 1");
                KycLevel::Level1
            }
        }
    }
    async fn get_access_token(&self) -> Result<HostingCriteria> {
        let config: hpos_config_core::Config = get_hpos_config()?;

        let email = match config {
            hpos_config_core::Config::V1 { settings, .. }
            | hpos_config_core::Config::V2 { settings, .. } => settings.admin.email,
        };

        let mut core_app_agent = CoreAppAgent::connect().await?;
        let (_, pub_key) = core_app_agent
            .get_cell(hpos_hc_connect::CoreAppRoleName::HHA)
            .await?;
        tracing::debug!("email: {:?}, pub_key: {:?}", email, pub_key);
        #[derive(Serialize, Deserialize, Debug, PartialEq, Clone, SerializedBytes)]
        #[allow(non_snake_case)]
        struct Body {
            email: String,
            timestamp: String,
            pubKey: String,
        }

        let payload = Body {
            email,
            timestamp: chrono::offset::Utc::now().to_string(),
            pubKey: pub_key.to_string(),
        };
        let signature: Signature = core_app_agent
            .sign_raw(
                SerializedBytes::try_from(payload.clone())
                    .expect("Failed to serialize body")
                    .bytes()
                    .to_owned()
                    .into(),
            )
            .await?;
        tracing::debug!("Signature: {:?}", signature);

        let connection = Self::connect()?;
        let mut headers = reqwest::header::HeaderMap::new();

        headers.insert("Content-Type", "application/json".parse()?);
        headers.insert("X-Signature", BASE64_STANDARD.encode(signature).parse()?);
        let json: serde_json::Value = serde_json::to_value(payload)?;
        let request = connection
            .client
            .request(
                reqwest::Method::POST,
                format!("{}/auth/api/v1/holo-client", hbs_url()?),
            )
            .headers(headers)
            .json(&json);

        let response = request.send().await?;
        tracing::debug!("response received");
        let body = response.text().await?;
        tracing::debug!("Result: {}", body);
        let result: serde_json::Value = serde_json::from_str(&body)?;
        let h: HostingCriteria = serde_json::from_value(result)?;
        tracing::debug!("HostingCriteria: {:?}", h);
        Ok(h)
    }
}

pub fn hbs_url() -> Result<String> {
    std::env::var("HBS_URL").context("Failed to read HBS_URL. Is it set in env?")
}
