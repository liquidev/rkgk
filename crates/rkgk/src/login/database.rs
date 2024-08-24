use std::path::PathBuf;

use argon2::{
    password_hash::{PasswordHasher, SaltString},
    Argon2, PasswordHash, PasswordVerifier,
};
use base64::Engine;
use chrono::Utc;
use eyre::{eyre, Context};
use rand::{RngCore, SeedableRng};
use rusqlite::{Connection, OptionalExtension};
use tokio::sync::{mpsc, oneshot};
use tracing::instrument;

use super::UserId;

pub struct Settings {
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct Database {
    command_tx: mpsc::Sender<Command>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginStatus {
    ValidUser,
    InvalidUser,
}

#[derive(Debug, Clone)]
pub struct UserInfo {
    pub nickname: String,
}

#[derive(Debug, Clone)]
pub struct NewUser {
    pub user_id: UserId,
    // NOTE: This is kind of unusual, but rkgk generates a secret server-side and sends it to the
    // user, rather than having the user come up with a password.
    pub secret: String,
}

enum Command {
    NewUser {
        nickname: String,
        reply: oneshot::Sender<eyre::Result<NewUser>>,
    },

    LogIn {
        user_id: UserId,
        secret: Vec<u8>,
        reply: oneshot::Sender<LoginStatus>,
    },

    UserInfo {
        user_id: UserId,
        reply: oneshot::Sender<eyre::Result<Option<UserInfo>>>,
    },
}

impl Database {
    pub const MIN_SECRET_LEN: usize = 256;
    pub const MAX_SECRET_LEN: usize = 256;
    pub const CURRENT_SECRET_LEN: usize = 256;

    pub async fn new_user(&self, nickname: String) -> eyre::Result<NewUser> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(Command::NewUser {
                nickname,
                reply: tx,
            })
            .await
            .map_err(|_| eyre!("database is too contended"))?;
        rx.await.map_err(|_| eyre!("database is not available"))?
    }

    pub async fn log_in(&self, user_id: UserId, secret: Vec<u8>) -> eyre::Result<LoginStatus> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(Command::LogIn {
                user_id,
                secret,
                reply: tx,
            })
            .await
            .map_err(|_| eyre!("database is too contended"))?;
        rx.await.map_err(|_| eyre!("database is not available"))
    }

    pub async fn user_info(&self, user_id: UserId) -> eyre::Result<Option<UserInfo>> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(Command::UserInfo { user_id, reply: tx })
            .await
            .map_err(|_| eyre!("database is too contended"))?;
        rx.await.map_err(|_| eyre!("database is not available"))?
    }
}

#[instrument(name = "login::database::start", skip(settings))]
pub fn start(settings: &Settings) -> eyre::Result<Database> {
    let db = Connection::open(&settings.path).context("cannot open login database")?;

    db.execute_batch(
        r#"
            PRAGMA application_id = 0x726B674C; -- rkgL
        
            CREATE TABLE IF NOT EXISTS
            t_users (
                user_index           INTEGER PRIMARY KEY,
                long_user_id         BLOB NOT NULL,
                secret_hash          BLOB NOT NULL,
                nickname             TEXT NOT NULL,
                last_login_timestamp INTEGER NOT NULL
            );
        "#,
    )?;

    let (command_tx, mut command_rx) = mpsc::channel(8);

    let mut user_id_rng = rand_chacha::ChaCha20Rng::from_entropy();

    let mut secret_rng = rand_chacha::ChaCha20Rng::from_entropy();
    let argon2 = Argon2::default();

    std::thread::Builder::new()
        .name("login database thread".into())
        .spawn(move || {
            let mut s_insert_user = db
                .prepare(
                    r#"
                        INSERT INTO t_users
                        (long_user_id, nickname, secret_hash, last_login_timestamp)
                        VALUES (?, ?, ?, ?);
                    "#,
                )
                .unwrap();

            let mut s_get_secret = db
                .prepare(
                    r#"
                        SELECT secret_hash
                        FROM t_users
                        WHERE long_user_id = ?;
                    "#,
                )
                .unwrap();

            let mut s_log_in = db
                .prepare(
                    r#"
                        UPDATE OR ABORT t_users
                        SET last_login_timestamp = ?
                        WHERE long_user_id = ?;
                    "#,
                )
                .unwrap();

            let mut s_user_info = db
                .prepare(
                    r#"
                        SELECT nickname
                        FROM t_users
                        WHERE long_user_id = ?
                        LIMIT 1;
                    "#,
                )
                .unwrap();

            while let Some(command) = command_rx.blocking_recv() {
                match command {
                    Command::NewUser { nickname, reply } => {
                        let result = || -> eyre::Result<_> {
                            let user_id = UserId::new(&mut user_id_rng);

                            let mut secret = [0; Database::CURRENT_SECRET_LEN];
                            secret_rng.fill_bytes(&mut secret);
                            let salt = SaltString::generate(&mut secret_rng);
                            let password_hash = argon2
                                .hash_password(&secret, &salt)
                                .expect("bad argon2 parameters");

                            s_insert_user
                                .execute((
                                    user_id.0,
                                    nickname,
                                    password_hash.to_string(),
                                    Utc::now().timestamp(),
                                ))
                                .context("could not execute query")?;

                            Ok(NewUser {
                                user_id,
                                secret: base64::engine::general_purpose::URL_SAFE.encode(secret),
                            })
                        }();
                        _ = reply.send(result);
                    }

                    Command::LogIn {
                        user_id,
                        secret,
                        reply,
                    } => {
                        // TODO: User expiration.
                        let result = || -> eyre::Result<_> {
                            let secret_hash: String = s_get_secret
                                .query_row((user_id.0,), |row| row.get(0))
                                .context("no such user")?;

                            let hash = PasswordHash::new(&secret_hash)
                                .map_err(|_| eyre!("invalid secret hash"))?;
                            argon2
                                .verify_password(&secret, &hash)
                                .map_err(|_| eyre!("invalid secret"))?;

                            s_log_in
                                .execute((Utc::now().timestamp(), user_id.0))
                                .context("no such user")?;

                            Ok(())
                        }();

                        _ = reply.send(match result {
                            Ok(_) => LoginStatus::ValidUser,
                            Err(_) => LoginStatus::InvalidUser,
                        });
                    }

                    Command::UserInfo { user_id, reply } => {
                        let result = s_user_info
                            .query_row((user_id.0,), |row| {
                                Ok(UserInfo {
                                    nickname: row.get(0)?,
                                })
                            })
                            .optional()
                            .context("could not execute query");
                        _ = reply.send(result);
                    }
                }
            }
        })
        .context("cannot spawn thread")?;

    Ok(Database { command_tx })
}
