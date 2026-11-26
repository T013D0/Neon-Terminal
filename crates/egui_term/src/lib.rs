mod backend;
mod bindings;
mod font;
mod theme;
mod types;
mod view;

/// Patched (neon): arrastrar selecciona aunque la app use el mouse (Shift se lo
/// manda a la app). Lo fija la aplicación según Ajustes.
pub static MOUSE_SELECT: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(true);

/// Patched (neon): en apps de pantalla alterna con mouse (Claude Code) la
/// selección la maneja la aplicación (no la de alacritty, que se borra al
/// redibujar). La vista solo pinta las celdas que se le pasan con `set_highlight`.
pub static VIRTUAL_SELECT: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Patched (neon): animaciones (cursor suave). Lo fija la aplicación.
pub static ANIMATIONS: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(true);

pub use backend::settings::BackendSettings;
pub use backend::{BackendCommand, PtyEvent, TerminalBackend, TerminalMode};
pub use bindings::{Binding, BindingAction, InputKind, KeyboardBinding};
pub use font::{FontSettings, TerminalFont};
pub use theme::{ColorPalette, TerminalTheme};
pub use view::TerminalView;
