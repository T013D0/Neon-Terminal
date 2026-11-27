//! Historial de comandos en SQLite + ranking de sugerencias.
//!
//! Cada comando que la shell reporta (ver `shell.rs`) se guarda con su carpeta,
//! repo, código de salida y duración. Las sugerencias combinan:
//! - qué tan bien calza con lo que escribiste (prefijo > contiene > difuso),
//! - cuántas veces lo usaste y qué tan reciente fue,
//! - si lo usaste en esta misma carpeta o repo.

use anyhow::Result;
use rusqlite::{Connection, params};
use std::path::Path;

pub struct History {
    conn: Connection,
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub command: String,
    pub count: i64,
    pub last_ts: i64,
    pub last_ok: bool,
    pub here_count: i64,
    pub repo_count: i64,
}

#[derive(Debug, Clone)]
pub struct Suggestion {
    pub entry: Entry,
    pub score: f64,
}

pub struct NewCommand<'a> {
    pub command: &'a str,
    pub cwd: &'a str,
    pub repo_root: Option<&'a str>,
    pub ok: bool,
    pub exit_code: Option<i64>,
    pub duration_ms: Option<i64>,
    pub ts: i64,
}

impl History {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        // WAL permite que varias ventanas (procesos) compartan el historial.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.busy_timeout(std::time::Duration::from_secs(2))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS commands (
                id          INTEGER PRIMARY KEY,
                command     TEXT NOT NULL,
                cwd         TEXT NOT NULL,
                repo_root   TEXT,
                ok          INTEGER NOT NULL,
                exit_code   INTEGER,
                duration_ms INTEGER,
                ts          INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_commands_cmd ON commands(command);
            CREATE INDEX IF NOT EXISTS idx_commands_ts  ON commands(ts);",
        )?;
        // Limpia el script de arranque de Neon que versiones anteriores
        // guardaban por error como si fuera un comando.
        let _ = conn.execute(
            "DELETE FROM commands WHERE command LIKE '# Generado por Neon%'",
            [],
        );
        Ok(Self { conn })
    }

    pub fn record(&self, c: &NewCommand) -> Result<()> {
        let cmd = c.command.trim();
        // El script con el que Neon prepara la shell no es un comando tuyo.
        if cmd.is_empty() || cmd.starts_with("# Generado por Neon") {
            return Ok(());
        }
        self.conn.execute(
            "INSERT INTO commands (command, cwd, repo_root, ok, exit_code, duration_ms, ts)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                cmd,
                c.cwd,
                c.repo_root,
                c.ok as i64,
                c.exit_code,
                c.duration_ms,
                c.ts
            ],
        )?;
        Ok(())
    }

    /// Comandos distintos con estadísticas. `ok` sale de la ejecución más
    /// reciente (SQLite toma las columnas "sueltas" de la fila con MAX(ts)).
    fn candidates(&self, cwd: &str, repo_root: Option<&str>, limit: usize) -> Result<Vec<Entry>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT command, COUNT(*), MAX(ts), ok,
                    SUM(cwd = ?1), SUM(COALESCE(repo_root = ?2, 0))
             FROM commands
             GROUP BY command
             ORDER BY MAX(ts) DESC
             LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![cwd, repo_root, limit as i64], |r| {
            Ok(Entry {
                command: r.get(0)?,
                count: r.get(1)?,
                last_ts: r.get(2)?,
                last_ok: r.get::<_, i64>(3)? != 0,
                here_count: r.get(4)?,
                repo_count: r.get(5)?,
            })
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// Búsqueda para la paleta de historial (Ctrl+R).
    pub fn search(
        &self,
        query: &str,
        cwd: &str,
        repo_root: Option<&str>,
        now: i64,
        limit: usize,
    ) -> Vec<Suggestion> {
        let Ok(cands) = self.candidates(cwd, repo_root, 5000) else {
            return vec![];
        };
        let mut out: Vec<Suggestion> = cands
            .into_iter()
            .filter_map(|e| {
                let m = match_score(query, &e.command)?;
                let score = m + frecency(&e, now);
                Some(Suggestion { entry: e, score })
            })
            .collect();
        out.sort_by(|a, b| b.score.total_cmp(&a.score));
        out.truncate(limit);
        out
    }

    /// Historial para la flecha ↑: lo de esta carpeta primero, luego el resto,
    /// cada grupo de más reciente a más antiguo. Si escribiste algo, filtra por
    /// prefijo (o por contenido si nada empieza así).
    /// Lo que falta para completar `prefix` con un comando del historial
    /// (primero los de esta carpeta, después los más recientes).
    pub fn suggest(&self, prefix: &str, cwd: &str) -> Option<String> {
        if prefix.trim().chars().count() < 2 || prefix.starts_with(' ') {
            return None;
        }
        self.recent(prefix, cwd, 20)
            .into_iter()
            .find(|e| e.command.starts_with(prefix) && e.command.len() > prefix.len())
            .map(|e| e.command[prefix.len()..].to_string())
    }

    pub fn recent(&self, query: &str, cwd: &str, limit: usize) -> Vec<Entry> {
        let Ok(cands) = self.candidates(cwd, None, 5000) else {
            return vec![];
        };
        let q = query.trim_start().to_lowercase();
        let mut hits: Vec<Entry> = if q.is_empty() {
            cands
        } else {
            let prefix: Vec<Entry> = cands
                .iter()
                .filter(|e| e.command.to_lowercase().starts_with(&q) && e.command.len() > q.len())
                .cloned()
                .collect();
            if prefix.is_empty() {
                cands
                    .into_iter()
                    .filter(|e| e.command.to_lowercase().contains(&q))
                    .collect()
            } else {
                prefix
            }
        };
        hits.sort_by(|a, b| {
            (b.here_count > 0)
                .cmp(&(a.here_count > 0))
                .then(b.last_ts.cmp(&a.last_ts))
        });
        hits.truncate(limit);
        hits
    }
}

