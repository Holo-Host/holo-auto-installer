# Contributing

## CI and Git hooks

GitHub Actions is used for CI, where `cargo {test,fmt,clippy}` commands are required to pass.

This project uses [`cargo-husky`] to provide Git hooks for running these commands on `git push`.
If you need to skip running Git hooks, use `git push --no-verify`.

[`cargo-husky`]: https://github.com/rhysd/cargo-husky
