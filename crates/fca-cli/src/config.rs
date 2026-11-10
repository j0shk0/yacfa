//! Config file loading and database location resolution.
//!
//! Precedence for the database: `--db` / `$YAFCA_DB` (clap merges these), then
//! `database_url` in the config file, then `$XDG_DATA_HOME/yafca/yafca.db`.
//! Must NOT open the database itself.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::Deserialize;

use fca_core::{DEFAULT_ACCOUNT, DEFAULT_CURRENCY};
use fca_import::ImportConfig;

#[derive(Debug, Default, Deserialize)]
pub struct Config {
    /// File path or `sqlite:` URL.
    pub database_url: Option<String>,
    pub default_currency: Option<String>,
    pub default_account: Option<String>,
    /// `[import.profiles.<name>]` tables.
    #[serde(default)]
    pub import: ImportConfig,
}

impl Config {
    /// Load from `explicit` (must exist) or from the default location (may be missing).
    pub fn load(explicit: Option<&Path>) -> Result<Config> {
        let (path, required) = match explicit {
            Some(p) => (Some(p.to_path_buf()), true),
            None => (default_config_path(), false),
        };
        let Some(path) = path else {
            return Ok(Config::default());
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                toml::from_str(&text).with_context(|| format!("parsing config {}", path.display()))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && !required => {
                Ok(Config::default())
            }
            Err(e) => Err(e).with_context(|| format!("reading config {}", path.display())),
        }
    }

    pub fn currency(&self) -> &str {
        self.default_currency.as_deref().unwrap_or(DEFAULT_CURRENCY)
    }

    pub fn account(&self) -> &str {
        self.default_account.as_deref().unwrap_or(DEFAULT_ACCOUNT)
    }
}

fn project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("", "", "yafca")
}

fn default_config_path() -> Option<PathBuf> {
    project_dirs().map(|d| d.config_dir().join("config.toml"))
}

fn default_db_path() -> PathBuf {
    project_dirs().map_or_else(
        || PathBuf::from("yafca.db"),
        |d| d.data_dir().join("yafca.db"),
    )
}

/// The database to open: flag/env, else config, else the default path.
pub fn resolve_db_target(flag_or_env: Option<&str>, config: &Config) -> String {
    pick_db_target(flag_or_env, config, default_db_path())
}

fn pick_db_target(flag_or_env: Option<&str>, config: &Config, default: PathBuf) -> String {
    flag_or_env
        .map(str::to_string)
        .or_else(|| config.database_url.clone())
        .unwrap_or_else(|| default.display().to_string())
}

/// Create the parent directory of a database file path (URLs are left alone).
pub fn prepare_db_target(target: &str) -> Result<()> {
    if target.starts_with("sqlite:") {
        return Ok(());
    }
    if let Some(dir) = Path::new(target)
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
    {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with_url() -> Config {
        Config {
            database_url: Some("from-config.db".into()),
            ..Config::default()
        }
    }

    #[test]
    fn flag_beats_config_beats_default() {
        let default = PathBuf::from("/data/yafca.db");
        assert_eq!(
            pick_db_target(Some("flag.db"), &config_with_url(), default.clone()),
            "flag.db"
        );
        assert_eq!(
            pick_db_target(None, &config_with_url(), default.clone()),
            "from-config.db"
        );
        assert_eq!(
            pick_db_target(None, &Config::default(), default),
            "/data/yafca.db"
        );
    }

    #[test]
    fn parses_toml_and_applies_defaults() {
        let c: Config =
            toml::from_str("database_url = \"x.db\"\ndefault_account = \"giro\"").unwrap();
        assert_eq!((c.currency(), c.account()), ("EUR", "giro"));
    }

    #[test]
    fn explicit_missing_config_is_an_error() {
        assert!(Config::load(Some(Path::new("/definitely/not/here.toml"))).is_err());
    }
}
