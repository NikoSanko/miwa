---
type: Reference
id: REF-SPEC-ALPHA
title: "仕様乖離検知ツール α版の要件・仕様"
description: "α版の目的・要件・仕様をまとめた参考資料。"
---

# 仕様乖離検知ツール（仮称） 要件・仕様 α版

## 1. 目的

| ID | 目的 |
|---|---|
| G1 | 非開発メンバーが、業務要求を Web 上で参照できる |
| G2 | 開発者・レビュアー・AI エージェントが、PR のドキュメント差分だけで振る舞いの変更を把握できる |

業務要求（doc）・システム要件（req）・テスト（test）の3層を依存関係で結ぶ。どれかが更新されたのに残りが追随していない状態は、機械的に検知する。それが本当に矛盾かどうかは Judge で判定する。

- 実装コードは扱わない。テストを実装側の仕様とみなし、「ドキュメント ↔ テスト」の追随だけを見る
- 機械的検知（ハッシュ比較）は CI で使う。意味判定（Judge）はローカルだけで使う
- req を変更すると、それに依存する doc・test とのエッジがすべてドリフトする。そのため、振る舞いの変更は doc まで確認が及ぶ

保証しないのは、要件の網羅性、テストの十分性、実装コードだけを変更した PR、テスト関数の外（ヘルパー等）の変更である。

## 2. 用語

| 用語 | 定義 |
|---|---|
| doc | 業務要求。OKF（`type: BusinessNeed`） |
| req | システム要件。OKF（`type: Requirement`） |
| test | req を検証するテスト関数 |
| エッジ | doc → req、または test → req の参照1本 |
| ドリフト | エッジの両端のどちらかが、前回の合意時から変わっている状態 |
| reconcile | エッジの現在の状態を「合意済み」としてロックに記録する操作 |
| Judge | ドリフトが意味的に矛盾しているかを判定する機能 |

## 3. スコープ（α版）

- コマンド: `init` / `check` / `reconcile` / `judge`
- 対応言語: Go・Rust・C#・Java・Kotlin（JUnit）・TypeScript / JavaScript（Vitest）（Rust は miwa 自身を miwa でチェックするために含める）
- doc / req / test が同じリポジトリにあること

## 4. モデル

```
doc ──依存──▶ req ◀──検証── test
```

- 1つの doc は複数の req に依存できる。1つの req は複数の doc から依存されてよい
- 1つの req は複数の test で検証できる。1つの test が検証する req は1つだけ

| 層 | 識別子 | ハッシュ範囲 |
|---|---|---|
| doc | フロントマター `id` | 本文（フロントマター除く） |
| req | フロントマター `id` | 本文（フロントマター除く） |
| test | Go: パッケージディレクトリ ＋ 関数名 / Rust: ファイルパス ＋ モジュールパス ＋ 関数名 / C#・Java・Kotlin: ファイルパス ＋ 型名の階層 ＋ メソッド名 / TypeScript・JavaScript: ファイルパス ＋ describe 名の階層 ＋ test 名 | 関数全体（Rust はシグネチャと本体を含む `fn` 全体。外側の属性は含まない）。C#・Java・Kotlin は属性（注釈）を除いたメソッド宣言全体。TypeScript・JavaScript は `test(...)` / `it(...)` の呼び出し文全体 |

- ハッシュは SHA-256。改行を LF に揃える以外の正規化はしない
- doc / req のフロントマター `id` に使える文字は `A-Z a-z 0-9 . _ -`（`:` はエッジ ID の区切りに使うため不可）

## 5. ファイル形式

**req**
```markdown
---
type: Requirement
id: FR-001
---
8文字未満のパスワードは登録時に拒否する。
```

**doc**
```markdown
---
type: BusinessNeed
id: DOC-AUTH
---
# ログイン
ユーザーはメールアドレスとパスワードで認証する。
パスワードは [最小長の制約](../req/auth/password-min-length.md) に従う。
```
- req ファイルへの相対リンクが doc → req のエッジになる

