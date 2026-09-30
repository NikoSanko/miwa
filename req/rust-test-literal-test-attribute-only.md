---
type: Requirement
id: FR-015
---
Rust では、リテラルな `#[test]` 属性が付与された `fn` のみを test として抽出する。`#[tokio::test]` や `#[rstest]` など `#[test]` と異なる属性が付与された `fn` は抽出対象としない。
