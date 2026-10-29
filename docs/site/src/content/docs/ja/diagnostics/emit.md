---
title: "出力 (E7xxx)"
sidebar:
  label: "バイトコード生成 (E7xxx)"
---

`kagari build` の最後の工程では、検査済みのプログラムを `.kagari` ファイルに書き出し、検証します。この工程は、それ以前の検査でエラーがなかったプログラムに対してのみ実行されます。

## E7001 `missing-main` {#e7001}

**エラー。** ルートモジュールに `fn main()` がないため、エンジンは起動時に実行するものがありません。`main` はルートファイルにある通常の `fn` で、ゲームシステムを記述します。

```kix
// main.kix: a root file with a story but no `fn main()`
#[entry]
#[scene]
pushdown fn Start() {
    s!(t!["Hello."]);
}
```

修正: ゲームの `System` を返す `fn main() -> System { … }` をルートファイルに追加します。

## E7002 `function-too-large` {#e7002}

**エラー。** 関数が 65 535 を超えるレジスターを必要とするか、`.kagari` ファイルで扱えるより多くのコードワードを必要とします。関数のローカル変数と一時値はそれぞれ 1 つのレジスターを使うため、これは巨大な自動生成関数でのみ起こります。

```kis
// A function that keeps more than 65 535 locals alive at once.
pub fn huge() -> u32 {
    let a0 = 0u32;
    let a1 = 1u32;
    // … up to a65999 …
    let mut s = 0u32;
    s = s + a0;
    s = s + a1;
    // … up to a65999 …
    s
}
```

この例は省略しています。実際に発生させるには 66 000 個の `let` をすべて書き出し、それぞれを後で使う必要があります。使われないローカル変数はレジスターを使いません。

修正: 関数をいくつかの小さな関数に分割します。

## E7003 `invalid-bytecode` {#e7003}

**エラー。** コンパイラの内部エラーです。出力された `.kagari` ファイルが読み込み時の検証に失敗しました。メッセージには検証器の指摘が含まれます。正しいプログラムからこのエラーが出ることはありません。

```text
error[E7003]: internal compiler error: the emitted bytecode fails verification: …
```

この例は説明用です。正しいコンパイラはどのプログラムからもこのエラーを出さないため、メッセージの形だけを示しています。

修正: このエラーを引き起こすプログラムを添えてバグを報告してください。
