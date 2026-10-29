---
title: "モジュール、アイテム、名前 (E1xxx)"
---

これらのエラーは、モジュールツリーの構築、アイテム宣言の処理、名前・インポート・ラベル・パスの解決で報告されます。型検査より前に報告され、ここで名前が解決できないと後続の型エラーは抑制されることがよくあります。

## E1001 `bad-root` {#e1001}

**エラー。** プログラムのルートファイルの拡張子が `.kis` でも `.kix` でもありません。`kagari check` と `kagari build` はルートファイルを `kagari.toml` の `project.entry` から読み、そこで他の拡張子はすでに拒否されるため、CLI でビルドするプロジェクトでは通常このコードは出ません。

```toml
[project]
entry = "src/main.txt"
```

修正方法: `project.entry` を `.kis` または `.kix` のルートファイル（例: `src/main.kix`）に向けます。

## E1002 `file-not-found` {#e1002}

**エラー。** `mod name;` 宣言に対応するファイルがありません。コンパイラは `name.kis`、`name.kix`、`name/mod.kis`、`name/mod.kix` を探します。

```kis
mod missing;
```

修正方法: 想定されるファイルのいずれか（例: `missing.kis`）を作成するか、宣言を削除します。

## E1003 `ambiguous-file` {#e1003}

**エラー。** `mod name;` 宣言が複数のファイルに一致します。たとえば `twice.kis` と `twice.kix` が両方存在する場合です。

```kis
mod twice; // both `twice.kis` and `twice.kix` exist next to this file
```

修正方法: 候補のファイルを 1 つだけ残し、他は削除または改名します。

## E1004 `file-reused` {#e1004}

**エラー。** すでにあるモジュールとして読み込まれたファイルが、別のモジュールとして再び宣言されています。たとえばルートファイル `main.kis` が `mod main;` で自分自身を宣言する場合です。

```kis
mod main;
```

修正方法: 各ファイルはモジュールとしてちょうど 1 回だけ宣言し、他の場所からは `use` で参照します。

## E1005 `file-module-in-block` {#e1005}

**エラー。** ファイルモジュール（本体のない `mod name;`）が関数やブロックの本体の中で宣言されています。ファイルモジュールはモジュールの最上位でのみ宣言できます。

```kis
fn run() {
    mod helpers;
}
```

修正方法: `mod helpers;` をファイルの最上位に移します。

## E1006 `unknown-attribute` {#e1006}

**エラー。** コンパイラが知らない属性です。

```kis
#[foo]
fn attribute() {}
```

修正方法: 属性を削除するか、`#[scene]` のような既知の属性を使います。

## E1007 `attribute-arguments` {#e1007}

**エラー。** 既知の属性に引数が書かれていますが、Kagari の属性は引数を取りません。

```kis
#[scene(main)]
pushdown fn Scene() {}
```

修正方法: 引数を外します: `#[scene]`。

## E1008 `builtin-outside-std` {#e1008}

**エラー。** 標準ライブラリの外で `#[builtin]` が使われています。コンパイラが提供するアイテムを宣言できるのは `std` だけです。

```kis
#[builtin]
fn x();
```

修正方法: `#[builtin]` を外し、関数に本体を与えます: `fn x() {}`。

## E1009 `builtin-with-body` {#e1009}

**エラー。** `#[builtin]` 関数に本体があるか、`#[builtin]` 構造体がフィールドを宣言しています。ビルトインアイテムはコンパイラが実装するため、本体を持てません。標準ライブラリの外では `E1008` も報告されます。

```kis
#[builtin]
fn len(n: u64) -> u64 {
    0
}
```

修正方法: 本体を削除します: `fn len(n: u64) -> u64;`。

## E1010 `missing-body` {#e1010}

**エラー。** `#[builtin]` でない関数が本体なしで宣言されています。

```kis
fn y();
```

修正方法: 本体を追加します: `fn y() {}`。

## E1011 `missing-value` {#e1011}

**エラー。** `const` に値がないか、`type` エイリアスに型がありません。

```kis
const LIMIT: u64;
```

修正方法: 値を与えます: `const LIMIT: u64 = 10;`。

## E1012 `trait-outside-std` {#e1012}

**エラー。** 標準ライブラリの外でトレイトが宣言されています。ユーザーコードはトレイトを宣言できません。

```kis
trait Shape {
    fn sides(&self) -> u64;
}
```

修正方法: トレイトを削除し、代わりに各型の固有 `impl` メソッドを使います。

