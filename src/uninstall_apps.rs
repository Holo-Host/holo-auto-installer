pub use crate::config;
pub use crate::get_apps;
pub use crate::websocket::AdminWebsocket;
use anyhow::{Context, Result};
use tracing::info;
use tracing::warn;

/// uninstalled old hosted happs
/// Currently this completely removes the happ
/// This will be updated to checked enabled uninstalled and disable the happ accordingly
pub async fn uninstall_removed_happs(
    happs: &[get_apps::HappBundle],
    config: &config::Config,
) -> Result<()> {
    info!("Checking to uninstall happs that were removed from the hosted list....");

    let mut admin_websocket = AdminWebsocket::connect(config.admin_port)
        .await
        .context("failed to connect to holochain's admin interface")?;

    let active_apps = admin_websocket
        .list_running_app()
        .await
        .context("failed to get installed hApps")?;

    let ano_happ = filter_for_hosted_happ(active_apps.to_vec());

    let happ_ids_to_uninstall = ano_happ
        .into_iter()
        .filter(|h| {
            !happs
                .iter()
                .any(|get_apps::HappBundle { happ_id, .. }| &happ_id.to_string() == h)
        })
        .collect();

    let happ_to_uninstall = filter_for_hosted_happ_to_uninstall(happ_ids_to_uninstall, active_apps);

    for app in happ_to_uninstall {
        info!("Disabling {}", app);
        admin_websocket.uninstall_app(&app).await?;
        let sl_instance = format!("{}::servicelogger", app);
        if let Err(e) = admin_websocket.uninstall_app(&sl_instance).await {
            warn!(
                "Unable to disable sl instance: {} with error: {}",
                sl_instance, e
            )
        }
    }
    info!("Done uninstall happs that were removed from the hosted list.");

    Ok(())
}

/// Takes a list of hApp IDs and returns a list of `installed_app_id`s corresponding with the anonymous and identified instances of those hApps.
fn filter_for_hosted_happ_to_uninstall(
    happ_ids: Vec<String>,
    active_installed_app_ids: Vec<String>,
) -> Vec<String> {
    active_installed_app_ids
        .into_iter()
        .filter(|installed_app_id| {
            happ_ids
                .iter()
                .any(|happ_id| is_instance_of_happ(happ_id, installed_app_id))
        })
        .collect()
}

/// Returns true if `installed_app_id` represents an anonymous or identified instance of `happ_id`
fn is_instance_of_happ(happ_id: &str, installed_app_id: &str) -> bool {
    // An `installed_app_id` is one of
    // - A core hApp (e.g. `servicelogger:0_2_1::251e7cc8-9c48-4841-9eb0-435f0bf97373`)
    // - An anonymous instance with installed_app_id == happ_id
    // - An identified instance matching /happ_id::agent_id/
    // - A happ-specific servicelogger instance matching /happ_id::servicelogger/
    !installed_app_id.ends_with("servicelogger") && installed_app_id.starts_with(happ_id)
}

fn filter_for_hosted_happ(active_apps: Vec<String>) -> Vec<String> {
    active_apps
        .into_iter()
        .filter(|app| is_anonymous(app))
        .collect()
}

fn is_anonymous(app: &str) -> bool {
    app.starts_with("uhCkk") && app.len() == 53
}
