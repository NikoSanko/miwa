---
type: Requirement
id: FR-047
---
エージェントに渡すプロンプトには、次の共通の指示を含める。各エッジについて両端が意味的に整合しているかを判定し、結論と理由を報告すること（doc–req は「req は doc の該当部分と矛盾せず具体化しているか」、test–req は「test は req の振る舞いを検証しており、矛盾する期待値を持たないか」を基準とする）。doc / req / test のファイルは編集しないこと。加えて、`--auto-reconcile` を指定した場合だけ「整合していると判断したエッジは `miwa reconcile --edge <id> --reason "<理由>"` を実行して合意する。矛盾しているものは reconcile せず、どちら側を直すべきかを報告する」を含める。`--auto-reconcile` を指定しない場合は、代わりに「reconcile は実行しない」と明記する。
