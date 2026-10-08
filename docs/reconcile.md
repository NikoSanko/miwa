---
type: Specification
id: DOC-RECONCILE
title: "合意を記録する（reconcile）"
description: "check が見つけた drift や new を人が確認して合意した結果を、ロックに記録するコマンドの仕様。"
---

# 合意を記録する（`reconcile`）

`check` が見つけた drift や new は、必ずしも「間違い」ではありません。doc・req・test それぞれの担当者が内容を見比べて「この組み合わせで問題ない」と判断したら、その合意を記録しておく必要があります。この記録が「ロック」（`.miwa/lock.json`）です。

ロックには、依存関係（エッジ）ごとに、合意した時点での両端の内容のハッシュ値と、誰がいつどういう理由で合意したかが記録されます。次に `check` を実行すると、miwa はこの記録と現在の内容を比べて、drift の有無を判断します。

ロックファイルがまだ存在しない場合は、依存関係が1つも合意されていない状態として扱われ、正常に実行されます。一方、ロックファイルが存在するのに壊れている（読み込めない）場合は、実行エラーとして扱われます（[ロックの欠落と破損](../req/lock-missing-or-malformed.md)）。差分レビューをしやすくするため、ロックの中身は依存関係の識別子順に並べて保存されます（[ロックの並び順](../req/lock-edges-sorted-by-key.md)）。

## 1件ずつ合意する

```
miwa reconcile --edge doc-req:DOC-AUTH:FR-001 --reason "最小長の記述が一致"
```

`--edge` に依存関係の識別子を指定すると、その依存関係の現在の内容のハッシュ値がロックに記録されます（[記録されるハッシュ値](../req/reconcile-edge-records-current-hash.md)）。`--reason` を指定すると、なぜ問題ないと判断したかを書き残せます。省略も可能です。

以前はロックにあったのに今はソースから消えている依存関係（`removed` の状態）を指定して `reconcile --edge` を実行すると、その記録はロックから削除されます（[消えた依存関係の扱い](../req/reconcile-removes-stale-lock-only-edge.md)）。

`removed` の依存関係は、意味判定（`judge`）の対象外です（[judge の対象](../req/judge-target-edges.md)）。ソースから消えた理由は人が判断する必要があります。そのため、消えたものを復元するか、`reconcile --edge` で削除を承認するかを選んでください。詳しくは [意味判定](judge.md) を参照してください。

リンク切れ（broken）や不正な状態（invalid）に関わる依存関係は、reconcile できません（[reconcile できない状態](../req/reconcile-rejects-broken-or-invalid.md)）。まずソース側（doc・req・test）を直して、壊れた状態を解消してください。

要約と本文の対（`desc:doc:<id>` / `desc:req:<id>`）も、他のエッジと同じように合意できます（[desc エッジの規則](../req/desc-edge-exists-for-indexed-nodes.md)）。`--edge desc:doc:DOC-AUTH` なら1件ずつ、`--all` ならまとめて合意します。リンク切れや不正な状態に関わる doc の対は、`doc-req` と同様に拒否されます。

## まとめて合意する

miwa を導入した直後など、すべての依存関係を一括で合意させたい場合は `--all` を使います。

```
miwa reconcile --all
```

現在ソースから読み取れるすべての依存関係の内容で、ロックをまるごと置き換えます（[ロックの置き換え](../req/reconcile-all-atomic-replace.md)）。ソースから消えた依存関係の記録は削除され、内容が変わっていない依存関係はそのまま据え置かれます。ただし、対象の中に1件でもリンク切れや不正な状態のものがあれば、ロックはまったく変更されずに失敗します。処理は、まず全体を確認し、問題がなければ一度に書き込む流れです。

## 記録される内容

`reconcile` が記録するエントリの合意者は、常に [`by: human`](../req/reconcile-records-by-human-and-reason.md) です。[`judge --auto-reconcile`](judge.md) で Jev が合意した場合は、`by: jev` とスコアが記録されます。`--reason` を指定するとその内容が記録され、指定しなければ理由は記録されません。

依存関係の識別子（エッジ ID）や、doc・req・test それぞれの書き方については [3層モデル](model.md) を参照してください。

索引の乱れ（`check` の `index` 種別）はロックと関係がないため、`index` の finding が残っていても `reconcile` は成功します（[索引の finding と reconcile](../req/index-findings-do-not-block-reconcile.md)）。
