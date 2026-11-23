//! Un panel = una shell + su archivo de eventos + su estado de git.

use crate::git::GitWatcher;
use crate::profiles::{Profile, ProfileKind};
use crate::shell::{self, SessionLog, ShellEvent};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags;
use egui_term::{BackendCommand, PtyEvent, TerminalBackend};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

pub struct LastCommand {
    pub command: String,
    pub ok: bool,
    pub duration_ms: Option<i64>,
}

/// Un comando que terminó, con cuánto tardó medido por Neon (sirve para
/// cualquier shell, incluso las que no informan la duración).
pub struct Finished {
    pub event: ShellEvent,
    pub elapsed: Option<Duration>,
}

/// Búsqueda en la salida de un panel (Ctrl+Shift+F).
#[derive(Default)]
pub struct Search {
    pub open: bool,
    pub query: String,
    pub focus: bool,
    /// (línea absoluta, columna inicial, columna final exclusiva)
    pub matches: Vec<(i64, usize, usize)>,
    pub current: usize,
    computed: Option<(String, usize, i64)>,
}

pub struct Pane {
    pub id: u64,
    pub profile: ProfileKind,
    pub backend: TerminalBackend,
    pub title: Option<String>,
    /// Sonó la campana mientras no mirabas este panel.
    pub attention: bool,
    /// Hubo salida nueva mientras no mirabas este panel.
    pub activity: bool,
    pub cwd: String,
    pub log: SessionLog,
    pub git: GitWatcher,
    pub last: Option<LastCommand>,
    /// La shell está esperando un comando (llegó un prompt y no se ha dado Enter).
    pub at_prompt: bool,
    /// Lo que se ha escrito en la línea desde el último prompt (aproximado).
    pub typed: String,
    /// false si se usaron flechas/Tab/etc. y `typed` ya no es confiable.
    pub typed_exact: bool,
    /// Dónde se dibujó la terminal en el último frame (para ubicar popups).
    pub term_rect: Option<egui::Rect>,
    /// Líneas que ocupa el prompt actual.
    prompt_lines: u32,
    /// Línea (absoluta) donde empieza la salida del comando en curso.
    cmd_start_line: Option<i64>,
    cmd_started_at: Option<Instant>,
    /// Línea donde empieza la salida del último comando terminado.
    last_output_line: Option<i64>,
    /// Líneas donde se escribió cada comando (para Ctrl+↑ / Ctrl+↓).
    prompt_marks: Vec<i64>,
    last_history: usize,
    /// Comando que está corriendo (para confirmar antes de cerrar).
    running_cmd: Option<String>,
    pub search: Search,
    /// Para mostrar la barra de scroll un rato después de moverse.
    pub scroll_seen: (usize, Instant),
}

impl Pane {
    pub fn new(
        id: u64,
        ctx: &egui::Context,
        pty_tx: Sender<(u64, PtyEvent)>,
        cwd: Option<String>,
        profile: &Profile,
    ) -> anyhow::Result<Self> {
        let events =
            shell::data_dir()
                .join("sessions")
                .join(format!("{}-{}.jsonl", std::process::id(), id));
        let log = SessionLog::new(events.clone());
        let start_dir = cwd
            .clone()
            .map(PathBuf::from)
            .filter(|p| p.is_dir())
            .or_else(dirs::home_dir);
        let settings = shell::backend_settings(profile, &events, start_dir.clone());
        let backend = TerminalBackend::new(id, ctx.clone(), pty_tx, settings)?;
        Ok(Self {
            id,
            profile: profile.kind,
            backend,
            title: None,
            attention: false,
            activity: false,
            cwd: start_dir
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
            log,
            git: GitWatcher::new(),
            last: None,
            at_prompt: false,
            typed: String::new(),
            typed_exact: true,
            term_rect: None,
            prompt_lines: 1,
            cmd_start_line: None,
            cmd_started_at: None,
            last_output_line: None,
            prompt_marks: vec![],
            last_history: 0,
            running_cmd: None,
            search: Search::default(),
            scroll_seen: (0, Instant::now() - Duration::from_secs(10)),
        })
    }

    /// Escribe texto en la línea de la shell (sin ejecutarlo).
    pub fn insert(&mut self, text: &str) {
        self.backend
            .process_command(BackendCommand::Write(text.as_bytes().to_vec()));
    }

