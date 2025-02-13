# https://fasterthanli.me/series/building-a-rust-service-with-nix/part-10
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs = {
        nixpkgs.follows = "nixpkgs";
      };
    };
    cxlmalloc = {
      url = "github:nwtnni/sosp-paper19-ae";
    };
    lightning = {
      url = "github:nwtnni/lightning";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay, cxlmalloc, lightning }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [
          (import rust-overlay)
          (_: _: { libcxlmalloc = cxlmalloc.packages.${system}.default; })
          (_: _: { liblightning = lightning.packages.${system}.default; })
        ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        rustToolchain = pkgs.pkgsBuildHost.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
      in
      with pkgs; {
        devShells.default = mkShell {
          nativeBuildInputs = [
            boost
            clang
            libcxlmalloc
            liblightning
            gdb
            libndctl
            linuxPackages_latest.perf
            numactl
            pkg-config
            rust-cbindgen
            # https://gist.github.com/yihuang/b874efb97e99d4b6d12bf039f98ae31e?permalink_comment_id=4311076#gistcomment-4311076
            rustPlatform.bindgenHook
            rustToolchain
            rr
            taplo
          ];

          buildInputs = [
            (python3.withPackages (python-pkgs: with python-pkgs; [
              matplotlib
              plotly
              polars
              python-lsp-ruff
              python-lsp-server
            ]))
          ];

          # https://discourse.nixos.org/t/libclang-path-and-rust-bindgen-in-nixpkgs-unstable/13264
          LIBCLANG_PATH = "${llvmPackages_latest.libclang.lib}/lib";
        };
      }
    );
}
