use std::env;
use std::net::{AddrParseError, SocketAddr};
use std::path::{Path, PathBuf};

pub const DEFAULT_BIND: &str = "127.0.0.1:3000";

pub const DEFAULT_DB_PATH: &str = "./data/flashcards.db";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub bind: SocketAddr,
    pub db_path: PathBuf,
    /// When true, session/CSRF cookies include `Secure`. Defaults to false on
    /// loopback binds so `cargo run` over HTTP works in Safari; true otherwise.
    pub cookie_secure: bool,
}

#[derive(Debug)]
pub enum ConfigError {
    InvalidBind {
        value: String,
        source: AddrParseError,
    },
    InvalidCookieSecure {
        value: String,
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
            Self::InvalidCookieSecure { value } => {
                write!(
                    f,
                    "invalid FLASHCARDS_COOKIE_SECURE `{value}`: use true, false, 1, or 0"
                )
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
            Self::InvalidCookieSecure { .. } => None,
            Self::CreateDataDir { source, .. } => Some(source),
        }
    }
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let config = Self::parse(
            env_nonempty("FLASHCARDS_BIND"),
            env_nonempty("FLASHCARDS_DB"),
            env_nonempty("FLASHCARDS_COOKIE_SECURE"),
        )?;
        config.ensure_db_parent()?;
        Ok(config)
    }

    /// Empty strings are treated as unset so `Ok("")` from the environment
    /// falls back to the defaults.
    pub fn parse(
        bind: Option<impl AsRef<str>>,
        db_path: Option<impl AsRef<str>>,
        cookie_secure: Option<impl AsRef<str>>,
    ) -> Result<Self, ConfigError> {
        let bind_raw = nonempty_or(bind.as_ref().map(AsRef::as_ref), DEFAULT_BIND).to_string();
        let bind = bind_raw
            .parse()
            .map_err(|source| ConfigError::InvalidBind {
                value: bind_raw,
                source,
            })?;
        let db_path = PathBuf::from(nonempty_or(
            db_path.as_ref().map(AsRef::as_ref),
            DEFAULT_DB_PATH,
        ));
        let cookie_secure = cookie_secure_for(bind, cookie_secure.as_ref().map(AsRef::as_ref))?;
        Ok(Self {
            bind,
            db_path,
            cookie_secure,
        })
    }

    pub fn ensure_db_parent(&self) -> Result<(), ConfigError> {
        ensure_parent_dir(&self.db_path)
    }
}

/// `env::var` yields `Ok("")` when the variable is set but empty; treat that as unset.
fn env_nonempty(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.is_empty())
}

fn nonempty_or<'a>(value: Option<&'a str>, default: &'a str) -> &'a str {
    match value {
        Some(value) if !value.is_empty() => value,
        _ => default,
    }
}

fn cookie_secure_for(bind: SocketAddr, raw: Option<&str>) -> Result<bool, ConfigError> {
    match raw.filter(|value| !value.is_empty()) {
        Some(raw) => parse_cookie_secure_flag(raw),
        None => Ok(!bind.ip().is_loopback()),
    }
}

fn parse_cookie_secure_flag(raw: &str) -> Result<bool, ConfigError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "true" | "1" => Ok(true),
        "false" | "0" => Ok(false),
        _ => Err(ConfigError::InvalidCookieSecure {
            value: raw.to_string(),
        }),
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
        let config = Config::parse(None::<&str>, None::<&str>, None::<&str>).unwrap();
        assert_eq!(config.bind, "127.0.0.1:3000".parse().unwrap());
        assert_eq!(config.db_path, PathBuf::from("./data/flashcards.db"));
        assert!(!config.cookie_secure);
    }

    #[test]
    fn bind_and_db_overrides() {
        let config =
            Config::parse(Some("0.0.0.0:4000"), Some("/tmp/custom.db"), None::<&str>).unwrap();
        assert_eq!(config.bind, "0.0.0.0:4000".parse().unwrap());
        assert_eq!(config.db_path, PathBuf::from("/tmp/custom.db"));
        assert!(config.cookie_secure);
    }

    #[test]
    fn empty_bind_and_db_use_defaults() {
        let config = Config::parse(Some(""), Some(""), Some("")).unwrap();
        assert_eq!(config.bind, "127.0.0.1:3000".parse().unwrap());
        assert_eq!(config.db_path, PathBuf::from("./data/flashcards.db"));
        assert!(!config.cookie_secure);
    }

    #[test]
    fn invalid_bind_is_error() {
        let err = Config::parse(Some("not-an-addr"), None::<&str>, None::<&str>).unwrap_err();
        match err {
            ConfigError::InvalidBind { value, .. } => assert_eq!(value, "not-an-addr"),
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn cookie_secure_follows_bind_and_env() {
        assert!(
            !Config::parse(Some("[::1]:3000"), None::<&str>, None::<&str>)
                .unwrap()
                .cookie_secure
        );
        assert!(
            Config::parse(Some("127.0.0.1:3000"), None::<&str>, Some("true"))
                .unwrap()
                .cookie_secure
        );
        assert!(
            !Config::parse(Some("0.0.0.0:3000"), None::<&str>, Some("0"))
                .unwrap()
                .cookie_secure
        );
        match Config::parse(None::<&str>, None::<&str>, Some("yes")).unwrap_err() {
            ConfigError::InvalidCookieSecure { value } => assert_eq!(value, "yes"),
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
            cookie_secure: false,
        };
        let _ = std::fs::remove_dir_all(&tmp);
        assert!(!db_path.parent().unwrap().exists());
        config.ensure_db_parent().unwrap();
        assert!(db_path.parent().unwrap().is_dir());
        assert!(!db_path.exists(), "must not open or create the SQLite file");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