    /// Reemplaza la línea actual por `text` (si estamos en el prompt).
    pub fn replace_line(&mut self, text: &str) {
        if self.at_prompt {
            let clear = self.profile.clear_line();
            self.insert(clear);
        }
        self.insert(text);
        self.typed = text.to_string();
        self.typed_exact = true;
    }

    pub fn run(&mut self, text: &str) {
        self.replace_line(text);
        self.command_started();
        self.insert("\r");
        self.at_prompt = false;
        self.typed.clear();
    }

    fn command_started(&mut self) {
        if self.at_prompt {
            let line = self.cursor_line();
            self.cmd_start_line = Some(line + 1);
            self.cmd_started_at = Some(Instant::now());
            self.running_cmd = Some(self.typed.trim().to_string()).filter(|t| !t.is_empty());
            if self.prompt_marks.last() != Some(&line) {
                self.prompt_marks.push(line);
            }
        }
    }

    /// Qué está corriendo, si hay algo (None si la shell espera un comando).
    pub fn running(&self) -> Option<String> {
        if self.at_prompt || !self.profile.has_integration() || self.cmd_started_at.is_none() {
            return None;
        }
        Some(
            self.running_cmd
                .clone()
                .unwrap_or_else(|| "un comando".into()),
        )
    }

    /// (líneas de historial, líneas en pantalla, cuánto se subió con el scroll)
    pub fn scroll_info(&self) -> (usize, usize, usize) {
        let g = &self.backend.last_content().grid;
        (g.history_size(), g.screen_lines(), g.display_offset())
    }

    pub fn scroll_by(&mut self, lines: i32) {
        if lines != 0 {
            self.backend.process_command(BackendCommand::Scroll(lines));
        }
    }

    /// Deja la línea absoluta `abs` arriba (o al centro) de la pantalla.
    pub fn scroll_to(&mut self, abs: i64, center: bool) {
        let (hist, screen, offset) = self.scroll_info();
        let top = if center { abs - screen as i64 / 2 } else { abs };
        let desired = (hist as i64 - top).clamp(0, hist as i64);
        self.scroll_by((desired - offset as i64) as i32);
    }

    /// Ctrl+↑ / Ctrl+↓: salta al comando anterior / siguiente.
    pub fn jump_prompt(&mut self, prev: bool) {
        let (hist, _, offset) = self.scroll_info();
        let top = hist as i64 - offset as i64;
        let target = if prev {
            self.prompt_marks.iter().rev().find(|m| **m < top).copied()
        } else {
            self.prompt_marks.iter().find(|m| **m > top).copied()
        };
        match target {
            Some(l) => self.scroll_to(l, false),
            None if prev => self.scroll_to(0, false),
            None => self.scroll_by(-(offset as i32)),
        }
    }

    /// Se llama cada frame: detecta `clear` (el historial se achica) y el scroll.
    pub fn tick(&mut self) {
        let (hist, _, offset) = self.scroll_info();
        if hist < self.last_history {
            // Se borró el historial: las marcas ya no sirven.
            self.prompt_marks.clear();
            self.last_output_line = None;
            self.search.computed = None;
        }
        self.last_history = hist;
        if offset != self.scroll_seen.0 {
            self.scroll_seen = (offset, Instant::now());
        }
        if self.search.open {
            self.update_search();
        }
    }

    /// Abre la búsqueda; si hay texto seleccionado, lo usa.
    pub fn open_search(&mut self) {
        let sel = self.backend.selectable_content();
        let sel = sel.trim();
        if !sel.is_empty() && !sel.contains('\n') && sel.len() < 200 {
            self.search.query = sel.to_string();
        }
        self.search.open = true;
        self.search.focus = true;
        self.search.computed = None;
    }

    pub fn close_search(&mut self) {
        self.search.open = false;
        self.search.matches.clear();
    }

