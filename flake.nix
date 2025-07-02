{
  description = "Template for Holochain app development";

  inputs = {
    holonix.url = "github:holochain/holonix/main-0.5";

    nixpkgs.follows = "holonix/nixpkgs";
    flake-parts.follows = "holonix/flake-parts";

    # Rust toolchain overlay for importing specific versions of Rust
    rust-overlay.follows = "holonix/rust-overlay";
  };

  outputs = inputs@{ flake-parts, nixpkgs, rust-overlay, ... }: flake-parts.lib.mkFlake { inherit inputs; } {
    systems = builtins.attrNames inputs.holonix.devShells;
    perSystem = { system, inputs', pkgs, ... }: {
      formatter = pkgs.nixpkgs-fmt;

      # Custom Holochain with unstable functions and sharding enabled
      packages.customHolochain = inputs'.holonix.packages.holochain.override {
        cargoExtraArgs = "--features chc,unstable-sharding,unstable-functions,unstable-countersigning";
      };

      # Custom hc CLI with chc feature
      packages.customHc = inputs'.holonix.packages.hc.override {
        cargoExtraArgs = "--features chc";
      };

      devShells.default =
        let
          overlays = [ (import rust-overlay) ];
          pkgs = import nixpkgs {
            inherit system overlays;
          };

          # Define a Rust setup that works for your project. This should be a functional default
          # for a scaffolded project but can be adjusted.
          # Options: https://github.com/oxalica/rust-overlay?tab=readme-ov-file#cheat-sheet-common-usage-of-rust-bin
          rust = (pkgs.rust-bin.stable.latest.minimal.override
            {
              extensions = [ "clippy" "rustfmt" ];
              targets = [ "wasm32-unknown-unknown" ];
            });
        in
        pkgs.mkShell {
          packages = [
            rust
          ] ++ [
            # Use custom Holochain builds with unstable features
            inputs.self.packages.${system}.customHolochain
            inputs.self.packages.${system}.customHc
          ] ++ (with inputs'.holonix.packages; [
            hcterm
            bootstrap-srv
            lair-keystore
            hc-launch
            hc-scaffold
            hn-introspect
            hc-playground
          ]) ++ (with pkgs; [
            nodejs_20 # For UI development
            binaryen # For WASM optimisation
            wasm-strip
            git
            # Add any other packages you need here
          ]);

          shellHook = ''
            export PS1='\[\033[1;34m\][holonix:\w]\$\[\033[0m\] '
          '';
        };
    };
  };
}