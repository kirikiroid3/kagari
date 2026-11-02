---
title: "モジュール、アイテム、名前 (E1xxx)"
---

この章のエラーは、モジュールツリーの構築、アイテム宣言の変換、名前・インポート・ラベル・パスの解決の段階で出ます。どれも型検査より前に報告されます。ここで名前が解決できないと、その後に続くはずの型エラーは出ないことがよくあります。

## E1001 `bad-root` {#e1001}

**エラー。** プログラムのルートファイルの拡張子が `.kis` でも `.kix` でもありません。`kagari check` と `kagari build` はルートファイルを `kagari.toml` の `project.entry` から読み、ほかの拡張子はその時点で弾かれます。そのため、CLI でビルドするプロジェクトでは通常このエラーは出ません。

```toml
[project]
entry = "src/main.txt"
```

直し方：`project.entry` に `.kis` か `.kix` のルートファイル（例：`src/main.kix`）を指定します。

## E1002 `file-not-found` {#e1002}

**エラー。** `mod name;` 宣言に対応するファイルが見つかりません。コンパイラは `name.kis`、`name.kix`、`name/mod.kis`、`name/mod.kix` を探します。

```kis
mod missing;
```

直し方：探されるファイルのどれか（例：`missing.kis`）を作るか、宣言を削除します。

## E1003 `ambiguous-file` {#e1003}

**エラー。** `mod name;` 宣言に当てはまるファイルが複数あります。`twice.kis` と `twice.kix` が両方ある場合などです。

```kis
mod twice; // both `twice.kis` and `twice.kix` exist next to this file
```

直し方：候補のファイルを一つだけ残し、ほかは削除するか名前を変えます。

## E1004 `file-reused` {#e1004}

**エラー。** あるモジュールとして読み込み済みのファイルを、別のモジュールとしてもう一度宣言しています。ルートファイル `main.kis` が `mod main;` で自分自身を宣言する場合などです。

```kis
mod main;
```

直し方：各ファイルのモジュール宣言は一度だけにし、ほかの場所からは `use` で参照します。

## E1005 `file-module-in-block` {#e1005}

**エラー。** 関数やブロックの本体の中でファイルモジュール（本体のない `mod name;`）を宣言しています。ファイルモジュールはモジュールの最上位でしか宣言できません。

```kis
fn run() {
    mod helpers;
}
```

直し方：`mod helpers;` をファイルの最上位へ移します。

## E1006 `unknown-attribute` {#e1006}

**エラー。** コンパイラの知らない属性です。

```kis
#[foo]
fn attribute() {}
```

直し方：属性を削除するか、`#[scene]` などの既存の属性を使います。

## E1007 `attribute-arguments` {#e1007}

**エラー。** 既存の属性に引数を書いています。Kagari の属性は引数を取りません。

```kis
#[scene(main)]
pushdown fn Scene() {}
```

直し方：引数を外して `#[scene]` とします。

## E1008 `builtin-outside-std` {#e1008}

**エラー。** 標準ライブラリ以外で `#[builtin]` を使っています。コンパイラが提供するアイテムを宣言できるのは `std` だけです。

```kis
#[builtin]
fn x();
```

直し方：`#[builtin]` を外し、`fn x() {}` のように関数に本体を書きます。

## E1009 `builtin-with-body` {#e1009}

**エラー。** `#[builtin]` 関数に本体があるか、`#[builtin]` 構造体にフィールドがあります。組み込みのアイテムはコンパイラが実装するので、本体を書くことはできません。標準ライブラリ以外では `E1008` も報告されます。

```kis
#[builtin]
fn len(n: u64) -> u64 {
    0
}
```

直し方：本体を削除して `fn len(n: u64) -> u64;` とします。

## E1010 `missing-body` {#e1010}

**エラー。** `#[builtin]` ではない関数を本体なしで宣言しています。

```kis
fn y();
```

直し方：`fn y() {}` のように本体を書きます。

## E1011 `missing-value` {#e1011}

**エラー。** `const` に値がないか、`type` エイリアスに型がありません。

```kis
const LIMIT: u64;
```

直し方：`const LIMIT: u64 = 10;` のように値を書きます。

## E1012 `trait-outside-std` {#e1012}

**エラー。** 標準ライブラリ以外でトレイトを宣言しています。ユーザーのコードではトレイトを宣言できません。

```kis
trait Shape {
    fn sides(&self) -> u64;
}
```

直し方：トレイトを削除し、代わりに各型の固有 `impl` にメソッドを書きます。

## E1013 `trait-not-builtin` {#e1013}

