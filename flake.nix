{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";

    devenv.url = "github:cachix/devenv";
    devenv-root = {
      url = "file+file:///dev/null";
      flake = false;
    };

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  nixConfig = {
    extra-trusted-public-keys = "devenv.cachix.org-1:w1cLUi8dv3hnoSPGAuibQv+f9TZLr6cv/Hm9XgU50cw=";
    extra-substituters = "https://devenv.cachix.org";
  };

  outputs = { self, flake-parts, devenv-root,... } @ inputs:
    flake-parts.lib.mkFlake { inherit inputs; } {
      imports = [
        inputs.devenv.flakeModule
      ];

      # 定义支持的系统架构
      systems = [
        "x86_64-linux"
        "x86_64-darwin"
        "aarch64-linux"
        "aarch64-darwin"
      ];

      perSystem = { config, self', inputs', pkgs, system, ... }: {
        _module.args.pkgs = import inputs.nixpkgs {
            inherit system;
            overlays = [ inputs.rust-overlay.overlays.default ];
            config.allowUnfree = true;
          };

        # 核心：在这里引用外部的 devenv.nix
        devenv.shells.default = {
          imports = [ ./devenv.nix ];

          _module.args.inputs = inputs;

          env.PROJECT_SRC = "${self.outPath}";
          # devenv.root = "${self.outPath}";
          devenv.root =
            let
              devenvRootFileContent = builtins.readFile devenv-root.outPath;
            in
              pkgs.lib.mkIf (devenvRootFileContent != "") devenvRootFileContent;
          # pre-commit.enable = false;
          cachix.enable = false;
        };
      };
  };
}