## E1013 `trait-not-builtin` {#e1013}

**エラー。** 標準ライブラリで宣言されたトレイトに `#[builtin]` がありません。標準ライブラリのトレイトはすべてコンパイラ提供でなければなりません。

```kis
// in the standard library's `std/mod.kis`
pub trait Eq {}
```

この例は説明用です。標準ライブラリはコンパイラに同梱されており、ユーザーコードではどのトレイトも `E1012` になるため、このコードに到達するのは標準ライブラリを改変した場合だけです。

修正方法: ビルトインにします: `#[builtin] pub trait Eq {}`。

## E1014 `duplicate-definition` {#e1014}

**エラー。** 1 つのモジュールの同じ名前空間で同じ名前が複数回定義されています。注記（note）が最初の定義を指します。

```kis
fn a() {}
fn a() {}
```

修正方法: どちらかの定義を改名または削除します。

## E1015 `unresolved-import` {#e1015}

**エラー。** `use` のパスがどのアイテムにも到達しません。

```kis
use nowhere::x;
```

修正方法: パスを既存のモジュールとアイテムに直すか、インポートを削除します。

## E1016 `unresolved-name` {#e1016}

**エラー。** 値、型、パスのセグメント、または関連アイテムがスコープ内に見つかりません。

```kis
fn unresolved() {
    let _ = nothing;
    let _value: Nothing = 1;
}
```

修正方法: 綴りを直すか、`use` でアイテムをインポートするか、定義します。

## E1017 `private-item` {#e1017}

**エラー。** パスが指すアイテム（または関連アイテム）は存在しますが、`pub` ではなく、使用箇所から見えません。

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

修正方法: アイテムを `pub` にする（`pub const SECRET: u64 = 1;`）か、外部から使うのをやめます。

## E1018 `ambiguous-name` {#e1018}

**エラー。** ある名前が複数のグロブインポートでスコープに入り、それぞれ別のアイテムを指しています。注記が各インポートを指します。

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

修正方法: 意図したアイテムを明示的にインポートする（例: `use a::X;`）か、パスを修飾します: `a::X`。

## E1019 `undeclared-label` {#e1019}

**エラー。** `break` または `continue` が、どの外側のループにも宣言されていないラベルを参照しています。

```kis
fn labels() {
    loop {
        break 'nope;
    }
}
```

修正方法: ループにラベルを付ける（`'nope: loop { ... }`）か、ラベルを外します: `break;`。

## E1020 `outside-loop` {#e1020}

**エラー。** `break` または `continue` がループの外にあります。

```kis
fn labels() {
    break;
    continue;
}
```

修正方法: 文を `loop`、`while`、`for` の中に移すか、`return` を使います。

## E1021 `self-outside-impl` {#e1021}

**エラー。** `Self` 型が `impl` やトレイトの外で使われています。

```kis
fn self_outside() -> Self {}
```

修正方法: `Self` の代わりに具体的な型を書くか、関数を `impl` の中に移します。

## E1022 `bad-impl-target` {#e1022}

**エラー。** 固有 `impl` が、拡張できない型を対象にしています。たとえば標準ライブラリ外でのプリミティブ型です。

```kis
impl bool {}
```

修正方法: プリミティブを独自の構造体で包み、その構造体にメソッドを実装します。

## E1023 `trait-impl-outside-std` {#e1023}

**エラー。** ユーザーコードがトレイトを実装しています。トレイトを実装できるのは標準ライブラリだけです。`Eq` については、すべてのフィールドが `Eq` である構造体と列挙型にコンパイラが自動で導出します。

```kis
struct Square {
    side: u64,
}

impl Eq for Square {}
```

修正方法: トレイトの impl を削除し、メソッドには固有の `impl Square { ... }` を使います。

## E1024 `duplicate-binding` {#e1024}

**エラー。** 1 つのパターンの中で同じ識別子が複数回束縛されています。

```kis
fn patterns(p: (u64, u64)) {
    let (x, x) = p;
}
```

修正方法: 束縛ごとに別の名前を付けます: `let (x, y) = p;`。

## E1025 `inconsistent-binding` {#e1025}

**エラー。** or パターンで、ある変数が一部の選択肢でだけ束縛され、全部では束縛されていません。

```kis
fn patterns(o: Option<u64>) {
    match o {
        Some(a) | None => {}
    }
}
```

修正方法: 選択肢を別々のアームに分けるか、すべての選択肢でその変数を束縛します。

