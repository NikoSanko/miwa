---
type: Requirement
id: FR-056
---
TypeScript / JavaScript の test の識別子は `<拡張子を除いたファイルパス>:<describe 名を外側から内側の順に `/` で連結したパス>:<test 名>` の形式とする。拡張子として除くのは FR-053 の 8 種（`ts` `tsx` `mts` `cts` `js` `jsx` `mjs` `cjs`）で、`.test` / `.spec` は残す。describe の外にある test では describe 部分は空文字列として連結する。describe 名・test 名は引用符の内側のソース表記をそのまま使い、エスケープは解釈しない（`"same"` と `` `same` `` は同名、`"\u0073ame"` は別名）。名前が空文字列、または改行・行区切り文字（U+2028 / U+2029 など）・制御文字を含む test / describe、および置換（`${...}`）を含むテンプレート文字列を名前に持つ test / describe は無視する。例: `src/auth.test.ts` の `describe("auth", () => { it("rejects short", ...) })` は `src/auth.test:auth:rejects short`（`src/a.test.mjs` なら `src/a.test`）、describe の外の `it("rejects short", ...)` は `src/auth.test::rejects short`。
