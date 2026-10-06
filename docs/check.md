---
type: Specification
id: DOC-CHECK
title: "更新漏れを検知する（check）"
description: "doc・req・test の追随漏れや索引の乱れを、ロックとの比較で機械的に見つけて finding として報告するコマンドの仕様。"
---

# 更新漏れを検知する（`check`）

`miwa check` は、doc・req・test の3層のうち、どこかが更新されたのに他が追随していない状態を機械的に見つけるコマンドです。実装コードそのものは扱わず、「ドキュメントとテストの追随」だけを見ます。

## 報告される内容（finding）

`check` は、次の種類の finding を報告します。

- **drift（ドリフト）**: 前回合意した内容から、依存関係の片側または両側が変わっている状態。doc と req をつなぐ関係も、test と req をつなぐ関係も対象です。[doc・req・test のいずれか一方でも内容が変わっていれば drift として報告](../req/finding-drift-either-side-changed.md)されます。
  - doc だけが変わった場合は、req が追随できていない可能性があります。
  - req だけが変わった場合は、doc や test が追随できていない可能性があります。
  - test だけが変わった場合は、test が req の範囲に収まっているかを確認してください。
  - `index` を設定していると、索引の対象になっている doc・req について、要約（`description`）と本文の対も依存関係（`desc:doc:<id>` / `desc:req:<id>` エッジ）として見ます。[理由は `DescriptionChanged`（要約だけが変わった）、`BodyChanged`（本文だけが変わった）、`BothChanged`（両方）](../req/desc-edge-drift-reasons.md)です。要約が本文に追随しているかを確かめる合図になります。
  - **本文を 1 回編集すると 2 件の finding が出ます**。doc の本文を編集すると、その doc の `doc-req` エッジ（リンクしている req の本数分）の `DocChanged` と、`desc` エッジの `BodyChanged` が別々に出ます（索引配下の req なら `ReqChanged` と `BodyChanged`）。意図した動作で、要約はそのままでよいかを確かめたうえで、それぞれ [`reconcile`](reconcile.md) します。`description` を消すと、`index` の `MissingDescription` と `desc` の `drift`（`DescriptionChanged`。ロックに記録された要約が空でなかった場合）の 2 件が出ます。
- **new / removed**: [前回合意した記録（ロック）にない新しい依存関係は new、逆にロックにあったのにソースから消えた依存関係は removed](../req/finding-new-and-removed.md)として報告されます。
- **orphan（孤立）**: [どの doc からもリンクされていない req、どの req にもリンクしていない doc](../req/finding-orphan.md)を報告します。書いたのに使われていない req や、参照が漏れている doc を見つけられます。
- **no-test**: [1件もテストで検証されていない req](../req/finding-no-test.md)を報告します。免除の仕組みはなく、すべての req が対象です。
- **invalid**: id の重複・欠落、テスト関数に複数のマーカーが付いている、テストファイルの構文が壊れている、といった不正な状態です。[テスト関数に2件以上のマーカーが付いている場合や、テストファイルが構文エラーで解析できない場合も invalid として報告](../req/test-invalid-conditions.md)されます（Go・Rust・C#・Java・Kotlin・TypeScript / JavaScript 共通）。
  - マーカーを持つ test の識別子が重複している場合（C# や Java のオーバーロード、Rust の `#[cfg]` 分岐による同名関数、TypeScript / JavaScript の同名 test など）は、invalid ではなく[出現をまとめて 1 つの test として扱われます](../req/test-duplicate-identifier-merged.md)。invalid になるのは、まとめようとした出現のマーカーが異なる req を指している場合だけで、その識別子のすべての出現が invalid となり、依存関係は作られません。同じ識別子にマーカーが複数ある不正な出現が残っている間は、まとめた test の依存関係も reconcile や judge の対象になりません。先に不正な出現を直してください。
