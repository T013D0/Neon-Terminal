//! Perfiles de shell: qué programas se pueden abrir en una pestaña.
//!
//! En Windows detecta PowerShell (7 o 5.1), Símbolo del sistema, Git Bash y WSL.
//! En otros sistemas, bash.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Hash)]
pub enum ProfileKind {
    PowerShell,
    Cmd,
    GitBash,
    Wsl,
    Bash,
}

#[derive(Clone, Debug)]
pub struct Profile {
    pub kind: ProfileKind,
    pub name: &'static str,
    pub exe: PathBuf,
}

impl ProfileKind {
    /// Secuencia que borra la línea que se está escribiendo.
    pub fn clear_line(self) -> &'static str {
        match self {
            // PSReadLine (modo Windows) y cmd: Esc borra la línea.
            ProfileKind::PowerShell | ProfileKind::Cmd => "\x1b",
            // readline: ir al final y borrar hasta el inicio.
            _ => "\x05\x15",
        }
    }

    /// Si la shell reporta comandos/carpeta a Neon (cmd no tiene cómo).
    pub fn has_integration(self) -> bool {
        self != ProfileKind::Cmd
    }
}

fn in_path(exe: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(exe))
        .find(|p| p.is_file())
}

fn first_existing(paths: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    paths.into_iter().find(|p| p.is_file())
}

/// Perfiles disponibles en este equipo, en orden. El primero es el predeterminado.
pub fn available() -> Vec<Profile> {
    let mut out = vec![];
    if cfg!(windows) {
        let sys = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
            .join("System32");
        let pf = std::env::var_os("ProgramFiles")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Program Files"));

        let pwsh = in_path("pwsh.exe")
            .or_else(|| first_existing([pf.join(r"PowerShell\7\pwsh.exe")]))
            .unwrap_or_else(|| sys.join(r"WindowsPowerShell\v1.0\powershell.exe"));
        out.push(Profile {
            kind: ProfileKind::PowerShell,
            name: "PowerShell",
            exe: pwsh,
        });

        let cmd = std::env::var_os("ComSpec")
            .map(PathBuf::from)
            .unwrap_or_else(|| sys.join("cmd.exe"));
        out.push(Profile {
            kind: ProfileKind::Cmd,
            name: "Símbolo del sistema",
            exe: cmd,
        });

        // Git Bash: junto a git.exe (…\Git\cmd\git.exe → …\Git\bin\bash.exe) o en rutas típicas.
        let mut candidates = vec![];
        if let Some(git) = in_path("git.exe") {
            if let Some(root) = git.parent().and_then(Path::parent) {
                candidates.push(root.join(r"bin\bash.exe"));
            }
        }
        candidates.push(pf.join(r"Git\bin\bash.exe"));
        if let Some(local) = dirs::data_local_dir() {
            candidates.push(local.join(r"Programs\Git\bin\bash.exe"));
        }
        if let Some(bash) = first_existing(candidates) {
            out.push(Profile {
                kind: ProfileKind::GitBash,
                name: "Git Bash",
                exe: bash,
            });
        }

        let wsl = sys.join("wsl.exe");
        if wsl.is_file() {
            out.push(Profile {
                kind: ProfileKind::Wsl,
                name: "WSL",
                exe: wsl,
            });
        }
    } else {
        let bash = in_path("bash").unwrap_or_else(|| PathBuf::from("/bin/bash"));
        out.push(Profile {
            kind: ProfileKind::Bash,
            name: "Bash",
            exe: bash,
        });
    }
    out
}

/// `C:\Users\x\a.txt` → `/mnt/c/Users/x/a.txt` (para pasarle rutas a WSL).
pub fn to_wsl_path(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    let b = s.as_bytes();
    if b.len() >= 2 && b[1] == b':' {
        format!("/mnt/{}{}", s[..1].to_lowercase(), &s[2..])
    } else {
        s
    }
}

/// `C:\Users\x` → `C:/Users/x` (Git Bash entiende esa forma sin problemas).
pub fn to_forward_slashes(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_paths_for_wsl_and_git_bash() {
        assert_eq!(
            to_wsl_path(Path::new(r"C:\Users\nico\hook.bash")),
            "/mnt/c/Users/nico/hook.bash"
        );
        assert_eq!(to_wsl_path(Path::new("/tmp/x")), "/tmp/x");
        assert_eq!(to_forward_slashes(Path::new(r"C:\a\b")), "C:/a/b");
    }

    #[test]
    fn there_is_always_a_default_profile() {
        assert!(!available().is_empty());
    }
}
