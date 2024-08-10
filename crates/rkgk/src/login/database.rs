use std::path::PathBuf;

use chrono::Utc;
use eyre::{eyre, Context};
use rand::SeedableRng;
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

pub enum LoginStatus {
    ValidUser,
    UserDoesNotExist,
}

#[derive(Debug, Clone)]
pub struct UserInfo {
    pub nickname: String,
}

enum Command {
    NewUser {
        nickname: String,
        reply: oneshot::Sender<eyre::Result<UserId>>,
    },
    LogIn {
        user_id: UserId,
        reply: oneshot::Sender<LoginStatus>,
    },
    UserInfo {
        user_id: UserId,
        reply: oneshot::Sender<eyre::Result<Option<UserInfo>>>,
    },
}

impl Database {
    pub async fn new_user(&self, nickname: String) -> eyre::Result<UserId> {
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

    pub async fn log_in(&self, user_id: UserId) -> eyre::Result<LoginStatus> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(Command::LogIn { user_id, reply: tx })
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

    db.execute(
        r#"
            CREATE TABLE IF NOT EXISTS
            t_users (
                user_index           INTEGER PRIMARY KEY,
                long_user_id         BLOB NOT NULL,
                nickname             TEXT NOT NULL,
                last_login_timestamp INTEGER NOT NULL
            );
        "#,
        (),
    )?;

    let (command_tx, mut command_rx) = mpsc::channel(8);

    let mut user_id_rng = rand_chacha::ChaCha20Rng::from_entropy();

    tokio::task::spawn_blocking(move || {
        let mut s_insert_user = db
            .prepare(
                r#"
                    INSERT INTO t_users
                    (long_user_id, nickname, last_login_timestamp)
                    VALUES (?, ?, ?);
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
                    let user_id = UserId::new(&mut user_id_rng);
                    let result = s_insert_user
                        .execute((user_id.0, nickname, Utc::now().timestamp()))
                        .context("could not execute query");
                    _ = reply.send(result.map(|_| user_id));
                }

                Command::LogIn { user_id, reply } => {
                    // TODO: User expiration.
                    let login_status = match s_log_in.execute((Utc::now().timestamp(), user_id.0)) {
                        Ok(_) => LoginStatus::ValidUser,
                        Err(_) => LoginStatus::UserDoesNotExist,
                    };
                    _ = reply.send(login_status);
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
    });

    Ok(Database { command_tx })
}
