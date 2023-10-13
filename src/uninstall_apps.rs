pub use crate::config;
pub use crate::get_apps::HappBundle;
pub use crate::websocket::AdminWebsocket;
use anyhow::{Context, Result};
use tracing::{info, trace, warn};

/// uninstalled old hosted happs
/// Currently this completely removes the happ
/// This will be updated to checked enabled uninstalled and disable the happ accordingly
pub async fn uninstall_removed_happs(
    expected_happs: &[HappBundle],
    config: &config::Config,
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

    let happ_ids_to_uninstall: Vec<String> = running_happ_ids
        .into_iter()
        .filter(|running_happ_id: &String| {
            should_uninstall_happ(running_happ_id, expected_happs, is_kyc_level_2)
        })
        .collect();

    for happ_id in happ_ids_to_uninstall {
        info!("Disabling {}", happ_id);
        admin_websocket.uninstall_app(&happ_id).await?;
        let sl_instance = format!("{}::servicelogger", happ_id);
        if let Err(e) = admin_websocket.uninstall_app(&sl_instance).await {
            warn!(
                "Unable to disable sl instance: {} with error: {}",
                sl_instance, e
            )
        }
    }
    info!("Done uninstall happs that were removed from the hosted list.");

    Ok(())

    // let ano_happ_ids = filter_for_anonymous_happ_ids(active_app_ids.to_vec());

    // trace!("All anonymous happs {:?}", ano_happ_ids);

    // let happ_ids_to_uninstall = ano_happ_ids
    //     .into_iter()
    //     .filter(|installed_happ_id| {
    //         !happs.iter().any(
    //             |get_apps::HappBundle {
    //                  happ_id: expected_happ_id,
    //                  ..
    //              }| { &expected_happ_id.to_string() == installed_happ_id },
    //         ) || (!is_kyc_level_2 && happ_is_not_free(installed_happ_id, happs))
    //     })
    //     .collect();

    // let happ_to_uninstall =
    //     filter_for_hosted_happ_to_uninstall(happ_ids_to_uninstall, active_app_ids);
}

fn should_uninstall_happ(
    running_happ_id: &String,
    expected_happs: &[HappBundle],
    is_kyc_level_2: bool,
) -> bool {
    trace!("should_uninstall_happ {}", running_happ_id);

    if !is_hosted_happ_or_sl(running_happ_id) {
        trace!(
            "shouldn't uninstall infrastructure happ {}",
            running_happ_id
        );
        return false;
    }

    let expected_happ = expected_happs.iter().find(|expected_happ| {
        is_instance_or_sl_of_happ(&expected_happ.happ_id.to_string(), running_happ_id)
    });

    trace!(
        "found expected_happ {:?}",
        &expected_happ.map(|eh| &eh.happ_id)
    );

    if let Some(expected_happ) = expected_happ {
        // The running happ is an instance of an expected happ
        if is_kyc_level_2 {
            // nothing more to check, we should keep this happ
            false
        } else {
            trace!(
                "is free? {:?}",
                expected_happ.publisher_pricing_pref.is_free()
            );
            // if kyc is not level 2 and happ isn't free, we should uninstall
            !expected_happ.publisher_pricing_pref.is_free()
        }
    } else {
        // The running happ is not an instance of any expected happ, so we should uninstall
        true
    }
}

/// Takes a list of hApp IDs and returns a list of `installed_app_id`s corresponding with the anonymous and identified instances of those hApps.
// fn filter_for_hosted_happ_to_uninstall(
//     happ_ids: Vec<String>,
//     active_installed_app_ids: Vec<String>,
// ) -> Vec<String> {
//     active_installed_app_ids
//         .into_iter()
//         .filter(|installed_app_id| {
//             happ_ids
//                 .iter()
//                 .any(|happ_id| is_instance_of_happ(happ_id, installed_app_id))
//         })
//         .collect()
// }

/// Returns true if `installed_app_id` represents an anonymous or identified instance of `happ_id`
// fn is_instance_of_happ(happ_id: &str, installed_app_id: &str) -> bool {
//     // An `installed_app_id` is one of
//     // - A core hApp (e.g. `servicelogger:0_2_1::251e7cc8-9c48-4841-9eb0-435f0bf97373`)
//     // - An anonymous instance with installed_app_id == happ_id
//     // - An identified instance matching /happ_id::agent_id/
//     // - A happ-specific servicelogger instance matching /happ_id::servicelogger/
//     !installed_app_id.ends_with("servicelogger") && installed_app_id.starts_with(happ_id)
// }

fn is_instance_or_sl_of_happ(expected_happ_id: &str, running_app_id: &str) -> bool {
    // An `installed_app_id` is one of
    // - A core hApp (e.g. `servicelogger:0_2_1::251e7cc8-9c48-4841-9eb0-435f0bf97373`)
    // - An anonymous instance with installed_app_id == happ_id
    // - An identified instance matching /happ_id::agent_id/
    // - A happ-specific servicelogger instance matching /happ_id::servicelogger/
    running_app_id.starts_with(expected_happ_id)
}

// fn filter_for_anonymous_happ_ids(active_apps: Vec<String>) -> Vec<String> {
//     active_apps
//         .into_iter()
//         .filter(|app| is_anonymous(app))
//         .collect()
// }

// There are core infrastructure happs that should never be uninstall. All uninstallable happs start with "uhCkk"
fn is_hosted_happ_or_sl(app: &str) -> bool {
    app.starts_with("uhCkk")
}

// fn is_anonymous(app: &str) -> bool {
//     app.starts_with("uhCkk") && app.len() == 53
// }

// fn happ_is_not_free(happ_id: &str, happs: &[get_apps::HappBundle]) -> bool {
//     let happ = happs.iter().find(
//         |get_apps::HappBundle {
//              happ_id: expected_happ_id,
//              ..
//          }| { &expected_happ_id.to_string() == happ_id },
//     );
//     if let Some(found_happ) = happ {
//         return !found_happ.publisher_pricing_pref.is_free();
//     } else {
//         // if we can't find the pricing, we act as if happ is free so as to not uninstall happs too aggresively
//         trace!("Can't find happ with happ_id {}", happ_id);
//         return false;
//     }
// }
