pub use crate::config;
pub use crate::host_zome_calls::HappBundle;
pub use crate::websocket::AdminWebsocket;
use anyhow::{Context, Result};
use itertools::Itertools;
use tracing::{debug, info, trace};

/// Ineligible Happs = old holo-hosted happs, holo-disabled happs, or happs with invalid pricing for kyc level
/// Handles ineligible happs for 2 cases - identified and anonymous hosted agents:
///  - Identified: Uninstalls & removes identified instances of ineligible happs
///  - Anonymous: Disables anonymous instance of ineligible happs
pub async fn uninstall_ineligible_happs(
    config: &config::Config,
    published_happs: &[HappBundle],
    is_kyc_level_2: bool,
    suspended_happs: Vec<String>,
    jurisdiction: Option<String>,
) -> Result<()> {
    info!("Checking to uninstall happs that were removed from the hosted list....");

    let mut admin_websocket = AdminWebsocket::connect(config.admin_port)
        .await
        .context("Failed to connect to holochain's admin interface")?;

    let running_happ_ids = admin_websocket
        .list_running_app()
        .await
        .context("Failed to get installed and running hApps")?;

    let unique_running_happ_ids: Vec<&String> = running_happ_ids.iter().unique().collect();

    trace!("Unique_running_happ_ids {:?}", unique_running_happ_ids);

    for happ_id in unique_running_happ_ids {
        if should_be_installed(
            happ_id,
            published_happs,
            is_kyc_level_2,
            suspended_happs.clone(),
            jurisdiction.clone(),
        )
        .await
        {
            info!(
                "Skipping uninstall of {} as it should remain installed",
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
    info!("Done uninstalling happs that were removed from the hosted list.");

    Ok(())
}

// There are core infrastructure happs that should never be uninstalled. All uninstallable happs start with "uhCkk" and don't contain ::servicelogger
fn is_hosted_happ(app: &str) -> bool {
    app.starts_with("uhCkk") && !app.contains("::servicelogger")
}

fn is_anonymous_instance(happ_id: &str) -> bool {
    happ_id.starts_with("uhCkk") && happ_id.len() == 53
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

pub async fn should_be_installed(
    running_happ_id: &String,
    published_happs: &[HappBundle],
    is_kyc_level_2: bool,
    suspended_happs: Vec<String>,
    jurisdiction: Option<String>,
) -> bool {
    if suspended_happs.contains(running_happ_id) {
        trace!("Disabling suspended happ {}", running_happ_id);
        return false;
    }

    match jurisdiction {
        Some(jurisdiction) => {
            if let Some(happ) = published_happs
                .iter()
                .find(|&happ| happ.happ_id.to_string() == *running_happ_id)
            {
                let mut is_jurisdiction_in_list = false;
                if let Some(_happ_jurisdiction) = happ
                    .jurisdictions
                    .iter()
                    .find(|&happ_jurisdiction| *happ_jurisdiction == jurisdiction)
                {
                    is_jurisdiction_in_list = true;
                }
                if happ.exclude_jurisdictions && is_jurisdiction_in_list {
                    return false;
                }
                if !happ.exclude_jurisdictions && !is_jurisdiction_in_list {
                    return false;
                }
            }
        }
        None => {
            warn!("jurisdiction not available for holoport");
            warn!("happ {} won't be installed", running_happ_id);
            return false;
        }
    }

    trace!("`should_be_installed check` for {}", running_happ_id);

    if !is_hosted_happ(running_happ_id) {
        trace!("Keeping infrastructure happ {}", running_happ_id);
        return true;
    }

    // The running happ is an instance of an expected happ
    let expected_happ = published_happs.iter().find(|published_happ| {
        is_instance_of_happ(&published_happ.happ_id.to_string(), running_happ_id)
    });

    trace!(
        "Found expected_happ {:?}",
        &expected_happ.map(|eh| &eh.happ_id)
    );

    if let Some(expected_happ) = expected_happ {
        // if the expected happ is disabled by the host, happ shouldn't be installed
        if expected_happ.is_host_disabled {
            trace!(
                "Disabling happ {} because host was disabled it in hha",
                expected_happ.happ_id
            );
            return false;
        }

        // happ hosting is only valid (despite price prefs) if the host is >= kyc level 2
        is_kyc_level_2
    } else {
        // The running happ is not an instance of any expected happ, so shouldn't be installed
        false
    }
}
