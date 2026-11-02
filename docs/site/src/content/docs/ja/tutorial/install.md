---
title: "Kagari のインストール"
---

Kagari は次の 2 つのプログラムで構成されています。

- `kagari`：ツールチェインです。プロジェクトの作成、検査、ビルド、整形を行うほか、言語サーバーとストーリーデバッガーも動かします。
- `kagari-desktop-runtime`：ランタイムです。`kagari build` でビルドしたゲームを再生します。ゲームを配布するときにも同梱します。

以下のインストーラーを使うと、どちらも `PATH` から使えるようになります。Rust や Cargo は必要ありません。

## Linux {#linux}

x86_64 の Linux では次を実行します。

```sh
curl -fsSL https://kagari-download.suitsu31.club/installer.sh | bash
```

スクリプトは最新版をダウンロードし、チェックサムを確かめたうえで `~/.local/share/kagari` にインストールします。ランタイムがムービー再生に使う FFmpeg のライブラリも、プログラムと同じ場所に置きます。そのあと 2 つのプログラムへのリンクを `~/.local/bin` に作ります。管理者権限は要りません。`~/.local/bin` が `PATH` に入っていなければ、`~/.profile` に追記する行を表示するので、追記してから新しいターミナルを開いてください。

Linux 版は glibc 2.35 以降（Ubuntu 22.04、Debian 12、Fedora 36 以降）で動きます。Alpine など musl ベースのディストリビューションには対応していません。ランタイムを動かすには、さらに ALSA、libudev、libva（Debian と Ubuntu では `libasound2 libudev1 libva2 libva-drm2`、Fedora では `alsa-lib systemd-libs libva`）、Wayland または X11、GPU ドライバーが必要です。これらのライブラリが欠けていると、スクリプトが警告を出します。詳しくは [システム要件](../reference/tools/runtime.md#system-requirements) を参照してください。

スクリプトには次のオプションを指定できます（パイプで渡すときは `bash -s --` の後ろに書きます）。

| オプション | 意味 |
| ---------- | ---- |
| `--version <VERSION>` | 最新版ではなく、指定したバージョンをインストールします。 |
| `--uninstall` | スクリプトでインストールしたものを削除します。 |

インストール先の 2 つのディレクトリは `KAGARI_HOME` と `KAGARI_BIN_DIR` で変えられます。たとえばバージョン 0.1.0 を入れるには次のようにします。

```sh
curl -fsSL https://kagari-download.suitsu31.club/installer.sh | bash -s -- --version 0.1.0
```

## Windows {#windows}

インストーラー `kagari-<version>-x86_64-windows-setup.exe` をダウンロードして実行します。PowerShell なら、次のコマンドで最新版をダウンロードして起動できます。

```powershell
$v = (Invoke-RestMethod https://kagari-download.suitsu31.club/windows_x86_installer/latest).Trim()
Invoke-WebRequest "https://kagari-download.suitsu31.club/windows_x86_installer/$v/kagari-$v-x86_64-windows-setup.exe" -OutFile kagari-setup.exe
.\kagari-setup.exe
```

インストール先は `%LOCALAPPDATA%\Programs\Kagari` で、その中の `bin` ディレクトリがユーザーの `PATH` に追加されます。新しいターミナルを開く（またはエディタを再起動する）と反映されます。対応しているのは x86_64 の 64 ビット版 Windows です。

Kagari のプログラムを動かすには Microsoft Visual C++ 再頒布可能パッケージ（x64）が必要です。入っていないか古い場合は、インストーラーが同梱のパッケージを先にインストールします。Windows が管理者権限を求めるのはこのときだけで、拒否するとインストールは中止されます。すでに入っていれば管理者権限は要りません。

インストーラーとプログラムには、まだコード署名がありません。「Windows によって PC が保護されました」と表示されたら、**詳細情報** を押してから **実行** を押してください。Kagari が公開したファイルかどうかを確かめるには、`(Get-FileHash kagari-setup.exe).Hash` の結果を、ダウンロードサーバー上でインストーラーと並んでいる `.sha256` ファイル（同じ URL の末尾に `.sha256` を付けたもの）と照合します。

## macOS {#macos}

Apple シリコン（M1 以降）搭載で macOS 11 以降の Mac では、Linux と同じスクリプトを実行します。

```sh
curl -fsSL https://kagari-download.suitsu31.club/installer.sh | bash
```

インストールの流れも Linux と同じです。`~/.local/share/kagari` にインストールし（FFmpeg のライブラリもプログラムと同じ場所に置きます）、2 つのプログラムへのリンクを `~/.local/bin` に作ります。管理者権限は要りません。`~/.local/bin` が `PATH` に入っていなければ `~/.zprofile` に追記する行を表示するので、追記してから新しいターミナルを開いてください。オプションと環境変数も [Linux](#linux) と共通です。macOS 版が用意されているのは 0.3.0 からで、それより古いバージョンを指定するとスクリプトが拒否します。

macOS 版では、ほかのライブラリは必要ありません。FFmpeg も同梱されています。Intel Mac には対応しておらず、スクリプトはその時点で止まります。スクリプトでインストールしたプログラムは、セキュリティの確認ダイアログなしで起動します。Mac のプレイヤー向けにゲームをパッケージにする方法は [`kagari release`](../reference/tools/cli.md#kagari-release) を参照してください。

## NixOS {#nixos}

ビルド済みの Linux 版バイナリは NixOS では動きません。インストーラースクリプトを実行しても、この節を案内します。システムの flake の入力に Kagari の flake を追加し、そのパッケージを `environment.systemPackages` に入れてください。

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

あとは `sudo nixos-rebuild switch --flake .#my-machine` で適用します。`stable` は最新リリース、`unstable` は `main` ブランチの最新コミットをビルドしたものです。Home Manager を使っている場合は、同じパッケージを `home.packages` に入れます。

## インストールの確認 {#checking-the-installation}

```sh
kagari --version
kagari-desktop-runtime --version
```

どちらも同じバージョンが表示されれば成功です。プロジェクトで使えるツールチェインのバージョンは [`[toolchain] kagari`](../reference/tools/projects.md#toolchain) で決まり、`kagari init` はプロジェクトを作ったときのバージョンをここに書き込みます。

エディタで Kagari を使う方法は [エディタ対応](../reference/tools/editors.md) を参照してください。言語サーバーは `kagari lsp` で、エディタは `PATH` からこれを見つけます。

## 更新とアンインストール {#updating-and-uninstalling}

| システム | 更新 | アンインストール |
| -------- | ---- | ---------------- |
| Linux、macOS | インストーラースクリプトをもう一度実行 | `--uninstall` を付けて実行 |
| Windows | 新しいインストーラーを実行（インストール済みのバージョンを置き換えます） | **設定 → アプリ → インストールされているアプリ → Kagari → アンインストール** |
| NixOS | `nix flake update kagari` のあと `nixos-rebuild switch` | パッケージ（と flake の入力）を削除してから `nixos-rebuild switch` |

自動更新はありません。