**エラー。** 標準ライブラリで宣言したトレイトに `#[builtin]` が付いていません。標準ライブラリのトレイトは、すべてコンパイラが提供するものでなければなりません。

```kis
// in the standard library's `std/mod.kis`
pub trait Eq {}
```

これは説明のための例です。標準ライブラリはコンパイラに同梱されており、ユーザーのコードではどのトレイトも `E1012` になります。このエラーが出るのは、標準ライブラリを書き換えた場合だけです。

直し方：`#[builtin] pub trait Eq {}` のように組み込みにします。

## E1014 `duplicate-definition` {#e1014}

**エラー。** 同じモジュールの同じ名前空間で、一つの名前を複数回定義しています。注記（note）で最初の定義の位置を示します。

```kis
fn a() {}
fn a() {}
```

直し方：どちらかの定義の名前を変えるか、削除します。

## E1015 `unresolved-import` {#e1015}

**エラー。** `use` のパスが、どのアイテムも指していません。

```kis
use nowhere::x;
```

直し方：パスを実在するモジュールとアイテムに直すか、インポートを削除します。

## E1016 `unresolved-name` {#e1016}

**エラー。** 値、型、パスの一部、または関連アイテムがスコープ内に見つかりません。

```kis
fn unresolved() {
    let _ = nothing;
    let _value: Nothing = 1;
}
```

直し方：綴りを直すか、`use` でアイテムをインポートするか、アイテムを定義します。

## E1017 `private-item` {#e1017}

**エラー。** パスが指すアイテム（または関連アイテム）は存在しますが、`pub` ではないため、使っている場所からは見えません。

```kis
mod shapes {
    pub struct Dot {}

    impl Dot {
        const SECRET: u64 = 1;
    }
}

fn assoc_uses() -> u64 {
    shapes::Dot::SECRET
}
```

直し方：アイテムを `pub` にする（`pub const SECRET: u64 = 1;`）か、外から使うのをやめます。

## E1018 `ambiguous-name` {#e1018}

**エラー。** 複数のグロブインポートが同じ名前をスコープに持ち込み、それぞれ別のアイテムを指しています。注記で各インポートの位置を示します。

```kis
mod a {
    pub const X: u64 = 1;
}
mod b {
    pub const X: u64 = 2;
}
use a::*;
use b::*;

fn uses() -> u64 {
    X
}
```

直し方：使いたいアイテムを `use a::X;` のように明示的にインポートするか、`a::X` とパスで修飾します。

## E1019 `undeclared-label` {#e1019}

**エラー。** `break` や `continue` が、外側のどのループにも付いていないラベルを指しています。

```kis
fn labels() {
    loop {
        break 'nope;
    }
}
```

直し方：ループにラベルを付ける（`'nope: loop { ... }`）か、ラベルを外して `break;` とします。

## E1020 `outside-loop` {#e1020}

**エラー。** ループの外に `break` や `continue` があります。

```kis
fn labels() {
    break;
    continue;
}
```

直し方：文を `loop`、`while`、`for` の中へ移すか、`return` を使います。

## E1021 `self-outside-impl` {#e1021}

**エラー。** `impl` やトレイトの外で `Self` 型を使っています。

```kis
fn self_outside() -> Self {}
```

直し方：`Self` の代わりに具体的な型を書くか、関数を `impl` の中へ移します。

## E1022 `bad-impl-target` {#e1022}

**エラー。** 固有 `impl` の対象が、拡張できない型になっています。標準ライブラリ以外でプリミティブ型を対象にした場合などです。

```kis
impl bool {}
```

直し方：プリミティブ型を独自の構造体で包み、その構造体にメソッドを実装します。

## E1023 `trait-impl-outside-std` {#e1023}

**エラー。** ユーザーのコードでトレイトを実装しています。トレイトを実装できるのは標準ライブラリだけです。`Eq` は、すべてのフィールドが `Eq` である構造体と列挙型に対して、コンパイラが自動で導出します。

```kis
struct Square {
    side: u64,
}

impl Eq for Square {}
```

直し方：トレイトの impl を削除し、メソッドは固有の `impl Square { ... }` に書きます。

## E1024 `duplicate-binding` {#e1024}

**エラー。** 一つのパターンの中で、同じ識別子を複数回束縛しています。

```kis
fn patterns(p: (u64, u64)) {
    let (x, x) = p;
}
```

直し方：`let (x, y) = p;` のように、束縛ごとに別の名前を付けます。

## E1025 `inconsistent-binding` {#e1025}

**エラー。** or パターンで、一部の選択肢でしか束縛されていない変数があります。

```kis
fn patterns(o: Option<u64>) {
    match o {
        Some(a) | None => {}
    }
}
```

