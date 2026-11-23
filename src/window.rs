//! Detalles nativos de Windows:
//! - Esquinas redondeadas, borde del color del tema, modo oscuro y sombra
//!   (la ventana no usa la barra de título del sistema, así que hay que
//!   pedírselo a DWM a mano). En Windows 10 las esquinas quedan rectas.
//! - Posición global del mouse y estado del botón, para arrastrar pestañas
//!   entre ventanas aunque el puntero salga de la ventana de origen.

/// En Windows el borde lo dibuja el sistema (redondeado); en otros sistemas lo dibuja la app.
pub const SYSTEM_DRAWS_BORDER: bool = cfg!(windows);

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

static DARK: AtomicBool = AtomicBool::new(true);
/// 0 = sin efecto, 2 = Mica, 3 = Acrylic (valores de DWMWA_SYSTEMBACKDROP_TYPE).
static BACKDROP: AtomicU32 = AtomicU32::new(0);

/// Modo oscuro/claro y efecto de fondo que se aplican a las ventanas.
pub fn set_style(dark: bool, backdrop: u32) {
    DARK.store(dark, Ordering::Relaxed);
    BACKDROP.store(backdrop, Ordering::Relaxed);
}

/// Aplica el estilo a todas las ventanas de Neon (las nuevas aparecen al
/// sacar una pestaña o con Ctrl+Shift+N).
pub fn style_all_windows() {
    #[cfg(windows)]
    unsafe {
        win::style_all()
    };
}

/// Posición del mouse en pantalla, en píxeles físicos.
pub fn global_cursor_px() -> Option<(f32, f32)> {
    #[cfg(windows)]
    {
        win::cursor()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Si la tecla V está apretada (None si el sistema no lo informa).
pub fn v_key_down() -> Option<bool> {
    #[cfg(windows)]
    {
        Some(win::v_down())
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Si el botón izquierdo está apretado (None si el sistema no lo informa).
pub fn left_button_down() -> Option<bool> {
    #[cfg(windows)]
    {
        Some(win::left_down())
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(windows)]
mod win {
    use core::ffi::c_void;

    #[repr(C)]
    struct Margins {
        left: i32,
        right: i32,
        top: i32,
        bottom: i32,
    }

    #[repr(C)]
    struct Point {
        x: i32,
        y: i32,
    }

    #[link(name = "dwmapi")]
    unsafe extern "system" {
        fn DwmSetWindowAttribute(hwnd: isize, attr: u32, value: *const c_void, size: u32) -> i32;
        fn DwmExtendFrameIntoClientArea(hwnd: isize, margins: *const Margins) -> i32;
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn EnumThreadWindows(
            thread: u32,
            callback: unsafe extern "system" fn(isize, isize) -> i32,
            lparam: isize,
        ) -> i32;
        fn IsWindowVisible(hwnd: isize) -> i32;
        fn GetCursorPos(p: *mut Point) -> i32;
        fn GetAsyncKeyState(vk: i32) -> i16;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThreadId() -> u32;
    }

    const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
    const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
    const DWMWA_BORDER_COLOR: u32 = 34;
    const DWMWA_SYSTEMBACKDROP_TYPE: u32 = 38;
    /// DWMSBT_NONE
    const BACKDROP_NONE: u32 = 1;
    const DWMWCP_ROUND: u32 = 2;
    const VK_LBUTTON: i32 = 0x01;

    unsafe fn style(hwnd: isize) {
        use std::sync::atomic::Ordering;
        let set = |attr: u32, value: u32| unsafe {
            DwmSetWindowAttribute(hwnd, attr, &value as *const u32 as *const c_void, 4);
        };
        let dark = super::DARK.load(Ordering::Relaxed);
        let backdrop = super::BACKDROP.load(Ordering::Relaxed);
        set(DWMWA_USE_IMMERSIVE_DARK_MODE, dark as u32);
        set(DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND);
        // COLORREF = 0x00BBGGRR
        let b = crate::theme::pal().border;
        set(
            DWMWA_BORDER_COLOR,
            (b.b() as u32) << 16 | (b.g() as u32) << 8 | b.r() as u32,
        );
        // Mica (2) / Acrylic (3). En Windows 10 este atributo no existe y se ignora.
        set(
            DWMWA_SYSTEMBACKDROP_TYPE,
            if backdrop == 0 {
                BACKDROP_NONE
            } else {
                backdrop
            },
        );
        // Con efecto de fondo, el marco se extiende a toda la ventana para que se vea
        // detrás de las partes transparentes. Sin efecto, 1px alcanza para la sombra.
        let m = if backdrop == 0 {
            Margins {
                left: 1,
                right: 1,
                top: 1,
                bottom: 1,
            }
        } else {
            Margins {
                left: -1,
                right: -1,
                top: -1,
                bottom: -1,
            }
        };
        unsafe { DwmExtendFrameIntoClientArea(hwnd, &m) };
    }

    unsafe extern "system" fn each(hwnd: isize, _: isize) -> i32 {
        unsafe {
            if IsWindowVisible(hwnd) != 0 {
                style(hwnd);
            }
        }
        1
    }

    /// Todas las ventanas de Neon viven en el hilo principal.
    pub unsafe fn style_all() {
        unsafe { EnumThreadWindows(GetCurrentThreadId(), each, 0) };
    }

    pub fn cursor() -> Option<(f32, f32)> {
        let mut p = Point { x: 0, y: 0 };
        (unsafe { GetCursorPos(&mut p) } != 0).then_some((p.x as f32, p.y as f32))
    }

    pub fn v_down() -> bool {
        (unsafe { GetAsyncKeyState(0x56) } as u16 & 0x8000) != 0
    }

    pub fn left_down() -> bool {
        (unsafe { GetAsyncKeyState(VK_LBUTTON) } as u16 & 0x8000) != 0
    }
}