**test（Go）**
```go
// [FR-001]
func TestRejectsShortPassword(t *testing.T) { ... }
```
- `*_test.go` のトップレベルの `func TestXxx` の直前に隣接するコメント（空行を挟まない）に書いた `[req id]` が test → req のエッジになる
- マーカーのない Test 関数は無視する

**test（Rust）**
```rust
// [FR-001]
#[test]
fn rejects_short_password() { ... }
```
- 属性パスの末尾セグメントが `test` の属性（`#[test]` / `#[tokio::test]` / `#[async_std::test]`、引数付きも可）を持つ `fn` が対象。`#[cfg_attr(x, test)]` のような他属性の引数の中は見ない
- `.miwa/config.yaml` の `rust.test_attributes`（既定は空）に列挙した名前も、test 属性とみなす。比較は末尾セグメントで行う（`rstest` を列挙すると `#[rstest]` / `#[rstest::rstest]` が対象になる）
- マーカーは、test 属性のうちソース上で最初に現れるものの直前に隣接するコメント（空行を挟まない）に書く。属性間・属性と `fn` の間のコメントは対象外

**test（C#）**
```csharp
public class AuthTests
{
    // [FR-001]
    [Fact]
    public void RejectsShortPassword() { ... }
}
```
- `Fact` `Theory` `Test` `TestCase` `TestCaseSource` `TestMethod` `DataTestMethod` のいずれかの属性を持つメソッドが対象（xUnit / NUnit / MSTest）
- マーカーはメソッド宣言（属性を含む）の直前に隣接するコメントに書く

**test（Java / Kotlin / JUnit）**
```kotlin
class AuthTest {
    // [FR-001]
    @Test
    fun `短い パスワード は 拒否`() { ... }
}
```
- `tests` 配下の `*.java` `*.kt` が対象（`*.kts` は対象外）
- `Test` `ParameterizedTest` `RepeatedTest` `TestFactory` `TestTemplate` のいずれかの注釈（修飾付き・引数付き可）を持つ、型の直下のメソッドが対象（Kotlin はトップレベル関数も対象）。メソッド本体・匿名オブジェクト・enum 定数本体・プロパティの中は対象外
- マーカーはメソッド宣言（注釈・修飾子を含む）の直前に隣接する `//` または `/* */` のコメントに書く。Javadoc / KDoc（`/** */`）と、注釈と署名の間のコメントは対象外
- 識別子は `<拡張子を除いたパス>:<型名を `/` 連結>:<メソッド名>`（package は含めない。Kotlin のバッククォート名は中身のみ）
- 識別子に非 ASCII 文字を含む Kotlin のテストも抽出できる

**test（TypeScript / JavaScript / Vitest）**
```typescript
describe("auth", () => {
  // [FR-001]
  it("rejects short password", () => { ... });
});
```
- `<name>.test.<ext>` `<name>.spec.<ext>`（ext は `ts` `tsx` `mts` `cts` `js` `jsx` `mjs` `cjs`）のファイル直下または `describe` 直下の `test(...)` / `it(...)` が対象
- マーカーは呼び出し文の直前に隣接するコメントに書く

## 6. 検知（`check`）

ロックには、エッジごとに「合意時の両端のハッシュ」を記録する。`check` は、現在のハッシュと比べて次を報告する。

| finding | 条件 |
|---|---|
| `drift` | エッジの片側または両側が合意時から変わった |
| `new` | ロックにないエッジがある |
| `removed` | ロックにあるエッジが、ソースから消えた |
| `broken` | リンク先・マーカーの req が存在しない |
| `orphan` | どの doc からも依存されない req、どの req にも依存しない doc |
| `no-test` | test が1件もない req |
| `invalid` | `id` の重複・欠落、1つの test にマーカーが複数ある、同じ識別子の test が異なる req を指す |

`drift` の読み方:
- doc だけ変わった → req が追随していない可能性
- req だけ変わった → doc / test が追随していない可能性
- test だけ変わった → test が req の範囲内に収まっているか確認

