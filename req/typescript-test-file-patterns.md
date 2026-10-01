---
type: Requirement
id: FR-053
---
TypeScript の test の抽出対象ファイルは、`tests` に設定されたディレクトリ配下の `*.test.ts` `*.spec.ts` `*.test.tsx` `*.spec.tsx` とする。`.js` `.mjs` `.cjs` `.mts` `.cts` の拡張子を持つファイルや、`.test` / `.spec` を含まない単なる `.ts` ファイルは対象外とする。`.tsx` ファイルは TSX 構文として解析する。
