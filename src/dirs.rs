//! Lista de carpetas que aparece al escribir `cd ` en el prompt.

use std::path::{Path, PathBuf};

pub struct DirPopup {
    pub pane_id: u64,
    /// Carpeta de la shell.
    cwd: PathBuf,
    /// Lo que ya se eligió con Tab (relativo a `cwd`, termina en separador).
    pub path: String,
    /// Lo que se escribe para filtrar.
    pub query: String,
    pub entries: Vec<String>,
    pub selected: usize,
    sep: char,
    listed_for: Option<(String, String)>,
}

/// Lo que hay que hacer en la shell.
pub enum DirAction {
    /// Dejar escrito `cd ruta` (sin ejecutar).
    Write(String),
    /// Ejecutar `cd ruta`.
    Run(String),
    Close,
    None,
}

impl DirPopup {
    pub fn new(pane_id: u64, cwd: &str, sep: char) -> Option<Self> {
        let cwd = PathBuf::from(cwd);
        if !cwd.is_dir() {
            return None;
        }
        let mut p = Self {
            pane_id,
            cwd,
            path: String::new(),
            query: String::new(),
            entries: vec![],
            selected: 0,
            sep,
            listed_for: None,
        };
        p.refresh();
        Some(p)
    }

    /// Vuelve a leer la carpeta si cambió la ruta o el filtro.
    pub fn refresh(&mut self) {
        let key = (self.path.clone(), self.query.clone());
        if self.listed_for.as_ref() == Some(&key) {
            return;
        }
        let dir = self.cwd.join(&self.path);
        self.entries = filter(&subdirs(&dir), &self.query);
        let at_root = dir.parent().is_none();
        if !at_root && "..".starts_with(&self.query) {
            // Al final: lo normal es querer entrar, no subir.
            self.entries.push("..".into());
        }
        self.selected = 0;
        self.listed_for = Some(key);
    }

    /// Carpeta donde se está buscando (para el título).
    pub fn here(&self) -> String {
        if self.path.is_empty() {
            ".".into()
        } else {
            self.path.clone()
        }
    }

    fn line(&self, extra: &str) -> String {
        let p = format!("{}{extra}", self.path);
        if p.contains(' ') {
            format!("cd \"{p}\"")
        } else {
            format!("cd {p}")
        }
    }

    /// Tab: entrar a la carpeta elegida y seguir eligiendo.
    pub fn descend(&mut self) -> DirAction {
        let Some(name) = self.entries.get(self.selected).cloned() else {
            return DirAction::None;
        };
        self.path = format!("{}{name}{}", self.path, self.sep);
        self.query.clear();
        self.refresh();
        DirAction::Write(self.line(""))
    }

    /// Enter: ir a la carpeta elegida (o a la ruta escrita si no hay elección).
    pub fn choose(&self) -> DirAction {
        match self.entries.get(self.selected) {
            Some(name) => DirAction::Run(self.line(name)),
            None if !self.path.is_empty() => DirAction::Run(self.line("")),
            None => DirAction::Close,
        }
    }

    /// Borrar con el filtro vacío: subir un nivel (o cerrar si ya no hay ruta).
    pub fn back(&mut self) -> DirAction {
        if self.query.pop().is_some() {
            self.refresh();
            return DirAction::None;
        }
        if self.path.is_empty() {
            return DirAction::Close;
        }
        let trimmed = self.path.trim_end_matches(self.sep);
        self.path = match trimmed.rfind(self.sep) {
            Some(i) => trimmed[..=i].to_string(),
            None => String::new(),
        };
        self.refresh();
        DirAction::Write(self.line(""))
    }

    pub fn type_text(&mut self, s: &str) {
        self.query.push_str(s);
        self.refresh();
    }
}

/// Subcarpetas (sin las ocultas del sistema), ordenadas sin importar mayúsculas.
fn subdirs(dir: &Path) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut v: Vec<String> = rd
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir() || (t.is_symlink() && e.path().is_dir())))
        .filter(|e| !is_hidden(e))
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    v.sort_by_key(|s| s.to_lowercase());
    v
}

#[cfg(windows)]
fn is_hidden(e: &std::fs::DirEntry) -> bool {
    use std::os::windows::fs::MetadataExt;
    const HIDDEN: u32 = 0x2;
    const SYSTEM: u32 = 0x4;
    e.metadata()
        .is_ok_and(|m| m.file_attributes() & (HIDDEN | SYSTEM) != 0)
}

#[cfg(not(windows))]
fn is_hidden(_: &std::fs::DirEntry) -> bool {
    false
}

/// Filtra: primero las que empiezan con el texto, después las que lo contienen.
/// Las que empiezan con punto solo aparecen si el filtro empieza con punto.
fn filter(names: &[String], query: &str) -> Vec<String> {
    let q = query.to_lowercase();
    let dot = q.starts_with('.');
    let visible = names.iter().filter(|n| dot || !n.starts_with('.'));
    let (mut starts, mut contains): (Vec<String>, Vec<String>) = (vec![], vec![]);
    for n in visible {
        let l = n.to_lowercase();
        if l.starts_with(&q) {
            starts.push(n.clone());
        } else if l.contains(&q) {
            contains.push(n.clone());
        }
    }
    starts.append(&mut contains);
    starts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn filter_prefers_prefix_and_hides_dotdirs() {
        let n = names(&[".git", "docs", "src", "assets", "crates", "installer"]);
        assert_eq!(filter(&n, ""), names(&["docs", "src", "assets", "crates", "installer"]));
        assert_eq!(filter(&n, "sr"), names(&["src"]));
        assert_eq!(filter(&n, "s"), names(&["src", "docs", "assets", "crates", "installer"]));
        assert_eq!(filter(&n, ".g"), names(&[".git"]));
    }

    #[test]
    fn navigate_and_build_lines() {
        let tmp = std::env::temp_dir().join(format!("neon-dirs-{}", std::process::id()));
        std::fs::create_dir_all(tmp.join("Mis Docs").join("sub")).unwrap();
        std::fs::create_dir_all(tmp.join("src")).unwrap();
        std::fs::write(tmp.join("archivo.txt"), "x").unwrap();
        let mut p = DirPopup::new(1, tmp.to_str().unwrap(), '/').unwrap();
        assert_eq!(p.entries, names(&["Mis Docs", "src", ".."]));
        p.type_text("mi");
        assert_eq!(p.entries, names(&["Mis Docs"]));
        match p.descend() {
            DirAction::Write(l) => assert_eq!(l, "cd \"Mis Docs/\""),
            _ => panic!(),
        }
        assert_eq!(p.entries, names(&["sub", ".."]));
        p.selected = 0;
        match p.choose() {
            DirAction::Run(l) => assert_eq!(l, "cd \"Mis Docs/sub\""),
            _ => panic!(),
        }
        match p.back() {
            DirAction::Write(l) => assert_eq!(l, "cd "),
            _ => panic!(),
        }
        assert!(matches!(p.back(), DirAction::Close));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
