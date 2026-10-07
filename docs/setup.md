---
type: Specification
id: DOC-SETUP
title: "miwa を使い始める"
description: "miwa をリポジトリで使い始めるための init と設定ファイル、索引の導入手順を説明する。"
---

# miwa を使い始める

miwa をリポジトリで使うには、最初に `init` コマンドで設定ファイルを作ります。

```
miwa init
```

`init` を実行すると、既定値で `.miwa/config.yaml` を作ります。すでに `.miwa/config.yaml` がある状態で `init` を実行すると、[既存の設定を上書きせずに失敗します](../req/init-generates-default-config.md)。設定をやり直したい場合は、先に既存のファイルを削除するか、直接編集してください。

## 設定ファイルで何を決めるか

`.miwa/config.yaml` には次のような項目があります。

- `docs`: doc（業務要件）を探すフォルダの一覧
- `requirements`: req（技術要件）を探すフォルダの一覧
- `tests`: test（テストコード）を探すフォルダの一覧
- `diff_base`: `check --diff` で比較する既定のブランチ・タグ
- `judge`: 意味判定（[`judge`](judge.md)）の設定
  - `threshold`: Jev のスコアがこの値以上なら「整合」とみなします。範囲は0〜1で、既定値は 0.9 です。
  - `max_body_bytes`: 1つの依存関係で Jev に送る両端の本文の、合計バイト数の上限です。超えると `skipped` になります。既定は 200000 です。
  - API キーは設定ファイルではなく環境変数 `TYPESAFE_API_KEY` で渡します。
- `rust.test_attributes`: Rust で `test` 以外の名前の test 属性（例: `rstest`）を test として扱うための、属性名の一覧。省略時は空。属性パスの末尾の名前だけを書きます（`rstest::rstest` ではなく `rstest`）。
- [`exclude`](../req/config-exclude-dirs.md): 探索から除外するディレクトリ名の一覧。省略時は空。パスではなく名前だけを書きます。どの深さでも、一致するディレクトリ配下が除外されます（例: Gradle の `build`、Next.js の `.next`）。Vite や tsc の出力先 `dist` は、既定で除外されます。
- [`index`](../req/config-index-roots.md): 索引（`index.md`）を検査・生成する根のフォルダの一覧。省略時は空で、空のときは索引の機能は無効です。書けるのは、`docs` か `requirements` のフォルダと同じフォルダか、その配下のフォルダです。`exclude` の配下は書けません。要素同士が包含関係にあるのも不可です。子孫フォルダの `index.md` は、設定しなくても担当を持ちます。詳細は [索引を保つ](index-command.md) を参照してください。
- [`site`](../req/config-site-settings.md): [`miwa site`](site.md) が作る閲覧サイトのトップページの設定。省略でき、書けるのは `title`（省略時は miwa を実行したフォルダの名前）と `description`（省略時は要約なし）です。前後の空白を除くと空になる値、改行を含む値、文字列でない値は、設定エラーになります。この検証は、すべてのコマンドで行います。

`docs` / `requirements` / `tests` に指定したフォルダの外にあるファイルは、miwa の対象になりません。さらに、`.git` `target` `.miwa` `.worktrees` `node_modules` `obj` `dist` という名前のフォルダは、自動的に探索から除外されます。シンボリックリンクの先は辿りません。こうした[探索範囲の制限](../req/source-scan-scope-and-exclusions.md)があるので、大きなリポジトリでも、意図しないファイルまで読み込まれる心配はありません。`obj` は C# のビルド生成物を読み込まないための除外で、docs・requirements・tests のすべてに適用されます。

既知の制約として、miwa が使っている構文解析器が未対応の構文を含むテストファイルは、構文エラーとして `invalid` になります（例: C# のリストパターン内のスライス `[.., var r]`）。該当するファイルは、その構文を避けて書くか、`tests` の対象から外してください。Kotlin には、さらに次の制約があります。

- 非 ASCII 文字（漢字・カタカナ・ハングルなど）を含む行では、構文エラーの検知が緩くなります。この制約で緩むのは構文エラーの検知であり、有効なコードが `invalid` になる方向ではありません。
- メンバーを1つでも持つ1行の class / object / interface / companion object 本体（`class A { fun x() {} }`、`object O { const val X = 1 }` など）は、構文エラーになります。複数行に分ければ問題ありません。
- 注釈を2つ以上付けた、supertype のないクラスの本体に、`@Nested inner class` のように注釈付きの inner class があると、構文解析器の不具合でクラス全体が認識できなくなる場合があります。miwa は、[このパターンを自動で回避します](../req/kotlin-non-ascii-identifiers.md)。回避できない場合は、構文エラー（`invalid`）として扱います。その場合は、supertype（`: Any()` など）を付けるか、クラスの注釈を1つにしてください。

Java / Kotlin では、Gradle / Maven のモジュール直下であれば `tests: ["src/test"]` のように指定します。monorepo ではモジュールのパスを先頭に付けて、`tests: ["packages/api/src/test"]` のようにします。生成コードの `src/generated` は `tests` に含めないでください。ビルド出力の `build` などは、`exclude: ["build"]` のように指定して探索から除外します。

`init` が作る[既定の設定](../req/init-generates-default-config.md)では、`judge.threshold` は 0.9、`judge.max_body_bytes` は 200000、`index` は `[docs]` です。

設定ファイルの値には[制約](../req/config-validation-rules.md)があります。`judge` のしきい値は0から1の範囲、`judge.max_body_bytes` は1以上の整数でなければなりません。`docs` / `requirements` / `tests` は、いずれも空にできません。範囲外の値や空の一覧が設定されていると、miwa は実行時にエラーで止まります。設定を直接編集したときは、この点に注意してください。

`exclude` の各項目は、ディレクトリ名として[次の条件](../req/config-exclude-dirs.md)を満たす必要があります。前後の空白を除いて空でないこと、空白・`/`・`\`・`*` `?` `[` を含まないこと、`.` や `..` でないことです。また、`docs` / `requirements` / `tests` のパスに、`exclude` や既定の除外名と同じ名前のフォルダが含まれていると、探索先が丸ごと消えてしまうため、エラーになります。

`rust.test_attributes` の各項目は、[英数字とアンダースコアだけからなる空でない文字列](../req/config-rust-test-attributes.md)でなければなりません。違反するとエラーになります。なお、`rust.test_attributes` に属性を追加すると、マーカーを置く位置（最初の対象属性の直前）が変わり、既存の test → req の依存関係が外れることがあります。

## 予約名

`index.md` と `log.md` は予約名です。`docs` と `requirements` のどちらの配下でも、`type` を持っていても、[doc や req として扱いません](../req/index-reserved-names.md)。設定で変えることはできません。

## 索引を導入する手順

1. `.miwa/config.yaml` に `index: ["docs"]` を書きます（`init` した直後なら既定で入っています）。
2. 文書に `description`（必須）と `title`（任意）を付けます。`type` の無い `.md` には `type` を付けるか、索引のフォルダの外へ出します。
3. `miwa index` で索引を生成・更新します。末尾に追加された行を節へ移します。子ディレクトリに索引を置いた場合は、その `index.md` のフロントマターに `title` / `description` を書きます。
4. `miwa reconcile --all` で現状の合意を記録します。索引の対象になっているすべての doc・req（`description` の有無にかかわらず）では、要約と本文の対（`desc:doc:<id>` / `desc:req:<id>` エッジ）が `new` になります。これも合わせて合意します。
5. `miwa check --gate` が通ることを確認します。

索引の形式や `miwa index` の動作は [索引を保つ](index-command.md) を参照してください。
