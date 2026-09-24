{
  description = "Rust News Builder — development environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };

        # Honours rust-toolchain.toml, so the pinned toolchain has exactly one
        # source of truth (constitution: Technology and Output Constraints).
        rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

        # Tauri needs a system webview plus the GTK stack on Linux.
        tauriDeps = with pkgs; [
          webkitgtk_4_1
          gtk3
          libsoup_3
          glib
          glib-networking
          librsvg
          at-spi2-atk
          gdk-pixbuf
          cairo
          pango
          openssl
        ];

        # libwebp backs the lossy WebP encoder (plan.md Complexity Tracking).
        imageDeps = with pkgs; [ libwebp ];

        # The capture harness runs the Python reference to produce parity goldens.
        pythonForHarness = pkgs.python312;
      in
      {
        devShells.default = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [
            pkg-config
            wrapGAppsHook3
          ];

          buildInputs = [
            rustToolchain
            pkgs.nodejs_22
            pkgs.pnpm
            pythonForHarness
            pkgs.cargo-tauri
            pkgs.openssh
            pkgs.git
            pkgs.just
            pkgs.unzip
            # `php -l` on the Joomla bridge (002 research R9).
            pkgs.php
          ] ++ tauriDeps ++ imageDeps;

          shellHook = ''
            export RUST_BACKTRACE=1
            # Required so the webview finds its GIO modules under Nix.
            export GIO_MODULE_DIR="${pkgs.glib-networking}/lib/gio/modules/"
            export WEBKIT_DISABLE_COMPOSITING_MODE=1

            echo "rust-news-builder dev shell"
            echo "  rustc  $(rustc --version 2>/dev/null | cut -d' ' -f2)"
            echo "  node   $(node --version 2>/dev/null)"
            echo "  python $(python3 --version 2>/dev/null | cut -d' ' -f2)"
            echo
            echo "  just gates   — fmt, clippy, test, parity fixtures"
          '';
        };
      });
}
