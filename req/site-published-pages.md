---
type: Requirement
id: FR-091
---
`miwa site` が HTML にするページは、次の和集合に限る。(1) doc ノード（`type: Specification`）、(2) req ノード（`type: Requirement`）、(3) 各 `index` の根の配下で索引の対象になる文書、(4) 各 `index` の根の配下にある `index.md`（索引ページ）。

(3) は `miwa index` と同じ判定で、フロントマターに `type` を持つ Markdown であればよい。`type` の値は問わず（例: `type: Reference`）、`id` の欠落・不正・重複も問わない。同じファイルが複数の分類に当たってもページは 1つである。タイトルと要約の規則は分類によらず同じ（FR-096）なので、優先順位は要らない。

次のファイルは、ページにしない。`type` を持たない Markdown、`log.md`、`exclude` や既定の除外名で探索から外れるもの、拡張子が `.md` でないファイルである（画像は FR-100）。フロントマターを解析できない doc・req・文書は、ノードにも文書にもならないため公開しない（索引ページは FR-096）。
