---
type: BusinessNeed
id: DOC-CHECK
title: "更新漏れを検知する（check）"
description: "doc・req・test の追随漏れや索引の乱れを、ロックとの比較で機械的に見つけて finding として報告するコマンドの仕様。"
---

# 更新漏れを検知する（`check`）

`miwa check` は、doc・req・test の3層のうち、どれかが更新されたのに他の層が追随していない状態を機械的に見つけるコマンドです。見る対象は、ドキュメントとテストの追随だけです。実装コードそのものは扱いません。

## 報告される内容（finding）

`check` は、次の種類の finding を報告します。

- **drift（ドリフト）**: 前回合意した内容から、依存関係の片側または両側が変わっている状態です。doc と req をつなぐ関係も、test と req をつなぐ関係も対象です。doc・req・test の[いずれか一方でも内容が変わっていれば drift](../req/finding-drift-either-side-changed.md)として報告します。
  - doc だけが変わった場合は、req が追随できていない可能性があります。
  - req だけが変わった場合は、doc や test が追随できていない可能性があります。
  - test だけが変わった場合は、test が req の範囲に収まっているかを確認してください。
  - `index` を設定していると、索引の対象になっている doc・req について、要約（`description`）と本文の対も依存関係として見ます。この依存関係は `desc:doc:<id>` / `desc:req:<id>` エッジです。[drift の理由](../req/desc-edge-drift-reasons.md)は、`DescriptionChanged`（要約だけが変わった）、`BodyChanged`（本文だけが変わった）、`BothChanged`（両方が変わった）のいずれかです。要約が本文に追随しているかを確かめる合図になります。
  - 本文を1回編集すると、finding が2件出ます。doc の本文を編集すると、その doc の `doc-req` エッジ（リンクしている req の本数分）の `DocChanged` と、`desc` エッジの `BodyChanged` が、別々に出ます。索引配下の req なら、`ReqChanged` と `BodyChanged` が出ます。これは意図した動作です。要約はそのままでよいかを確かめたうえで、それぞれ [`reconcile`](reconcile.md) します。`description` を消した場合も2件出ます。1件は `index` の `MissingDescription` で、もう1件は `desc` の `drift`（`DescriptionChanged`）です。後者が出るのは、ロックに記録された要約が空でなかった場合です。
- **new / removed**: 前回合意した記録（ロック）にない新しい依存関係は new、ロックにあったのにソースから消えた依存関係は removed です。この[2つの finding](../req/finding-new-and-removed.md)を報告します。
- **orphan（孤立）**: どの doc からもリンクされていない req と、どの req にもリンクしていない doc を、[孤立したもの](../req/finding-orphan.md)として報告します。書いたのに使われていない req や、参照が漏れている doc を見つけられます。
- **no-test**: 1件もテストで検証されていない req を、[no-test](../req/finding-no-test.md)として報告します。免除の仕組みはなく、すべての req が対象です。
- **invalid**: id の重複や欠落、テスト関数に複数のマーカーが付いている状態、テストファイルが構文エラーで解析できない、といった不正な状態です。後ろの2つは、Go・Rust・C#・Java・Kotlin・TypeScript / JavaScript・Dart に共通の[invalid の条件](../req/test-invalid-conditions.md)です。
  - マーカーを持つ test の識別子が重複している場合は、invalid にはなりません。例えば C# や Java のオーバーロード、Rust の `#[cfg]` 分岐による同名関数、TypeScript / JavaScript や Dart の同名 test が該当します。この場合は、[出現をまとめて1つの test](../req/test-duplicate-identifier-merged.md)として扱います。
  - 重複した識別子が invalid になるのは、まとめようとした出現のマーカーが異なる req を指している場合だけです。その場合は、その識別子のすべての出現が invalid となり、依存関係は作られません。
  - 同じ識別子にマーカーが複数ある不正な出現が残っている間は、まとめた test の依存関係も reconcile や judge の対象になりません。先に不正な出現を直してください。