直し方：選択肢を別々のアームに分けるか、すべての選択肢でその変数を束縛します。

## E1026 `missing-std-item` {#e1026}

**エラー。** コンパイラが前提とするアイテム（`std::` 以下の言語型や関数など）が、コンパイル中の標準ライブラリにありません。ユーザーの誤りではなく、標準ライブラリが壊れているか、コンパイラと合っていないことを示します。

```kis
// in the standard library's `std/mod.kis`, with this line deleted:
// #[builtin] pub struct ImageAsset;
#[builtin] pub struct AudioAsset;
```

これは説明のための例です。標準ライブラリはコンパイラに同梱されているので、このエラーが出るのは標準ライブラリを書き換えた場合だけです。

直し方：欠けている `std::` アイテムを元に戻すか、コンパイラに同梱の標準ライブラリを使います。

## E1027 `asset-not-found` {#e1027}

**エラー。** `asset("...")` の呼び出しで指定したファイルが、プロジェクトのアセットにありません。

```kis
const MISSING: ImageAsset = asset("missing.png");
```

直し方：ファイルを追加するか、パスを直します（例：`asset("cg_001.png")`）。

## E1028 `empty-asset-path` {#e1028}

**エラー。** `asset("...")` や `system_font("...")` の呼び出しに空のパスを渡しています。

```kis
const EMPTY: ImageAsset = asset("");
```

直し方：空でないアセットパスを渡します（例：`asset("cg_001.png")`）。

## E1029 `asset-outside-asset-dir` {#e1029}

**エラー。** `asset("...")` のパスがアセットディレクトリ（`kagari.toml` の `[assets] dir`。既定はマニフェストと同じ場所の `assets`）の外を指しているか、`system_font("...")` のパスが絶対パスになっているか、つなげた先のシステムフォントディレクトリの外を指しています。アセットパスを検査するのはプロジェクトの中だけです。`kagari check` と `kagari build` には `kagari.toml` が必要で、どのプロジェクトにも属さないファイルについては、言語サーバーは構文エラーしか報告しません。

```kis
const OUTSIDE: ImageAsset = asset("../secret.png");
```

直し方：ファイルをアセットディレクトリへ移し、そのディレクトリからの相対パスで指定します（例：`asset("bgimage/room.png")`）。
システムフォントなら、システムフォントディレクトリからの相対パスで指定します。たとえば `system_font("C:/Windows/Fonts/msgothic.ttc")` ではなく `system_font("msgothic.ttc")` と書きます。

## E1030 `system-font-not-found` {#e1030}

**エラー。** `system_font("...")` の呼び出しで指定したファイルが、どのシステムフォントディレクトリにもありません。メッセージには、探したディレクトリが順に表示されます。Windows では `%LOCALAPPDATA%\Microsoft\Windows\Fonts` と `%WINDIR%\Fonts`、macOS では `~/Library/Fonts`、`/Library/Fonts`、`/System/Library/Fonts`、それ以外では `$XDG_DATA_HOME/fonts`（既定は `~/.local/share/fonts`）、`~/.fonts`、`$XDG_DATA_DIRS`（既定は `/usr/local/share:/usr/share`）の各ディレクトリ内の `fonts` です。存在しないディレクトリは飛ばします。

```kis
const MINCHO: FontAsset = system_font("missing-mincho.ttc");
```

直し方：フォントをインストールするか、フォントディレクトリからの相対パスに直すか（`/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf` なら `system_font("truetype/dejavu/DejaVuSans.ttf")`）、ファイルをアセットディレクトリにコピーして `asset("…")` を使います。

## E1031 `system-fonts-disabled` {#e1031}

**エラー。** `kagari.toml` の `[assets]` に `system-fonts = false` を指定したプロジェクトで、`system_font("...")` を呼んでいます。

```toml
[assets]
system-fonts = false
```

```kis
const GOTHIC: FontAsset = system_font("msgothic.ttc");
```

直し方：フォントをアセットディレクトリにコピーして `asset("fonts/msgothic.ttc")` を使うか、この設定を削除してシステムフォントを使えるようにします。

## E1032 `reserved-asset-dir` {#e1032}

**エラー。** `asset("...")` のパスが、アセットディレクトリ直下の `system-fonts` か `std` ディレクトリの中を指しています。この二つは、バイトコードやアーカイブがシステムフォントと標準ライブラリのアセット（シェーダー）を置く場所なので、アセットには使えません。

```kis
const GOTHIC: FontAsset = asset("system-fonts/msgothic.ttc");
```

直し方：ディレクトリの名前を変えます（例：`asset("fonts/msgothic.ttc")`）。
