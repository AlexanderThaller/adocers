{
  description = "Render AsciiDoc documents to HTML, with source-anchored diagnostics and a watch mode";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
    }:
    let
      inherit (nixpkgs) lib;

      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];

      forAllSystems = f: lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});

      # The toolchain comes from `rust-overlay`, pinned by this flake's lock,
      # rather than from whatever nixpkgs the caller brought. Two dependencies
      # want rustc 1.96 and a release branch is easily behind that — 26.05
      # carries 1.95 — which is no reason to make the caller find a newer
      # nixpkgs. `mkRustBin` builds the toolchain set without pushing an
      # overlay onto the caller's package set.
      rustBinFor = pkgs: rust-overlay.lib.mkRustBin { } pkgs;

      rustPlatformFor =
        pkgs:
        let
          toolchain = (rustBinFor pkgs).stable.latest.minimal;
        in
        pkgs.makeRustPlatform {
          cargo = toolchain;
          rustc = toolchain;
        };

      # The build reads the version from the manifest, so a release only has to
      # be cut in one place.
      cargoToml = lib.importTOML ./Cargo.toml;

      # Everything the build needs, and nothing else — a rendered
      # `resources/*.html` or a touched `target` must not invalidate it.
      #
      # `.cargo/config.toml` is deliberately left out: it asks for
      # `-Ctarget-cpu=x86-64-v3`, which is right for the machine this is
      # developed on and wrong for a package that has to run on whatever CPU
      # installs it — and is not even a valid target on aarch64.
      source = lib.fileset.toSource {
        root = ./.;
        fileset = lib.fileset.unions [
          ./Cargo.toml
          ./Cargo.lock
          ./src
          ./tests
          ./benches
          ./README.md
          (lib.fileset.fileFilter (file: file.hasExt != "html") ./resources)
        ];
      };

      # The package as a nixpkgs-style file, so `callPackage` can supply its
      # arguments and an overlay can reuse it unchanged.
      adocersPackage =
        {
          lib,
          rustPlatform,
        }:
        rustPlatform.buildRustPackage {
          pname = cargoToml.package.name;
          inherit (cargoToml.package) version;

          src = source;

          cargoLock.lockFile = ./Cargo.lock;

          # The tree-sitter grammars are C, compiled by the `cc` crate; the
          # stdenv compiler is all they need. Nothing else here links against a
          # system library — Typst brings its own fonts and there is no
          # OpenSSL — so the binary is self-contained.

          # The suites drive the built binary, so they run here as they do
          # under `cargo test` — with one exception. Nix copies the git tree
          # without its submodules, so `resources/asciidoctor-doctest` is not
          # in `src` and `tests/doctest.rs` takes the skip it documents for a
          # fresh clone. To hold a build to the corpus as well, fetch this
          # flake as `github:AlexanderThaller/adocers?submodules=1`.
          doCheck = true;

          meta = {
            inherit (cargoToml.package) description;
            homepage = "https://github.com/AlexanderThaller/adocers";
            license = with lib.licenses; [
              mit
              asl20
            ];
            mainProgram = "adocers";
            platforms = lib.platforms.unix;
          };
        };
    in
    {
      overlays.default = final: _prev: {
        adocers = final.callPackage adocersPackage { rustPlatform = rustPlatformFor final; };
      };

      packages = forAllSystems (pkgs: rec {
        adocers = pkgs.callPackage adocersPackage { rustPlatform = rustPlatformFor pkgs; };
        default = adocers;
      });

      apps = forAllSystems (pkgs: rec {
        adocers = {
          type = "app";
          program = lib.getExe self.packages.${pkgs.stdenv.hostPlatform.system}.adocers;
          meta.description = "Render AsciiDoc documents to HTML or PDF";
        };
        default = adocers;
      });

      devShells = forAllSystems (
        pkgs:
        let
          rustBin = rustBinFor pkgs;
        in
        {
          default = pkgs.mkShell {
            packages = [
              # The toolchain the package is built with, plus what it takes to
              # work on it. `rust-src` is here so `rust-analyzer` can follow a
              # jump into the standard library.
              (rustBin.stable.latest.minimal.override {
                extensions = [
                  "clippy"
                  "rust-analyzer"
                  "rust-src"
                ];
              })

              # `.rustfmt.toml` asks for options only a nightly rustfmt
              # accepts. It is a separate toolchain so that nothing else here
              # is nightly — `minimal` without `rustfmt` above keeps the two
              # from both putting a `rustfmt` on PATH.
              (rustBin.selectLatestNightlyWith (
                toolchain: toolchain.minimal.override { extensions = [ "rustfmt" ]; }
              ))

              # `resources/` holds two submodules the doctest suite and the
              # benchmarks read.
              pkgs.git
            ];
          };
        }
      );

      checks = forAllSystems (pkgs: {
        inherit (self.packages.${pkgs.stdenv.hostPlatform.system}) adocers;
      });

      formatter = forAllSystems (pkgs: pkgs.nixfmt);
    };
}
