---
type: Requirement
id: FR-053
---
TypeScript / JavaScript の test の抽出対象ファイルは、`tests` に設定されたディレクトリ配下の `<name>.test.<ext>` と `<name>.spec.<ext>` とする。`<ext>` は Vitest の既定 include のうち実在する 8 種、`ts` `tsx` `mts` `cts` `js` `jsx` `mjs` `cjs` とする。`.test` / `.spec` を含まないファイルは、拡張子が上記のいずれであっても対象外とする。

`.ts` `.mts` `.cts` のファイルは TypeScript 構文で解析する。それ以外（`.tsx` `.jsx` `.js` `.mjs` `.cjs`）は TSX 構文（JSX を含められる）で解析する。

同名の `.test.js` と `.test.ts` が共存すると識別子が同じになり、FR-057 により 1 つの test として扱われる。そのため、tsc の出力などの生成物のディレクトリは、`exclude` で除外する（FR-065）か `tests` に含めない。
