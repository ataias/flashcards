use std::env;
use std::net::{AddrParseError, SocketAddr};
use std::path::{Path, PathBuf};

/// Default listen address from SPEC-v1.
pub const DEFAULT_BIND: &str = "127.0.0.1:3000";

/// Default SQLite path from SPEC-v1. The file is not opened here (issue #4).
pub const DEFAULT_DB_PATH: &str = "./data/flashcards.db";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub bind: SocketAddr,
    pub db_path: PathBuf,
}

#[derive(Debug)]
pub enum ConfigError {
    InvalidBind {
        value: String,
        source: AddrParseError,
    },
    CreateDataDir {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidBind { value, source } => {
                write!(f, "invalid FLASHCARDS_BIND `{value}`: {source}")
            }
            Self::CreateDataDir { path, source } => {
                write!(
                    f,
                    "failed to create directory for FLASHCARDS_DB ({}): {source}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidBind { source, .. } => Some(source),
            Self::CreateDataDir { source, .. } => Some(source),
        }
    }
}

impl Config {
    /// Parse `FLASHCARDS_BIND` / `FLASHCARDS_DB` and create the DB parent directory.
    pub fn from_env() -> Result<Self, ConfigError> {
        let config = Self::parse(
            env::var("FLASHCARDS_BIND").ok(),
            env::var("FLASHCARDS_DB").ok(),
        )?;
        config.ensure_db_parent()?;
        Ok(config)
    }

    /// Parse bind/DB values without touching the filesystem.
    pub fn parse(
        bind: Option<impl AsRef<str>>,
        db_path: Option<impl AsRef<str>>,
    ) -> Result<Self, ConfigError> {
        let bind_raw = bind
            .as_ref()
            .map(AsRef::as_ref)
            .unwrap_or(DEFAULT_BIND)
            .to_string();
        let bind = bind_raw
            .parse()
            .map_err(|source| ConfigError::InvalidBind {
                value: bind_raw,
                source,
            })?;
        let db_path = PathBuf::from(
            db_path
                .as_ref()
                .map(AsRef::as_ref)
                .unwrap_or(DEFAULT_DB_PATH),
        );
        Ok(Self { bind, db_path })
    }

    /// Create the parent directory of `db_path` when it is missing.
    pub fn ensure_db_parent(&self) -> Result<(), ConfigError> {
        ensure_parent_dir(&self.db_path)
    }
}

fn ensure_parent_dir(db_path: &Path) -> Result<(), ConfigError> {
    let Some(parent) = db_path.parent().filter(|p| !p.as_os_str().is_empty()) else {
        return Ok(());
    };
    std::fs::create_dir_all(parent).map_err(|source| ConfigError::CreateDataDir {
        path: parent.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_spec() {
        let config = Config::parse(None::<&str>, None::<&str>).unwrap();
        assert_eq!(config.bind, "127.0.0.1:3000".parse().unwrap());
        assert_eq!(config.db_path, PathBuf::from("./data/flashcards.db"));
    }

    #[test]
    fn bind_and_db_overrides() {
        let config = Config::parse(Some("0.0.0.0:4000"), Some("/tmp/custom.db")).unwrap();
        assert_eq!(config.bind, "0.0.0.0:4000".parse().unwrap());
        assert_eq!(config.db_path, PathBuf::from("/tmp/custom.db"));
    }

    #[test]
    fn invalid_bind_is_error() {
        let err = Config::parse(Some("not-an-addr"), None::<&str>).unwrap_err();
        match err {
            ConfigError::InvalidBind { value, .. } => assert_eq!(value, "not-an-addr"),
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn ensure_db_parent_creates_missing_directory() {
        let tmp = std::env::temp_dir().join(format!(
            "flashcards-config-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db_path = tmp.join("nested").join("flashcards.db");
        let config = Config {
            bind: DEFAULT_BIND.parse().unwrap(),
            db_path: db_path.clone(),
        };
        let _ = std::fs::remove_dir_all(&tmp);
        assert!(!db_path.parent().unwrap().exists());
        config.ensure_db_parent().unwrap();
        assert!(db_path.parent().unwrap().is_dir());
        assert!(!db_path.exists(), "must not open or create the SQLite file");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
