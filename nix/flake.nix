{
  description = "A very basic flake rewritten with devenv + flake-parts";

  inputs = {
    # 基础依赖
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";

    # Devenv 核心
    devenv.url = "github:cachix/devenv";
    devenv-root = {
      url = "file+file:///dev/null";
      flake = false;
    };

    # Rust 专用
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  # 配置 Nix 缓存，加速构建
  nixConfig = {
    extra-trusted-public-keys = "devenv.cachix.org-1:w1cLUi8dv3hnoSPGAuibQv+f9TZLr6cv/Hm9XgU50cw=";
    extra-substituters = "https://devenv.cachix.org";
  };

  outputs =
    inputs@{ flake-parts, devenv-root, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      imports = [
        inputs.devenv.flakeModule
      ];

      # 定义支持的系统架构
      systems = [
        "x86_64-linux"
        "i686-linux"
        "x86_64-darwin"
        "aarch64-linux"
        "aarch64-darwin"
      ];

      perSystem =
        {
          config,
          self',
          inputs',
          pkgs,
          system,
          ...
        }:
        let
          inherit (pkgs) lib stdenv;

  cmake-compat = pkgs.writeShellScriptBin "cmake" ''
    exec ${pkgs.cmake}/bin/cmake \
      -DCMAKE_POLICY_DEFAULT_CMP0000=OLD \
      -DCMAKE_POLICY_VERSION_MINIMUM=3.5 \
      "$@"
  '';
        in {
          # 1. 这里覆盖当前系统的 pkgs，注入 rust-overlay
          # 这样在下面的 packages 列表中就可以直接使用 pkgs.rust-bin 了
          _module.args.pkgs = import inputs.nixpkgs {
            inherit system;
            overlays = [ inputs.rust-overlay.overlays.default ];
            config.allowUnfree = true;
          };

          devenv.shells.default = {
            # 2. Devenv Root hack (用于保持 direnv 状态稳定)
            devenv.root =
              let
                devenvRootFileContent = builtins.readFile devenv-root.outPath;
              in
              pkgs.lib.mkIf (devenvRootFileContent != "") devenvRootFileContent;

            # 3. 环境变量设置 (可选)
            # env.RUST_SRC_PATH = "${pkgs.rust-bin.stable.latest.default}/lib/rustlib/src/rust/library";

            # 4. 软件包列表 (对应原 mkShell 的 buildInputs)
            packages = [
              # Node.js 相关
              pkgs.nodejs
              pkgs.biome
              pkgs.yarn-berry

              # Shell 工具
              pkgs.bashInteractive
              pkgs.just
              # 使用较新的 nixfmt 格式化工具，如果需要旧版可改为 pkgs.nixfmt
              pkgs.nixfmt-rfc-style
              pkgs.nil

              # Rust 工具链 (保留了原项目的具体配置)
              (pkgs.rust-bin.stable.latest.default.override {
                extensions = [
                  "rust-src"
                  "rust-analyzer"
                ];
                targets = [
                  "wasm32-unknown-unknown"
                  "x86_64-unknown-linux-gnu"
                ];
              })

              # WebAssembly 工具
              pkgs.wasm-bindgen-cli
              pkgs.wasm-pack

              # build tools
              pkgs.pkg-config
              pkgs.autoconf
              pkgs.automake
              pkgs.libtool
              pkgs.cmake
              # cmake-compat
              pkgs.opusTools
              # pkgs.libopus

            ] ++ lib.optionals stdenv.isLinux [ pkgs.alsa-lib.dev ];

          };
        };
  #       shellHook = ''
  #   export CMAKE_ARGS="-DCMAKE_POLICY_DEFAULT_CMP0000=OLD -DCMAKE_POLICY_VERSION_MINIMUM=3.5"
  # '';
    };
}