オプション:
- `--gate`: finding が1件でもあれば exit 2 を返す（エラー時は exit 1）
- `--diff`: merge-base との比較で、新たに発生した finding だけを報告する。merge-base 側は、その時点の設定・ソース・ロックで評価する。CI でどちらを使うかはプロジェクトに任せる

## 7. 合意（`reconcile`）

- `reconcile --edge <id>`: 指定したエッジの現在のハッシュを、ロックに記録する
- `reconcile --all`: 全エッジをまとめて記録する（初期導入用）
- `removed` のエッジは、reconcile するとロックから削除される
- `broken` / `invalid` は reconcile できない。ソースを直して解消する
- `--reason <text>` で理由を残せる

## 8. 意味判定（`judge`）

- 対象: `drift` と `new` のエッジ
- ローカルでだけ実行する（CI では実行しない）
- 両端の現在の本文を渡して、整合しているかを問う
  - doc–req: 「req は doc の該当部分と矛盾せず、それを具体化しているか」
  - test–req: 「test は req の振る舞いを検証しているか」

**判定**
- Jev: Noul 型の質問で 0〜1 のスコアを得る。スコアだけで、文章は返さない
- スコアが閾値以上なら `consistent`、未満なら `review` として報告する。`review` の判断は呼び出し元（人や LLM）に任せる

**自動 reconcile**
- 既定では結果を表示するだけで、ロックは更新しない
- `judge --auto-reconcile` を指定したときだけ、自動で reconcile する。対象は Jev のスコアが閾値（既定 0.9）以上のエッジで、ロックに `by: jev` とスコアを記録する

## 9. ロック

`.miwa/lock.json` に、エッジ単位で記録する。設定は `.miwa/config.yaml`。

```json
{
  "edges": {
    "doc-req:DOC-AUTH:FR-001": {
      "from": "sha256:aaa…",
      "to": "sha256:bbb…",
      "by": "human",
      "reason": "最小長の記述が一致",
      "at": "2026-09-29T00:00:00Z"
    },
    "test-req:internal/auth:TestRejectsShortPassword:FR-001": {
      "from": "sha256:ccc…",
      "to": "sha256:bbb…",
      "by": "jev",
      "score": 0.94,
      "at": "2026-09-29T00:00:00Z"
    }
  }
}
```

- キーはエッジ ID 順に並べる（差分を読みやすくするため）
- `check` はロックに書き込まない

## 10. 非機能

- Rust の単一バイナリ
- `check` は同じ入力に対して同じ結果を返す。ネットワークを使うのは `judge` だけ

## 11. 決定事項

| # | 決定 |
|---|---|
| D1 | CI の責務は、更新漏れの機械的な検知だけ |
| D2 | 意味判定はローカルで行う。Jev でスコア化し、閾値未満は `review` として呼び出し元に任せる |
| D3 | 自動 reconcile は `--auto-reconcile` を明示したときだけ行う |
| D4 | 実装コードへのリンクは持たない。3層固定 |
| D5 | doc → req に依存し、req は doc を知らない |
| D6 | doc はファイル全体を1ノードとする |
| D7 | doc / req の識別子はフロントマターの `id` |
| D8 | 1つの test が検証する req は1つ |
| D9 | 対応言語は Go・Rust・C#・Java・Kotlin・TypeScript / JavaScript。miwa 自身を miwa でチェックする（dogfooding） |

## 12. 次のスコープ以降

- 公開サイト（doc / req ごとの検証状態の表示）
- AI エージェント統合（Stop Hook で `check --gate`）
- `eval` による Judge の閾値の校正
- 対応言語の追加
- α版の運用で問題になったエッジケースへの対応（詳細な検討案は v2 を参照）

## 13. 未確定事項

| # | 論点 |
|---|---|
| Q1 | Jev のスコアの校正の前提（閾値 0.9 は仮置き） |
| Q2 | OKF の採用版（v0.1 か v0.2 か） |
