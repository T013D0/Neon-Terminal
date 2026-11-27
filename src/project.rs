//! `.neon.toml` en la carpeta de un proyecto: nombre y color de la pestaña, y
//! comandos del proyecto en la paleta. Los comandos NUNCA se ejecutan solos
//! (un repo clonado no puede correr nada sin que lo elijas).
//!
//! ```toml
//! nombre = "API pagos"
//! color = "verde"          # rojo, naranja, amarillo, verde, cian, violeta
//!
//! [comandos]
//! dev = "npm run dev"
//! tests = "cargo test"
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(serde::Deserialize, Debug, Default, Clone, PartialEq)]
#[serde(default)]
pub struct Project {
    #[serde(alias = "name")]
    pub nombre: Option<String>,
    pub color: Option<String>,
    #[serde(alias = "commands")]
    pub comandos: BTreeMap<String, String>,
    /// Carpeta donde está el archivo (no viene del TOML).
    #[serde(skip)]
    pub dir: PathBuf,
}

impl Project {
    /// Índice en `workspace::tab_colors()`.
    pub fn color_index(&self) -> Option<u8> {
        let c = self.color.as_deref()?.trim().to_lowercase();
        let names = [
            ["rojo", "red"],
            ["naranja", "orange"],
            ["amarillo", "yellow"],
            ["verde", "green"],
            ["cian", "cyan"],
            ["violeta", "violet"],
        ];
        names
            .iter()
            .position(|n| n.contains(&c.as_str()))
            .map(|i| i as u8)
    }
}

/// Busca `.neon.toml` desde `cwd` hacia arriba (hasta 8 niveles).
pub fn find(cwd: &str) -> Option<Project> {
    let mut dir = Some(Path::new(cwd));
    for _ in 0..8 {
        let d = dir?;
        let f = d.join(".neon.toml");
        if f.is_file() {
            let text = std::fs::read_to_string(&f).ok()?;
            let mut p: Project = toml::from_str(&text).ok()?;
            p.dir = d.to_path_buf();
            return Some(p);
        }
        dir = d.parent();
    }
    None
}

/// Proyecto del panel enfocado (para los comandos de la paleta).
static CURRENT: std::sync::Mutex<Option<Project>> = std::sync::Mutex::new(None);

pub fn set_current(p: Option<Project>) {
    *CURRENT.lock().unwrap() = p;
}

pub fn current() -> Option<Project> {
    CURRENT.lock().unwrap().clone()
}

/// Lee `.neon.toml` como máximo una vez cada pocos segundos por carpeta.
pub struct Cache {
    entries: Vec<(String, Option<Project>, std::time::Instant)>,
}

impl Cache {
    pub const fn new() -> Self {
        Self { entries: Vec::new() }
    }

    pub fn get(&mut self, cwd: &str) -> Option<Project> {
        let fresh = std::time::Duration::from_secs(5);
        if let Some((_, p, t)) = self.entries.iter().find(|(c, _, _)| c == cwd) {
            if t.elapsed() < fresh {
                return p.clone();
            }
        }
        let p = find(cwd);
        self.entries.retain(|(c, _, _)| c != cwd);
        self.entries.push((cwd.to_string(), p.clone(), std::time::Instant::now()));
        // Pocas carpetas recientes, nada más.
        if self.entries.len() > 32 {
            self.entries.remove(0);
        }
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_upwards_and_parses() {
        let tmp = std::env::temp_dir().join(format!("neon-proj-{}", std::process::id()));
        std::fs::create_dir_all(tmp.join("src/deep")).unwrap();
        std::fs::write(
            tmp.join(".neon.toml"),
            "nombre = \"API pagos\"\ncolor = \"Verde\"\n[comandos]\ndev = \"npm run dev\"\ntests = \"cargo test\"\n",
        )
        .unwrap();
        let p = find(tmp.join("src/deep").to_str().unwrap()).unwrap();
        assert_eq!(p.nombre.as_deref(), Some("API pagos"));
        assert_eq!(p.color_index(), Some(3));
        assert_eq!(p.comandos.get("dev").map(String::as_str), Some("npm run dev"));
        assert_eq!(p.dir, tmp);
        // Un TOML roto no rompe nada: simplemente no hay proyecto.
        std::fs::write(tmp.join(".neon.toml"), "nombre = ").unwrap();
        assert!(find(tmp.to_str().unwrap()).is_none());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
