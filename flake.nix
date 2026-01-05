{
  description = "Rust Audio Dev: Fixed Local & Cross Compilation";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        pkgsMinGW = pkgs.pkgsCross.mingwW64;

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
          targets = [ "x86_64-pc-windows-gnu" "wasm32-unknown-unknown" ];
        };

        # --- 核心修复：智能 CMake 包装器 ---
        # 强制注入 -DCMAKE_POLICY_VERSION_MINIMUM=3.5 解决 audiopus 报错
        cmakeSmart = pkgs.writeShellScriptBin "cmake" ''
          is_build=0
          for arg in "$@"; do
            if [ "$arg" = "--build" ]; then
              is_build=1
              break
            fi
          done

          if [ $is_build -eq 0 ]; then
             exec ${pkgs.cmake}/bin/cmake -DCMAKE_POLICY_VERSION_MINIMUM=3.5 "$@"
          else
             exec ${pkgs.cmake}/bin/cmake "$@"
          fi
        '';

      in
      {
        devShells.default = pkgs.mkShell {
          # -----------------------------------------------------------
          # Native Build Inputs (构建工具)
          # -----------------------------------------------------------
          nativeBuildInputs = [
            pkgs.pkg-config
            pkgs.just
            pkgs.nodejs
            pkgs.yarn-berry
            pkgs.gnumake
            pkgs.autoconf
            pkgs.automake
            pkgs.libtool
            pkgs.m4

            # 使用我们的兼容性包装器
            cmakeSmart

            # MinGW 工具链
            pkgsMinGW.stdenv.cc
            pkgsMinGW.binutils
          ]
          ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
            # pkgs.libiconv
            pkgs.glibtool
          ]
          ++ pkgs.lib.optionals pkgs.stdenv.isLinux [
            pkgs.libclang
            pkgs.alsa-lib
          ];

          # -----------------------------------------------------------
          # Build Inputs
          # -----------------------------------------------------------
          buildInputs = [
            rustToolchain
          ];

          # -----------------------------------------------------------
          # 环境变量
          # -----------------------------------------------------------

          CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = "x86_64-w64-mingw32-gcc";
          CC_x86_64_pc_windows_gnu = "x86_64-w64-mingw32-gcc";
          CXX_x86_64_pc_windows_gnu = "x86_64-w64-mingw32-g++";
          AR_x86_64_pc_windows_gnu = "x86_64-w64-mingw32-ar";

          CMAKE_SYSTEM_NAME_x86_64_pc_windows_gnu = "Windows";

          # 解决符号过多报错
          CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS = "-C link-arg=-Wl,--exclude-all-symbols";

          shellHook = ''
            echo "🚀 Build Environment Ready"
            echo "   Host CMake: $(which cmake)"
            echo "   Target: Windows (MinGW) & Local (macOS)"
          '';
        };
      }
    );
}
