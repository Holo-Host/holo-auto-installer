use std::{collections::HashMap, env, fs, fs::File, io::prelude::*, sync::Arc};

use anyhow::{anyhow, Context, Result};
use holochain::conductor::api::{
    AdminRequest, AdminResponse, AppRequest, AppResponse, InstalledAppInfo, ZomeCall,
};
use holochain_types::prelude::MembraneProof;
use holochain_types::{
    app::{AppBundleSource, InstallAppBundlePayload, InstalledAppId},
    dna::AgentPubKey,
};
use holochain_websocket::{connect, WebsocketConfig, WebsocketSender};
use tracing::{info, instrument, trace};
use url::Url;

use crate::config::Happ;

#[derive(Clone)]
pub struct AdminWebsocket {
    tx: WebsocketSender,
    agent_key: Option<AgentPubKey>,
}

impl AdminWebsocket {
    #[instrument(err)]
    pub async fn connect(admin_port: u16) -> Result<Self> {
        let url = format!("ws://localhost:{}/", admin_port);
        let url = Url::parse(&url).context("invalid ws:// URL")?;
        let websocket_config = Arc::new(WebsocketConfig::default());
        let (tx, _rx) = again::retry(|| {
            let websocket_config = Arc::clone(&websocket_config);
            connect(url.clone().into(), websocket_config)
        })
        .await?;
        Ok(Self {
            tx,
            agent_key: None,
        })
    }

    #[instrument(skip(self), err)]
    pub async fn get_agent_key(&mut self) -> Result<AgentPubKey> {
        // Try agent key from memory
        if let Some(key) = self.agent_key.clone() {
            info!("returning agent key from memory");
            return Ok(key);
        }
        // Try agent key from disc
        if let Ok(pubkey_path) = env::var("PUBKEY_PATH") {
            if let Ok(key_vec) = fs::read(&pubkey_path) {
                if let Ok(key) = AgentPubKey::from_raw_39(key_vec) {
                    info!("returning agent key from file");
                    self.agent_key = Some(key.clone());
                    return Ok(key);
                }
            }
        }

        // Create agent key in Lair and save it in file
        let response = self.send(AdminRequest::GenerateAgentPubKey).await?;
        match response {
            AdminResponse::AgentPubKeyGenerated(key) => {
                let key_vec = key.get_raw_39();
                if let Ok(pubkey_path) = env::var("PUBKEY_PATH") {
                    let mut file = File::create(pubkey_path)?;
                    file.write_all(key_vec)?;
                }
                info!("returning newly created agent key");
                self.agent_key = Some(key.clone());
                Ok(key)
            }
            _ => Err(anyhow!("unexpected response: {:?}", response)),
        }
    }

    #[instrument(skip(self))]
    pub async fn attach_app_interface(&mut self, happ_port: u16) -> Result<AdminResponse> {
        info!(port = ?happ_port, "starting app interface");
        let msg = AdminRequest::AttachAppInterface {
            port: Some(happ_port),
        };
        self.send(msg).await
    }

    #[instrument(skip(self), err)]
    pub async fn list_active_happs(&mut self) -> Result<Vec<InstalledAppId>> {
        let response = self.send(AdminRequest::ListEnabledApps).await?;
        match response {
            AdminResponse::EnabledAppsListed(app_ids) => Ok(app_ids),
            _ => Err(anyhow!("unexpected response: {:?}", response)),
        }
    }

    #[instrument(skip(self, happ, membrane_proofs))]
    pub async fn install_and_activate_happ(
        &mut self,
        happ: &Happ,
        membrane_proofs: HashMap<String, MembraneProof>,
    ) -> Result<()> {
        self.install_happ(happ, membrane_proofs).await?;
        self.activate_app(happ).await?;
        info!("installed & activated hApp: {}", happ.id());
        Ok(())
    }

    #[instrument(skip(self, happ))]
    pub async fn activate_happ(&mut self, happ: &Happ) -> Result<()> {
        self.activate_app(happ).await?;
        info!("activated hApp: {}", happ.id());
        Ok(())
    }

