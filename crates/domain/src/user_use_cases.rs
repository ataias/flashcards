use chrono::{DateTime, Utc};

use crate::password::{hash_password, require_password, verify_password};
use crate::store::Store;
use crate::user::{DEFAULT_DECK_NAME, normalize_username};
use crate::{Error, Session, User, UserId};

pub async fn bootstrap_admin<S: Store>(
    store: &S,
    username: &str,
    password: &str,
) -> Result<User, Error> {
    if !store.list_users().await?.is_empty() {
        return Err(Error::BootstrapNotAllowed);
    }
    let username = normalize_username(username)?;
    let password_hash = hash_password(password)?;
    let user = store.create_user(&username, &password_hash, true).await?;
    let assigned = store.assign_orphan_decks(user.id).await?;
    if assigned == 0 {
        store.create_deck(user.id, DEFAULT_DECK_NAME).await?;
    }
    Ok(user)
}

pub async fn authenticate<S: Store>(
    store: &S,
    username: &str,
    password: &str,
    now: DateTime<Utc>,
) -> Result<(User, Session), Error> {
    let username = username.trim();
    let user = store
        .get_user_by_username(username)
        .await?
        .ok_or(Error::InvalidCredentials)?;
    if user.disabled {
        return Err(Error::UserDisabled);
    }
    if !verify_password(password, &user.password_hash)? {
        return Err(Error::InvalidCredentials);
    }
    let session = store.create_session(user.id, now).await?;
    Ok((user, session))
}

pub async fn change_password<S: Store>(
    store: &S,
    user_id: UserId,
    current: &str,
    new_password: &str,
) -> Result<(), Error> {
    let user = require_user(store, user_id).await?;
    if user.disabled {
        return Err(Error::UserDisabled);
    }
    if !verify_password(current, &user.password_hash)? {
        return Err(Error::InvalidCredentials);
    }
    require_password(new_password)?;
    let password_hash = hash_password(new_password)?;
    store.set_password_hash(user_id, &password_hash).await?;
    store.delete_sessions_for_user(user_id).await?;
    Ok(())
}

pub async fn admin_create_user<S: Store>(
    store: &S,
    actor_id: UserId,
    username: &str,
    password: &str,
) -> Result<User, Error> {
    require_admin(store, actor_id).await?;
    let username = normalize_username(username)?;
    if store.get_user_by_username(&username).await?.is_some() {
        return Err(Error::UsernameTaken);
    }
    let password_hash = hash_password(password)?;
    let user = store.create_user(&username, &password_hash, false).await?;
    store.create_deck(user.id, DEFAULT_DECK_NAME).await?;
    Ok(user)
}

pub async fn admin_disable_user<S: Store>(
    store: &S,
    actor_id: UserId,
    target_id: UserId,
) -> Result<User, Error> {
    let actor = require_admin(store, actor_id).await?;
    let target = require_user(store, target_id).await?;
    if !target.disabled {
        reject_last_admin(store, &target).await?;
    }
    reject_self(&actor, target_id)?;
    let user = store.set_disabled(target_id, true).await?;
    store.delete_sessions_for_user(target_id).await?;
    Ok(user)
}

pub async fn admin_delete_user<S: Store>(
    store: &S,
    actor_id: UserId,
    target_id: UserId,
) -> Result<(), Error> {
    let actor = require_admin(store, actor_id).await?;
    let target = require_user(store, target_id).await?;
    reject_last_admin(store, &target).await?;
    reject_self(&actor, target_id)?;
    store.delete_user(target_id).await
}

pub async fn admin_reset_password<S: Store>(
    store: &S,
    actor_id: UserId,
    target_id: UserId,
    new_password: &str,
) -> Result<User, Error> {
    let actor = require_admin(store, actor_id).await?;
    reject_self(&actor, target_id)?;
    let target = require_user(store, target_id).await?;
    if target.disabled {
        return Err(Error::UserDisabled);
    }
    require_password(new_password)?;
    let password_hash = hash_password(new_password)?;
    let user = store.set_password_hash(target_id, &password_hash).await?;
    store.delete_sessions_for_user(target_id).await?;
    Ok(user)
}

async fn require_user<S: Store>(store: &S, user_id: UserId) -> Result<User, Error> {
    store
        .get_user(user_id)
        .await?
        .ok_or(Error::UserNotFound { user_id })
}