- **broken（壊れたリンク）**: doc から参照している req が存在しない、または req でないファイルを指している状態です（詳細は [モデル](model.md) を参照）。
- **index（索引の乱れ）**: [索引（`index.md`）が文書と食い違っている状態](index-command.md)です。`.miwa/config.yaml` の `index` を設定したときだけ検査されます。reason は次の 9 種類で、すべて `target_kind` は `Index` です。
  - `MissingIndex`: 索引の根に `index.md` が無く、配下に文書が 1 件以上ある（[詳細](../req/index-finding-missing-index.md)。文書が 0 件なら出ません。出た根では、索引側の検査（`NotListed` `BrokenEntry` `TitleMismatch` `DescriptionMismatch` `MissingDirectoryDescription` `UnlinkedIndex`）は省かれ、`UntypedFile` と `MissingDescription` だけが併せて報告されます）。
  - `NotListed`: 文書が、担当する索引に文書エントリとして載っていない（[詳細](../req/index-finding-not-listed.md)）。
  - `UntypedFile`: 索引の根の配下に `type` のない `.md` がある。木から黙って消えるのを防ぎます（[詳細](../req/index-finding-untyped-file.md)）。
  - `BrokenEntry`: エントリのリンク先が解決できない・存在しない・文書でない、またはディレクトリエントリのリンク先に `index.md` が無い。区別は detail に出ます（[詳細](../req/index-finding-broken-entry.md)）。
  - `TitleMismatch` / `DescriptionMismatch`: エントリのリンク文・要約が、文書のタイトル・`description`（ディレクトリエントリでは子 `index.md` の `title` / `description`）と一致しない（[詳細](../req/index-finding-title-description-mismatch.md)）。
  - `MissingDescription`: 文書に `description` が無い、空、または改行を含む（[詳細](../req/index-finding-missing-description.md)）。
  - `MissingDirectoryDescription`: ディレクトリエントリのリンク先の子 `index.md` に、フロントマターの `description` が無い・空・改行を含む（target は子の索引のパス。エントリ側の要約が空かどうかは見ません）。`UnlinkedIndex`: 子孫ディレクトリの `index.md` が、親の索引のディレクトリエントリから辿れない（[詳細](../req/index-finding-directory-entry.md)）。

  既存の `broken` / `invalid` はグラフ自体が信用できないことを示すので、`reconcile --all` は 1 件でもあれば失敗し、`--edge` も関係するエッジを拒否します（[FR-035](../req/reconcile-rejects-broken-or-invalid.md) 参照）。これに対して `index` の finding は `index.md` と文書の食い違いであり、エッジやハッシュとは無関係です。そのため [`reconcile` の成否にも記録内容にも影響しません](../req/index-findings-do-not-block-reconcile.md)。解消する手段は `reconcile` ではなく、`miwa index` の実行や文書の修正（`description` / `type` を付ける）です。索引が乱れていても、doc / req / test の合意は記録できます。`--gate` では他の finding と同様に終了コード 2 になり、`--diff` では基準側の設定に `index` が無ければ、現在の `index` finding がすべて新規として報告されます。text 出力では種別は `index` と表示され、JSON 出力では `kind` は `Index` です。`BrokenEntry` と、ディレクトリエントリの `TitleMismatch` / `DescriptionMismatch` の target は空白を含む（`docs/index.md -> x.md`）ため、text 出力を空白で列に分けるときは注意してください。

`check` は診断だけを行い、[前回合意した記録（ロック）の内容を書き換えることはありません](../req/check-does-not-write-lock.md)。同じ入力に対しては、[常に同じ順序で同じ finding を返します](../req/check-findings-deterministic-order.md)。

## CI での使い方（`--gate`）

`check --gate` を CI に組み込むと、更新漏れがあるときにビルドを失敗させられます。[finding が1件でもあれば終了コード 2、設定不備などの実行エラーがあれば終了コード 1、問題がなければ終了コード 0](../req/check-gate-exit-codes.md) を返します。

## 差分だけを見る（`--diff`）

PR ごとのレビューでは、リポジトリ全体の finding ではなく、そのブランチで新たに発生した finding だけを知りたいことがほとんどです。`check --diff` を使うと、[現在のブランチと比較対象（既定は設定の `diff_base`、`--base` で上書き可能）の分岐点（merge-base）の時点で評価した finding と比較し、merge-base の時点にはなかった finding だけを報告します](../req/check-diff-scoped-to-new-findings.md)。merge-base が特定できないリポジトリの状態（浅いクローン等）では、実行エラーとして終了コード 1 を返します。

CI のどの段階で `--gate` と `--diff` のどちらを使うか（あるいは両方使うか）はプロジェクトの判断に任されています。

## 出力形式（`--format`）

`--format text`（既定）は人が読みやすい形式で、`--format json` は他のツールと連携しやすい機械可読な形式です。[どちらの形式で出力しても、報告される finding の集合と順序は同じ](../req/check-format-text-and-json.md)です。
