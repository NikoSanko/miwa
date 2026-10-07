//! `.miwa/config.yaml` and `.miwa/lock.json` paths, and project configuration.

pub const CONFIG_PATH: &str = ".miwa/config.yaml";
pub const LOCK_PATH: &str = ".miwa/lock.json";

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct JudgeConfig {
    pub threshold: f64,
    #[serde(default = "default_max_body_bytes")]
    pub max_body_bytes: usize,
}

fn default_max_body_bytes() -> usize {
    crate::judge::DEFAULT_MAX_BODY_BYTES
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RustConfig {
    #[serde(default)]
    pub test_attributes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Config {
    pub docs: Vec<String>,
    pub requirements: Vec<String>,
    pub tests: Vec<String>,
    pub diff_base: String,
    pub judge: JudgeConfig,
    #[serde(default)]
    pub rust: RustConfig,
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Index roots (directories whose `index.md` lists the documents below).
    #[serde(default)]
    pub index: Vec<String>,
    /// Settings for `miwa site`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site: Option<SiteConfig>,
}

/// `site` section of `.miwa/config.yaml`.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct SiteConfig {
    #[serde(
        default,
        deserialize_with = "deserialize_site_title",
        skip_serializing_if = "Option::is_none"
    )]
    pub title: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_site_description",
        skip_serializing_if = "Option::is_none"
    )]
    pub description: Option<String>,
}

/// `null` and an absent key are "unspecified"; anything but a string is an
/// error (YAML numbers and booleans must not be coerced to strings).
fn deserialize_site_string<'de, D>(d: D, field: &str) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    match Option::<serde_yaml_ng::Value>::deserialize(d)? {
        None => Ok(None),
        Some(serde_yaml_ng::Value::String(s)) => Ok(Some(s)),
        Some(_) => Err(serde::de::Error::custom(format!(
            "site.{field} must be a string"
        ))),
    }
}

fn deserialize_site_title<'de, D>(d: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserialize_site_string(d, "title")
}

fn deserialize_site_description<'de, D>(d: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserialize_site_string(d, "description")
}

impl SiteConfig {
    /// `title` with surrounding whitespace removed.
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref().map(str::trim)
    }

    /// `description` with surrounding whitespace removed.
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref().map(str::trim)
    }

    fn validate(&self) -> anyhow::Result<()> {
        for (field, value) in [("title", &self.title), ("description", &self.description)] {
            let Some(v) = value else { continue };
            let v = v.trim();
            if v.is_empty() {
                anyhow::bail!("site.{field} must not be empty");
            }
            if v.contains('\n') || v.contains('\r') {
                anyhow::bail!("site.{field} must be a single line");
            }
        }
        Ok(())
    }
}

impl Config {
    /// docs=["docs"], requirements=["req"], tests=["."], diff_base="main"
    pub fn default_config() -> Config {
        Config {
            docs: vec!["docs".to_string()],
            requirements: vec!["req".to_string()],
            tests: vec![".".to_string()],
            diff_base: "main".to_string(),
            judge: JudgeConfig {
                threshold: 0.9,
                max_body_bytes: crate::judge::DEFAULT_MAX_BODY_BYTES,
            },
            rust: RustConfig::default(),
            exclude: vec![],
            index: vec!["docs".to_string()],
            site: None,
        }
    }

    /// Parse `yaml` into a `Config` and validate it.
    pub fn parse(yaml: &str) -> anyhow::Result<Config> {
        let config: Config = serde_yaml_ng::from_str(yaml)?;
        config.validate()?;
        Ok(config)
    }

    pub fn to_yaml(&self) -> String {
        serde_yaml_ng::to_string(self).expect("Config serializes to yaml")
    }

