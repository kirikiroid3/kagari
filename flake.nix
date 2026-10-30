{
  description = "Kagari: the toolchain and desktop runtime (packages), and the dev shell (system libraries; Rust comes from rustup)";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  outputs = { self, nixpkgs }:
    let
      lib = nixpkgs.lib;
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAll = f: lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});

      # dlopen'ed at run time by winit, wgpu (Vulkan / EGL) and xkbcommon-dl
      runtimeLibs = pkgs: with pkgs; [ wayland libxkbcommon vulkan-loader libGL libx11 libxcursor libxi libxrandr ];

      # The last release tag, written by the release workflow (.github/workflows/release.yml)
      # after it uploads a tagged version: { "version": "x.y.z", "rev": "<commit of the tag>" }.
      # Absent until the first release.
      stableFile = ./nix/stable.json;
      stable = if builtins.pathExists stableFile then lib.importJSON stableFile else null;

      # The repository's Cargo sources, so that changes elsewhere (docs, CI) do not rebuild.
      localSource = lib.fileset.toSource {
        root = ./.;
        fileset = lib.fileset.unions [ ./Cargo.toml ./Cargo.lock ./core ./runtime ./runtime_libs ./tooling ];
      };
      workspaceVersion = (lib.importTOML ./Cargo.toml).workspace.package.version;

      # `kagari` and `kagari-desktop-runtime` from a source tree of this repository.
      kagari = pkgs: { src, version }: pkgs.rustPlatform.buildRustPackage {
        pname = "kagari";
        inherit src version;
        cargoLock.lockFile = "${src}/Cargo.lock";
        cargoBuildFlags = [ "-p" "kagari-cli" "-p" "kagari-desktop-runtime" ];
        # The tests run in the repository's own checks; they need no Nix sandbox.
        doCheck = false;
        nativeBuildInputs = [ pkgs.pkg-config ];
        # linked at build time: alsa-sys (cpal) and libudev-sys (gilrs)
        buildInputs = [ pkgs.alsa-lib pkgs.udev ];
        # After fixup's --shrink-rpath, which would drop entries no DT_NEEDED uses.
        postFixup = ''
          patchelf --add-rpath ${lib.makeLibraryPath (runtimeLibs pkgs)} $out/bin/kagari-desktop-runtime
        '';
        meta = {
          description = "The Kagari visual novel engine: the kagari toolchain and the desktop runtime";
          homepage = "https://kagari.suitsu31.club/";
          license = lib.licenses.asl20;
          mainProgram = "kagari";
          platforms = lib.platforms.linux;
        };
      };
    in {
      packages = forAll (pkgs:
        let
          unstable = kagari pkgs {
            src = localSource;
            version = "${workspaceVersion}-unstable-${lib.substring 0 8 (self.lastModifiedDate or "19700101")}";
          };
          stablePackages = lib.optionalAttrs (stable != null) {
            stable = kagari pkgs {
              src = fetchTree {
                type = "github";
                owner = "kirikiroid3";
                repo = "kagari";
                inherit (stable) rev;
              };
              inherit (stable) version;
            };
          };
        in
        stablePackages // {
          inherit unstable;
          default = stablePackages.stable or unstable;
        });

      devShells = forAll (pkgs: {
        default = pkgs.mkShell {
          nativeBuildInputs = [ pkgs.pkg-config ];
          # linked at build time: alsa-sys (cpal) and libudev-sys (gilrs)
          buildInputs = [ pkgs.alsa-lib pkgs.udev ] ++ runtimeLibs pkgs;
          LD_LIBRARY_PATH = lib.makeLibraryPath (runtimeLibs pkgs ++ [ pkgs.alsa-lib pkgs.udev ]);
        };
      });
    };
}
