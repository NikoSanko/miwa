---
type: Requirement
id: FR-092
---
ページの出力パスは、元の Markdown のリポジトリ相対パスの拡張子 `.md` を `.html` に置き換えたものを、出力先の直下からの相対パスとする（`docs/a/b.md` は `<out>/docs/a/b.html`、`docs/index.md` は `<out>/docs/index.html`）。リポジトリのディレクトリ構造をそのまま保つので、本文中の相対リンクの位置関係が変わらない。`index` の根は `.` にならない（FR-066）ので、トップページの `<out>/index.html` と衝突するページは生じない。
