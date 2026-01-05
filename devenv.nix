{ pkgs, lib, config, inputs, ... }:
 let
  inherit (pkgs) lib stdenv;

  # overlays = [ (import inputs.rust-overlay) ];
  # pkgs = import pkgs { inherit overlays; };

  rustToolchain = pkgs.rust-bin.stable.latest.default.override {
    extensions = [
      "rust-src"
      "rust-analyzer"
      "rustfmt"
      "clippy"
    ];
    targets = [
      "x86_64-pc-windows-gnu"
      "wasm32-unknown-unknown"
    ];
  };

  llvm = pkgs.llvmPackages_19;

  # cmake-compat = pkgs.writeShellScriptBin "cmake" ''
  #   exec ${pkgs.cmake}/bin/cmake \
  #     -DCMAKE_POLICY_DEFAULT_CMP0000=OLD \
  #     -DCMAKE_POLICY_VERSION_MINIMUM=3.5 \
  #     "$@"
  # '';
in {
  env.GREET = "devenv";

  # cachix.enable = false;

  env = {
    # RUST_SRC_PATH = "${pkgs.rust-bin.stable.latest.default}/lib/rustlib/src/rust/library";
    LIBCLANG_PATH = "${llvm.libclang.lib}/lib";
  };

  languages.rust = {
    enable = true;
    channel = "stable";
    components = [ "rustc" "cargo" "clippy" "rustfmt" "rust-analyzer" ];
    toolchainPackage = rustToolchain;
  };

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

              # WebAssembly 工具
              pkgs.wasm-bindgen-cli
              pkgs.wasm-pack

              # rustToolchain

              # build tools
              pkgs.pkg-config
              pkgs.autoconf
              pkgs.automake
              pkgs.cmake
              llvm.libclang
              # cmake-compat
              pkgs.webrtc-audio-processing
              pkgs.libopus
            ]
            ++ lib.optionals stdenv.isLinux [ pkgs.libtool pkgs.alsa-lib.dev ]
            ++ lib.optionals stdenv.isDarwin [ pkgs.glibtool ];

  scripts.hello.exec = ''
    echo hello from $GREET
  '';

  # https://devenv.sh/basics/
  enterShell = ''
    hello         # Run scripts directly
    git --version # Use packages
  '';

  # https://devenv.sh/git-hooks/
  # git-hooks.hooks.shellcheck.enable = true;
}
