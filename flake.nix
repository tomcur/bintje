{
  description = "Bintje development shell";
  inputs.flake-utils.url = "github:numtide/flake-utils";
  inputs.rust-overlay.url = "github:oxalica/rust-overlay";
  inputs.flake-compat = {
    url = "github:edolstra/flake-compat";
    flake = false;
  };
  outputs = { nixpkgs, rust-overlay, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
      in
      {
        devShell = pkgs.mkShell.override { stdenv = pkgs.clangStdenv; } {
          nativeBuildInputs = with pkgs; [
            # rust-analyzer
            (rust-bin.fromRustupToolchainFile ./rust-toolchain.toml)
            rustfmt
          ];

          shellHook =
            let
              libraryPath = with pkgs;
                lib.strings.makeLibraryPath [
                  libGL
                ];
            in
            ''
              RUST_SRC_PATH="${pkgs.rust.packages.stable.rustPlatform.rustLibSrc}";
              export RUST_LOG="warn,response=trace,response_app=debug,response_view=debug,skimgui=trace";
              # workaround for npm dep compilation
              # https://github.com/imagemin/optipng-bin/issues/108

              LD_LIBRARY_PATH=$LD_LIBRARY_PATH:${libraryPath}
              LD=$CC
            '';

        };
      });
}
