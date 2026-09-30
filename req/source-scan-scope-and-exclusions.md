---
type: Requirement
id: FR-018
---
ファイル探索は設定の `docs` / `requirements` / `tests` に列挙されたディレクトリ配下のみを対象とする。`.git` `target` `.miwa` `.worktrees` `node_modules` という名前のディレクトリは探索対象から除外し、シンボリックリンクは辿らない。
