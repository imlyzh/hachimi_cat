{
  description = "Rust Cross-Compilation: Bundled WebRTC Fix";

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

        inherit (pkgs) lib stdenv;

        pkgsMinGW = pkgs.pkgsCross.mingwW64;

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
          targets = [ "x86_64-pc-windows-gnu" "wasm32-unknown-unknown" ];
        };

        # 1. CMake 包装器 (保留)
        cmakeSmart = pkgs.writeShellScriptBin "cmake" ''
          is_build=0
          for arg in "$@"; do
            if [ "$arg" = "--build" ]; then
              is_build=1
              break
            fi
          done
          if [ $is_build -eq 1 ]; then
            exec ${pkgs.cmake}/bin/cmake "$@"
          else
            exec ${pkgs.cmake}/bin/cmake -DCMAKE_POLICY_VERSION_MINIMUM=3.5 "$@"
          fi
        '';

        # 2. Lib Shim (保留，用于解决 ar 参数不兼容)
        libShim = pkgs.writeShellScriptBin "lib" ''
          args=()
          outfile=""
          for arg in "$@"; do
            case "$arg" in
              -OUT:*|-out:*) outfile="''${arg#*:}" ;;
              -nologo|-NOLOGO) ;;
              *) args+=("$arg") ;;
            esac
          done
          if [ -n "$outfile" ]; then
            exec x86_64-w64-mingw32-ar cru "$outfile" "''${args[@]}"
          else
            exec x86_64-w64-mingw32-ar "$@"
          fi
        '';

        # 3. Glibtoolize Shim (保留)
        glibtoolizeShim = pkgs.writeShellScriptBin "glibtoolize" ''
          exec ${pkgs.libtool}/bin/libtoolize "$@"
        '';

      in
      {
        devShells.default = pkgs.mkShell {
          # -----------------------------------------------------------
          # Native Build Inputs
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

            # Shims
            cmakeSmart
            libShim
            glibtoolizeShim

            # MinGW 工具链
            pkgsMinGW.stdenv.cc
            pkgsMinGW.binutils
          ]
          ++ lib.optionals stdenv.isDarwin [
            pkgs.libiconv
          ];

          # -----------------------------------------------------------
          # Build Inputs
          # -----------------------------------------------------------
          buildInputs = [
            rustToolchain
            # 注意：不引入系统 webrtc，因为版本不匹配
          ];

          # -----------------------------------------------------------
          # 环境变量
          # -----------------------------------------------------------

          CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = "x86_64-w64-mingw32-gcc";

          # 强制指定编译器
          CC_x86_64_pc_windows_gnu = "x86_64-w64-mingw32-gcc";
          CXX_x86_64_pc_windows_gnu = "x86_64-w64-mingw32-g++";
          AR_x86_64_pc_windows_gnu = "x86_64-w64-mingw32-ar";

          # 工具链定义
          AR = "x86_64-w64-mingw32-ar";
          RANLIB = "x86_64-w64-mingw32-ranlib";
          RC = "x86_64-w64-mingw32-windres";
          WINDRES = "x86_64-w64-mingw32-windres";
          DLLTOOL = "x86_64-w64-mingw32-dlltool";
          OBJDUMP = "x86_64-w64-mingw32-objdump";

          lt_cv_deplibs_check_method = "pass_all";
          CMAKE_x86_64_pc_windows_gnu = "${cmakeSmart}/bin/cmake";

          # Cache Injection: 强制指定 Host，绕过 old config.sub
          ac_cv_host = "x86_64-w64-mingw32";
          ac_cv_target = "x86_64-w64-mingw32";

          # --- 关键修复：禁用 execinfo.h 检测 ---
          # 告诉 configure：“我没有 execinfo.h”，这样它就不会尝试编译 backtrace 相关代码
          ac_cv_header_execinfo_h = "no";

          # --- 终极修复：编译器参数 ---
          # 1. -fms-extensions: 允许 __try/__except 语法
          # 2. -UWEBRTC_POSIX: 确保不启用 POSIX 代码路径
          # 3. -DWEBRTC_WIN: 强制启用 Windows 代码路径
          WEBRTC_AUDIO_PROCESSING_SYS_CONFIGURE_ARGS = lib.concatStringsSep " " [
            "--host=x86_64-w64-mingw32"
            "--build=${system}"
            "CC=x86_64-w64-mingw32-gcc"
            "CXX=x86_64-w64-mingw32-g++"
            "CFLAGS='-O2 -g -m64 -fms-extensions -UWEBRTC_POSIX -DWEBRTC_WIN -D_WIN32'"
            "CXXFLAGS='-O2 -g -m64 -fms-extensions -UWEBRTC_POSIX -DWEBRTC_WIN -D_WIN32'"
          ];

          # Pthreads & Link Args
          CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS =
            "-L native=${pkgsMinGW.windows.pthreads}/lib " +
            "-C link-arg=-Wl,--exclude-all-symbols";

          PKG_CONFIG_ALLOW_CROSS = "1";

          shellHook = ''
            echo "💉 Bundled Fix Environment Loaded"
            echo "   Disabled execinfo.h (ac_cv_header_execinfo_h=no)"
            echo "   Enabled MS Extensions (-fms-extensions)"
          '';
        };
      }
    );
}