- **broken（壊れたリンク）**: doc から参照している req が存在しない、または req でないファイルを指している状態です。詳細は [モデル](model.md) を参照してください。
- **index（索引の乱れ）**: [索引](index-command.md)（`index.md`）が文書と食い違っている状態です。`.miwa/config.yaml` の `index` を設定したときだけ検査します。reason は次の9種類あり、どれも `target_kind` は `Index` です。
  - `MissingIndex`: 索引の根に `index.md` が無く、配下に文書が1件以上ある（[詳細](../req/index-finding-missing-index.md)）。文書が0件なら出ません。この finding が出た根では、索引側の検査（`NotListed` `BrokenEntry` `TitleMismatch` `DescriptionMismatch` `MissingDirectoryDescription` `UnlinkedIndex`）を省きます。併せて報告されるのは、`UntypedFile` と `MissingDescription` だけです。
  - `NotListed`: 文書が、担当する索引に文書エントリとして載っていない（[詳細](../req/index-finding-not-listed.md)）。
  - `UntypedFile`: 索引の根の配下に `type` のない `.md` がある。気づかないうちに木から消えるのを防ぎます（[詳細](../req/index-finding-untyped-file.md)）。
  - `BrokenEntry`: エントリのリンク先が解決できない、存在しない、または文書でない、あるいはディレクトリエントリのリンク先に `index.md` が無い。どの場合かは detail に出ます（[詳細](../req/index-finding-broken-entry.md)）。
  - `TitleMismatch` / `DescriptionMismatch`: エントリのリンク文が文書のタイトルと、要約が `description` と一致しない。ディレクトリエントリでは、子 `index.md` の `title` / `description` と比べます（[詳細](../req/index-finding-title-description-mismatch.md)）。
  - `MissingDescription`: 文書に `description` が無い、空、または改行を含む（[詳細](../req/index-finding-missing-description.md)）。
  - `MissingDirectoryDescription` / `UnlinkedIndex`: `MissingDirectoryDescription` は、ディレクトリエントリのリンク先の子 `index.md` に、フロントマターの `description` が無い、空、または改行を含む。target は子の索引のパスで、エントリ側の要約が空かどうかは見ません。`UnlinkedIndex` は、子孫ディレクトリの `index.md` が、親の索引のディレクトリエントリから辿れない（[詳細](../req/index-finding-directory-entry.md)）。

  既存の `broken` / `invalid` は、グラフ自体が信用できないことを示します。そのため `reconcile --all` は、1件でもあれば失敗します。`--edge` も、関係するエッジを拒否します（[FR-035](../req/reconcile-rejects-broken-or-invalid.md) 参照）。これに対して `index` の finding は、`index.md` と文書の食い違いを示すもので、エッジやハッシュとは無関係です。そのため、[`reconcile` の成否と記録内容](../req/index-findings-do-not-block-reconcile.md)は、`index` の finding に左右されません。解消する手段は `reconcile` ではなく、`miwa index` の実行や文書の修正（`description` / `type` を付ける）です。索引が乱れていても、doc / req / test の合意は記録できます。

  `--gate` では、他の finding と同様に終了コード 2 になります。`--diff` では、基準側の設定に `index` が無ければ、現在の `index` finding がすべて新規として報告されます。text 出力では種別が `index` と表示され、JSON 出力では `kind` が `Index` になります。`BrokenEntry` と、ディレクトリエントリの `TitleMismatch` / `DescriptionMismatch` の target には、空白が含まれます（`docs/index.md -> x.md`）。text 出力を空白で列に分けるときは注意してください。

`check` は診断だけを行います。[前回合意した記録（ロック）の内容](../req/check-does-not-write-lock.md)を書き換えることはありません。同じ入力に対する[finding の内容と順序は常に同じ](../req/check-findings-deterministic-order.md)です。

## CI での使い方（`--gate`）

`check --gate` を CI に組み込むと、更新漏れがあるときにビルドを失敗させられます。[終了コード](../req/check-gate-exit-codes.md)は、finding が1件でもあれば 2、設定不備などの実行エラーがあれば 1、問題がなければ 0 です。

## 差分だけを見る（`--diff`）

PR ごとのレビューでは、リポジトリ全体の finding ではなく、そのブランチで新たに発生した finding だけを知りたいことがほとんどです。`check --diff` は、現在のブランチと比較対象の分岐点（merge-base）の時点で評価した finding を、現在の finding と比べます。[merge-base の時点にはなかった finding だけ](../req/check-diff-scoped-to-new-findings.md)を報告します。比較対象は、既定では設定の `diff_base` で、`--base` で上書きできます。浅いクローンなどで merge-base が特定できない場合は、実行エラーとして終了コード 1 を返します。

CI のどの段階で `--gate` と `--diff` のどちらを使うか、あるいは両方使うかは、プロジェクトが判断します。pull request ごとに `check --diff --gate` を実行する GitHub Actions の例は、リポジトリの [`templates/miwa-check.yml`](../templates/miwa-check.yml) にあります。

## 出力形式（`--format`）

`--format text`（既定）は人が読みやすい形式です。`--format json` は、他のツールと連携しやすい機械可読な形式です。どちらの形式でも、[報告される finding の集合と順序は同じ](../req/check-format-text-and-json.md)です。
