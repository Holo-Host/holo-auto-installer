pub use crate::config;
use crate::host_zome_calls::CoreAppClient;
pub use crate::host_zome_calls::{is_happ_free, HappBundle};
pub use crate::websocket::AdminWebsocket;
use anyhow::{Context, Result};
use itertools::Itertools;
use tracing::{info, trace, warn};

/// uninstalled old hosted happs
/// Currently this completely removes the happ
/// This will be updated to checked enabled uninstalled and disable the happ accordingly
pub async fn uninstall_removed_happs(
    core_app_client: &mut CoreAppClient,
    config: &config::Config,
    expected_happs: &[HappBundle],
    is_kyc_level_2: bool,
) -> Result<()> {
    info!("Checking to uninstall happs that were removed from the hosted list....");

    let mut admin_websocket = AdminWebsocket::connect(config.admin_port)
        .await
        .context("failed to connect to holochain's admin interface")?;

    let running_happ_ids = admin_websocket
        .list_running_app()
        .await
        .context("failed to get installed hApps")?;

    let unique_running_happ_ids: Vec<&String> = running_happ_ids.iter().unique().collect();

    trace!("unique_running_happ_ids {:?}", unique_running_happ_ids);

    for happ_id in unique_running_happ_ids {
        if should_be_installed(core_app_client, happ_id, expected_happs, is_kyc_level_2).await {
            info!(
                "Skipping uninstall of {} as it should be installed",
                happ_id
            );
            continue;
        }

        if is_anonymous_instance(happ_id) {
            info!("Disabling {}", happ_id);
            admin_websocket.disable_app(happ_id).await?;
        } else {
            info!("Uninstalling {}", happ_id);
            admin_websocket.uninstall_app(happ_id).await?;
        }
    }
    info!("Done uninstall happs that were removed from the hosted list.");

    Ok(())
}

// There are core infrastructure happs that should never be uninstall. All uninstallable happs start with "uhCkk"
fn is_hosted_happ_or_sl(app: &str) -> bool {
    app.starts_with("uhCkk")
}

fn is_anonymous_instance(happ_id: &str) -> bool {
    happ_id.starts_with("uhCkk") && happ_id.len() == 53
}

pub async fn should_be_installed(
    core_app_client: &mut CoreAppClient,
    running_happ_id: &String,
    expected_happs: &[HappBundle],
    is_kyc_level_2: bool,
) -> bool {
    trace!("should_be_installed {}", running_happ_id);

    if !is_hosted_happ_or_sl(running_happ_id) {
        trace!("keeping infrastructure happ {}", running_happ_id);
        return true;
    }

    let expected_happ = expected_happs.iter().find(|expected_happ| {
        is_instance_of_happ(&expected_happ.happ_id.to_string(), running_happ_id)
    });

    trace!(
        "found expected_happ {:?}",
        &expected_happ.map(|eh| &eh.happ_id)
    );

    if let Some(_expected_happ) = expected_happ {
        // The running happ is an instance of an expected happ
        if is_kyc_level_2 {
            // nothing more to check, we should keep this happ
            true
        } else {
            let is_free = match is_happ_free(expected_happ.happ_id, core_app_client).await {
                Ok(is_free) => is_free,
                Err(e) => {
                    warn!("is_happ_free failed with {}", e);
                    false
                }
            };
            // if kyc is not level 2 and happ isn't free, we should not install
            is_free
        }
    } else {
        // The running happ is not an instance of any expected happ, so shouldn't be installed
        false
    }
}

/// Returns true if `installed_app_id` represents an anonymous or identified instance of `happ_id`
fn is_instance_of_happ(happ_id: &str, installed_app_id: &str) -> bool {
    // An `installed_app_id` is one of
    // - A core hApp (e.g. `servicelogger:0_2_1::251e7cc8-9c48-4841-9eb0-435f0bf97373`)
    // - An anonymous instance with installed_app_id == happ_id
    // - An identified instance matching /happ_id::agent_id/
    // - A happ-specific servicelogger instance matching /happ_id::servicelogger/
    installed_app_id.starts_with(happ_id) && !installed_app_id.ends_with("servicelogger")
}
