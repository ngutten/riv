use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub defaults: Defaults,
    #[serde(default)]
    pub tools: HashMap<String, ToolConfig>,
}

#[derive(Debug, Deserialize, Default)]
pub struct Defaults {
    pub view: Option<String>,
    pub sort: Option<String>,
    pub sort_desc: Option<bool>,
    pub thumb_size: Option<u32>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ToolConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

impl Config {
    pub fn load() -> Self {
        let config_path = config_path();
        if let Some(path) = config_path {
            if path.exists() {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(config) = toml::from_str(&content) {
                        return config;
                    }
                }
            }
        }
        Config::default()
    }

    pub fn tool_command(&self, name: &str) -> (String, Vec<String>) {
        if let Some(tool) = self.tools.get(name) {
            (tool.command.clone(), tool.args.clone())
        } else {
            (name.to_string(), Vec::new())
        }
    }
}

fn config_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "riv")
        .map(|dirs| dirs.config_dir().join("config.toml"))
}
