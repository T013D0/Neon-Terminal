//! Claude sin API key: usa Claude Code (ya instalado y con sesión iniciada) en
//! modo sin interfaz, `claude -p`. Las preguntas van por la entrada estándar,
//! así no hay problemas de comillas entre shells.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

/// Un pedido a Claude que corre en segundo plano.
pub struct AiJob {
    pub pane_id: u64,
    pub kind: AiKind,
    pub rx: Receiver<Result<String, String>>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AiKind {
    /// `# pedido` → comando.
    Command { request: String },
    /// Explicar por qué falló un comando.
    Explain { command: String },
}

/// Busca el ejecutable de Claude Code en el PATH (en Windows puede ser
/// `claude.exe` o el `claude.cmd` que instala npm).
pub fn claude_exe() -> Option<PathBuf> {
    let names: &[&str] = if cfg!(windows) {
        &["claude.exe", "claude.cmd", "claude.bat"]
    } else {
        &["claude"]
    };
    let path = std::env::var_os("PATH")?;
    let mut dirs: Vec<PathBuf> = std::env::split_paths(&path).collect();
    // Instalaciones típicas que a veces no están en el PATH de una app gráfica.
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".local").join("bin"));
        dirs.push(home.join(".claude").join("local"));
        if cfg!(windows) {
            dirs.push(home.join("AppData").join("Roaming").join("npm"));
        }
    }
    dirs.iter()
        .flat_map(|d| names.iter().map(move |n| d.join(n)))
        .find(|p| p.is_file())
}

/// Corre `claude -p` con la pregunta por stdin. Devuelve la respuesta (texto).
pub fn ask(system: &str, prompt: &str, cwd: &str, model: &str) -> Result<String, String> {
    let exe = claude_exe().ok_or(
        "No encontré Claude Code. Instálalo (https://claude.com/claude-code) e inicia sesión con «claude».",
    )?;
    let run = |model: Option<&str>| -> Result<String, String> {
        let mut cmd = Command::new(&exe);
        cmd.arg("-p")
            .args(["--tools", ""])
            .args(["--system-prompt", system])
            .arg("--no-session-persistence");
        if let Some(m) = model {
            cmd.args(["--model", m]);
        }
        if std::path::Path::new(cwd).is_dir() {
            cmd.current_dir(cwd);
        }
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let mut child = cmd.spawn().map_err(|e| format!("No pude abrir Claude Code: {e}"))?;
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(prompt.as_bytes());
        }
        // Espera con límite: si Claude se cuelga, no queda un proceso eterno.
        let t0 = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if t0.elapsed() > Duration::from_secs(90) => {
                    let _ = child.kill();
                    return Err("Claude tardó demasiado en responder.".into());
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                Err(e) => return Err(e.to_string()),
            }
        }
        let mut out = String::new();
        let mut err = String::new();
        if let Some(mut o) = child.stdout.take() {
            let _ = o.read_to_string(&mut out);
        }
        if let Some(mut e) = child.stderr.take() {
            let _ = e.read_to_string(&mut err);
        }
        let status = child.wait().map_err(|e| e.to_string())?;
        if status.success() && !out.trim().is_empty() {
            Ok(out.trim().to_string())
        } else {
            let msg = if err.trim().is_empty() { out } else { err };
            Err(msg.trim().lines().last().unwrap_or("Claude Code falló").to_string())
        }
    };
    // Con un modelo rápido; si ese nombre no existe en tu versión, con el de siempre.
    match run(Some(model).filter(|m| !m.is_empty())) {
        Ok(s) => Ok(s),
        Err(e) if e.to_lowercase().contains("model") => run(None),
        Err(e) => Err(e),
    }
}

/// Lanza `ask` en un hilo y avisa a la interfaz al terminar.
pub fn spawn(
    pane_id: u64,
    kind: AiKind,
    system: String,
    prompt: String,
    cwd: String,
    model: String,
    ctx: egui::Context,
) -> AiJob {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let r = ask(&system, &prompt, &cwd, &model);
        let _ = tx.send(r);
        ctx.request_repaint();
    });
    AiJob {
        pane_id,
        kind,
        rx,
    }
}

/// Instrucciones para convertir un pedido en un comando.
pub fn command_system(shell: &str, os: &str, cwd: &str) -> String {
    format!(
        "Conviertes pedidos en lenguaje natural en UN comando para la shell {shell} en {os}. \
         La carpeta actual es {cwd}. Responde SOLO con el comando, en una línea, sin \
         explicación, sin comillas de código ni ```. Si hacen falta varios pasos, \
         encadénalos en una sola línea como se haga en esa shell. Si el pedido no se \
         puede hacer con un comando, responde una línea que empiece con «# » y diga por qué."
    )
}

/// Instrucciones para explicar un error.
pub fn explain_system(shell: &str, os: &str) -> String {
    format!(
        "Eres un asistente dentro de una terminal ({shell} en {os}). Te paso un comando que \
         falló y su salida. Explica en español, breve (máximo 8 líneas), por qué falló y cómo \
         arreglarlo. Si la solución es un comando, ponlo en su propia línea empezando con «$ ». \
         Sin encabezados ni markdown elaborado."
    )
}

/// Limpia la respuesta: quita ``` y comillas invertidas que a veces se cuelan.
pub fn clean_command(s: &str) -> String {
    let lines: Vec<&str> = s
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("```"))
        .collect();
    let line = lines.first().copied().unwrap_or("");
    let line = line.trim_matches('`').trim();
    let line = line.strip_prefix("$ ").unwrap_or(line);
    line.to_string()
}

/// Los últimos ~`max` caracteres de la salida (lo más útil de un error está al final).
pub fn tail(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut start = s.len() - max;
    while !s.is_char_boundary(start) {
        start += 1;
    }
    format!("…{}", &s[start..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_commands() {
        assert_eq!(clean_command("```bash\nls -la\n```"), "ls -la");
        assert_eq!(clean_command("`git status`"), "git status");
        assert_eq!(clean_command("$ npm test"), "npm test");
        assert_eq!(clean_command("\n  Get-ChildItem | Sort Length  \n"), "Get-ChildItem | Sort Length");
    }

    #[test]
    fn tail_keeps_end() {
        assert_eq!(tail("abcdef", 10), "abcdef");
        assert_eq!(tail("abcdef", 3), "…def");
        // Sin cortar un carácter de varios bytes por la mitad.
        assert!(tail("añoñoño", 3).ends_with("ño"));
    }
}
