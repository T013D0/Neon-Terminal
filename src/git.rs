//! Estado de git para el panel activo: rama, ahead/behind, cambios y commits.
//!
//! Se consulta con el `git` del sistema en un hilo aparte (nunca bloquea la UI)
//! cuando cambia la carpeta, después de cada comando, y cada pocos segundos.

use std::process::Command;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct GitStatus {
    pub root: String,
    pub branch: String,
    pub detached: bool,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub staged: u32,
    pub modified: u32,
    pub untracked: u32,
    pub conflicts: u32,
    pub commits: Vec<Commit>,
    pub branches: Vec<String>,
}

impl GitStatus {
    pub fn is_clean(&self) -> bool {
        self.staged + self.modified + self.untracked + self.conflicts == 0
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Commit {
    pub hash: String,
    pub subject: String,
    /// Fecha del commit (unix).
    pub ts: i64,
}

/// Parsea `git status --porcelain=v2 --branch`.
pub fn parse_porcelain_v2(out: &str) -> GitStatus {
    let mut s = GitStatus::default();
    let mut oid = String::new();
    for line in out.lines() {
        if let Some(rest) = line.strip_prefix("# branch.head ") {
            if rest == "(detached)" {
                s.detached = true;
            } else {
                s.branch = rest.to_string();
            }
        } else if let Some(rest) = line.strip_prefix("# branch.oid ") {
            oid = rest.chars().take(7).collect();
        } else if let Some(rest) = line.strip_prefix("# branch.upstream ") {
            s.upstream = Some(rest.to_string());
        } else if let Some(rest) = line.strip_prefix("# branch.ab ") {
            for part in rest.split_whitespace() {
                if let Some(n) = part.strip_prefix('+') {
                    s.ahead = n.parse().unwrap_or(0);
                } else if let Some(n) = part.strip_prefix('-') {
                    s.behind = n.parse().unwrap_or(0);
                }
            }
        } else if line.starts_with("1 ") || line.starts_with("2 ") {
            // "1 XY ..." -> X = staged, Y = working tree ('.' = sin cambios)
            let xy: Vec<char> = line.chars().skip(2).take(2).collect();
            if xy.first().is_some_and(|c| *c != '.') {
                s.staged += 1;
            }
            if xy.get(1).is_some_and(|c| *c != '.') {
                s.modified += 1;
            }
        } else if line.starts_with("u ") {
            s.conflicts += 1;
        } else if line.starts_with("? ") {
            s.untracked += 1;
        }
    }
    if s.detached {
        s.branch = if oid.is_empty() { "HEAD".into() } else { oid };
    }
    s
}

fn git(cwd: &str, args: &[&str]) -> Option<String> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(cwd).args(args);
    #[cfg(windows)]
    {
        // Sin esto, cada llamada a git abriría una ventanita de consola.
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn query(cwd: &str) -> Option<GitStatus> {
    let root = git(cwd, &["rev-parse", "--show-toplevel"])?;
    let status = git(cwd, &["status", "--porcelain=v2", "--branch"])?;
    let mut s = parse_porcelain_v2(&status);
    s.root = root.trim().to_string();
    if let Some(log) = git(cwd, &["log", "-8", "--format=%h%x1f%s%x1f%ct"]) {
        s.commits = log
            .lines()
            .filter_map(|l| {
                let mut p = l.split('\u{1f}');
                Some(Commit {
                    hash: p.next()?.to_string(),
                    subject: p.next()?.to_string(),
                    ts: p.next().and_then(|t| t.trim().parse().ok()).unwrap_or(0),
                })
            })
            .collect();
    }
    if let Some(b) = git(
        cwd,
        &[
            "for-each-ref",
            "--sort=-committerdate",
            "--count=15",
            "--format=%(refname:short)",
            "refs/heads",
        ],
    ) {
        s.branches = b.lines().map(str::to_string).collect();
    }
    Some(s)
}

/// Mantiene actualizado el estado de git de un panel sin bloquear.
pub struct GitWatcher {
    tx: Sender<(String, Option<GitStatus>)>,
    rx: Receiver<(String, Option<GitStatus>)>,
    cwd: String,
    in_flight: bool,
    dirty: bool,
    last: Instant,
    pub status: Option<GitStatus>,
}

const REFRESH_EVERY: Duration = Duration::from_secs(4);

impl GitWatcher {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            tx,
            rx,
            cwd: String::new(),
            in_flight: false,
            dirty: true,
            last: Instant::now(),
            status: None,
        }
    }

    /// Marca para refrescar en el próximo `tick` (p. ej. tras un comando).
    pub fn invalidate(&mut self) {
        self.dirty = true;
    }

    pub fn tick(&mut self, cwd: &str, ctx: &egui::Context) {
        while let Ok((for_cwd, status)) = self.rx.try_recv() {
            self.in_flight = false;
            if for_cwd == self.cwd {
                self.status = status;
            }
        }
        if cwd.is_empty() {
            return;
        }
        if cwd != self.cwd {
            self.cwd = cwd.to_string();
            self.dirty = true;
        }
        let stale = self.last.elapsed() > REFRESH_EVERY;
        if (self.dirty || stale) && !self.in_flight {
            self.dirty = false;
            self.in_flight = true;
            self.last = Instant::now();
            let tx = self.tx.clone();
            let cwd = self.cwd.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let s = query(&cwd);
                let _ = tx.send((cwd, s));
                ctx.request_repaint();
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_branch_and_changes() {
        let out = "# branch.oid 1234567890abcdef\n# branch.head main\n# branch.upstream origin/main\n# branch.ab +2 -1\n1 M. N... 100644 100644 100644 a b src/a.rs\n1 .M N... 100644 100644 100644 a b src/b.rs\n1 MM N... 100644 100644 100644 a b src/c.rs\nu UU N... 1 2 3 4 a b c d x.rs\n? nuevo.txt\n? otro.txt\n";
        let s = parse_porcelain_v2(out);
        assert_eq!(s.branch, "main");
        assert_eq!(s.upstream.as_deref(), Some("origin/main"));
        assert_eq!((s.ahead, s.behind), (2, 1));
        assert_eq!(
            (s.staged, s.modified, s.untracked, s.conflicts),
            (2, 2, 2, 1)
        );
        assert!(!s.is_clean());
    }

    #[test]
    fn detached_head_shows_short_hash() {
        let s = parse_porcelain_v2("# branch.oid abcdef1234\n# branch.head (detached)\n");
        assert!(s.detached);
        assert_eq!(s.branch, "abcdef1");
        assert!(s.is_clean());
    }

    #[test]
    fn queries_a_real_repo() {
        let dir = std::env::temp_dir().join(format!("neon-git-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let d = dir.to_str().unwrap();
        let run = |a: &[&str]| {
            assert!(
                Command::new("git")
                    .arg("-C")
                    .arg(d)
                    .args(a)
                    .status()
                    .unwrap()
                    .success()
            )
        };
        run(&["init", "-q", "-b", "dev"]);
        run(&[
            "-c",
            "user.email=a@b",
            "-c",
            "user.name=a",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "primer commit",
        ]);
        std::fs::write(dir.join("x.txt"), "hola").unwrap();
        let s = query(d).expect("es un repo");
        assert_eq!(s.branch, "dev");
        assert_eq!(s.untracked, 1);
        assert_eq!(s.commits[0].subject, "primer commit");
        assert_eq!(s.branches, vec!["dev".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