async fn require_admin<S: Store>(store: &S, actor_id: UserId) -> Result<User, Error> {
    let actor = require_user(store, actor_id).await?;
    if actor.disabled {
        return Err(Error::UserDisabled);
    }
    if !actor.admin {
        return Err(Error::NotAdmin);
    }
    Ok(actor)
}

fn reject_self(actor: &User, target_id: UserId) -> Result<(), Error> {
    if actor.id == target_id {
        Err(Error::CannotModifySelf)
    } else {
        Ok(())
    }
}

async fn reject_last_admin<S: Store>(store: &S, target: &User) -> Result<(), Error> {
    if !target.admin || target.disabled {
        return Ok(());
    }
    let active_admins = store
        .list_users()
        .await?
        .into_iter()
        .filter(|user| user.admin && !user.disabled)
        .count();
    if active_admins <= 1 {
        Err(Error::CannotRemoveLastAdmin)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake_store::MemStore;
    use crate::{list_home, verify_password};
    use chrono::TimeZone;

    fn noon() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 13, 12, 0, 0).unwrap()
    }

    #[tokio::test]
    async fn bootstrap_creates_only_admin_and_default_when_empty() {
        let store = MemStore::empty();
        let admin = bootstrap_admin(&store, "  Admin  ", "secret")
            .await
            .unwrap();
        assert_eq!(admin.username, "Admin");
        assert!(admin.admin);
        assert!(!admin.disabled);
        assert!(verify_password("secret", &admin.password_hash).unwrap());

        let home = list_home(&store, admin.id, noon()).await.unwrap();
        assert_eq!(home.len(), 1);
        assert_eq!(home[0].deck.name, DEFAULT_DECK_NAME);

        assert!(matches!(
            bootstrap_admin(&store, "other", "secret")
                .await
                .unwrap_err(),
            Error::BootstrapNotAllowed
        ));
    }

    #[tokio::test]
    async fn bootstrap_assigns_orphans_instead_of_seeding_default() {
        let store = MemStore::empty();
        store.insert_orphan_deck("Preexisting");
        let admin = bootstrap_admin(&store, "admin", "secret").await.unwrap();
        let home = list_home(&store, admin.id, noon()).await.unwrap();
        assert_eq!(home.len(), 1);
        assert_eq!(home[0].deck.name, "Preexisting");
    }

    #[tokio::test]
    async fn bootstrap_rejects_invalid_username() {
        let store = MemStore::empty();
        assert!(matches!(
            bootstrap_admin(&store, "ab", "secret").await.unwrap_err(),
            Error::InvalidUsername
        ));
        assert!(store.list_users().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn authenticate_creates_session_and_is_case_insensitive() {
        let store = MemStore::empty();
        let admin = bootstrap_admin(&store, "Admin", "secret").await.unwrap();
        let (user, session) = authenticate(&store, "admin", "secret", noon())
            .await
            .unwrap();
        assert_eq!(user.id, admin.id);
        assert_eq!(session.user_id, admin.id);
        assert_eq!(store.session_count(), 1);
    }

    #[tokio::test]
    async fn authenticate_rejects_bad_password_unknown_and_disabled() {
        let store = MemStore::empty();
        let admin = bootstrap_admin(&store, "admin", "secret").await.unwrap();
        assert!(matches!(
            authenticate(&store, "admin", "nope", noon())
                .await
                .unwrap_err(),
            Error::InvalidCredentials
        ));
        assert!(matches!(
            authenticate(&store, "missing", "secret", noon())
                .await
                .unwrap_err(),
            Error::InvalidCredentials
        ));

        let member = admin_create_user(&store, admin.id, "member", "pw")
            .await
            .unwrap();
        authenticate(&store, "member", "pw", noon()).await.unwrap();
        admin_disable_user(&store, admin.id, member.id)
            .await
            .unwrap();
        assert!(matches!(
            authenticate(&store, "member", "pw", noon())
                .await
                .unwrap_err(),
            Error::UserDisabled
        ));
        assert_eq!(store.sessions_for(member.id), 0);
    }

    #[tokio::test]
    async fn change_password_requires_current_and_wipes_sessions() {
        let store = MemStore::empty();
        let admin = bootstrap_admin(&store, "admin", "secret").await.unwrap();
        authenticate(&store, "admin", "secret", noon())
            .await
            .unwrap();
        assert_eq!(store.session_count(), 1);

        change_password(&store, admin.id, "secret", "newer")
            .await
            .unwrap();
        assert_eq!(store.session_count(), 0);
        authenticate(&store, "admin", "newer", noon())
            .await
            .unwrap();
        assert!(matches!(
            change_password(&store, admin.id, "secret", "x")
                .await
                .unwrap_err(),
            Error::InvalidCredentials
        ));
        assert!(matches!(
            change_password(&store, admin.id, "newer", "")
                .await
                .unwrap_err(),
            Error::EmptyPassword
        ));
    }

    #[tokio::test]
    async fn admin_create_user_seeds_default_and_rejects_taken_names() {
        let store = MemStore::empty();
        let admin = bootstrap_admin(&store, "admin", "secret").await.unwrap();
        let member = admin_create_user(&store, admin.id, "Member", "pw")
            .await
            .unwrap();
        assert_eq!(member.username, "Member");
        assert!(!member.admin);
        let home = list_home(&store, member.id, noon()).await.unwrap();
        assert_eq!(home.len(), 1);
        assert_eq!(home[0].deck.name, DEFAULT_DECK_NAME);

        assert!(matches!(
            admin_create_user(&store, admin.id, "member", "pw")
                .await
                .unwrap_err(),
            Error::UsernameTaken
        ));
        assert!(matches!(
            admin_create_user(&store, member.id, "other", "pw")
                .await
                .unwrap_err(),
            Error::NotAdmin
        ));
    }

    #[tokio::test]
    async fn last_admin_cannot_be_disabled_or_deleted() {
        let store = MemStore::empty();
        let admin = bootstrap_admin(&store, "admin", "secret").await.unwrap();
        let member = admin_create_user(&store, admin.id, "member", "pw")
            .await
            .unwrap();

        assert!(matches!(
            admin_disable_user(&store, admin.id, admin.id)
                .await
                .unwrap_err(),
            Error::CannotRemoveLastAdmin
        ));
        assert!(matches!(
            admin_delete_user(&store, admin.id, admin.id)
                .await
                .unwrap_err(),
            Error::CannotRemoveLastAdmin
        ));
        assert!(matches!(
            admin_reset_password(&store, admin.id, admin.id, "x")
                .await
                .unwrap_err(),
            Error::CannotModifySelf
        ));

        let other_admin = store.insert_user("otheradmin", true);
        admin_disable_user(&store, admin.id, other_admin.id)
            .await
            .unwrap();
        assert!(
            store
                .get_user(other_admin.id)
                .await
                .unwrap()
                .unwrap()
                .disabled
        );
        admin_delete_user(&store, admin.id, member.id)
            .await
            .unwrap();
        assert!(matches!(
            admin_delete_user(&store, admin.id, admin.id)
                .await
                .unwrap_err(),
            Error::CannotRemoveLastAdmin
        ));
    }

    #[tokio::test]
    async fn admin_disable_and_reset_wipe_sessions() {
        let store = MemStore::empty();
        let admin = bootstrap_admin(&store, "admin", "secret").await.unwrap();
        let member = admin_create_user(&store, admin.id, "member", "pw")
            .await
            .unwrap();
        authenticate(&store, "member", "pw", noon()).await.unwrap();
        assert_eq!(store.sessions_for(member.id), 1);

        admin_reset_password(&store, admin.id, member.id, "reset")
            .await
            .unwrap();
        assert_eq!(store.sessions_for(member.id), 0);
        authenticate(&store, "member", "reset", noon())
            .await
            .unwrap();

        admin_disable_user(&store, admin.id, member.id)
            .await
            .unwrap();
        assert_eq!(store.sessions_for(member.id), 0);
        assert!(store.get_user(member.id).await.unwrap().unwrap().disabled);
    }

    #[tokio::test]
    async fn admin_delete_cascades_decks_and_sessions() {
        let store = MemStore::empty();
        let admin = bootstrap_admin(&store, "admin", "secret").await.unwrap();
        let member = admin_create_user(&store, admin.id, "member", "pw")
            .await
            .unwrap();
        authenticate(&store, "member", "pw", noon()).await.unwrap();
        admin_delete_user(&store, admin.id, member.id)
            .await
            .unwrap();
        assert!(store.get_user(member.id).await.unwrap().is_none());
        assert!(
            list_home(&store, member.id, noon())
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(store.sessions_for(member.id), 0);
    }
}