    /// `index` rules: each entry must be non-empty after normalization, equal to
    /// or under a `docs` / `requirements` entry, not contain an excluded
    /// directory name (`exclude` or the default exclusions) as a segment, and no
    /// two entries may be equal or nested in each other.
    ///
    /// `threshold` must be finite and within `0..=1`; each `Vec` must be
    /// non-empty; `diff_base` must be non-empty; each `rust.test_attributes`
    /// entry must be a non-empty `[A-Za-z0-9_]+` string; each `exclude` entry must
    /// be a bare directory name (no whitespace, separators, globs, `.`/`..`); and no
    /// `docs`/`requirements`/`tests` entry may lie under an excluded directory
    /// (`exclude` or the default exclusions).
    pub fn validate(&self) -> anyhow::Result<()> {
        if !self.judge.threshold.is_finite() || !(0.0..=1.0).contains(&self.judge.threshold) {
            anyhow::bail!(
                "judge.threshold must be within 0..=1, got {}",
                self.judge.threshold
            );
        }
        if self.judge.max_body_bytes < 1 {
            anyhow::bail!(
                "judge.max_body_bytes must be >= 1, got {}",
                self.judge.max_body_bytes
            );
        }
        if self.docs.is_empty() {
            anyhow::bail!("docs must not be empty");
        }
        if self.requirements.is_empty() {
            anyhow::bail!("requirements must not be empty");
        }
        if self.tests.is_empty() {
            anyhow::bail!("tests must not be empty");
        }
        if self.diff_base.is_empty() {
            anyhow::bail!("diff_base must not be empty");
        }
        for attr in &self.rust.test_attributes {
            if attr.is_empty() || !attr.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                anyhow::bail!(
                    "rust.test_attributes entries must be the last path segment only (e.g. `rstest`, not `rstest::rstest`): got `{attr}`"
                );
            }
        }
        for e in &self.exclude {
            if e.trim().is_empty()
                || e.chars()
                    .any(|c| c.is_whitespace() || matches!(c, '/' | '\\' | '*' | '?' | '['))
                || e == "."
                || e == ".."
            {
                anyhow::bail!(
                    "exclude entries must be a directory name only (no paths or globs): got `{e}`"
                );
            }
        }
        for (field, entries) in [
            ("docs", &self.docs),
            ("requirements", &self.requirements),
            ("tests", &self.tests),
        ] {
            for p in entries {
                check_path_segments(field, p)?;
                for seg in p
                    .split('/')
                    .map(str::trim)
                    .filter(|s| !s.is_empty() && *s != ".")
                {
                    let hit = self
                        .exclude
                        .iter()
                        .map(|s| s.as_str())
                        .chain(crate::source::DEFAULT_EXCLUDED_DIRS)
                        .find(|name| *name == seg);
                    if let Some(name) = hit {
                        anyhow::bail!(
                            "{field} entry `{p}` is under an excluded directory `{name}`"
                        );
                    }
                }
            }
        }
        self.validate_index()?;
        if let Some(site) = &self.site {
            site.validate()?;
        }
        Ok(())
    }

    fn validate_index(&self) -> anyhow::Result<()> {
        let normalized: Vec<String> = self.index.iter().map(|e| normalize_dir(e)).collect();
        for (raw, n) in self.index.iter().zip(&normalized) {
            if raw.trim().is_empty() {
                anyhow::bail!("index entries must not be empty");
            }
            if n == "." {
                anyhow::bail!("index entry `{raw}` must not be the whole repository (`.`)");
            }
            check_path_segments("index", raw)?;
            let in_scope = self
                .docs
                .iter()
                .chain(self.requirements.iter())
                .any(|d| is_under(n, &normalize_dir(d)));
            if !in_scope {
                anyhow::bail!(
                    "index entry `{raw}` must be equal to or under a docs / requirements entry"
                );
            }
            for seg in n.split('/').filter(|s| !s.is_empty() && *s != ".") {
                let hit = self
                    .exclude
                    .iter()
                    .map(|s| s.as_str())
                    .chain(crate::source::DEFAULT_EXCLUDED_DIRS)
                    .find(|name| *name == seg);
                if let Some(name) = hit {
                    anyhow::bail!("index entry `{raw}` is under an excluded directory `{name}`");
                }
            }
        }
        for (i, a) in normalized.iter().enumerate() {
            for (j, b) in normalized.iter().enumerate() {
                if i != j && is_under(a, b) {
                    anyhow::bail!(
                        "index entries `{}` and `{}` must not be equal or nested",
                        self.index[i],
                        self.index[j]
                    );
                }
            }
        }
        Ok(())
    }
}

