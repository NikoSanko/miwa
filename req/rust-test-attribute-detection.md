---
type: Requirement
id: FR-015
---
Rust では、属性パスの末尾セグメントが `test` である属性が付与された `fn` を、test として抽出する。例えば、`#[test]`、`#[tokio::test]`、`#[async_std::test]`、および引数付きの `#[tokio::test(flavor = "multi_thread")]` が該当する。`#[cfg_attr(x, test)]` のように他の属性の引数の中に現れる `test` は、判定に用いない。

さらに、`.miwa/config.yaml` の `rust.test_attributes` に列挙した名前も test 属性とみなし、属性パスの末尾セグメントと比較する。`rstest` を列挙すると、`#[rstest]` と `#[rstest::rstest]` の両方が対象になる。`rust.test_attributes` の既定は空とする。
