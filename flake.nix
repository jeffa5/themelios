{
  description = "Model checked orchestration";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils = {
      url = "github:numtide/flake-utils";
    };
    rust-overlay.url = "github:oxalica/rust-overlay";
    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    self,
    nixpkgs,
    flake-utils,
    rust-overlay,
    crane,
  }:
    flake-utils.lib.eachDefaultSystem
    (system: let
      pkgs = import nixpkgs {
        overlays = [rust-overlay.overlays.default];
        inherit system;
      };
      rust = pkgs.rust-bin.stable.latest.default;
      craneLib = crane.lib.${system};
      src = craneLib.cleanCargoSource (craneLib.path ./.);

      commonArgs = {inherit src;};
      cargoArtifacts = craneLib.buildDepsOnly commonArgs;
      model-checked-orchestration = craneLib.buildPackage (commonArgs
        // {
          inherit cargoArtifacts;
        });
    in {
      packages = {
        inherit model-checked-orchestration;
      };

      devShells.default = pkgs.mkShell {
        packages = [
          (rust.override {
            extensions = ["rust-src"];
          })
          pkgs.cargo-flamegraph
        ];
      };
    });
}
