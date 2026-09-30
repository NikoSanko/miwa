---
type: Requirement
id: FR-013
---
Go の test の識別子は `<パッケージディレクトリ>:<関数名>` の形式とする。パッケージディレクトリはテストファイルが属するディレクトリのリポジトリ相対パス（`/` 区切り）とし、リポジトリ直下なら `.` とする。例: `internal/auth/login_test.go` の `TestLogin` は `internal/auth:TestLogin`。
