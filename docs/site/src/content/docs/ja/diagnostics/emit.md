---
title: "出力 (E7xxx)"
sidebar:
  label: "バイトコード生成 (E7xxx)"
---

`kagari build` は最後に、検査を通ったプログラムを `.kagari` ファイルに書き出して検証します。この工程に進むのは、それまでの検査でエラーが出なかったプログラムだけです。

## E7001 `missing-main` {#e7001}

**エラー。** ルートモジュールに `fn main()` がありません。これではエンジンが起動時に実行するものがありません。`main` はルートファイルに置く通常の `fn` で、ゲームシステムを記述します。

```kix
// main.kix: a root file with a story but no `fn main()`
#[entry]
#[scene]
pushdown fn Start() {
    s!(t!["Hello."]);
}
```

直し方：ゲームの `System` を返す `fn main() -> System { … }` をルートファイルに追加します。

## E7002 `function-too-large` {#e7002}

**エラー。** 関数に必要なレジスターが 65 535 個を超えたか、コードワード数がバイトコードで扱える上限を超えました。ローカル変数や一時値は一つにつきレジスターを一つ使うので、このエラーが出るのは自動生成された巨大な関数くらいです。

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

この例は途中を省いています。実際にエラーを出すには、66 000 個の `let` をすべて書き、それぞれを後で使う必要があります。使われないローカル変数はレジスターを消費しません。

直し方：関数をいくつかの小さな関数に分けます。

## E7003 `invalid-bytecode` {#e7003}

**エラー。** コンパイラの内部エラーです。生成したバイトコードが、読み込み時の検証に通りませんでした。メッセージには検証器が指摘した内容が入ります。正しいプログラムからこのエラーが出ることはありません。

```text
error[E7003]: internal compiler error: the emitted bytecode fails verification: …
```

この例は説明のためのものです。正しいコンパイラはどんなプログラムからもこのエラーを出さないので、メッセージの形だけを示しています。

直し方：このエラーが出るプログラムを添えて、バグとして報告してください。