## E1026 `missing-std-item` {#e1026}

**エラー。** コンパイラが依存するアイテム（`std::` 以下の言語型や関数など）が、コンパイル中の標準ライブラリにありません。これはユーザーの誤りではなく、標準ライブラリが壊れているか合っていないことを示します。

```kis
// in the standard library's `std/mod.kis`, with this line deleted:
// #[builtin] pub struct ImageAsset;
#[builtin] pub struct AudioAsset;
```

この例は説明用です。標準ライブラリはコンパイラに同梱されているため、このコードに到達するのは標準ライブラリを改変した場合だけです。

修正方法: 欠けている `std::` アイテムを戻すか、コンパイラに同梱の標準ライブラリを使います。

## E1027 `asset-not-found` {#e1027}

**エラー。** `asset("...")` 呼び出しが、プロジェクトのアセットに存在しないファイルを指しています。

```kis
const MISSING: ImageAsset = asset("missing.png");
```

修正方法: ファイルを追加するかパスを直します。例: `asset("cg_001.png")`。

## E1028 `empty-asset-path` {#e1028}

**エラー。** `asset("...")` または `system_font("...")` 呼び出しに空のパスが渡されています。

```kis
const EMPTY: ImageAsset = asset("");
```

修正方法: 空でないアセットパスを渡します。例: `asset("cg_001.png")`。

## E1029 `asset-outside-asset-dir` {#e1029}

**エラー。** `asset("...")` のパスがアセットディレクトリ（`kagari.toml` の `[assets] dir`。既定ではマニフェストと同じ場所の `assets`）の外に出ているか、`system_font("...")` のパスが絶対パスであるか、結合先のシステムフォントディレクトリの外に出ています。アセットパスが検査されるのはプロジェクトの中だけです。`kagari check` と `kagari build` には `kagari.toml` が必要で、言語サーバーはどのプロジェクトにも属さないファイルについては構文エラーだけを報告します。

```kis
const OUTSIDE: ImageAsset = asset("../secret.png");
```

修正方法: ファイルをアセットディレクトリに移し、そのディレクトリからの相対パスで指定します。例: `asset("bgimage/room.png")`。
システムフォントの場合は、システムフォントディレクトリからの相対パスで指定します。例: `system_font("C:/Windows/Fonts/msgothic.ttc")` ではなく `system_font("msgothic.ttc")`。

## E1030 `system-font-not-found` {#e1030}

**エラー。** `system_font("...")` 呼び出しが、どのシステムフォントディレクトリにもないファイルを指しています。メッセージには探したディレクトリが順に表示されます。Windows では `%LOCALAPPDATA%\Microsoft\Windows\Fonts` と `%WINDIR%\Fonts`、macOS では `~/Library/Fonts`、`/Library/Fonts`、`/System/Library/Fonts`、それ以外では `$XDG_DATA_HOME/fonts`（既定は `~/.local/share/fonts`）、`~/.fonts`、および `$XDG_DATA_DIRS`（既定は `/usr/local/share:/usr/share`）の各要素の下の `fonts` です。存在しないディレクトリは飛ばされます。

```kis
const MINCHO: FontAsset = system_font("missing-mincho.ttc");
```

修正方法: フォントをインストールするか、フォントディレクトリからの相対パスに直す（例: `/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf` なら `system_font("truetype/dejavu/DejaVuSans.ttf")`）か、ファイルをアセットディレクトリにコピーして `asset("…")` を使います。

## E1031 `system-fonts-disabled` {#e1031}

**エラー。** `kagari.toml` の `[assets]` で `system-fonts = false` としたプロジェクトで `system_font("...")` が呼ばれています。

```toml
[assets]
system-fonts = false
```

```kis
const GOTHIC: FontAsset = system_font("msgothic.ttc");
```

修正方法: フォントをアセットディレクトリにコピーして `asset("fonts/msgothic.ttc")` を使うか、設定を削除してシステムフォントを許可します。

## E1032 `reserved-asset-dir` {#e1032}

**エラー。** `asset("...")` のパスが、アセットディレクトリ最上位の `system-fonts` または `std` ディレクトリに入っています。これらの名前はバイトコードとアーカイブがシステムフォントと標準ライブラリのアセット（シェーダー）を置く場所なので、アセットには使えません。

```kis
const GOTHIC: FontAsset = asset("system-fonts/msgothic.ttc");
```

修正方法: ディレクトリを改名します。例: `asset("fonts/msgothic.ttc")`。
