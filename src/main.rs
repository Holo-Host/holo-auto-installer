// TODO: https://github.com/tokio-rs/tracing/issues/843
#![allow(clippy::unit_arg)]

use anyhow::{Context, Result};

use holo_auto_pilot::{activate_holo_hosted_happs, load_happ_file, Config};
use tracing::info;
use tracing::instrument;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    let filter = EnvFilter::from_default_env().add_directive("again=trace".parse().unwrap());
    tracing_subscriber::fmt().with_env_filter(filter).init();
    run().await
}

#[instrument(err)]
async fn run() -> Result<()> {
    let config = Config::load();
    let happ_file =
        load_happ_file(&config.happs_file_path).context("failed to load hApps YAML config")?;
    let core_happ_list = happ_file.core_app();
    match &core_happ_list {
        Some(core) => activate_holo_hosted_happs(core, &config).await,
        None => {
            info!("No Core apps found in configuration");
            Ok(())
        }
    }
}
