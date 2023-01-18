// TODO: https://github.com/tokio-rs/tracing/issues/843
#![allow(clippy::unit_arg)]
use anyhow::{anyhow, Context, Result};
use holo_auto_installer::{self, config};
use tracing::error;
use tracing::instrument;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    let filter = EnvFilter::from_default_env().add_directive("again=trace".parse().unwrap());
    tracing_subscriber::fmt().with_env_filter(filter).init();
    spawn().await
}

#[instrument(err)]
async fn spawn() -> Result<()> {
    let config = config::Config::load();
    let happ_file = config::HappsFile::load_happ_file(&config.happs_file_path)
        .context("failed to load hApps YAML config")?;
    let core_happ_list = happ_file.core_app();
    match &core_happ_list {
        Some(core) => holo_auto_installer::run(core, &config).await,
        None => {
            error!("No Core apps found in configuration");
            Err(anyhow!("Please check that the happ config file is present. No Core apps found in configuration"))
        }
    }
}
