---
type: Requirement
id: FR-041
---
`judge` は対象エッジ 1本につき、`POST https://api.typesafe.ai/v1/systemone` を 1回呼ぶ。ヘッダは `Authorization: Bearer <TYPESAFE_API_KEY>` と `Content-Type: application/json` とする。本体は、`model: "jev-latest"`、構造化オブジェクトの `state`、1 問だけの `questions`（キー `consistent`、`type: "noul"`）を持つ。

`state` はエッジの種別ごとに次のとおりとする。
- doc–req エッジ: `{ "doc": <doc 本文>, "req": <req 本文> }`
- test–req エッジ: `{ "test": <test 関数のソース>, "req": <req 本文> }`
- desc エッジ: `{ "description": <要約>, "body": <本文> }`（FR-087 と同じ出所）

質問の `instructions` と `criteria` は次のとおり。
- doc–req: `instructions` は「`req` は `doc` の該当部分と矛盾せず、その部分を具体化した技術要件になっているか」。`criteria.true` は「req が扱う事項について doc と矛盾がなく、doc の記述を具体化している」、`criteria.false` は「req が doc と矛盾している、または doc に対応する記述がない」。
- test–req: `instructions` は「`test` は `req` に書かれた振る舞いを検証しており、`req` と矛盾する期待値を持たないか」。`criteria.true` は「test が req の振る舞いを検証しており、req と矛盾する期待値がない」、`criteria.false` は「test が req の振る舞いを検証していない、または req と矛盾する期待値を持つ」。
- desc: `instructions` は「`description` は `body` の内容を正しく要約しているか」。`criteria.true` は「description が body の主題と内容を正しく表しており、body にない事柄を述べていない」、`criteria.false` は「description が body の内容と食い違っている、または body の主題を表していない」。
