use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Pool, Sqlite};
use std::path::PathBuf;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Recording {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub duration_ms: i64,
    pub raw_text: String,
    pub cleaned_text: String,
    pub wav_path: String,
}

impl Recording {
    pub fn display_text(&self) -> &str {
        if !self.cleaned_text.is_empty() {
            &self.cleaned_text
        } else {
            &self.raw_text
        }
    }
}

#[derive(Clone)]
pub struct Store {
    pool: Pool<Sqlite>,
    recordings_dir: PathBuf,
}

impl Store {
    pub async fn open() -> Result<Self> {
        let dirs = ProjectDirs::from("dev", "whispr", "whispr")
            .context("resolve data dir")?;
        let data_dir = dirs.data_dir().to_path_buf();
        let recordings_dir = data_dir.join("recordings");
        std::fs::create_dir_all(&recordings_dir).context("create recordings dir")?;

        let db_path = data_dir.join("whispr.db");
        let opts = SqliteConnectOptions::from_str(&format!("sqlite://{}", db_path.display()))?
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(opts)
            .await
            .context("connect sqlite")?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS recordings (
                id TEXT PRIMARY KEY,
                created_at TEXT NOT NULL,
                duration_ms INTEGER NOT NULL,
                raw_text TEXT NOT NULL,
                cleaned_text TEXT NOT NULL,
                wav_path TEXT NOT NULL
            );
            "#,
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_recordings_created_at ON recordings(created_at DESC);",
        )
        .execute(&pool)
        .await?;

        Ok(Self {
            pool,
            recordings_dir,
        })
    }

    pub fn new_recording_path(&self) -> (String, PathBuf) {
        let id = Uuid::new_v4().to_string();
        let path = self.recordings_dir.join(format!("{id}.wav"));
        (id, path)
    }

    pub async fn insert(&self, rec: &Recording) -> Result<()> {
        sqlx::query(
            "INSERT INTO recordings (id, created_at, duration_ms, raw_text, cleaned_text, wav_path)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(&rec.id)
        .bind(rec.created_at.to_rfc3339())
        .bind(rec.duration_ms)
        .bind(&rec.raw_text)
        .bind(&rec.cleaned_text)
        .bind(&rec.wav_path)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn recent(&self, limit: i64) -> Result<Vec<Recording>> {
        let rows: Vec<(String, String, i64, String, String, String)> = sqlx::query_as(
            "SELECT id, created_at, duration_ms, raw_text, cleaned_text, wav_path
             FROM recordings ORDER BY created_at DESC LIMIT ?1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(id, created_at, duration_ms, raw, cleaned, wav)| Recording {
                id,
                created_at: DateTime::parse_from_rfc3339(&created_at)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                duration_ms,
                raw_text: raw,
                cleaned_text: cleaned,
                wav_path: wav,
            })
            .collect())
    }

    pub async fn delete(&self, id: &str) -> Result<()> {
        let wav: Option<(String,)> =
            sqlx::query_as("SELECT wav_path FROM recordings WHERE id = ?1")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;

        sqlx::query("DELETE FROM recordings WHERE id = ?1")
            .bind(id)
            .execute(&self.pool)
            .await?;

        if let Some((path,)) = wav {
            let _ = std::fs::remove_file(path);
        }
        Ok(())
    }
}
