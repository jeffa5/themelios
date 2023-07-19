{
  description = "Model checked orchestration";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils = {
      url = "github:numtide/flake-utils";
    };
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = {
    self,
    nixpkgs,
    flake-utils,
    rust-overlay,
  }:
    flake-utils.lib.eachDefaultSystem
    (system: let
      pkgs = import nixpkgs {
        overlays = [rust-overlay.overlay];
        inherit system;
      };
      rust = pkgs.rust-bin.stable.latest.default;
    in {
      devShells.default = pkgs.mkShell {
        packages = [
          (rust.override {
            extensions = ["rust-src"];
          })
        ];
      };
    });
}
