//! Atajo global (modo Quake): muestra u oculta Neon desde cualquier app.
//!
//! En Windows se registra con `RegisterHotKey` en un hilo propio que espera
//! `WM_HOTKEY`; cada vez que llega avisa por un canal y despierta la interfaz.

use std::sync::mpsc::{Receiver, channel};

#[cfg_attr(not(windows), allow(dead_code))]
pub enum HotkeyMsg {
    Pressed,
    /// No se pudo registrar (otra app ya usa esa combinación, o no se entiende).
    Failed(String),
}

pub struct GlobalHotkey {
    pub combo: String,
    pub rx: Receiver<HotkeyMsg>,
    #[cfg(windows)]
    thread_id: std::sync::Arc<std::sync::atomic::AtomicU32>,
}

/// Modificadores y tecla virtual de un atajo como "Ctrl+Alt+T".
pub fn parse(combo: &str) -> Option<(u32, u32)> {
    const MOD_ALT: u32 = 0x1;
    const MOD_CONTROL: u32 = 0x2;
    const MOD_SHIFT: u32 = 0x4;
    const MOD_WIN: u32 = 0x8;
    let mut mods = 0;
    let mut vk = None;
    for part in combo.split('+').map(str::trim) {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => mods |= MOD_CONTROL,
            "alt" => mods |= MOD_ALT,
            "shift" | "mayús" | "mayus" => mods |= MOD_SHIFT,
            "win" | "super" | "windows" => mods |= MOD_WIN,
            "space" | "espacio" => vk = Some(0x20),
            "`" | "backquote" | "º" | "|" => vk = Some(0xC0),
            "ñ" => vk = Some(0xC0),
            k if k.len() == 1 && k.chars().all(|c| c.is_ascii_alphanumeric()) => {
                vk = Some(k.to_ascii_uppercase().as_bytes()[0] as u32)
            }
            k if k.starts_with('f') && k[1..].parse::<u32>().is_ok_and(|n| (1..=24).contains(&n)) => {
                vk = Some(0x6F + k[1..].parse::<u32>().unwrap())
            }
            _ => return None,
        }
    }
    Some((mods, vk?))
}

impl GlobalHotkey {
    pub fn start(combo: &str, ctx: egui::Context) -> Self {
        let (tx, rx) = channel();
        #[cfg(windows)]
        {
            let thread_id = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
            match parse(combo) {
                Some((mods, vk)) => win::spawn(mods, vk, combo.to_string(), tx, ctx, thread_id.clone()),
                None => {
                    let _ = tx.send(HotkeyMsg::Failed(format!("No entiendo el atajo «{combo}»")));
                }
            }
            Self { combo: combo.to_string(), rx, thread_id }
        }
        #[cfg(not(windows))]
        {
            let _ = (tx, ctx);
            Self { combo: combo.to_string(), rx }
        }
    }

    pub fn stop(&self) {
        #[cfg(windows)]
        win::stop(&self.thread_id);
    }
}

impl Drop for GlobalHotkey {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(windows)]
mod win {
    use super::HotkeyMsg;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::mpsc::Sender;

    #[repr(C)]
    struct Msg {
        hwnd: isize,
        message: u32,
        wparam: usize,
        lparam: isize,
        time: u32,
        pt: [i32; 2],
        private: u32,
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn RegisterHotKey(hwnd: isize, id: i32, mods: u32, vk: u32) -> i32;
        fn UnregisterHotKey(hwnd: isize, id: i32) -> i32;
        fn GetMessageW(msg: *mut Msg, hwnd: isize, min: u32, max: u32) -> i32;
        fn PostThreadMessageW(thread: u32, msg: u32, wparam: usize, lparam: isize) -> i32;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThreadId() -> u32;
    }

    const WM_HOTKEY: u32 = 0x0312;
    const WM_QUIT: u32 = 0x0012;
    const MOD_NOREPEAT: u32 = 0x4000;

    pub fn spawn(
        mods: u32,
        vk: u32,
        combo: String,
        tx: Sender<HotkeyMsg>,
        ctx: egui::Context,
        thread_id: Arc<AtomicU32>,
    ) {
        std::thread::Builder::new()
            .name("neon-hotkey".into())
            .spawn(move || unsafe {
                thread_id.store(GetCurrentThreadId(), Ordering::SeqCst);
                if RegisterHotKey(0, 1, mods | MOD_NOREPEAT, vk) == 0 {
                    let _ = tx.send(HotkeyMsg::Failed(format!(
                        "El atajo {combo} ya lo usa otra aplicación"
                    )));
                    ctx.request_repaint();
                    return;
                }
                let mut msg = std::mem::zeroed::<Msg>();
                while GetMessageW(&mut msg, 0, 0, 0) > 0 {
                    if msg.message == WM_HOTKEY {
                        crate::window::toggle_main_window();
                        let _ = tx.send(HotkeyMsg::Pressed);
                        ctx.request_repaint();
                    }
                }
                UnregisterHotKey(0, 1);
            })
            .ok();
    }

    pub fn stop(thread_id: &AtomicU32) {
        let id = thread_id.load(Ordering::SeqCst);
        if id != 0 {
            unsafe { PostThreadMessageW(id, WM_QUIT, 0, 0) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn parses_combos() {
        assert_eq!(parse("Ctrl+Alt+T"), Some((0x3, b'T' as u32)));
        assert_eq!(parse("Win+Space"), Some((0x8, 0x20)));
        assert_eq!(parse("F12"), Some((0, 0x7B)));
        assert_eq!(parse("Ctrl+Shift+1"), Some((0x6, b'1' as u32)));
        assert_eq!(parse("Ctrl+Nada"), None);
        assert_eq!(parse("Ctrl+Alt"), None);
    }
}
