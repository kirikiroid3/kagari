---
title: "Kagari のインストール"
---

Kagari は 2 つのプログラムからなります。

- `kagari`：ツールチェインです。プロジェクトの作成、検査、ビルド、整形を行い、言語サーバーとストーリーデバッガを動かします。
- `kagari-desktop-runtime`：ランタイムです。`kagari build` でビルドしたゲームを再生します。ゲームを配布するときにも同梱します。

以下のインストーラーは、この 2 つを `PATH` に置きます。Rust や Cargo は必要ありません。

## Linux {#linux}

x86_64 の Linux で次を実行します。

```sh
curl -fsSL https://kagari-download.suitsu31.club/installer.sh | bash
```

スクリプトは最新版をダウンロードし、チェックサムを確かめてから `~/.local/share/kagari` にインストールし、2 つのプログラムを `~/.local/bin` にリンクします。管理者権限は不要です。`~/.local/bin` が `PATH` に含まれていない場合は、`~/.profile` に追加する行を表示します。追加したら新しいターミナルを開いてください。

Linux 版には glibc 2.35 以降（Ubuntu 22.04、Debian 12、Fedora 36 以降）が必要です。Alpine など musl ベースのディストリビューションには対応していません。ランタイムにはさらに ALSA と libudev、Wayland または X11、GPU ドライバが必要です。ALSA か libudev がなければスクリプトが警告します。[システム要件](../reference/tools/runtime.md#system-requirements)を参照してください。

スクリプトは次のオプションを受け取ります（パイプで渡すときは `bash -s --` の後に書きます）。

| オプション | 意味 |
| ---------- | ---- |
| `--version <VERSION>` | 最新版の代わりにこのバージョンをインストールします。 |
| `--uninstall` | スクリプトがインストールしたものを削除します。 |

`KAGARI_HOME` と `KAGARI_BIN_DIR` で 2 つのディレクトリを変更できます。たとえばバージョン 0.1.0 をインストールするには次のようにします。

```sh
curl -fsSL https://kagari-download.suitsu31.club/installer.sh | bash -s -- --version 0.1.0
```

## Windows {#windows}

インストーラー `kagari-<version>-x86_64-windows-setup.exe` をダウンロードして実行します。PowerShell では、次のコマンドで最新版をダウンロードして起動できます。

```powershell
$v = (Invoke-RestMethod https://kagari-download.suitsu31.club/windows_x86_installer/latest).Trim()
Invoke-WebRequest "https://kagari-download.suitsu31.club/windows_x86_installer/$v/kagari-$v-x86_64-windows-setup.exe" -OutFile kagari-setup.exe
.\kagari-setup.exe
```

インストーラーに管理者権限は不要です。`%LOCALAPPDATA%\Programs\Kagari` にインストールし、その `bin` ディレクトリをユーザーの `PATH` に追加します。反映させるには新しいターミナルを開く（またはエディタを再起動する）必要があります。対応するのは x86_64 の 64 ビット Windows です。

インストーラーとプログラムにはまだコード署名がありません。「Windows によって PC が保護されました」と表示されたら、**詳細情報** を選び、**実行** を選んでください。Kagari が公開したファイルであることを確かめるには、`(Get-FileHash kagari-setup.exe).Hash` を、ダウンロードサーバーでインストーラーの隣にある `.sha256` ファイル（同じ URL の末尾に `.sha256` を付けたもの）と比べます。

## macOS {#macos}

macOS 版はまだありません。

## NixOS {#nixos}

ビルド済みの Linux バイナリは NixOS では動きません。インストーラースクリプトもここを案内します。システムの flake の入力に Kagari の flake を加え、そのパッケージを `environment.systemPackages` に入れます。

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

そのあと `sudo nixos-rebuild switch --flake .#my-machine` で適用します。`stable` は最新のリリース、`unstable` は `main` ブランチの先頭をビルドしたものです。Home Manager を使う場合は、同じパッケージを `home.packages` に入れます。

## インストールの確認 {#checking-the-installation}

```sh
kagari --version
kagari-desktop-runtime --version
```

どちらも同じバージョンを表示します。プロジェクトが受け付けるツールチェインのバージョンは [`[toolchain] kagari`](../reference/tools/projects.md#toolchain) で決まります。`kagari init` はプロジェクトを作成したバージョンを書き込みます。

エディタから Kagari を使うには[エディタ](../reference/tools/editors.md)を参照してください。言語サーバーは `kagari lsp` で、エディタは `PATH` からこれを見つけます。

## 更新とアンインストール {#updating-and-uninstalling}

| システム | 更新 | アンインストール |
| -------- | ---- | ---------------- |
| Linux | インストーラースクリプトをもう一度実行します。 | `--uninstall` を付けて実行します。 |
| Windows | 新しいインストーラーを実行します。インストール済みのバージョンを置き換えます。 | **設定 → アプリ → インストールされているアプリ → Kagari → アンインストール**。 |
| NixOS | `nix flake update kagari` を実行し、`nixos-rebuild switch` します。 | パッケージ（と入力）を削除し、`nixos-rebuild switch` します。 |

自動では更新されません。
