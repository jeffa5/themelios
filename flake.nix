{
  description = "Model checked orchestration";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils = {
      url = "github:numtide/flake-utils";
    };
    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    self,
    nixpkgs,
    flake-utils,
    crane,
  }:
    flake-utils.lib.eachDefaultSystem
    (system: let
      pkgs = import nixpkgs {inherit system;};
      craneLib = crane.lib.${system};
      src = craneLib.cleanCargoSource (craneLib.path ./.);

      commonArgs = {inherit src;};
      cargoArtifacts = craneLib.buildDepsOnly commonArgs;
      themelios = craneLib.buildPackage (commonArgs
        // {
          inherit cargoArtifacts;
        });
      python =
        pkgs.python3.withPackages (ps: [ps.pandas ps.seaborn]);
      plot = pkgs.writeShellScriptBin "plot" ''
        ${pkgs.lib.getExe python} ${./plot.py}
      '';
    in {
      packages = {
        inherit themelios plot;
      };

      devShells.default = pkgs.mkShell {
        packages = [
          pkgs.rustc
          pkgs.cargo
          pkgs.rustfmt
          pkgs.clippy

          pkgs.cargo-flamegraph

          pkgs.kubectl
          pkgs.kind
          pkgs.etcd
          pkgs.cargo-tarpaulin
        ];
      };
    });
}
