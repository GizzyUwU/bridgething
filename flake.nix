{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };
  outputs =
    {
      nixpkgs,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs { inherit system; };
        lib = pkgs.lib;
      in
      rec {
        packages = {
          bridgething = pkgs.rustPlatform.buildRustPackage {
            pname = "bridgething";
            version = "0.1.0";

            nativeBuildInputs = with pkgs; [
              pkg-config
              rustPlatform.cargoSetupHook
              rustc
              cargo
            ];
            buildInputs = with pkgs; [
              dbus
              systemd
            ];

            src = ./.;
            cargoLock = {
              lockFile = ./Cargo.lock;
              allowBuiltinFetchGit = true;
            };

            buildFeatures = [ "superbird" ];
            doCheck = false;

            meta = {
              description = "A daemon for controlling functionality on the Car Thing";
              homepage = "https://github.com/JoeyEamigh/bridgething";
              license = lib.licenses.mit;
              maintainers = [ "Joey Eamigh" ];
            };
          };

          default = packages.bridgething;
        };
      }
    );
}