    /// Recalcula los resultados si cambió el texto buscado o la salida.
    fn update_search(&mut self) {
        let (hist, _, _) = self.scroll_info();
        let key = (self.search.query.clone(), hist, self.cursor_line());
        if self.search.computed.as_ref() == Some(&key) {
            return;
        }
        let first = self.search.computed.as_ref().is_none_or(|c| c.0 != key.0);
        self.search.matches = find_all(&self.backend.last_content().grid, &self.search.query);
        self.search.computed = Some(key);
        if first {
            // Empieza por el resultado más reciente (el de más abajo).
            self.search.current = self.search.matches.len().saturating_sub(1);
            self.reveal_current();
        } else {
            self.search.current = self
                .search
                .current
                .min(self.search.matches.len().saturating_sub(1));
        }
    }

    pub fn search_step(&mut self, forward: bool) {
        let n = self.search.matches.len();
        if n == 0 {
            return;
        }
        self.search.current = if forward {
            (self.search.current + 1) % n
        } else {
            (self.search.current + n - 1) % n
        };
        self.reveal_current();
    }

    fn reveal_current(&mut self) {
        if let Some(&(line, _, _)) = self.search.matches.get(self.search.current) {
            let (hist, screen, offset) = self.scroll_info();
            let top = hist as i64 - offset as i64;
            if line < top || line >= top + screen as i64 {
                self.scroll_to(line, true);
            }
        }
    }

    /// Pega texto (como si se escribiera), respetando el "bracketed paste".
    /// ¿Hay un Claude Code corriendo en este panel?
    pub fn is_claude(&self) -> bool {
        let cmd = self.running_cmd.as_deref().unwrap_or("").to_lowercase();
        let title = self.title.as_deref().unwrap_or("");
        let first = cmd.split_whitespace().next().unwrap_or("");
        let first = first.rsplit(['/', '\\']).next().unwrap_or("");
        first.trim_end_matches(".exe") == "claude"
            || title.contains("Claude Code")
            || title.starts_with('✳')
    }

    /// Claude Code pone una animación braille al inicio del título mientras trabaja.
    pub fn is_working(&self) -> bool {
        self.title
            .as_deref()
            .and_then(|t| t.chars().next())
            .is_some_and(|c| ('\u{2800}'..='\u{28ff}').contains(&c))
    }

    /// Rutas listas para escribir en esta shell (con comillas si hay espacios).
    pub fn format_paths(&self, paths: &[PathBuf]) -> String {
        let mut out = String::new();
        for p in paths {
            let s = match self.profile {
                ProfileKind::Wsl => crate::profiles::to_wsl_path(p),
                ProfileKind::GitBash => crate::profiles::to_forward_slashes(p),
                _ => p.to_string_lossy().into_owned(),
            };
            if s.contains(char::is_whitespace) {
                out.push_str(&format!("\"{s}\" "));
            } else {
                out.push_str(&format!("{s} "));
            }
        }
        out
    }

    pub fn paste(&mut self, text: &str) {
        let bracketed = self
            .backend
            .last_content()
            .terminal_mode
            .contains(egui_term::TerminalMode::BRACKETED_PASTE);
        let text = text.replace("\r\n", "\r").replace('\n', "\r");
        if bracketed {
            self.insert(&format!("\x1b[200~{text}\x1b[201~"));
        } else {
            self.insert(&text);
        }
        self.typed_exact = false;
    }

    /// Limpia la pantalla con el comando propio de cada shell.
    pub fn clear_screen(&mut self) {
        let cmd = match self.profile {
            ProfileKind::PowerShell => "Clear-Host",
            ProfileKind::Cmd => "cls",
            _ => "clear",
        };
        if self.at_prompt || !self.profile.has_integration() {
            self.run(cmd);
        }
    }

    /// No hay un programa a pantalla completa (vim, htop…) corriendo.
    pub fn is_alt_screen(&self) -> bool {
        self.backend
            .last_content()
            .terminal_mode
            .contains(egui_term::TerminalMode::ALT_SCREEN)
    }

    /// Línea absoluta del cursor (contando el historial de scroll).
    fn cursor_line(&self) -> i64 {
        let g = &self.backend.last_content().grid;
        g.history_size() as i64 + g.cursor.point.line.0 as i64
    }

    /// Posición en pantalla de la fila del cursor: (y de arriba, alto de celda).
    pub fn cursor_row(&self) -> Option<(f32, f32)> {
        let rect = self.term_rect?;
        let c = self.backend.last_content();
        let h = c.terminal_size.cell_height as f32;
        let line = c.grid.cursor.point.line.0 + c.grid.display_offset() as i32;
        Some((rect.min.y + line.max(0) as f32 * h, h))
    }

