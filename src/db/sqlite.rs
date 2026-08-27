#![allow(dead_code)]
use anyhow::Result;
use rusqlite::{Connection, params};
use std::path::Path;

pub struct LogEntry {
    pub level: String,
    pub message: String,
    pub created_at: String,
}

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        let db = Self { conn };
        db.init_tables()?;
        Ok(db)
    }

    fn init_tables(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS recent_files (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                path TEXT NOT NULL UNIQUE,
                opened_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS command_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                command TEXT NOT NULL,
                executed_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS app_logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                level TEXT NOT NULL,
                message TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS sessions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                data TEXT NOT NULL,
                saved_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS favorites (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                path TEXT NOT NULL UNIQUE,
                added_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS music_library (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                path TEXT NOT NULL UNIQUE,
                title TEXT NOT NULL,
                artist TEXT NOT NULL DEFAULT '',
                album TEXT NOT NULL DEFAULT '',
                duration_secs REAL NOT NULL DEFAULT 0.0,
                scanned_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS notes_index (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                path TEXT NOT NULL UNIQUE,
                title TEXT NOT NULL,
                tags TEXT NOT NULL DEFAULT '',
                modified_at INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS tasks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                date TEXT NOT NULL,
                text TEXT NOT NULL,
                done INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS weather_cities (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                latitude REAL NOT NULL,
                longitude REAL NOT NULL
            );
            CREATE TABLE IF NOT EXISTS ssh_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                alias TEXT NOT NULL,
                connected_at TEXT NOT NULL,
                duration_secs INTEGER NOT NULL DEFAULT 0,
                exit_code INTEGER
            );
            CREATE TABLE IF NOT EXISTS ssh_groups (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE
            );
            CREATE TABLE IF NOT EXISTS ssh_host_groups (
                alias TEXT PRIMARY KEY,
                group_id INTEGER NOT NULL REFERENCES ssh_groups(id)
            );",
        )?;
        Ok(())
    }

    // ── Weather cities (Phase 6.5) ──────────────────────────────────────────

    /// Add a saved weather city. Returns its id.
    pub fn add_weather_city(&self, name: &str, lat: f64, lon: f64) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO weather_cities (name, latitude, longitude) VALUES (?1, ?2, ?3)",
            params![name, lat, lon],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// All saved cities, ordered by insertion. Returns (id, name, lat, lon).
    pub fn get_weather_cities(&self) -> Result<Vec<(i64, String, f64, f64)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, latitude, longitude FROM weather_cities ORDER BY id ASC")?;
        let cities = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, f64>(2)?,
                    row.get::<_, f64>(3)?,
                ))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(cities)
    }

    pub fn delete_weather_city(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM weather_cities WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ── Calendar tasks (Phase 6.5) ──────────────────────────────────────────

    /// Insert a task for a given day (date as "YYYY-MM-DD"). Returns its id.
    pub fn add_task(&self, date: &str, text: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO tasks (date, text, done) VALUES (?1, ?2, 0)",
            params![date, text],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// All tasks for a day, ordered by id. Returns (id, date, text, done).
    pub fn get_tasks_for(&self, date: &str) -> Result<Vec<(i64, String, String, bool)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, date, text, done FROM tasks WHERE date = ?1 ORDER BY id ASC")?;
        let tasks = stmt
            .query_map(params![date], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)? != 0,
                ))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(tasks)
    }

    /// All tasks whose date is within `[start, end]` (inclusive, "YYYY-MM-DD"),
    /// ordered by date then id. Used to populate a whole month's grid at once.
    pub fn get_tasks_between(
        &self,
        start: &str,
        end: &str,
    ) -> Result<Vec<(i64, String, String, bool)>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, date, text, done FROM tasks WHERE date >= ?1 AND date <= ?2 \
             ORDER BY date ASC, id ASC",
        )?;
        let tasks = stmt
            .query_map(params![start, end], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)? != 0,
                ))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(tasks)
    }

    /// All distinct dates that have at least one task (for grid markers).
    pub fn get_task_dates(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT DISTINCT date FROM tasks")?;
        let dates = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(dates)
    }

    pub fn toggle_task(&self, id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE tasks SET done = 1 - done WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn delete_task(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM tasks WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Upsert a note into the index (title/tags/mtime keyed on path).
    pub fn upsert_note(&self, path: &str, title: &str, tags: &str, modified_at: u64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO notes_index (path, title, tags, modified_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(path) DO UPDATE SET title = ?2, tags = ?3, modified_at = ?4",
            params![path, title, tags, modified_at as i64],
        )?;
        Ok(())
    }

    /// All indexed notes (path, title, tags) ordered by most-recently modified.
    pub fn get_notes(&self) -> Result<Vec<(String, String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, title, tags FROM notes_index ORDER BY modified_at DESC, title ASC",
        )?;
        let notes = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(notes)
    }

    pub fn clear_notes_index(&self) -> Result<()> {
        self.conn.execute("DELETE FROM notes_index", [])?;
        Ok(())
    }

    pub fn insert_recent_file(&self, path: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO recent_files (path, opened_at) VALUES (?1, datetime('now'))
             ON CONFLICT(path) DO UPDATE SET opened_at = datetime('now')",
            params![path],
        )?;
        Ok(())
    }

    pub fn get_recent_files(&self, limit: usize) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT path FROM recent_files ORDER BY opened_at DESC LIMIT ?1")?;
        let paths = stmt
            .query_map(params![limit as i64], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(paths)
    }

    pub fn insert_command(&self, cmd: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO command_history (command, executed_at) VALUES (?1, datetime('now'))",
            params![cmd],
        )?;
        Ok(())
    }

    pub fn insert_log(&self, level: &str, message: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO app_logs (level, message, created_at) VALUES (?1, ?2, datetime('now'))",
            params![level, message],
        )?;
        Ok(())
    }

    pub fn get_logs(&self, limit: usize) -> Result<Vec<LogEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT level, message, created_at FROM app_logs ORDER BY created_at DESC LIMIT ?1",
        )?;
        let entries = stmt
            .query_map(params![limit as i64], |row| {
                Ok(LogEntry {
                    level: row.get(0)?,
                    message: row.get(1)?,
                    created_at: row.get(2)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(entries)
    }

    pub fn save_session(&self, name: &str, data: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO sessions (name, data, saved_at) VALUES (?1, ?2, datetime('now'))
             ON CONFLICT(name) DO UPDATE SET data = ?2, saved_at = datetime('now')",
            params![name, data],
        )?;
        Ok(())
    }

    pub fn load_session(&self, name: &str) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT data FROM sessions WHERE name = ?1")?;
        let mut rows = stmt.query(params![name])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    pub fn insert_favorite(&self, path: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO favorites (path, added_at) VALUES (?1, datetime('now'))
             ON CONFLICT(path) DO NOTHING",
            params![path],
        )?;
        Ok(())
    }

    pub fn remove_favorite(&self, path: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM favorites WHERE path = ?1", params![path])?;
        Ok(())
    }

    pub fn get_favorites(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT path FROM favorites ORDER BY added_at ASC")?;
        let paths = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(paths)
    }

    pub fn upsert_music_track(
        &self,
        path: &str,
        title: &str,
        artist: &str,
        album: &str,
        duration: f64,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO music_library (path, title, artist, album, duration_secs, scanned_at) VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))
             ON CONFLICT(path) DO UPDATE SET title = ?2, artist = ?3, album = ?4, duration_secs = ?5, scanned_at = datetime('now')",
            params![path, title, artist, album, duration],
        )?;
        Ok(())
    }

    pub fn get_music_tracks(&self) -> Result<Vec<(String, String, String, String, f64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, title, artist, album, duration_secs FROM music_library ORDER BY artist, album, title"
        )?;
        let tracks = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, f64>(4)?,
                ))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(tracks)
    }

    pub fn clear_music_library(&self) -> Result<()> {
        self.conn.execute("DELETE FROM music_library", [])?;
        Ok(())
    }

    // ── SSH (Phase 6.8) ──────────────────────────────────────────────────────

    /// Record a finished SSH session (alias, duration, exit code).
    pub fn record_ssh_session(
        &self,
        alias: &str,
        duration_secs: i64,
        exit_code: Option<i32>,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO ssh_history (alias, connected_at, duration_secs, exit_code) VALUES (?1, datetime('now'), ?2, ?3)",
            params![alias, duration_secs, exit_code],
        )?;
        Ok(())
    }

    /// Most recent SSH sessions, newest first. Returns
    /// (alias, connected_at, duration_secs, exit_code).
    pub fn get_ssh_history(&self, limit: usize) -> Result<Vec<(String, String, i64, Option<i32>)>> {
        let mut stmt = self.conn.prepare(
            "SELECT alias, connected_at, duration_secs, exit_code FROM ssh_history ORDER BY id DESC LIMIT ?1",
        )?;
        let entries = stmt
            .query_map(params![limit as i64], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Option<i32>>(3)?,
                ))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(entries)
    }

    /// Insert `name` into `ssh_groups` if missing, then return its id.
    pub fn upsert_group(&self, name: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT OR IGNORE INTO ssh_groups (name) VALUES (?1)",
            params![name],
        )?;
        let id = self.conn.query_row(
            "SELECT id FROM ssh_groups WHERE name = ?1",
            params![name],
            |row| row.get(0),
        )?;
        Ok(id)
    }

    /// Assign host `alias` to group `group_id` (upsert).
    pub fn assign_host_group(&self, alias: &str, group_id: i64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO ssh_host_groups (alias, group_id) VALUES (?1, ?2)
             ON CONFLICT(alias) DO UPDATE SET group_id = ?2",
            params![alias, group_id],
        )?;
        Ok(())
    }

    /// All host-to-group assignments. Returns (alias, group_name).
    pub fn get_host_groups(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT ssh_host_groups.alias, ssh_groups.name
             FROM ssh_host_groups
             JOIN ssh_groups ON ssh_groups.id = ssh_host_groups.group_id",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }
}
