//! Panel de cambios: qué archivos están modificados en el repo (marcando los que
//! cambiaron desde que arrancó Claude Code), con su diff. Todo se lee de git
//! solo mientras el panel está abierto; no hay nada vigilando en segundo plano.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

/// Líneas de diff que se guardan como máximo (para no gastar memoria con
/// archivos enormes).
const MAX_DIFF_LINES: usize = 2000;

#[derive(Clone, Debug, PartialEq)]
pub struct FileChange {
    /// Dos letras de `git status --porcelain` ("M ", " M", "??", "A ", " D"…).
    pub status: String,
    pub path: String,
    /// Cambió después de que arrancó Claude Code.
    pub by_claude: bool,
}

impl FileChange {
    pub fn untracked(&self) -> bool {
        self.status == "??"
    }
    /// Se puede volver a la versión del último commit.
    pub fn restorable(&self) -> bool {
        !self.untracked() && !self.status.contains('A')
    }
}

pub struct Changes {
    pub root: PathBuf,
    pub files: Vec<FileChange>,
    pub selected: Option<usize>,
    pub diff: Vec<String>,
    pub loaded: Instant,
    /// Para pedir confirmación antes de descartar.
    pub confirm_discard: Option<String>,
}

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let mut c = Command::new("git");
    c.arg("-C").arg(root).args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = c.output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Raíz del repo que contiene `cwd`.
pub fn repo_root(cwd: &str) -> Option<PathBuf> {
    git(Path::new(cwd), &["rev-parse", "--show-toplevel"]).map(|s| PathBuf::from(s.trim()))
}

/// Archivos modificados (para guardar como "antes de Claude").
pub fn snapshot(root: &Path) -> HashSet<String> {
    status(root)
        .into_iter()
        .map(|(st, p)| format!("{st}{p}"))
        .collect()
}

fn status(root: &Path) -> Vec<(String, String)> {
    git(root, &["status", "--porcelain", "--untracked-files=all"])
        .map(|s| parse_status(&s))
        .unwrap_or_default()
}

pub fn parse_status(s: &str) -> Vec<(String, String)> {
    s.lines()
        .filter(|l| l.len() > 3)
        .map(|l| {
            let st = l[..2].to_string();
            let mut path = l[3..].to_string();
            if let Some((_, to)) = path.split_once(" -> ") {
                path = to.to_string();
            }
            (st, path.trim_matches('"').to_string())
        })
        .collect()
}

impl Changes {
    pub fn open(root: PathBuf, baseline: Option<&HashSet<String>>) -> Self {
        let mut c = Self {
            root,
            files: vec![],
            selected: None,
            diff: vec![],
            loaded: Instant::now(),
            confirm_discard: None,
        };
        c.refresh(baseline);
        c
    }

    /// Vuelve a leer `git status` (y el diff del elegido si sigue ahí).
    pub fn refresh(&mut self, baseline: Option<&HashSet<String>>) {
        let sel = self.selected.and_then(|i| self.files.get(i)).map(|f| f.path.clone());
        self.files = status(&self.root)
            .into_iter()
            .map(|(status, path)| FileChange {
                by_claude: baseline.is_some_and(|b| !b.contains(&format!("{status}{path}"))),
                status,
                path,
            })
            .collect();
        // Primero lo que cambió Claude.
        self.files.sort_by_key(|f| (!f.by_claude, f.path.to_lowercase()));
        self.selected = sel.and_then(|p| self.files.iter().position(|f| f.path == p));
        if self.selected.is_none() {
            self.diff.clear();
        } else {
            self.load_diff();
        }
        self.loaded = Instant::now();
    }

    pub fn select(&mut self, i: usize) {
        self.selected = Some(i);
        self.confirm_discard = None;
        self.load_diff();
    }

    fn load_diff(&mut self) {
        let Some(f) = self.selected.and_then(|i| self.files.get(i)).cloned() else {
            self.diff.clear();
            return;
        };
        let text = if f.untracked() {
            // Archivo nuevo: todo es "+".
            std::fs::read_to_string(self.root.join(&f.path))
                .map(|t| t.lines().map(|l| format!("+{l}")).collect::<Vec<_>>().join("\n"))
                .unwrap_or_else(|_| "(archivo binario o ilegible)".into())
        } else {
            git(&self.root, &["diff", "HEAD", "--no-color", "--", &f.path])
                .or_else(|| git(&self.root, &["diff", "--no-color", "--", &f.path]))
                .unwrap_or_default()
        };
        self.diff = text
            .lines()
            // Sin el encabezado técnico de git.
            .skip_while(|l| !l.starts_with("@@") && !f.untracked())
            .take(MAX_DIFF_LINES)
            .map(str::to_string)
            .collect();
        if self.diff.is_empty() {
            self.diff.push("(sin diferencias de texto)".into());
        }
    }

    /// Vuelve el archivo a como estaba en el último commit.
    pub fn discard(&mut self, path: &str) -> Result<(), String> {
        let ok = git(&self.root, &["restore", "--staged", "--worktree", "--source=HEAD", "--", path])
            .or_else(|| git(&self.root, &["checkout", "HEAD", "--", path]));
        ok.map(|_| ()).ok_or_else(|| format!("No pude descartar los cambios de {path}"))
    }

    pub fn full_path(&self, path: &str) -> PathBuf {
        self.root.join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_porcelain() {
        let v = parse_status(" M src/app.rs\n?? nuevo.txt\nR  viejo.rs -> nuevo.rs\nA  \"con espacio.md\"\n");
        assert_eq!(
            v,
            vec![
                (" M".into(), "src/app.rs".into()),
                ("??".into(), "nuevo.txt".into()),
                ("R ".into(), "nuevo.rs".into()),
                ("A ".into(), "con espacio.md".into()),
            ]
        );
    }

    #[test]
    fn marks_changes_since_baseline() {
        let tmp = std::env::temp_dir().join(format!("neon-changes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let g = |args: &[&str]| {
            Command::new("git").arg("-C").arg(&tmp).args(args).output().unwrap();
        };
        g(&["init", "-q"]);
        g(&["config", "user.email", "t@t"]);
        g(&["config", "user.name", "t"]);
        std::fs::write(tmp.join("a.txt"), "uno\n").unwrap();
        std::fs::write(tmp.join("b.txt"), "dos\n").unwrap();
        g(&["add", "."]);
        g(&["commit", "-qm", "x"]);
        std::fs::write(tmp.join("a.txt"), "uno cambiado\n").unwrap();
        let base = snapshot(&tmp);
        // "Claude" cambia b y crea c.
        std::fs::write(tmp.join("b.txt"), "dos cambiado\n").unwrap();
        std::fs::write(tmp.join("c.txt"), "nuevo\n").unwrap();
        let mut ch = Changes::open(tmp.clone(), Some(&base));
        let names: Vec<(&str, bool)> = ch.files.iter().map(|f| (f.path.as_str(), f.by_claude)).collect();
        assert_eq!(names, vec![("b.txt", true), ("c.txt", true), ("a.txt", false)]);
        ch.select(0);
        assert!(ch.diff.iter().any(|l| l == "+dos cambiado"));
        ch.discard("b.txt").unwrap();
        ch.refresh(Some(&base));
        assert!(!ch.files.iter().any(|f| f.path == "b.txt"));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
