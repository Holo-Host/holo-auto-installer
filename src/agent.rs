use anyhow::{Context, Result};
use ed25519_dalek::Keypair;
use hpos_config_core::Config;
use std::{env, fs};
use tracing::instrument;

pub struct HostAgent {
    pub key: Keypair,
}

impl HostAgent {
    #[instrument(err)]
    pub async fn get() -> Result<Self> {
        let config = get_hpos_config()?;
        Ok(HostAgent {
            key: hpos_config_seed_bundle_explorer::holoport_key(&config, Some(default_password()?))
                .await
                .unwrap(),
        })
    }
}

/// Reads hpos-config into a struct
pub fn get_hpos_config() -> Result<Config> {
    let config_path = env::var("HPOS_CONFIG_PATH")
        .context("Failed to read HPOS_CONFIG_PATH. Is it set in env?")?;
    let config_json = fs::read(config_path)?;
    let config: Config = serde_json::from_slice(&config_json)?;
    Ok(config)
}

pub fn default_password() -> Result<String> {
    env::var("DEVICE_SEED_DEFAULT_PASSWORD")
        .context("Failed to read DEVICE_SEED_DEFAULT_PASSWORD. Is it set in env?")
}
