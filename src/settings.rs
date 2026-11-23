//! Ajustes de Neon, guardados en `%LOCALAPPDATA%\neon\settings.toml`.
//!
//! Se cambian desde la pantalla de Ajustes (Ctrl+,) o editando el archivo:
//! al guardarlo, Neon lo recarga solo.

use crate::profiles::ProfileKind;
use crate::themes::Theme;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::RwLock;
use std::time::SystemTime;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Backdrop {
    /// Fondo sólido.
    Ninguno,
    /// Efecto Mica de Windows 11 (toma el tono del fondo de pantalla).
    Mica,
    /// Vidrio esmerilado de Windows 11.
    Acrylic,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Nombre del tema (incluido o importado).
    pub theme: String,
    pub font_size: f32,
    /// Shell de las pestañas nuevas (None = la primera disponible).
    pub default_profile: Option<ProfileKind>,
    /// Líneas que se guardan al hacer scroll hacia arriba (por panel).
    pub scrollback: usize,
    pub restore_session: bool,
    /// Prompt con cápsulas de Neon (si no, el de tu shell).
    pub neon_prompt: bool,
    /// Preguntar antes de cerrar si hay un comando corriendo.
    pub confirm_close: bool,
    pub notify: bool,
    /// Avisar cuando un comando tarde al menos esto (segundos).
    pub notify_after_secs: u64,
    pub backdrop: Backdrop,
    /// Opacidad del fondo (1 = sólido).
    pub opacity: f32,
    /// Atajos cambiados por el usuario: acción → "Ctrl+Shift+T" (vacío = sin atajo).
    pub keys: BTreeMap<String, String>,
    /// Temas importados.
    pub custom_themes: Vec<Theme>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "Brasa".into(),
            font_size: crate::theme::TERM_FONT_SIZE,
            default_profile: None,
            scrollback: 10_000,
            restore_session: true,
            neon_prompt: true,
            confirm_close: true,
            notify: true,
            notify_after_secs: 10,
            backdrop: Backdrop::Ninguno,
            opacity: 1.0,
            keys: BTreeMap::new(),
            custom_themes: vec![],
        }
    }
}

static CURRENT: RwLock<Option<Settings>> = RwLock::new(None);

pub fn path() -> PathBuf {
    crate::shell::data_dir().join("settings.toml")
}

/// Ajustes actuales (copia).
pub fn get() -> Settings {
    if let Some(s) = CURRENT.read().unwrap().as_ref() {
        return s.clone();
    }
    let s = load_file().unwrap_or_default();
    *CURRENT.write().unwrap() = Some(s.clone());
    s
}

pub fn set(s: Settings) {
    *CURRENT.write().unwrap() = Some(s);
}

fn load_file() -> Option<Settings> {
    let text = std::fs::read_to_string(path()).ok()?;
    toml::from_str(&text).ok()
}

/// Escribe el archivo. Devuelve la fecha de modificación resultante.
pub fn save(s: &Settings) -> Option<SystemTime> {
    let p = path();
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let body = toml::to_string_pretty(s).ok()?;
    let text = format!(
        "# Ajustes de Neon. Puedes editarlo a mano: al guardar, Neon lo recarga.\n\
         # Temas incluidos: Brasa, Brasa claro, Medianoche, Sumi (o los importados).\n\
         # backdrop: \"Ninguno\", \"Mica\" o \"Acrylic\" (Windows 11).\n\
         # keys: atajos cambiados, por ejemplo  new_tab = \"Ctrl+Shift+T\"\n\n{body}"
    );
    std::fs::write(&p, text).ok()?;
    modified()
}

pub fn modified() -> Option<SystemTime> {
    std::fs::metadata(path()).and_then(|m| m.modified()).ok()
}

/// Si el archivo cambió desde `since`, lo vuelve a leer.
pub fn reload_if_changed(since: Option<SystemTime>) -> Option<(Settings, Option<SystemTime>)> {
    let now = modified();
    if now.is_none() || now == since {
        return None;
    }
    let s = load_file()?;
    Some((s, now))
}

/// Todos los temas disponibles: incluidos + importados.
pub fn all_themes(s: &Settings) -> Vec<Theme> {
    let mut v = crate::themes::builtin();
    for t in &s.custom_themes {
        if !v.iter().any(|b| b.name == t.name) {
            v.push(t.clone());
        }
    }
    v
}

pub fn theme_by_name(s: &Settings) -> Theme {
    let all = all_themes(s);
    all.iter()
        .find(|t| t.name == s.theme)
        .cloned()
        .unwrap_or_else(|| all[0].clone())
}

// ───────────────────────── Atajos ─────────────────────────

/// "Ctrl+Shift+T" → atajo de egui.
pub fn parse_shortcut(s: &str) -> Option<egui::KeyboardShortcut> {
    let mut mods = egui::Modifiers::NONE;
    let mut key = None;
    for part in s.split('+').map(str::trim).filter(|p| !p.is_empty()) {
        match part.to_lowercase().as_str() {
            "ctrl" | "control" => mods.ctrl = true,
            "shift" => mods.shift = true,
            "alt" => mods.alt = true,
            "cmd" | "command" => mods.command = true,
            _ => {
                key = egui::Key::from_name(part)
                    .or_else(|| egui::Key::from_name(&part.to_uppercase()))
            }
        }
    }
    // En Windows/Linux, Ctrl es "command" para egui.
    if mods.ctrl {
        mods.command = true;
    }
    Some(egui::KeyboardShortcut::new(mods, key?))
}

pub fn format_shortcut(sc: &egui::KeyboardShortcut) -> String {
    let mut parts = vec![];
    if sc.modifiers.ctrl || sc.modifiers.command {
        parts.push("Ctrl".to_string());
    }
    if sc.modifiers.shift {
        parts.push("Shift".into());
    }
    if sc.modifiers.alt {
        parts.push("Alt".into());
    }
    parts.push(sc.logical_key.name().to_string());
    parts.join("+")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_roundtrip() {
        for s in [
            "Ctrl+Shift+T",
            "Alt+ArrowLeft",
            "Ctrl+Comma",
            "Ctrl+Shift+Enter",
            "Ctrl+Equals",
        ] {
            let sc = parse_shortcut(s).unwrap_or_else(|| panic!("{s}"));
            // El texto puede cambiar de forma ("ArrowLeft" → "Left") pero vuelve al mismo atajo.
            assert_eq!(parse_shortcut(&format_shortcut(&sc)), Some(sc), "{s}");
        }
        assert_eq!(
            format_shortcut(&parse_shortcut("ctrl+shift+t").unwrap()),
            "Ctrl+Shift+T"
        );
        assert!(parse_shortcut("Ctrl+").is_none());
    }

    #[test]
    fn settings_roundtrip_through_toml() {
        let mut s = Settings::default();
        s.theme = "Sumi".into();
        s.keys.insert("new_tab".into(), "Ctrl+T".into());
        s.custom_themes.push(crate::themes::builtin().remove(1));
        let text = toml::to_string_pretty(&s).unwrap();
        let back: Settings = toml::from_str(&text).unwrap();
        assert_eq!(back, s);
        // Un archivo incompleto usa los valores por defecto para lo que falte.
        let partial: Settings = toml::from_str("font_size = 16.0").unwrap();
        assert_eq!(partial.font_size, 16.0);
        assert!(partial.restore_session);
    }
}