    /// Sigue lo que se escribe en el prompt para saber qué hay en la línea.
    pub fn track_key_events(&mut self, events: &[egui::Event]) {
        use egui::{Event, Key};
        for e in events {
            match e {
                Event::Text(t) | Event::Paste(t) => self.typed.push_str(t),
                Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => match key {
                    Key::Enter => {
                        self.command_started();
                        self.at_prompt = false;
                        self.typed.clear();
                        self.typed_exact = true;
                    }
                    Key::Backspace => {
                        self.typed.pop();
                    }
                    Key::C if modifiers.ctrl => {
                        self.typed.clear();
                        self.typed_exact = true;
                    }
                    Key::Escape if self.profile.clear_line() == "\x1b" => self.typed.clear(),
                    Key::ArrowLeft
                    | Key::ArrowRight
                    | Key::ArrowUp
                    | Key::ArrowDown
                    | Key::Home
                    | Key::End
                    | Key::Tab
                    | Key::Delete => self.typed_exact = false,
                    _ => {}
                },
                _ => {}
            }
        }
    }

    /// Lee eventos nuevos de la shell. Devuelve los comandos terminados.
    pub fn poll_events(&mut self) -> Vec<Finished> {
        let events = self.log.poll();
        let mut done = vec![];
        for e in events {
            // Cualquier evento llega justo antes de dibujar el prompt.
            self.at_prompt = true;
            self.typed.clear();
            self.typed_exact = true;
            if let Some(n) = e.prompt_lines {
                self.prompt_lines = n.max(1);
            }
            if let Some(c) = e.new_cwd.as_ref().or(e.cwd.as_ref()) {
                if e.kind == "prompt" || e.new_cwd.is_some() {
                    self.cwd = c.clone();
                }
            }
            if e.kind == "command" {
                self.last = Some(LastCommand {
                    command: e.command.clone().unwrap_or_default(),
                    ok: e.ok.unwrap_or(true),
                    duration_ms: e.duration_ms,
                });
                self.git.invalidate();
                self.last_output_line = self.cmd_start_line.take();
                self.running_cmd = None;
                let elapsed = self.cmd_started_at.take().map(|t| t.elapsed());
                done.push(Finished { event: e, elapsed });
            }
        }
        done
    }

    /// Texto que imprimió el último comando (sin el prompt).
    pub fn last_output(&self) -> Result<String, &'static str> {
        if !self.at_prompt {
            return Err("Hay un comando corriendo; espera a que termine.");
        }
        let start = self
            .last_output_line
            .ok_or("Todavía no hay un comando para copiar.")?;
        // La salida termina donde empieza el prompt actual.
        let end = self.cursor_line() - (self.prompt_lines as i64 - 1);
        let text = self.text_between(start, end);
        if text.trim().is_empty() {
            Err("El último comando no imprimió nada.")
        } else {
            Ok(text)
        }
    }

    /// Texto de las líneas absolutas [start, end).
    fn text_between(&self, start: i64, end: i64) -> String {
        let g = &self.backend.last_content().grid;
        let hist = g.history_size() as i64;
        let cols = g.columns();
        let (top, bottom) = (g.topmost_line().0 as i64, g.bottommost_line().0 as i64);
        let mut out = String::new();
        for abs in start..end {
            let l = abs - hist;
            if l < top || l > bottom {
                continue;
            }
            let row = &g[Line(l as i32)];
            let mut s = String::new();
            for c in 0..cols {
                let cell = &row[Column(c)];
                if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    continue;
                }
                s.push(cell.c);
            }
            let wraps = cols > 0 && row[Column(cols - 1)].flags.contains(Flags::WRAPLINE);
            if wraps {
                out.push_str(&s);
            } else {
                out.push_str(s.trim_end());
                out.push('\n');
            }
        }
        out.trim_end_matches('\n')
            .trim_start_matches('\n')
            .to_string()
    }

    /// Abre un link que se hizo Ctrl+click: URL → navegador; ruta → editor.
    /// Devuelve un mensaje si no se pudo.
    pub fn open_link(&self, link: &str) -> Option<String> {
        let link = link.trim_end_matches(['.', ',', ')', ']', ';', ':', '\'', '"']);
        if link.contains("://") || link.starts_with("mailto:") || link.starts_with("magnet:") {
            return open::that_detached(link)
                .err()
                .map(|e| format!("No pude abrir el link: {e}"));
        }
        let (path, line, col) = split_line_col(link);
        let p = PathBuf::from(path);
        let full = if p.is_absolute() {
            p
        } else {
            PathBuf::from(&self.cwd).join(p)
        };
        if !full.exists() {
            return Some(format!("No encontré {path}"));
        }
        if let Some(line) = line {
            if open_in_vscode(&full, line, col) {
                return None;
            }
        }
        open::that_detached(&full)
            .err()
            .map(|e| format!("No pude abrir {path}: {e}"))
    }

    pub fn display_title(&self) -> String {
        let dir = std::path::Path::new(&self.cwd)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| self.cwd.clone());
        match &self.git.status {
            Some(g) => format!("{dir} · {}", g.branch),
            None if !dir.is_empty() => dir,
            None => self.title.clone().unwrap_or_else(|| "terminal".into()),
        }
    }
}

