# Maintainer: Dorian Luyet (@DurianTheWAV), with linux/nix/package.nix.
{
  description = "Ronron: Ronron lives at the top of your screen and keeps an eye on your AI coding agents";

  # Try it:          nix run github:tisuiss/ronron
  # On NixOS, add the flake to your inputs, then:
  #   environment.systemPackages = [ inputs.ronron.packages.${pkgs.stdenv.hostPlatform.system}.default ];
  # Through the overlay instead, allow it in your own nixpkgs config, since the
  # artwork is not free (see meta.license in linux/nix/package.nix):
  #   nixpkgs.overlays = [ inputs.ronron.overlays.default ];
  #   nixpkgs.config.allowUnfreePredicate = pkg: lib.getName pkg == "ronron";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      inherit (nixpkgs) lib;

      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];

      forAllSystems =
        f:
        lib.genAttrs systems (
          system:
          f (
            import nixpkgs {
              inherit system;
              # Only for this package: building it for yourself is what its
              # license allows.
              config.allowUnfreePredicate = pkg: lib.getName pkg == "ronron";
            }
          )
        );
    in
    {
      packages = forAllSystems (pkgs: {
        ronron = pkgs.callPackage ./linux/nix/package.nix { };
        default = self.packages.${pkgs.stdenv.hostPlatform.system}.ronron;
      });

      overlays.default = final: prev: {
        ronron = final.callPackage ./linux/nix/package.nix { };
      };

      # The toolchain and libraries to work on the Linux app: `nix develop`, then
      # `cd windows && npm ci && npm run tauri dev`.
      devShells = forAllSystems (
        pkgs:
        let
          ronron = self.packages.${pkgs.stdenv.hostPlatform.system}.ronron;
        in
        {
          default = pkgs.mkShell {
            inputsFrom = [ ronron ];
            SHERPA_ONNX_LIB_DIR = "${ronron.sherpaOnnxStatic}/lib";
          };
        }
      );

      formatter = forAllSystems (pkgs: pkgs.nixfmt);
    };
}
