{
  description = "Render AsciiDoc documents to HTML or PDF, with source-anchored diagnostics, a watch mode and a preview server";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    crane.url = "github:ipetkov/crane";
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      crane,
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

      # Crane rather than `buildRustPackage`: the dependencies are a
      # derivation of their own, which stays valid until `Cargo.lock` or the
      # toolchain changes.
      craneLibFor = pkgs: (crane.mkLib pkgs).overrideToolchain (p: (rustBinFor p).stable.latest.minimal);

      # What a dynamically linked build should look for at run time, or `[]`
      # where the question does not arise. See `postFixup` in `adocersPackage`.
      runpathFor =
        pkgs:
        lib.optionals pkgs.stdenv.hostPlatform.isLinux [
          pkgs.stdenv.cc.libc
          pkgs.stdenv.cc.cc.libgcc
        ];

      # The build reads the version from the manifest, so a release only has to
      # be cut in one place — `[workspace.package]`, which every crate in the
      # workspace inherits.
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
          ./crates
          ./src
          ./tests
          ./benches
          ./README.md
          (lib.fileset.fileFilter (file: file.hasExt != "html") ./resources)
        ];
      };

      # One binary of the workspace as a nixpkgs-style package, so `callPackage`
      # can supply its arguments and an overlay can reuse it unchanged. `crate`
      # is the package's name in the workspace and the name of its binary:
      # `adocers` at the root, or `adocers-review` under `crates/`.
      binaryPackage =
        crate:
        {
          lib,
          craneLib,
          runpath ? null,
        }:
        let
          # The arguments of the dependency build below, the same for every
          # binary, so their `cargoArtifacts` are one derivation, built once.
          depsArgs = {
            pname = cargoToml.package.name;
            inherit (cargoToml.workspace.package) version;

            src = source;
            strictDeps = true;
          };
          crateManifest =
            if crate == cargoToml.package.name then
              cargoToml.package
            else
              (lib.importTOML ./crates/${crate}/Cargo.toml).package;
        in
        craneLib.buildPackage (
          depsArgs
          // {
            pname = crate;

            # The dependencies alone, from nothing but `Cargo.toml` and
            # `Cargo.lock`, so a change to the code does not compile them again.
            # Nothing downstream reads `cargo check`'s output, so that pass is
            # skipped; the tests' dev-dependencies are compiled along. Those of
            # every crate in the workspace, so that this one derivation serves
            # each binary: at the root alone, cargo would compile only the
            # dependencies of `adocers`.
            cargoArtifacts = craneLib.buildDepsOnly (
              depsArgs
              // {
                cargoCheckCommand = "true";
                cargoExtraArgs = "--locked --workspace";
              }
            );

            # Build, test and install this crate alone. Without `--package`,
            # cargo at the root would build only `adocers`, whatever `crate` is.
            cargoExtraArgs = "--locked --package ${crate}";

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

            # A full strip rather than the default `-S -p`. What is being removed
            # is not only size: the debug information names the store paths the
            # binary was linked against, and nix reads those as references — so
            # an unstripped binary drags a compiler's worth of shared objects it
            # never opens into the image behind it.
            stripAllList = [ "bin" ];

            # `libgcc_s.so.1` is the only file this binary ever opens out of
            # gcc's `lib` output, and that output is ten megabytes of libstdc++
            # and sanitizer runtimes it never touches — all of which the linker's
            # runpath drags into the image behind that one shared object. nixpkgs
            # also ships `libgcc_s.so.1` on its own, at 200 kB, and glibc already
            # puts that in the closure, so narrowing the runpath to what is
            # actually opened costs nothing and sheds the fat output.
            postFixup = lib.optionalString (runpath != null && runpath != [ ]) ''
              patchelf --set-rpath ${lib.makeLibraryPath runpath} $out/bin/${crate}
            '';

            meta = {
              inherit (crateManifest) description;
              homepage = "https://github.com/AlexanderThaller/adocers";
              license = with lib.licenses; [
                mit
                asl20
              ];
              mainProgram = crate;
              platforms = lib.platforms.unix;
            };
          }
        );

      # The container: the binary, the shared objects it opens and the libc
      # closure behind them, and nothing else. No shell, no `coreutils`, no
      # package manager — an image with a shell in it is an image someone will
      # debug in, and there is nothing in this one to debug.
      #
      # It is the ordinary glibc build and not a static musl one, which would
      # make the image a single file with no loader in it. A render is almost
      # entirely allocation — a document is parsed into a tree of owned strings
      # and rendered into another, and the tree-sitter grammars are C and call
      # `malloc` themselves — and musl's allocator is slow enough at that to
      # dominate the run. antors, which renders whole sites through these very
      # crates, measured 1697 ms against glibc's 821 ms on a 232-page site
      # until it brought an allocator of its own along. Nothing here brings
      # one, so the image keeps the libc that is fast without it, and pays for
      # it in the shared objects it has to carry.
      containerFor =
        pkgs:
        let
          adocers = self.packages.${pkgs.stdenv.hostPlatform.system}.adocers;
        in
        pkgs.dockerTools.buildLayeredImage {
          name = "adocers";
          tag = cargoToml.workspace.package.version;

          # `WorkingDir` has to exist for a run that does *not* mount over it —
          # `adocers --help`, or a document given by absolute path. `/tmp` is
          # there because `-o /tmp/...` is the obvious way to render a document
          # whose output you do not want to keep, and an image with no writable
          # directory at all fails that with `Permission denied`.
          extraCommands = ''
            mkdir -p docs tmp
            chmod 1777 tmp
          '';

          config = {
            # The store path itself rather than a symlink at the root, which is
            # what keeps even `/bin` out of the image.
            Entrypoint = [ (lib.getExe adocers) ];

            # Documents are mounted here and named relative to it, so the
            # common case is `docker run -v "$PWD:/docs" adocers doc.adoc`. Add
            # `--user "$(id -u):$(id -g)"` and the pages it writes belong to
            # you rather than to root.
            WorkingDir = "/docs";

            # `adocers serve` defaults to loopback, which reaches nothing from
            # outside the container; `--bind 0.0.0.0:8080` is what makes this
            # port worth exposing.
            ExposedPorts = {
              "8080/tcp" = { };
            };

            Labels = {
              "org.opencontainers.image.title" = "adocers";
              "org.opencontainers.image.description" = cargoToml.package.description;
              "org.opencontainers.image.source" = "https://github.com/AlexanderThaller/adocers";
              "org.opencontainers.image.version" = cargoToml.workspace.package.version;
              "org.opencontainers.image.licenses" = "MIT OR Apache-2.0";
            };
          };
        };
    in
    {
      overlays.default = final: _prev: {
        adocers = final.callPackage (binaryPackage "adocers") {
          craneLib = craneLibFor final;
          runpath = runpathFor final;
        };
        adocers-review = final.callPackage (binaryPackage "adocers-review") {
          craneLib = craneLibFor final;
          runpath = runpathFor final;
        };
      };

      packages = forAllSystems (
        pkgs:
        rec {
          adocers = pkgs.callPackage (binaryPackage "adocers") {
            craneLib = craneLibFor pkgs;
            runpath = runpathFor pkgs;
          };

          adocers-review = pkgs.callPackage (binaryPackage "adocers-review") {
            craneLib = craneLibFor pkgs;
            runpath = runpathFor pkgs;
          };

          default = adocers;
        }
        // lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
          container = containerFor pkgs;
        }
      );

      apps = forAllSystems (pkgs: rec {
        adocers = {
          type = "app";
          program = lib.getExe self.packages.${pkgs.stdenv.hostPlatform.system}.adocers;
          meta.description = "Render AsciiDoc documents to HTML or PDF";
        };
        adocers-review = {
          type = "app";
          program = lib.getExe self.packages.${pkgs.stdenv.hostPlatform.system}.adocers-review;
          meta.description = "Read a directory of AsciiDoc on a phone and comment on its blocks";
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

              # `justfile` holds the release chain.
              pkgs.just
            ];
          };
        }
      );

      checks = forAllSystems (pkgs: {
        inherit (self.packages.${pkgs.stdenv.hostPlatform.system}) adocers adocers-review;
      });

      formatter = forAllSystems (pkgs: pkgs.nixfmt);
    };
}
