---
type: Specification
id: DOC-SETUP
---

# miwa を使い始める

miwa をリポジトリで使うには、最初に `init` コマンドで設定ファイルを作ります。

```
miwa init
```

`init` を実行すると `.miwa/config.yaml` が既定値で作られます。すでに `.miwa/config.yaml` がある状態で `init` を実行すると、[既存の設定を上書きせずに失敗します](../req/init-generates-default-config.md)。設定をやり直したい場合は、先に既存のファイルを削除するか、直接編集してください。

## 設定ファイルで何を決めるか

`.miwa/config.yaml` には次のような項目があります。

- `docs`: doc（業務要件）を探すフォルダの一覧
- `requirements`: req（技術要件）を探すフォルダの一覧
- `tests`: test（テストコード）を探すフォルダの一覧
- `diff_base`: `check --diff` で比較する既定のブランチ・タグ
- `judge`: 意味判定機能（今後追加予定）の設定

`docs` / `requirements` / `tests` に指定したフォルダの外にあるファイルは、miwa の対象になりません。また、[`.git` `target` `.miwa` `.worktrees` `node_modules` という名前のフォルダは自動的に探索から除外され、シンボリックリンクの先は辿りません](../req/source-scan-scope-and-exclusions.md)。大きなリポジトリでも、意図しないファイルまで読み込まれる心配はありません。

設定ファイルの値には制約があります。[`judge` のしきい値は 0 から 1 の範囲でなければならず、`docs` / `requirements` / `tests` のいずれも空にはできません](../req/config-validation-rules.md)。範囲外の値や空の一覧が設定されていると、miwa は実行時にエラーとして止まります。設定を直接編集した際は、この点に注意してください。