    #[instrument(err, skip(self, happ, membrane_proofs))]
    async fn install_happ(
        &mut self,
        happ: &Happ,
        membrane_proofs: HashMap<String, MembraneProof>,
    ) -> Result<AdminResponse> {
        let agent_key = self
            .get_agent_key()
            .await
            .context("failed to generate agent key")?;
        let path = match happ.bundle_path.clone() {
            Some(path) => path,
            None => crate::download_file(happ.bundle_url.as_ref().context("dna_url is None")?)
                .await
                .context("failed to download DNA archive")?,
        };
        let payload = if let Ok(id) = env::var("DEV_UID_OVERRIDE") {
            info!("using uid to install: {}", id);
            InstallAppBundlePayload {
                agent_key,
                installed_app_id: Some(happ.id()),
                source: AppBundleSource::Path(path),
                membrane_proofs,
                uid: Some(id),
            }
        } else {
            info!("using default uid to install");
            InstallAppBundlePayload {
                agent_key,
                installed_app_id: Some(happ.id()),
                source: AppBundleSource::Path(path),
                membrane_proofs,
                uid: None,
            }
        };

        let msg = AdminRequest::InstallAppBundle(Box::new(payload));
        let response = self.send(msg).await?;
        Ok(response)
    }

    #[instrument(skip(self), err)]
    async fn activate_app(&mut self, happ: &Happ) -> Result<AdminResponse> {
        let msg = AdminRequest::EnableApp {
            installed_app_id: happ.id(),
        };
        self.send(msg).await
    }

    #[instrument(skip(self), err)]
    pub async fn deactivate_app(&mut self, installed_app_id: &str) -> Result<AdminResponse> {
        let msg = AdminRequest::DisableApp {
            installed_app_id: installed_app_id.to_string(),
        };
        self.send(msg).await
    }

    #[instrument(skip(self))]
    async fn send(&mut self, msg: AdminRequest) -> Result<AdminResponse> {
        let response = self
            .tx
            .request(msg)
            .await
            .context("failed to send message")?;
        match response {
            AdminResponse::Error(error) => Err(anyhow!("error: {:?}", error)),
            _ => {
                trace!("send successful");
                Ok(response)
            }
        }
    }
}

#[derive(Clone)]
pub struct AppWebsocket {
    tx: WebsocketSender,
}

impl AppWebsocket {
    #[instrument(err)]
    pub async fn connect(app_port: u16) -> Result<Self> {
        let url = format!("ws://localhost:{}/", app_port);
        let url = Url::parse(&url).context("invalid ws:// URL")?;
        let websocket_config = Arc::new(WebsocketConfig::default());
        let (tx, _rx) = again::retry(|| {
            let websocket_config = Arc::clone(&websocket_config);
            connect(url.clone().into(), websocket_config)
        })
        .await?;
        Ok(Self { tx })
    }

    #[instrument(skip(self))]
    pub async fn get_app_info(&mut self, app_id: InstalledAppId) -> Option<InstalledAppInfo> {
        let msg = AppRequest::AppInfo {
            installed_app_id: app_id,
        };
        let response = self.send(msg).await.ok()?;
        match response {
            AppResponse::AppInfo(app_info) => app_info,
            _ => None,
        }
    }

    #[instrument(skip(self))]
    pub async fn zome_call(&mut self, msg: ZomeCall) -> Result<AppResponse> {
        let app_request = AppRequest::ZomeCall(Box::new(msg));
        let response = self.send(app_request).await;
        response
    }

    #[instrument(skip(self))]
    async fn send(&mut self, msg: AppRequest) -> Result<AppResponse> {
        let response = self
            .tx
            .request(msg)
            .await
            .context("failed to send message")?;
        match response {
            AppResponse::Error(error) => Err(anyhow!("error: {:?}", error)),
            _ => {
                trace!("send successful");
                Ok(response)
            }
        }
    }
}
