use anyhow::Context;
use anyhow::Result;
use base64::prelude::*;
use holochain_types::prelude::{holochain_serial, SerializedBytes, Signature, Timestamp};
use hpos_hc_connect::hha_agent::HHAAgent;
use hpos_hc_connect::hpos_agent::get_hpos_config;
use reqwest::Response;
use serde::{Deserialize, Serialize};
use tracing::warn;

#[derive(Debug, Deserialize, Clone, Default)]
pub struct HostCredentials {
    #[allow(dead_code)]
    pub id: Option<String>,
    pub jurisdiction: Option<String>,
    #[serde(default)]
    pub kyc: KycLevel,
    // #[serde(rename = "camel_case")]
    // pub public_key: Option<String>,
    // pub email: String,
}
impl HostCredentials {
    pub fn is_host_in_valid_jurisdiction(
        &self,
        should_exclude_happ_jurisdictions: bool,
        happ_jurisdictions: &[String],
    ) -> bool {
        let host_jurisdiction = match self.jurisdiction.to_owned() {
            Some(j) => j,
            None => {
                warn!("Host's jurisdiction not available");
                return false;
            }
        };
        if should_exclude_happ_jurisdictions {
            // If the host jurisdiction is present in the list that the hApp Manager has used,
            // then the host jurisdiction is invalid
            !happ_jurisdictions.contains(&host_jurisdiction)
        } else {
            // Otherwise, the host jurisdiction is valid if it exists in the happ's list of jurisdictionss
            happ_jurisdictions.contains(&host_jurisdiction)
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Default)]
pub enum KycLevel {
    #[serde(rename = "holo_kyc_1")]
    #[default]
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
    pub async fn get_host_hosting_criteria(&self) -> Option<HostCredentials> {
        match self.get_access_token().await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("Unable to get kyc & jurisdiction: {:?}", e);
                tracing::warn!("returning default kyc level 1");
                tracing::warn!("returning default jurisdiction of None");
                Some(HostCredentials::default())
            }
        }
    }

    async fn get_access_token(&self) -> Result<Option<HostCredentials>> {
        let response = self.inner_get_access_token().await?;
        tracing::debug!("response received");
        let mut body = response.text().await?;

        // 504 Gateway Timeout
        // here we either need to retry once more or end the script
        if body.contains("error code: 504") {
            tracing::warn!("Gateway Timeout. Retrying once more...");
            let response = self.inner_get_access_token().await?;
            body = response.text().await?;
            if body.contains("error code: 504") {
                tracing::warn!("Gateway Timeout. Exiting...");
                return Ok(None);
            }
        }

        tracing::debug!("Result: {}", body);
        let result: serde_json::Value = serde_json::from_str(&body)?;
        let h: HostCredentials = serde_json::from_value(result)?;
        tracing::debug!("HostCredentials: {:?}", h);
        Ok(Some(h))
    }

    async fn inner_get_access_token(&self) -> Result<Response> {
        let config: hpos_config_core::Config = get_hpos_config()?;

        let email = match config {
            hpos_config_core::Config::V1 { settings, .. }
            | hpos_config_core::Config::V2 { settings, .. } => settings.admin.email,
        };

        let mut core_app = HHAAgent::spawn(None).await?;
        let pub_key = core_app.pubkey().await?;

        tracing::debug!("email: {:?}, pub_key: {:?}", email, pub_key);

        #[derive(Serialize, Deserialize, Debug, PartialEq, Clone, SerializedBytes)]
        #[allow(non_snake_case)]
        struct Body {
            email: String,
            timestamp: i64,
            pubKey: String,
        }

        let payload = Body {
            email,
            timestamp: Timestamp::now().as_millis(),
            pubKey: pub_key.to_string(),
        };
        let signature: Signature = core_app
            .sign_raw(
                SerializedBytes::try_from(payload.clone())
                    .expect("Failed to serialize body")
                    .bytes()
                    .to_owned()
                    .into(),
            )
            .await?;

        tracing::trace!("Signature: {:?}", signature);

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

        Ok(request.send().await?)
    }
}

fn hbs_url() -> Result<String> {
    std::env::var("HBS_URL").context("Failed to read HBS_URL. Is it set in env?")
}