/// Rejects configured directory entries containing `..` or empty (`//`)
/// segments after normalization. `.` (the whole repo) is not checked here.
fn check_path_segments(field: &str, raw: &str) -> anyhow::Result<()> {
    let n = normalize_dir(raw);
    if n == "." {
        return Ok(());
    }
    for seg in n.split('/') {
        if seg.is_empty() {
            anyhow::bail!("{field} entry `{raw}` must not contain empty path segments");
        }
        if seg == ".." {
            anyhow::bail!("{field} entry `{raw}` must not contain `..`");
        }
    }
    Ok(())
}

/// Joins a normalized directory `root` and a relative path (`.` is the repo
/// root, so the result has no `./` prefix).
pub(crate) fn join_under(root: &str, rel: &str) -> String {
    if root == "." || root.is_empty() {
        rel.to_string()
    } else {
        format!("{root}/{rel}")
    }
}

/// Whether `path` is `dir` or lies under it. `dir` must be normalized by
/// [`normalize_dir`]; `.` (the whole repo) contains everything.
pub(crate) fn is_under(path: &str, dir: &str) -> bool {
    dir == "."
        || path == dir
        || path
            .strip_prefix(dir)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// Normalizes a configured directory entry: strips a leading `./`, strips a
/// trailing `/`, and maps `.` / `./` / `` (empty) to `.` (the whole repo).
pub(crate) fn normalize_dir(d: &str) -> String {
    let mut s = d.trim();
    while let Some(rest) = s.strip_prefix("./") {
        s = rest;
    }
    let s = s.trim_end_matches('/');
    if s.is_empty() {
        ".".to_string()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_config() -> Config {
        Config {
            docs: vec!["docs".to_string()],
            requirements: vec!["req".to_string()],
            tests: vec![".".to_string()],
            diff_base: "main".to_string(),
            judge: JudgeConfig {
                threshold: 0.9,
                max_body_bytes: 200_000,
            },
            rust: RustConfig::default(),
            exclude: vec![],
            index: vec![],
            site: None,
        }
    }

    // [FR-020]
    #[test]
    fn thresholdが0から1の範囲内であれば妥当() {
        assert!(valid_config().validate().is_ok());
    }

    // [FR-020]
    #[test]
    fn thresholdが範囲外なら設定エラーになる() {
        let mut config = valid_config();
        config.judge.threshold = 1.5;
        assert!(config.validate().is_err());

        let mut config = valid_config();
        config.judge.threshold = -0.1;
        assert!(config.validate().is_err());
    }

    // [FR-020]
    #[test]
    fn docs_requirements_testsのいずれかが空なら設定エラーになる() {
        let mut config = valid_config();
        config.docs = vec![];
        assert!(config.validate().is_err());

        let mut config = valid_config();
        config.requirements = vec![];
        assert!(config.validate().is_err());

        let mut config = valid_config();
        config.tests = vec![];
        assert!(config.validate().is_err());
    }

    // [FR-019]
    #[test]
    fn 既定値のthresholdは0_9で妥当な設定である() {
        let config = Config::default_config();
        assert_eq!(config.judge.threshold, 0.9);
        assert!(config.validate().is_ok());
    }

    // [FR-020]
    #[test]
    fn diff_baseが空文字列なら設定エラーになる() {
        let mut config = valid_config();
        config.diff_base = String::new();
        assert!(config.validate().is_err());
    }

    // [FR-019]
    #[test]
    fn 既定値のmax_body_bytesは200000で妥当な設定である() {
        let config = Config::default_config();
        assert_eq!(config.judge.max_body_bytes, 200_000);
        assert!(config.validate().is_ok());
    }

    // [FR-020]
    #[test]
    fn max_body_bytesが0なら設定エラーになる() {
        let mut config = valid_config();
        config.judge.max_body_bytes = 0;
        assert!(config.validate().is_err());
        config.judge.max_body_bytes = 1;
        assert!(config.validate().is_ok());
    }

    // [FR-019]
    #[test]
    fn max_body_bytesを省略したyamlは既定値で読める() {
        let yaml = "docs:\n  - docs\nrequirements:\n  - req\ntests:\n  - .\ndiff_base: main\njudge:\n  threshold: 0.9\n";
        let config = Config::parse(yaml).unwrap();
        assert_eq!(config.judge.max_body_bytes, 200_000);
    }

    // [FR-020]
    #[test]
    fn max_body_bytesが整数以外のyamlはparseエラーになる() {
        let base = "docs:\n  - docs\nrequirements:\n  - req\ntests:\n  - .\ndiff_base: main\njudge:\n  threshold: 0.9\n";
        for bad in ["1.5", "\"abc\"", "0", "-1"] {
            let yaml = format!("{base}  max_body_bytes: {bad}\n");
            assert!(Config::parse(&yaml).is_err(), "{bad}");
        }
        let yaml = format!("{base}  max_body_bytes: 1\n");
        assert_eq!(Config::parse(&yaml).unwrap().judge.max_body_bytes, 1);
    }

    const BASE_YAML: &str = "docs:\n  - docs\nrequirements:\n  - req\ntests:\n  - .\ndiff_base: main\njudge:\n  threshold: 0.9\n";

    // [FR-058]
    #[test]
    fn rustキーを省略_null_空マップのいずれでもtest_attributesは空になる() {
        for extra in ["", "rust:\n", "rust: {}\n", "rust:\n  test_attributes:\n"] {
            let yaml = format!("{BASE_YAML}{extra}");
            let config = Config::parse(&yaml).unwrap_or_else(|e| panic!("{extra:?}: {e}"));
            assert!(config.rust.test_attributes.is_empty(), "{extra:?}");
        }
    }

    // [FR-058]
    #[test]
    fn rustのtest_attributesを指定するとそのまま読める() {
        let yaml = format!("{BASE_YAML}rust:\n  test_attributes: [\"rstest\", \"test_case_2\"]\n");
        let config = Config::parse(&yaml).unwrap();
        assert_eq!(
            config.rust.test_attributes,
            vec!["rstest".to_string(), "test_case_2".to_string()]
        );
    }

    // [FR-058]
    #[test]
    fn rustのtest_attributesにパス区切りや空文字があるとparseエラーになる() {
        for bad in ["\"rs::test\"", "\"\"", "\"my-attr\"", "\"a b\""] {
            let yaml = format!("{BASE_YAML}rust:\n  test_attributes: [{bad}]\n");
            let err = Config::parse(&yaml).expect_err(bad);
            assert!(err.to_string().contains("segment"), "{bad}: {err}");
        }
    }

    // [FR-019]
    #[test]
    fn 既定のconfigのyamlはrustのtest_attributesを空配列で含む() {
        let yaml = Config::default_config().to_yaml();
        let value: serde_yaml_ng::Value = serde_yaml_ng::from_str(&yaml).unwrap();
        let attrs = value["rust"]["test_attributes"]
            .as_sequence()
            .expect("sequence");
        assert!(attrs.is_empty());
    }

    // [FR-065]
    #[test]
    fn excludeを省略したyamlは空になる() {
        let config = Config::parse(BASE_YAML).unwrap();
        assert!(config.exclude.is_empty());
    }

    // [FR-065]
    #[test]
    fn excludeを指定するとそのまま読める() {
        let yaml = format!("{BASE_YAML}exclude: [\".next\", \"build\"]\n");
        let config = Config::parse(&yaml).unwrap();
        assert_eq!(
            config.exclude,
            vec![".next".to_string(), "build".to_string()]
        );
    }

    // [FR-065]
    #[test]
    fn excludeにディレクトリ名として不正な値があるとparseエラーになる() {
        for bad in [
            "\" \"",
            "\"a b\"",
            "\"a/b\"",
            "\"a\\\\b\"",
            "\".\"",
            "\"..\"",
            "\"*.js\"",
            "\"a?\"",
            "\"a[b]\"",
        ] {
            let yaml = format!("{BASE_YAML}exclude: [{bad}]\n");
            let err = Config::parse(&yaml).expect_err(bad);
            assert!(err.to_string().contains("directory name"), "{bad}: {err}");
        }
    }

    // [FR-065]
    #[test]
    fn docsやrequirementsやtestsのパスが除外名を含むと設定エラーになる() {
        let yaml = "docs:\n  - docs\nrequirements:\n  - req\ntests:\n  - build/tests\ndiff_base: main\njudge:\n  threshold: 0.9\nexclude: [build]\n";
        let err = Config::parse(yaml).unwrap_err();
        assert!(err.to_string().contains("excluded directory"), "{err}");

        let yaml = "docs:\n  - dist\nrequirements:\n  - req\ntests:\n  - .\ndiff_base: main\njudge:\n  threshold: 0.9\n";
        let err = Config::parse(yaml).unwrap_err();
        assert!(err.to_string().contains("excluded directory"), "{err}");

        let yaml = "docs:\n  - docs\nrequirements:\n  - req\ntests:\n  - \" dist\"\ndiff_base: main\njudge:\n  threshold: 0.9\n";
        let err = Config::parse(yaml).unwrap_err();
        assert!(err.to_string().contains("excluded directory"), "{err}");
    }

    // [FR-019]
    #[test]
    fn 既定のconfigのyamlはexcludeを空配列で含む() {
        let yaml = Config::default_config().to_yaml();
        let value: serde_yaml_ng::Value = serde_yaml_ng::from_str(&yaml).unwrap();
        let exclude = value["exclude"].as_sequence().expect("sequence");
        assert!(exclude.is_empty());
    }

    fn with_index(index: &[&str], extra: &str) -> anyhow::Result<Config> {
        let list: String = index.iter().map(|e| format!("  - \"{e}\"\n")).collect();
        let yaml = format!("{BASE_YAML}{extra}index:\n{list}");
        Config::parse(&yaml)
    }

    // [FR-066]
    #[test]
    fn indexを省略すると空の一覧として扱われる() {
        let config = Config::parse(BASE_YAML).unwrap();
        assert!(config.index.is_empty());
    }

    // [FR-066]
    #[test]
    fn indexはdocsかrequirements配下なら正規化されて受理される() {
        assert!(with_index(&["docs"], "").is_ok());
        assert!(with_index(&["req"], "").is_ok());
        assert!(with_index(&["./docs/guide/"], "").is_ok());
        assert!(with_index(&["docs", "req/sub"], "").is_ok());
        assert!(with_index(&["docs/a", "docs/b"], "").is_ok());
    }

    // [FR-066]
    #[test]
    fn indexの要素が空なら設定エラーになる() {
        assert!(with_index(&[""], "").is_err());
        assert!(with_index(&["  "], "").is_err());
    }

    // [FR-066]
    #[test]
    fn indexがdocsでもrequirementsでもない場所なら設定エラーになる() {
        assert!(with_index(&["other"], "").is_err());
        assert!(with_index(&["docsx"], "").is_err());
        assert!(with_index(&["."], "").is_err());
        assert!(with_index(&["src/docs"], "").is_err());
    }

    // [FR-066]
    #[test]
    fn indexが除外名のセグメントを含むと設定エラーになる() {
        assert!(with_index(&["docs/node_modules"], "").is_err());
        assert!(with_index(&["docs/vendor/x"], "exclude: [vendor]\n").is_err());
        assert!(with_index(&["docs/vendor"], "").is_ok());
    }

    // [FR-066]
    #[test]
    fn index同士が同一または包含関係なら設定エラーになる() {
        assert!(with_index(&["docs", "docs"], "").is_err());
        assert!(with_index(&["docs", "docs/a"], "").is_err());
        assert!(with_index(&["docs/a", "docs"], "").is_err());
        assert!(with_index(&["docs/", "./docs"], "").is_err());
    }

    // [FR-019]
    #[test]
    fn 既定のconfigのindexはdocsを1件含む() {
        let config = Config::default_config();
        assert_eq!(config.index, vec!["docs".to_string()]);
        assert!(config.validate().is_ok());
    }

    // [FR-066]
    #[test]
    fn indexが正規化後にリポジトリ全体_ドットになると設定エラーになる() {
        for e in [".", "./", "././"] {
            let yaml = format!(
                "docs:\n  - .\nrequirements:\n  - req\ntests:\n  - .\ndiff_base: main\njudge:\n  threshold: 0.9\nindex:\n  - \"{e}\"\n"
            );
            assert!(Config::parse(&yaml).is_err(), "{e}");
        }
    }

    // [FR-066]
    #[test]
    fn indexに親ディレクトリや空セグメントがあると設定エラーになる() {
        assert!(with_index(&["docs/../docs"], "").is_err());
        assert!(with_index(&["docs//a"], "").is_err());
    }

    // [FR-066]
    #[test]
    fn docsとrequirementsとtestsに親ディレクトリや空セグメントがあると設定エラーになる() {
        for field in ["docs", "requirements", "tests"] {
            for bad in ["a/../b", "a//b"] {
                let mut yaml = String::new();
                for f in ["docs", "requirements", "tests"] {
                    let v = if f == field { bad } else { "ok" };
                    yaml.push_str(&format!("{f}:\n  - \"{v}\"\n"));
                }
                yaml.push_str("diff_base: main\njudge:\n  threshold: 0.9\n");
                assert!(Config::parse(&yaml).is_err(), "{field}: {bad}");
            }
        }
    }

    fn site_yaml(site: &str) -> String {
        format!("{BASE_YAML}site:\n{site}")
    }

    // [FR-094]
    #[test]
    fn siteの文字列でない値はフィールド名つきの設定エラーになる() {
        for field in ["title", "description"] {
            for bad in ["1", "true", "[a]", "{a: b}"] {
                let err = Config::parse(&site_yaml(&format!("  {field}: {bad}\n")))
                    .unwrap_err()
                    .to_string();
                assert!(
                    err.contains(&format!("site.{field}")),
                    "{field} {bad}: {err}"
                );
            }
        }
    }

    // [FR-094]
    #[test]
    fn siteのnullと空の節は未指定になる() {
        for site in ["  title: null\n  description: ~\n", "  {}\n"] {
            let c = Config::parse(&site_yaml(site)).unwrap();
            let s = c.site.unwrap();
            assert_eq!(s.title(), None, "{site}");
            assert_eq!(s.description(), None, "{site}");
        }
        assert!(Config::parse(&site_yaml("  null\n"))
            .unwrap()
            .site
            .is_none());
    }

    // [FR-094]
    #[test]
    fn siteの値は前後の空白が除かれ往復しても変わらない() {
        let c = Config::parse(&site_yaml(
            "  title: \"  題名  \"\n  description: \" 要約 \"\n",
        ))
        .unwrap();
        let s = c.site.as_ref().unwrap();
        assert_eq!(s.title(), Some("題名"));
        assert_eq!(s.description(), Some("要約"));
        let again = Config::parse(&c.to_yaml()).unwrap();
        let s2 = again.site.as_ref().unwrap();
        assert_eq!(s2.title(), Some("題名"));
        assert_eq!(s2.description(), Some("要約"));
    }
}
