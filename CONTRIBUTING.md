# Contributing

## CI and Git hooks

GitHub Actions is used for CI, where `cargo {test,fmt,clippy}` commands are required to pass.

This project uses [`cargo-husky`] to provide Git hooks for running these commands on `git push`.
If you need to skip running Git hooks, use `git push --no-verify`.

[`cargo-husky`]: https://github.com/rhysd/cargo-husky

# Installing and Uninstalling hApps
holo auto installer does 2 main things.
- Install a happs that are supposed to be installed on the holoport but are not installed
  `install_holo_hosted_happs`
- Uninstall happs that are not supposed to be installed on the holoport but are installed
  `uninstall_ineligible_happs`

Generally if you want to restrict something so it is not installed on the holoport you can use the function inside 
`uninstall_apps.rs` called `should_be_installed` If this returns a `false` the happ will be uninstalled form the holoport.

### HBS
There is a connection made to HBS at the start of the script. Please look at `hbs.rs` if you need to communicate with HBS. Please make any HBS requests in this file and then expose it to the app.

### Zome
If you are making a zome call. Look at `host_zome_calls.rs`. Currently there are 2 sell ids setup, `core_happ` and `holofuel`. If you want to expand the functionality and add aditional cell ids then please look at `connect` function. All zome calls should be made here and exposed to the other functions 