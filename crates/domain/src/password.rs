use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use rand_core::OsRng;

use crate::Error;

/// Hash a password with Argon2id. Empty passwords are rejected.
pub fn hash_password(password: &str) -> Result<String, Error> {
    require_password(password)?;
    let salt = SaltString::generate(&mut OsRng);
    hasher()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| Error::PasswordHash)
}

/// Verify `password` against a stored PHC Argon2id hash.
pub fn verify_password(password: &str, hash: &str) -> Result<bool, Error> {
    let parsed = PasswordHash::new(hash).map_err(|_| Error::PasswordHash)?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

pub(crate) fn require_password(password: &str) -> Result<(), Error> {
    if password.is_empty() {
        Err(Error::EmptyPassword)
    } else {
        Ok(())
    }
}

fn hasher() -> Argon2<'static> {
    #[cfg(test)]
    {
        use argon2::{Algorithm, Params, Version};
        let params = Params::new(8, 1, 1, None).expect("test Argon2id params");
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
    }
    #[cfg(not(test))]
    {
        Argon2::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_then_verify_accepts_matching_password() {
        let hash = hash_password("secret").unwrap();
        assert!(hash.contains("argon2id"));
        assert!(verify_password("secret", &hash).unwrap());
        assert!(!verify_password("other", &hash).unwrap());
    }

    #[test]
    fn empty_password_is_rejected() {
        assert!(matches!(
            hash_password("").unwrap_err(),
            Error::EmptyPassword
        ));
    }

    #[test]
    fn verify_rejects_corrupt_hash() {
        assert!(matches!(
            verify_password("secret", "not-a-phc-hash").unwrap_err(),
            Error::PasswordHash
        ));
    }
}
