//! Buscador de archivos (Ctrl+Shift+A): búsqueda difusa por nombre en la
//! carpeta o el repo. La lista se arma al abrirlo y se suelta al cerrarlo.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Máximo de archivos que se recorren (carpetas enormes como C:\ no traban).
const MAX_FILES: usize = 50_000;

pub struct Finder {
    pub root: PathBuf,
    files: Vec<String>,
    pub query: String,
    /// Índices en `files`, ya ordenados por puntaje.
    pub results: Vec<usize>,
    pub selected: usize,
    pub truncated: bool,
}

impl Finder {
    pub fn open(cwd: &str) -> Self {
        // En un repo: los archivos de git (respeta .gitignore y es rapidísimo).
        let root = crate::changes::repo_root(cwd).unwrap_or_else(|| PathBuf::from(cwd));
        let mut files = git_files(&root).unwrap_or_else(|| walk(&root));
        let truncated = files.len() >= MAX_FILES;
        files.truncate(MAX_FILES);
        let mut f = Self {
            root,
            files,
            query: String::new(),
            results: vec![],
            selected: 0,
            truncated,
        };
        f.search();
        f
    }

    pub fn set_query(&mut self, q: &str) {
        if q != self.query {
            self.query = q.to_string();
            self.search();
        }
    }

    fn search(&mut self) {
        let q: Vec<char> = self.query.to_lowercase().chars().filter(|c| !c.is_whitespace()).collect();
        let mut scored: Vec<(i32, usize)> = self
            .files
            .iter()
            .enumerate()
            .filter_map(|(i, f)| score(f, &q).map(|s| (s, i)))
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(self.files[a.1].len().cmp(&self.files[b.1].len())));
        self.results = scored.into_iter().take(200).map(|(_, i)| i).collect();
        self.selected = 0;
    }

    pub fn path(&self, i: usize) -> &str {
        &self.files[self.results[i]]
    }

    pub fn full(&self, i: usize) -> PathBuf {
        self.root.join(self.path(i))
    }

    pub fn count(&self) -> usize {
        self.files.len()
    }
}

/// Puntaje difuso: todas las letras en orden; suman las seguidas, las que
/// empiezan palabra y las que caen en el nombre del archivo (no en carpetas).
pub fn score(path: &str, q: &[char]) -> Option<i32> {
    if q.is_empty() {
        return Some(0);
    }
    let lower: Vec<char> = path.to_lowercase().chars().collect();
    let name_start = lower
        .iter()
        .rposition(|c| *c == '/' || *c == '\\')
        .map_or(0, |i| i + 1);
    let mut s = 0;
    let mut qi = 0;
    let mut prev: Option<usize> = None;
    for (i, c) in lower.iter().enumerate() {
        if qi < q.len() && *c == q[qi] {
            s += 1;
            if prev == Some(i.wrapping_sub(1)) {
                s += 5;
            }
            let boundary = i == 0 || matches!(lower[i - 1], '/' | '\\' | '_' | '-' | '.' | ' ');
            if boundary {
                s += 4;
            }
            if i >= name_start {
                s += 3;
            }
            prev = Some(i);
            qi += 1;
        }
    }
    (qi == q.len()).then_some(s)
}

fn git_files(root: &Path) -> Option<Vec<String>> {
    let mut c = Command::new("git");
    c.arg("-C")
        .arg(root)
        .args(["ls-files", "--cached", "--others", "--exclude-standard"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    let out = c.output().ok()?;
    out.status.success().then(|| {
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .take(MAX_FILES)
            .map(str::to_string)
            .collect()
    })
}

/// Sin git: recorre la carpeta saltando las pesadas de siempre.
fn walk(root: &Path) -> Vec<String> {
    const SKIP: &[&str] = &[
        "node_modules", "target", ".git", "dist", "build", ".next", ".venv", "venv",
        "__pycache__", "AppData", "$Recycle.Bin", "Windows",
    ];
    let mut out = vec![];
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let Ok(t) = e.file_type() else { continue };
            if t.is_dir() {
                if !SKIP.contains(&name.as_str()) && !name.starts_with('.') && stack.len() < 10_000 {
                    stack.push(e.path());
                }
            } else if let Ok(rel) = e.path().strip_prefix(root) {
                out.push(rel.to_string_lossy().into_owned());
                if out.len() >= MAX_FILES {
                    return out;
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    #[test]
    fn fuzzy_prefers_file_names() {
        let a = score("src/app.rs", &q("apprs")).unwrap();
        let b = score("crates/egui_term/src/backend/mod.rs", &q("apprs"));
        assert!(b.is_none());
        let c = score("assets/apple/prs.txt", &q("apprs")).unwrap();
        assert!(a > c, "{a} vs {c}");
        assert!(score("src/ui.rs", &q("xyz")).is_none());
    }

    #[test]
    fn walk_skips_heavy_dirs() {
        let tmp = std::env::temp_dir().join(format!("neon-finder-{}", std::process::id()));
        std::fs::create_dir_all(tmp.join("src")).unwrap();
        std::fs::create_dir_all(tmp.join("node_modules/x")).unwrap();
        std::fs::write(tmp.join("src/main.rs"), "").unwrap();
        std::fs::write(tmp.join("node_modules/x/a.js"), "").unwrap();
        let mut f = Finder::open(tmp.to_str().unwrap());
        f.set_query("main");
        assert_eq!(f.results.len(), 1);
        assert!(f.path(0).ends_with("main.rs"));
        assert_eq!(f.count(), 1);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
