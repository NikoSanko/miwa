---
type: Specification
id: DOC-CHECK
---

# 更新漏れを検知する（`check`）

`miwa check` は、doc・req・test の3層のうち、どこかが更新されたのに他が追随していない状態を機械的に見つけるコマンドです。実装コードそのものは扱わず、「ドキュメントとテストの追随」だけを見ます。

## 報告される内容（finding）

`check` は、次の種類の finding を報告します。

- **drift（ドリフト）**: 前回合意した内容から、依存関係の片側または両側が変わっている状態。doc と req をつなぐ関係も、test と req をつなぐ関係も対象です。[doc・req・test のいずれか一方でも内容が変わっていれば drift として報告](../req/finding-drift-either-side-changed.md)されます。
  - doc だけが変わった場合は、req が追随できていない可能性があります。
  - req だけが変わった場合は、doc や test が追随できていない可能性があります。
  - test だけが変わった場合は、test が req の範囲に収まっているかを確認してください。
- **new / removed**: [前回合意した記録（ロック）にない新しい依存関係は new、逆にロックにあったのにソースから消えた依存関係は removed](../req/finding-new-and-removed.md)として報告されます。
- **orphan（孤立）**: [どの doc からもリンクされていない req、どの req にもリンクしていない doc](../req/finding-orphan.md)を報告します。書いたのに使われていない req や、参照が漏れている doc を見つけられます。
- **no-test**: [1件もテストで検証されていない req](../req/finding-no-test.md)を報告します。免除の仕組みはなく、すべての req が対象です。
- **invalid**: id の重複・欠落、テスト関数に複数のマーカーが付いている、テストファイルの構文が壊れている、といった不正な状態です。[テスト関数に2件以上のマーカーが付いている場合や、テストファイルが構文エラーで解析できない場合も invalid として報告](../req/test-invalid-conditions.md)されます（Go・Rust・C#・TypeScript 共通）。
  - マーカーを持つ test の識別子が重複している場合（C# のオーバーロード、Rust の `#[cfg]` 分岐による同名関数、TypeScript の同名 test など）は、invalid ではなく[出現をまとめて 1 つの test として扱われます](../req/test-duplicate-identifier-merged.md)。invalid になるのは、まとめようとした出現のマーカーが異なる req を指している場合だけで、その識別子のすべての出現が invalid となり、依存関係は作られません。同じ識別子にマーカーが複数ある不正な出現が残っている間は、まとめた test の依存関係も reconcile や judge の対象になりません。先に不正な出現を直してください。
- **broken（壊れたリンク）**: doc から参照している req が存在しない、または req でないファイルを指している状態です（詳細は [モデル](model.md) を参照）。

`check` は診断だけを行い、[前回合意した記録（ロック）の内容を書き換えることはありません](../req/check-does-not-write-lock.md)。同じ入力に対しては、[常に同じ順序で同じ finding を返します](../req/check-findings-deterministic-order.md)。

## CI での使い方（`--gate`）

`check --gate` を CI に組み込むと、更新漏れがあるときにビルドを失敗させられます。[finding が1件でもあれば終了コード 2、設定不備などの実行エラーがあれば終了コード 1、問題がなければ終了コード 0](../req/check-gate-exit-codes.md) を返します。

## 差分だけを見る（`--diff`）

PR ごとのレビューでは、リポジトリ全体の finding ではなく、そのブランチで新たに発生した finding だけを知りたいことがほとんどです。`check --diff` を使うと、[現在のブランチと比較対象（既定は設定の `diff_base`、`--base` で上書き可能）の分岐点（merge-base）の時点で評価した finding と比較し、merge-base の時点にはなかった finding だけを報告します](../req/check-diff-scoped-to-new-findings.md)。merge-base が特定できないリポジトリの状態（浅いクローン等）では、実行エラーとして終了コード 1 を返します。

CI のどの段階で `--gate` と `--diff` のどちらを使うか（あるいは両方使うか）はプロジェクトの判断に任されています。

## 出力形式（`--format`）

`--format text`（既定）は人が読みやすい形式で、`--format json` は他のツールと連携しやすい機械可読な形式です。[どちらの形式で出力しても、報告される finding の集合と順序は同じ](../req/check-format-text-and-json.md)です。