/// Busca `query` (sin distinguir mayúsculas) en todas las líneas, incluido el historial.
pub fn find_all(
    g: &alacritty_terminal::Grid<alacritty_terminal::term::cell::Cell>,
    query: &str,
) -> Vec<(i64, usize, usize)> {
    let q: Vec<char> = query.chars().flat_map(char::to_lowercase).collect();
    if q.is_empty() {
        return vec![];
    }
    let hist = g.history_size() as i64;
    let cols = g.columns();
    let mut out = vec![];
    for l in g.topmost_line().0..=g.bottommost_line().0 {
        let row = &g[Line(l)];
        let chars: Vec<char> = (0..cols)
            .map(|c| row[Column(c)].c.to_lowercase().next().unwrap_or(' '))
            .collect();
        if chars.len() < q.len() {
            continue;
        }
        let mut c = 0;
        while c + q.len() <= chars.len() {
            if chars[c..c + q.len()] == q[..] {
                out.push((l as i64 + hist, c, c + q.len()));
                c += q.len();
            } else {
                c += 1;
            }
        }
    }
    out
}

/// `src/app.rs:120:5` → ("src/app.rs", Some(120), Some(5)). No confunde `C:\` con un número.
pub fn split_line_col(s: &str) -> (&str, Option<u32>, Option<u32>) {
    let mut path = s;
    let mut nums = vec![];
    for _ in 0..2 {
        match path.rsplit_once(':') {
            Some((head, tail)) if !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit()) => {
                nums.push(tail.parse().unwrap_or(0));
                path = head;
            }
            _ => break,
        }
    }
    nums.reverse();
    (path, nums.first().copied(), nums.get(1).copied())
}

/// Abre el archivo en VS Code en la línea indicada, si `code` está instalado.
fn open_in_vscode(path: &std::path::Path, line: u32, col: Option<u32>) -> bool {
    let target = format!("{}:{}:{}", path.display(), line, col.unwrap_or(1));
    let mut cmd = if cfg!(windows) {
        // `code` es un .cmd en Windows: hay que pasar por cmd.exe.
        let mut c = std::process::Command::new("cmd.exe");
        c.args(["/C", "code", "-g", &target]);
        c
    } else {
        let mut c = std::process::Command::new("code");
        c.args(["-g", &target]);
        c
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd.stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

#[cfg(test)]
mod tests {
    use super::split_line_col;

    #[test]
    fn splits_line_and_column() {
        assert_eq!(
            split_line_col("src/app.rs:120:5"),
            ("src/app.rs", Some(120), Some(5))
        );
        assert_eq!(
            split_line_col("src/app.rs:7"),
            ("src/app.rs", Some(7), None)
        );
        assert_eq!(split_line_col(r"C:\dev\x.rs"), (r"C:\dev\x.rs", None, None));
        assert_eq!(
            split_line_col(r"C:\dev\x.rs:3"),
            (r"C:\dev\x.rs", Some(3), None)
        );
        assert_eq!(split_line_col("README.md"), ("README.md", None, None));
    }
}
