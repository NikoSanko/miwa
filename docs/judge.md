---
type: Specification
id: DOC-JUDGE
title: "意味が合っているか判定する（judge）"
description: "check が見つけた drift と new について、両端が意味的に整合しているかを Jev でスコア化し、助言として報告するコマンドの仕様。"
---

# 意味が合っているか判定する（`judge`）

`check` は、内容が変わったことを機械的に見つけます。ただし、その変更が本当に問題（矛盾）なのかまでは分かりません。例えば doc の言い回しを直しただけなら、req やテストはそのままで構いません。

`miwa judge` は、`check` が見つけた `drift` と `new` について、依存関係の両端が意味的に整合しているかを Jev でスコア化します。スコアがしきい値以上なら `consistent`、未満なら `review` として報告します。`review` になったものが本当に矛盾しているかの判断は、呼び出し元（人や LLM）に任せます。

```
miwa judge
miwa judge --edge doc-req:DOC-AUTH:FR-001
miwa judge --auto-reconcile
```

`judge` は**ローカルでだけ使う機能**です。API キーが必要で、外部サービスにアクセスするため、CI では使いません。CI では `check --gate` を使います。

## 判定の対象

対象は、`check` が `drift` または `new` と判定する依存関係です。doc と req、test と req、要約と本文のいずれの組も、[判定の対象](../req/judge-target-edges.md)になります。要約と本文の対とは、索引の対象になっている doc・req の `desc:doc:<id>` / `desc:req:<id>` エッジのことです（[model](model.md) を参照）。

`--edge` を付けると、指定した依存関係だけに絞れます。対象でない ID を指定するとエラーになります。リンク切れ（`broken`）や不正な状態（`invalid`）に関わる依存関係は、先にソースを直す必要があるため対象になりません。

対象が1件もない場合は、「対象なし」と表示し、[正常終了](../req/judge-exit-codes-and-no-targets.md)します。

## 判定の流れ

`judge` は、依存関係の両端の現在の本文を読み、TypeSafe AI の Jev に「整合しているか」を 0〜1 のスコアで答えさせます。1 に近いほど、整合していることを表します。

1. doc と req の組、test と req の組、要約と本文の組を、1組ずつ Jev に送ります（[リクエストの内容](../req/judge-jev-request.md)）。要約と本文の組では、「`description` は `body` の内容を正しく要約しているか」を尋ねます。この組で送るのは、[正規化した `description` と、フロントマターを除いた本文](../req/desc-edge-hash-sources.md)です。Jev が返すのは数値だけで、文章による理由は返しません。[スコアが取り出せない応答](../req/judge-jev-response-score.md)は、実行エラーとして扱います。
2. [スコアとしきい値による判定](../req/judge-threshold-decides-consistent-or-review.md)では、スコアが設定のしきい値（`judge.threshold`）以上なら `consistent`（整合）、未満なら `review`（見直しが必要）とします。`review` について、矛盾かどうかを判断し、どちらを直すかを決めるのは呼び出し元です。miwa は理由の文章を作りません。

なお、1つの依存関係の[本文が大きすぎる場合](../req/judge-oversize-body-skipped.md)は、Jev に送らず `skipped`（見送り）と表示します。両端の合計が設定 `judge.max_body_bytes`（既定 200,000 バイト）を超えると、大きすぎると判断します。

上限を設けているのは、1本の巨大な依存関係が HTTP エラーになって全体が止まるのを防ぎ、送信コストを抑えるためです。上限を超える本文は、doc や req を小さく保つべきだという設計上の合図でもあります。

## 外部サービスに送られるもの

`judge` が**送る先は TypeSafe AI（`api.typesafe.ai`）だけで、送るのは対象の doc・req・test の本文です**。要約と本文の対では、要約も送ります。社外に出せない内容を含むリポジトリでは、使う前に必ず確認してください。実行を始めるときには、その旨を示す[1行の表示](../req/judge-announces-external-transmission.md)が出ます。

索引配下の doc・req は、他のエッジを持たなくても本文が送られます。索引を有効にすると、初回の `judge` の呼び出し数が、索引対象ノードの数だけ増えます。呼び出し数を抑えたい場合は、`--edge` で絞るか、先に `reconcile --all` で合意しておく運用もできます。

TypeSafe AI の API キーは、設定ファイルには書かず、環境変数 `TYPESAFE_API_KEY` で渡します。環境変数が設定されていない場合は、何も送信せず、[API キー未設定のエラー](../req/judge-api-key-required.md)で終了します。しきい値の設定は [miwa を使い始める](setup.md) を参照してください。

[通信エラーやタイムアウト](../req/judge-jev-http-errors-timeouts-and-redaction.md)が起きると、実行エラーとして止まります。このときのエラーメッセージに、API キーは含まれません。

## 結果の見方

判定結果は、[1依存関係につき1行](../req/judge-output-text.md)で、依存関係の識別子順に表示します。

```
consistent test-req:internal/auth:TestRejectsShortPassword:FR-001 score=0.94
review doc-req:DOC-AUTH:FR-001 score=0.31
skipped doc-req:DOC-BIG:FR-090 score=-
```

- `consistent`: Jev が整合していると判断した
- `review`: しきい値に届かなかった。整合しているかの判断は呼び出し元に任せる
- `skipped`: 本文が大きすぎて判定できなかった

全件の判定が完了すれば、`review` や `skipped` があっても、[終了コードは 0](../req/judge-exit-codes-and-no-targets.md)です。`judge` は結果を報告するコマンドで、CI のゲートではないためです。設定や API キーの問題、通信エラーなどで実行できなかった場合は、終了コード 1 になります。

## 自動で合意する（`--auto-reconcile`）

既定では、`judge` は表示と報告だけで、ロックを書き換えません。

`--auto-reconcile` を付けたときだけ、Jev が `consistent` と判断したものについて、合意（reconcile）まで進みます。ロックへの記録は、全件の判定が終わった後に、[まとめて1回](../req/judge-auto-reconcile-lock-policy.md)で行います。途中でエラーになった場合は、何も書きません。記録には、合意者 `by: jev` と Jev のスコアが残ります。`review` と `skipped` のものは記録しません。人が確認して [`reconcile`](reconcile.md) してください。

## 使い方の例

Jev が使える環境では、`check` で見つけた変更に `judge --auto-reconcile` をかけます。残った `review` は、人がレビューして `reconcile` します。Jev が使えない環境では、`check` の結果を人がレビューして `reconcile` します。これは一例で、ツールが定める手順ではありません。進め方は利用者が決めてください。

## `removed` は対象外

ロックにあるのにソースから消えた依存関係（`removed`）は、`judge` の対象ではありません。消えた理由は意味判定では分からないため、人が判断します。復元するか、`reconcile --edge` で削除を承認してください（詳しくは [合意を記録する](reconcile.md) を参照）。
