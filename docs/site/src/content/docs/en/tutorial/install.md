---
title: "Installing Kagari"
---

Kagari is two programs:

- `kagari`, the toolchain: it creates, checks, builds and formats projects, and runs the
  language server and the story debugger.
- `kagari-desktop-runtime`, the runtime: it plays a game that `kagari build` built. You also ship
  it with your game.

The installers below put both on your `PATH`. You do not need Rust or Cargo.

## Linux {#linux}

On an x86_64 Linux, run:

```sh
curl -fsSL https://kagari-download.suitsu31.club/installer.sh | bash
```

The script downloads the latest version, checks its checksum, installs it into
`~/.local/share/kagari` and links both programs into `~/.local/bin`. It needs no administrator
rights. When `~/.local/bin` is not on your `PATH`, it prints the line to add to `~/.profile`; open
a new terminal afterwards.

The Linux build needs glibc 2.35 or newer (Ubuntu 22.04, Debian 12, Fedora 36 and later);
musl-based distributions such as Alpine are not supported. The runtime also needs ALSA and
libudev, Wayland or X11, and a GPU driver; the script warns when ALSA or libudev is missing. See
[System requirements](../reference/tools/runtime.md#system-requirements).

The script takes these options (after `bash -s --` when piped):

| Option | Meaning |
| ------ | ------- |
| `--version <VERSION>` | Install this version instead of the latest one. |
| `--uninstall` | Remove what the script installed. |

`KAGARI_HOME` and `KAGARI_BIN_DIR` change the two directories. For example, to install version
0.1.0:

```sh
curl -fsSL https://kagari-download.suitsu31.club/installer.sh | bash -s -- --version 0.1.0
```

## Windows {#windows}

Download and run the installer `kagari-<version>-x86_64-windows-setup.exe`. In PowerShell, this
downloads the latest one and starts it:

```powershell
$v = (Invoke-RestMethod https://kagari-download.suitsu31.club/windows_x86_installer/latest).Trim()
Invoke-WebRequest "https://kagari-download.suitsu31.club/windows_x86_installer/$v/kagari-$v-x86_64-windows-setup.exe" -OutFile kagari-setup.exe
.\kagari-setup.exe
```

The installer needs no administrator rights. It installs into `%LOCALAPPDATA%\Programs\Kagari`
and adds its `bin` directory to your user `PATH`; open a new terminal (or restart your editor) to
see it. It supports 64-bit Windows on x86_64.

The installer and the programs are not code-signed yet. When Windows shows "Windows protected your
PC", choose **More info**, then **Run anyway**. To check that the file is the one Kagari
published, compare `(Get-FileHash kagari-setup.exe).Hash` with the `.sha256` file next to the
installer on the download server (same URL plus `.sha256`).

## macOS {#macos}

There is no macOS build yet.

## NixOS {#nixos}

The prebuilt Linux binaries do not run on NixOS; the installer script points you here. Add the
Kagari flake as an input of your system flake and put its package in
`environment.systemPackages`:

```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    kagari.url = "github:kirikiroid3/kagari";
  };

  outputs = { nixpkgs, kagari, ... }: {
    nixosConfigurations.my-machine = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        ./configuration.nix
        ({ pkgs, ... }: {
          environment.systemPackages = [
            kagari.packages.${pkgs.stdenv.hostPlatform.system}.stable
          ];
        })
      ];
    };
  };
}
```

Then apply it with `sudo nixos-rebuild switch --flake .#my-machine`. `stable` is the latest
release; `unstable` builds the head of the `main` branch. With Home Manager, put the same package
in `home.packages` instead.

## Checking the installation {#checking-the-installation}

```sh
kagari --version
kagari-desktop-runtime --version
```

Both print the same version. A project accepts only the toolchain versions in its
[`[toolchain] kagari`](../reference/tools/projects.md#toolchain); `kagari init` writes the version
that created the project.

To use Kagari from an editor, see [Editors](../reference/tools/editors.md): the language server
is `kagari lsp`, which the editor finds on your `PATH`.

## Updating and uninstalling {#updating-and-uninstalling}

| System | Update | Uninstall |
| ------ | ------ | --------- |
| Linux | Run the installer script again. | Run it with `--uninstall`. |
| Windows | Run a newer installer; it replaces the installed version. | **Settings → Apps → Installed apps → Kagari → Uninstall**. |
| NixOS | `nix flake update kagari`, then `nixos-rebuild switch` | Remove the package (and the input), then `nixos-rebuild switch`. |

Nothing updates by itself.
