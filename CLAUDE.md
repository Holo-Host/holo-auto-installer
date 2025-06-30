# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Development Commands

```bash
# Environment setup (recommended)
nix develop

# Build and test
cargo build
cargo test
cargo fmt
cargo clippy

# CI commands (must pass before pushing)
cargo test     # Required by CI
cargo fmt      # Required by CI (zero tolerance)
cargo clippy   # Required by CI (zero warnings allowed)

# Run the application
cargo run -- <happ-list-path> [--admin-port 4444] [--happ-port 42233]
```

Git hooks via `cargo-husky` run fmt/clippy on push. Use `git push --no-verify` to skip if needed.

## Architecture Overview

**holo-auto-installer** manages Holochain application (hApp) lifecycle on Holo hosting infrastructure. The core workflow orchestrates installation and uninstallation of hApps based on KYC levels, jurisdictions, payment status, and publisher preferences.

### Key Components

- **`lib.rs`**: Main orchestration logic for install/uninstall workflows
- **`utils.rs`**: Core business logic determining what should be installed/uninstalled  
- **`hbs.rs`**: HBS (Holo Business Service) client for KYC and jurisdiction data
- **`entries.rs`**: Data structures for hApp bundles, host settings, publisher preferences
- **`transaction_types.rs`**: Payment and transaction type definitions
- **`main.rs`**: CLI entry point with async runtime and tracing setup

### Core Operations

1. **Install hApps**: `install_holo_hosted_happs()` - installs approved hApps not currently installed
2. **Uninstall hApps**: `uninstall_ineligible_happs()` - removes hApps no longer eligible for hosting

The `should_be_installed()` function in utils.rs is the central decision point for hApp eligibility.

### External Integrations

- **Holochain Conductor**: Admin API on port 4444, hApp port 42233
- **HBS Service**: All requests should go through `hbs.rs` module
- **Holofuel**: Payment status checking for hosting eligibility

## Configuration

The application expects a YAML file with this structure:

```yaml
core_happs:
  - app_id: string
    version: string
    dna_url: string (optional)
    ui_url: string (optional)
self_hosted_happs: [same structure as core_happs]
```

Default ports: Admin 4444, hApp 42233 (configurable via `--admin-port`/`--happ-port` or `ADMIN_PORT`/`HAPP_PORT` env vars).

## Development Environment

This project uses Nix for reproducible environments with Holonix integration. The flake provides Holochain CLI tools and WASM optimization tools. Rust 1.78.0 stable toolchain is specified in `rust-toolchain`.