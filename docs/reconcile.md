---
type: Specification
id: DOC-RECONCILE
---

# 合意を記録する（`reconcile`）

`check` が見つけた drift や new は、必ずしも「間違い」ではありません。doc・req・test それぞれの担当者が内容を見比べて「この組み合わせで問題ない」と判断したら、その合意を記録しておく必要があります。この記録が **ロック**（`.miwa/lock.json`）です。

ロックには、依存関係（エッジ）ごとに「合意した時点での両端の内容のハッシュ値」「誰が、いつ、どういう理由で合意したか」が記録されます。次に `check` を実行したときは、この記録と現在の内容を比べて drift の有無を判断します。

ロックファイルがまだ存在しない場合は、依存関係が1つも合意されていない状態として扱われ、正常に実行されます。一方で、[ロックファイルが存在するのに壊れている（読み込めない）場合は、実行エラーとして扱われます](../req/lock-missing-or-malformed.md)。差分レビューをしやすくするため、[ロックの中身は依存関係の識別子順に並べて保存されます](../req/lock-edges-sorted-by-key.md)。

## 1件ずつ合意する

```
miwa reconcile --edge doc-req:DOC-AUTH:FR-001 --reason "最小長の記述が一致"
```

`--edge` に依存関係の識別子を指定すると、[その依存関係の現在の内容のハッシュ値がロックに記録されます](../req/reconcile-edge-records-current-hash.md)。`--reason` で、なぜ問題ないと判断したかを書き残せます（省略も可能です）。

指定した依存関係が、以前はロックにあったけれど今はソースから消えている場合（`removed` の状態）、`reconcile --edge` を実行すると[その記録はロックから削除されます](../req/reconcile-removes-stale-lock-only-edge.md)。

`removed` の依存関係は、[意味判定（`judge`）の対象外](../req/judge-target-edges.md)です。ソースから消えた理由は人が判断する必要があるため、消えたものを復元するか、`reconcile --edge` で削除を承認するかを選んでください（詳しくは [意味判定](judge.md) を参照）。

なお、[リンク切れ（broken）や不正な状態（invalid）に関わる依存関係は reconcile できません](../req/reconcile-rejects-broken-or-invalid.md)。まずソース側（doc・req・test）を直して、壊れた状態を解消してください。

## まとめて合意する

miwa を導入した直後など、すべての依存関係を一括で合意させたい場合は `--all` を使います。

```
miwa reconcile --all
```

[現在ソースから読み取れるすべての依存関係の内容で、ロックをまるごと置き換えます](../req/reconcile-all-atomic-replace.md)。ソースから消えた依存関係の記録は削除され、内容が変わっていない依存関係はそのまま据え置かれます。ただし、対象の中に1件でもリンク切れや不正な状態のものがあれば、ロックはまったく変更されずに失敗します。まず全体を確認してから、問題がなければ一度に書き込む、という流れです。

## 記録される内容

`reconcile` で記録されるエントリの合意者は、常に [`by: human`](../req/reconcile-records-by-human-and-reason.md) です（[`judge --auto-reconcile`](judge.md) で Jev が合意した場合は `by: jev` とスコアが記録されます）。`--reason` を指定すればその内容が記録され、指定しなければ理由は記録されません。

依存関係の識別子（エッジ ID）や、doc・req・test それぞれの書き方については [3層モデル](model.md) を参照してください。
