use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;
use std::{env, path::PathBuf};
use structopt::StructOpt;
use tracing::debug;
use tracing::instrument;
use url::Url;

pub fn default_password() -> Result<String> {
    env::var("HOLOCHAIN_DEFAULT_PASSWORD")
        .context("Failed to read HOLOCHAIN_DEFAULT_PASSWORD. Is it set in env?")
}

#[derive(Debug, StructOpt)]
pub struct Config {
    /// Holochain conductor port
    #[structopt(long, env, default_value = "4444")]
    pub admin_port: u16,
    /// hApp listening port
    #[structopt(long, env, default_value = "42233")]
    pub happ_port: u16,
    /// URL at which lair-keystore is running
    #[structopt(long)]
    pub lair_url: String,
    /// Path to a YAML file containing the lists of hApps to install
    pub happs_file_path: PathBuf,
}

impl Config {
    /// Create Config from CLI arguments with logging
    pub fn load() -> Self {
        let config = Config::from_args();
        debug!(?config, "loaded");
        config
    }
}

/// MembraneProof payload contaiing cell_nick
#[derive(Debug, Deserialize)]
pub struct ProofPayload {
    pub cell_nick: String,
    /// Base64-encoded MembraneProof.
    pub proof: String,
}
/// payload vec of all the mem_proof for one happ
/// current implementation is implemented to contain mem_proof for elemental_chat
#[derive(Debug, Deserialize)]
pub struct MembraneProofFile {
    pub payload: Vec<ProofPayload>,
}

/// Configuration of a single hApp from config.yaml
/// ui_path and dna_path takes precedence over ui_url and dna_url respectively
/// and is meant for running tests
#[derive(Debug, Deserialize, Clone)]
pub struct Happ {
    pub bundle_url: Option<Url>,
    pub bundle_path: Option<PathBuf>,
    pub ui_url: Option<Url>,
    pub ui_path: Option<PathBuf>,
}

impl Happ {
    /// generates the installed app id that should be used
    /// based on the path or url of the bundle.
    /// Assumes file name ends in .happ, and converts periods -> colons
    pub fn id(&self) -> String {
        let name = if let Some(ref bundle) = self.bundle_path {
            bundle
                .file_name()
                .unwrap()
                .to_os_string()
                .to_string_lossy()
                .to_string()
        } else if let Some(ref bundle) = self.bundle_url {
            bundle.path_segments().unwrap().last().unwrap().to_string()
        } else {
            //TODO fix
            "unreabable".to_string()
        };
        if let Ok(uid) = env::var("DEV_UID_OVERRIDE") {
            format!("{}::{}", name.replace(".happ", "").replace('.', ":"), uid)
        } else {
            name.replace(".happ", "").replace('.', ":")
        }
    }
}

/// config with list of core happ for the holoport
#[derive(Debug, Deserialize)]
pub struct HappsFile {
    pub self_hosted_happs: Vec<Happ>,
    pub core_happs: Vec<Happ>,
}

impl HappsFile {
    pub fn core_app(self) -> Option<Happ> {
        let core_app = &self
            .core_happs
            .into_iter()
            .find(|x| x.id().contains("core-app"));
        core_app.clone()
    }

    #[instrument(err, fields(path = %path.as_ref().display()))]
    pub fn load_happ_file(path: impl AsRef<Path>) -> Result<Self> {
        use std::fs::File;

        let file = File::open(path).context("failed to open file")?;
        let happ_file =
            serde_yaml::from_reader(&file).context("failed to deserialize YAML as HappsFile")?;
        debug!(?happ_file);
        Ok(happ_file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_install_app_id_format() {
        let cfg = Happ {
            bundle_path: Some("my/path/to/elemental_chat.1.0001.happ".into()),
            bundle_url: None,
            ui_url: None,
            ui_path: None,
        };
        assert_eq!(cfg.id(), String::from("elemental_chat:1:0001"));
        let cfg = Happ {
            bundle_path: None,
            bundle_url: Some(Url::parse("https://github.com/holochain/elemental-chat/releases/download/v0.1.0-alpha1/elemental_chat.1.0001.happ").unwrap()),
            ui_url: None,
            ui_path: None,
        };
        assert_eq!(cfg.id(), String::from("elemental_chat:1:0001"));
    }
}
