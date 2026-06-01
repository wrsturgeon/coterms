{
  inputs = {
    advisory-db = {
      flake = false;
      url = "github:rustsec/advisory-db";
    };
    crane-flake.url = "github:ipetkov/crane";
    flake-utils.url = "github:numtide/flake-utils";
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    treefmt-nix = {
      inputs.nixpkgs.follows = "nixpkgs";
      url = "github:numtide/treefmt-nix/main";
    };
  };
  outputs =
    {
      advisory-db,
      crane-flake,
      flake-utils,
      nixpkgs,
      rust-overlay,
      self,
      treefmt-nix,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };
        crane = (crane-flake.mkLib pkgs).overrideToolchain (
          pkgs: pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml
        );

        src = ./.;

        env = {
          MIRIFLAGS = "-Zmiri-disable-isolation";
          RUSTDOCFLAGS = "--deny warnings";
        };

        craneDepArgs = { inherit src; };
        cargoArtifacts = crane.buildDepsOnly craneDepArgs;
        craneArgs = craneDepArgs // {
          inherit cargoArtifacts env;
          cargoClippyExtraArgs = "--all-features --all-targets --workspace -- --deny warnings";
          cargoNextestExtraArgs = "--all-features --workspace";
        };
        craneArtifacts = crane.buildPackage (craneArgs // { doCheck = false; });

        treefmt = treefmt-nix.lib.evalModule pkgs ./.treefmt.nix;
      in
      {
        checks = {
          audit = crane.cargoAudit { inherit src advisory-db; };
          build = craneArtifacts;
          clippy = crane.cargoClippy craneArgs;
          deny = crane.cargoDeny craneArgs;
          doc = crane.cargoDoc craneArgs;
          fmt-rust = crane.cargoFmt craneArgs;
          fmt-toml = crane.taploFmt craneArgs;
          tests = crane.cargoNextest craneArgs;
        };
        devShells.default = crane.devShell (
          env
          // {
            checks = self.checks.${system};
            inputsFrom = builtins.attrValues self.packages.${system};
            packages = with pkgs; [
              cargo-expand
              cargo-outdated
              valgrind
            ];
          }
        );
        formatter = treefmt.config.build.wrapper;
        packages.default = craneArtifacts;
      }
    );
}
