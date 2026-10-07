---
type: Requirement
id: FR-093
---
`miwa site` は出力先の直下に `index.html`（トップページ）を作る。`<title>` と `<h1>` は設定 `site.title`（省略時は、miwa を実行したディレクトリ、つまり `.miwa/config.yaml` のあるディレクトリの名前）、`<meta name="description">` と `<h1>` 直下の要約は `site.description`（省略時はどちらも出さない）とする。本文は `index` の各根を 1 エントリとする一覧で、並びは `index` 設定に書かれた順とし、`<ul><li><a href="docs/index.html">タイトル</a> — 要約</li></ul>` の形で HTML を直接組む（要約が無いエントリは ` — 要約` を付けない）。href はトップページからの相対パスでファイル名まで含める。タイトルと要約はその根の `index.md` のフロントマターの `title` / `description` で決め、規則は FR-071 のディレクトリと同じ（`title` が無い・空・改行を含むときはディレクトリ名、`description` が無い・空・改行を含むときは要約なし）。根に `index.md` が無いときは FR-103 に従う。
