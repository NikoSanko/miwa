---
type: Requirement
id: FR-090
---
`miwa site` は、`index` が未設定（空）のとき、`index is not configured in .miwa/config.yaml` を示して実行エラーとし、exit 1 を返す（ナビゲーションを索引から作るため。`miwa index` と同じ扱い）。このとき何も書かない。

ロック（`.miwa/lock.json`）は読まず、ネットワークにも接続しない。出力は決定的で、同じ入力からは、ファイルの内容もファイルの集合も同じになる（生成時刻などを埋め込まない）。公開されないリンクなどがあっても exit 0 で終了し、警告だけを標準エラーへ出す（FR-099）。