/// Puntaje por calce de texto. `None` = no calza.
pub fn match_score(query: &str, command: &str) -> Option<f64> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Some(0.0);
    }
    let c = command.to_lowercase();
    if c.starts_with(&q) {
        return Some(100.0 - (c.len() - q.len()).min(40) as f64 * 0.25);
    }
    if let Some(pos) = c.find(&q) {
        // Bonus si calza al inicio de una palabra ("push" en "git push").
        let word_start = pos == 0 || !c.as_bytes()[pos - 1].is_ascii_alphanumeric();
        return Some(if word_start { 70.0 } else { 55.0 } - pos.min(30) as f64 * 0.3);
    }
    // Difuso: las letras de la búsqueda aparecen en orden.
    let mut gaps = 0usize;
    let mut chars = c.chars();
    for qc in q.chars() {
        if qc == ' ' {
            continue;
        }
        let mut found = false;
        for cc in chars.by_ref() {
            if cc == qc {
                found = true;
                break;
            }
            gaps += 1;
        }
        if !found {
            return None;
        }
    }
    Some((35.0 - gaps as f64 * 0.8).max(1.0))
}

/// Frecuencia + recencia + contexto.
pub fn frecency(e: &Entry, now: i64) -> f64 {
    let age_h = (now - e.last_ts).max(0) as f64 / 3600.0;
    let recency = if age_h < 1.0 {
        25.0
    } else if age_h < 24.0 {
        18.0
    } else if age_h < 24.0 * 7.0 {
        10.0
    } else if age_h < 24.0 * 30.0 {
        4.0
    } else {
        0.0
    };
    let freq = (1.0 + e.count as f64).ln() * 8.0;
    let context = if e.here_count > 0 {
        20.0 + (e.here_count as f64).ln() * 3.0
    } else if e.repo_count > 0 {
        10.0
    } else {
        0.0
    };
    let fail_penalty = if e.last_ok { 0.0 } else { 6.0 };
    recency + freq + context - fail_penalty
}

#[cfg(test)]
mod tests {
    use super::*;

    fn add(h: &History, cmd: &str, cwd: &str, ok: bool, ts: i64) {
        h.record(&NewCommand {
            command: cmd,
            cwd,
            repo_root: Some("/repo"),
            ok,
            exit_code: Some(if ok { 0 } else { 1 }),
            duration_ms: Some(10),
            ts,
        })
        .unwrap();
    }

    #[test]
    fn match_kinds_are_ordered() {
        let prefix = match_score("git", "git status").unwrap();
        let contains = match_score("status", "git status").unwrap();
        let fuzzy = match_score("gst", "git status").unwrap();
        assert!(prefix > contains && contains > fuzzy);
        assert!(match_score("xyz", "git status").is_none());
    }

    #[test]
    fn ignores_neon_init_script() {
        let h = History::open_in_memory().unwrap();
        add(&h, "# Generado por Neon: colores, prompt…\n$global:__wLog = 1", "/r", true, 10);
        add(&h, "git status", "/r", true, 11);
        let r = h.recent("", "/r", 10);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].command, "git status");
    }

    #[test]
    fn ranks_by_context_and_frequency() {
        let h = History::open_in_memory().unwrap();
        let now = 1_000_000;
        for i in 0..5 {
            add(&h, "cargo build", "/repo", true, now - 100 - i);
        }
        add(&h, "cargo bench", "/otro", true, now - 50);
        let r = h.search("cargo b", "/repo", Some("/repo"), now, 10);
        assert_eq!(r[0].entry.command, "cargo build");
        assert_eq!(r[0].entry.count, 5);
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn last_ok_comes_from_latest_run() {
        let h = History::open_in_memory().unwrap();
        add(&h, "npm test", "/repo", true, 10);
        add(&h, "npm test", "/repo", false, 20);
        let r = h.search("npm", "/repo", None, 30, 5);
        assert!(!r[0].entry.last_ok);
    }

    #[test]
    fn recent_puts_this_dir_first_then_by_time() {
        let h = History::open_in_memory().unwrap();
        add(&h, "git pull", "/otro", true, 50);
        add(&h, "git status", "/repo", true, 10);
        add(&h, "git push", "/repo", true, 20);
        add(&h, "ls", "/repo", true, 30);
        let all: Vec<_> = h
            .recent("", "/repo", 10)
            .into_iter()
            .map(|e| e.command)
            .collect();
        assert_eq!(all, ["ls", "git push", "git status", "git pull"]);
        let git: Vec<_> = h
            .recent("git p", "/repo", 10)
            .into_iter()
            .map(|e| e.command)
            .collect();
        assert_eq!(git, ["git push", "git pull"]);
        let contains: Vec<_> = h
            .recent("status", "/repo", 10)
            .into_iter()
            .map(|e| e.command)
            .collect();
        assert_eq!(contains, ["git status"]);
    }
}
