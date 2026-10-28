{
  description = "Kagari dev shell: system libraries only; Rust comes from rustup (rust-toolchain.toml)";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAll = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in {
      devShells = forAll (pkgs:
        let
          # dlopen'ed at run time by winit, wgpu (Vulkan / EGL) and xkbcommon-dl
          runtimeLibs = with pkgs; [ wayland libxkbcommon vulkan-loader libGL libx11 libxcursor libxi libxrandr ];
        in {
          default = pkgs.mkShell {
            nativeBuildInputs = [ pkgs.pkg-config ];
            # linked at build time: alsa-sys (cpal) and libudev-sys (gilrs)
            buildInputs = [ pkgs.alsa-lib pkgs.udev ] ++ runtimeLibs;
            LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (runtimeLibs ++ [ pkgs.alsa-lib pkgs.udev ]);
          };
        });
    };
}
