---
type: Requirement
id: FR-018
---
ファイル探索は、設定の `docs` / `requirements` / `tests` に列挙されたディレクトリ配下のみを対象とする。`.git` `target` `.miwa` `.worktrees` `node_modules` `obj` `dist` という名前のディレクトリは、docs・requirements・tests のいずれの探索でも対象から除外する。シンボリックリンクは辿らない。`.miwa/config.yaml` の `exclude` に列挙したディレクトリ名も、同じ規則で除外する。除外はディレクトリ名の一致で判定し、ファイル名には適用しない。
