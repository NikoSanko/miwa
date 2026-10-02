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
        Ok(())
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
}
