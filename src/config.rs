//! `.miwa/config.yaml` and `.miwa/lock.json` paths, and project configuration.

pub const CONFIG_PATH: &str = ".miwa/config.yaml";
pub const LOCK_PATH: &str = ".miwa/lock.json";

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct JudgeConfig {
    pub threshold: f64,
    pub llm_model: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Config {
    pub docs: Vec<String>,
    pub requirements: Vec<String>,
    pub tests: Vec<String>,
    pub diff_base: String,
    pub judge: JudgeConfig,
}

impl Config {
    /// docs=["docs"], requirements=["req"], tests=["."], diff_base="main",
    /// threshold=0.9, llm_model="claude-sonnet-5"
    pub fn default_config() -> Config {
        todo!()
    }

    /// Parse `yaml` into a `Config` and validate it.
    pub fn parse(yaml: &str) -> anyhow::Result<Config> {
        let _ = yaml;
        todo!()
    }

    pub fn to_yaml(&self) -> String {
        todo!()
    }

    /// `threshold` must be finite and within `0..=1`; each `Vec` must be
    /// non-empty; `diff_base` must be non-empty.
    pub fn validate(&self) -> anyhow::Result<()> {
        todo!()
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
                llm_model: "claude-sonnet-5".to_string(),
            },
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

    // [FR-020]
    #[test]
    fn diff_baseが空文字列なら設定エラーになる() {
        let mut config = valid_config();
        config.diff_base = String::new();
        assert!(config.validate().is_err());
    }
}
