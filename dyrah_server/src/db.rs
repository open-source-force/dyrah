use std::path::Path;

use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use rusqlite::{Connection, Result};

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS players (
                id INTEGER PRIMARY KEY,
                username TEXT UNIQUE NOT NULL,
                password_hash TEXT NOT NULL
            );
        ",
        )
        .unwrap();
        Self { conn }
    }

    pub fn register(&self, username: &str, password: &str) -> Result<bool, String> {
        // hash the password before storing it
        let salt = SaltString::generate(&mut OsRng);
        let hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| e.to_string())?
            .to_string();

        // UNIQUE constraint on username means this fails if already taken
        self.conn
            .execute(
                "INSERT INTO players (username, password_hash) VALUES (?1, ?2)",
                (username, &hash),
            )
            .map(|_| true)
            .map_err(|e| e.to_string())
    }

    pub fn login(&self, username: &str, password: &str) -> Result<bool, String> {
        // look up the stored hash for this username
        let result = self.conn.query_row(
            "SELECT password_hash FROM players WHERE username = ?1",
            [username],
            |row| row.get::<_, String>(0),
        );

        match result {
            Ok(stored_hash) => {
                // verify the provided password against the stored hash
                let parsed = PasswordHash::new(&stored_hash).map_err(|e| e.to_string())?;
                Ok(Argon2::default()
                    .verify_password(password.as_bytes(), &parsed)
                    .is_ok())
            }
            // username not found, return false instead of leaking it
            Err(_) => Ok(false),
        }
    }
}
