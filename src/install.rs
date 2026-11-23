//! Instalación en Windows (solo para el usuario actual, sin pedir administrador).
//!
//! `neon --instalar [--escritorio] [--sin-inicio] [--sin-menu] [--sin-explorador] [--silencioso]`:
//!   1. Copia el ejecutable a %LOCALAPPDATA%\Programs\neon\neon.exe.
//!   2. Lo registra en "App Paths": escribir `neon` en la barra de direcciones
//!      del Explorador o en Win+R lo abre en esa carpeta, igual que `cmd`.
//!   3. Agrega "Abrir en Neon" al menú contextual de carpetas.
//!   4. Crea el acceso directo del menú Inicio (y del escritorio con `--escritorio`).
//!   5. Aparece en Configuración → Aplicaciones, con su botón Desinstalar.
//!
//! Cada opción describe cómo debe quedar la integración: lo que no se pida se quita
//! (así sirve también para reparar).
//!
//! `neon --desinstalar [--silencioso] [--borrar-datos]` deshace todo y borra la carpeta del
//! programa (el historial y los ajustes en %LOCALAPPDATA%\neon se conservan, salvo `--borrar-datos`).
//!
//! El instalador gráfico (`installer/`) copia el exe y luego llama a
//! `neon --instalar --silencioso --escritorio`.

#[derive(Default, Clone, Copy)]
#[cfg_attr(not(windows), allow(dead_code))]
pub struct Options {
    pub silent: bool,
    /// Acceso directo en el escritorio.
    pub desktop: bool,
    /// No dejar el acceso directo del menú Inicio.
    pub no_start: bool,
    /// No agregar «Abrir en Neon» al menú contextual.
    pub no_menu: bool,
    /// No registrar `neon` para la barra de direcciones del Explorador y Win+R.
    pub no_explorer: bool,
    /// Al desinstalar, borrar también el historial y los ajustes.
    pub purge: bool,
}

#[cfg(windows)]
mod imp {
    use super::Options;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use winreg::HKCU;

    const APP_PATHS: &str = r"Software\Microsoft\Windows\CurrentVersion\App Paths\neon.exe";
    const UNINSTALL: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Neon";
    /// Identidad para las notificaciones (nombre e ícono propios).
    pub const AUMID: &str = "Neon.Terminal";
    const AUMID_KEY: &str = r"Software\Classes\AppUserModelId\Neon.Terminal";
    // Click derecho sobre una carpeta / sobre el fondo dentro de una carpeta.
    const MENU_KEYS: [&str; 2] = [
        r"Software\Classes\Directory\shell\neon",
        r"Software\Classes\Directory\Background\shell\neon",
    ];
    const MENU_LABEL: &str = "Abrir en Neon";
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const DETACHED_PROCESS: u32 = 0x0000_0008;

    pub fn install_dir() -> PathBuf {
        dirs::data_local_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("Programs")
            .join("neon")
    }

    fn start_menu_link() -> Option<PathBuf> {
        dirs::data_dir().map(|d| d.join(r"Microsoft\Windows\Start Menu\Programs\Neon.lnk"))
    }

    fn desktop_link() -> Option<PathBuf> {
        dirs::desktop_dir().map(|d| d.join("Neon.lnk"))
    }

    /// Crea un acceso directo .lnk usando PowerShell (WScript.Shell).
    fn create_shortcut(lnk: &Path, target: &Path) -> std::io::Result<()> {
        use std::os::windows::process::CommandExt;
        let q = |p: &Path| p.to_string_lossy().replace('\'', "''");
        let home = dirs::home_dir().unwrap_or_default();
        let script = format!(
            "$s=(New-Object -ComObject WScript.Shell).CreateShortcut('{}');\
             $s.TargetPath='{}';$s.WorkingDirectory='{}';$s.IconLocation='{},0';\
             $s.Description='Neon';$s.Save()",
            q(lnk),
            q(target),
            q(&home),
            q(target)
        );
        let status = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-EncodedCommand"])
            .arg(crate::shell::encode_pwsh_command(&script))
            .creation_flags(CREATE_NO_WINDOW)
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(std::io::Error::other("no se pudo crear el acceso directo"))
        }
    }

    pub fn install(opts: Options) -> std::io::Result<String> {
        // Limpia lo que haya dejado el nombre anterior del proyecto.
        for old in [
            r"Software\Microsoft\Windows\CurrentVersion\App Paths\warpito.exe",
            r"Software\Classes\Directory\shell\warpito",
            r"Software\Classes\Directory\Background\shell\warpito",
        ] {
            let _ = HKCU.delete_subkey_all(old);
        }

        let src = std::env::current_exe()?;
        let dir = install_dir();
        std::fs::create_dir_all(&dir)?;
        let exe = dir.join("neon.exe");
        if src != exe {
            // Si Neon está abierto, el exe está bloqueado: se puede renombrar,
            // pero no sobrescribir. Lo movemos aparte y copiamos el nuevo.
            let old = dir.join("neon.old.exe");
            let _ = std::fs::remove_file(&old);
            if exe.exists() {
                let _ = std::fs::rename(&exe, &old);
            }
            std::fs::copy(&src, &exe)?;
        }
        let exe_s = exe.to_string_lossy().into_owned();
        let dir_s = dir.to_string_lossy().into_owned();

        if opts.no_explorer {
            let _ = HKCU.delete_subkey_all(APP_PATHS);
        } else {
            let (k, _) = HKCU.create_subkey(APP_PATHS)?;
            k.set_value("", &exe_s)?;
            k.set_value("Path", &dir_s)?;
        }

        for key in MENU_KEYS {
            if opts.no_menu {
                let _ = HKCU.delete_subkey_all(key);
                continue;
            }
            let (k, _) = HKCU.create_subkey(key)?;
            k.set_value("", &MENU_LABEL)?;
            k.set_value("Icon", &format!("\"{exe_s}\",0"))?;
            let (cmd, _) = HKCU.create_subkey(format!(r"{key}\command"))?;
            // %V = la carpeta clickeada (o la carpeta abierta, si es el fondo).
            cmd.set_value("", &format!("\"{exe_s}\" \"%V\""))?;
        }

        // Notificaciones con nombre e ícono de Neon.
        let icon = dir.join("neon.png");
        let _ = std::fs::write(&icon, include_bytes!("../assets/icon-256.png"));
        let (a, _) = HKCU.create_subkey(AUMID_KEY)?;
        a.set_value("DisplayName", &"Neon")?;
        a.set_value("IconUri", &icon.to_string_lossy().into_owned())?;

        // Configuración → Aplicaciones.
        let (u, _) = HKCU.create_subkey(UNINSTALL)?;
        u.set_value("DisplayName", &"Neon")?;
        u.set_value("DisplayVersion", &env!("CARGO_PKG_VERSION"))?;
        u.set_value("Publisher", &"Neon")?;
        u.set_value("DisplayIcon", &format!("{exe_s},0"))?;
        u.set_value("InstallLocation", &dir_s)?;
        u.set_value("UninstallString", &format!("\"{exe_s}\" --desinstalar"))?;
        u.set_value(
            "QuietUninstallString",
            &format!("\"{exe_s}\" --desinstalar --silencioso"),
        )?;
        u.set_value("NoModify", &1u32)?;
        u.set_value("NoRepair", &1u32)?;
        let kb = std::fs::metadata(&exe)
            .map(|m| (m.len() / 1024) as u32)
            .unwrap_or(0);
        u.set_value("EstimatedSize", &kb)?;

        let mut warnings = vec![];
        if let Some(l) = start_menu_link() {
            if opts.no_start {
                let _ = std::fs::remove_file(&l);
            } else if create_shortcut(&l, &exe).is_err() {
                warnings.push("No se pudo crear el acceso directo del menú Inicio.");
            }
        }
        if let Some(l) = desktop_link() {
            if opts.desktop {
                if create_shortcut(&l, &exe).is_err() {
                    warnings.push("No se pudo crear el acceso directo del escritorio.");
                }
            } else {
                let _ = std::fs::remove_file(&l);
            }
        }

        let mut msg = format!("Neon quedó instalado en:\n{exe_s}\n");
        if !opts.no_start {
            msg.push_str("\n• Búscalo en el menú Inicio.");
        }
        if !opts.no_explorer {
            msg.push_str(
                "\n• Escribe \"neon\" en la barra de direcciones del Explorador (o en Win+R).",
            );
        }
        if !opts.no_menu {
            msg.push_str(&format!(
                "\n• Click derecho en una carpeta → \"{MENU_LABEL}\".\n   \
                 En Windows 11 está en \"Mostrar más opciones\" (o Shift + click derecho)."
            ));
        }
        for w in warnings {
            msg.push_str("\n\n");
            msg.push_str(w);
        }
        Ok(msg)
    }

    pub fn uninstall(opts: Options) -> std::io::Result<String> {
        let _ = HKCU.delete_subkey_all(APP_PATHS);
        let _ = HKCU.delete_subkey_all(UNINSTALL);
        let _ = HKCU.delete_subkey_all(AUMID_KEY);
        for key in MENU_KEYS {
            let _ = HKCU.delete_subkey_all(key);
        }
        for l in [start_menu_link(), desktop_link()].into_iter().flatten() {
            let _ = std::fs::remove_file(l);
        }
        // Borra la carpeta del programa un par de segundos después de cerrar
        // (un exe no puede borrarse a sí mismo mientras corre).
        let dir = install_dir();
        if dir.exists() {
            use std::os::windows::process::CommandExt;
            let _ = Command::new("cmd.exe")
                .raw_arg(format!(
                    "/C ping 127.0.0.1 -n 3 >NUL & rmdir /S /Q \"{}\"",
                    dir.display()
                ))
                .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
                .spawn();
        }
        if opts.purge {
            let _ = std::fs::remove_dir_all(crate::shell::data_dir());
            return Ok(
                "Neon se desinstaló.\n\nTambién se borraron tu historial y tus ajustes.".into(),
            );
        }
        Ok(
            "Neon se desinstaló.\n\nTu historial de comandos se conserva en %LOCALAPPDATA%\\neon, \
            por si vuelves a instalarlo."
                .to_string(),
        )
    }

    /// Si Neon está instalado, el identificador para sus notificaciones.
    pub fn notification_app_id() -> Option<&'static str> {
        HKCU.open_subkey(AUMID_KEY).ok().map(|_| AUMID)
    }

    /// Muestra un cuadro de mensaje de Windows (la app no tiene consola).
    pub fn message(text: &str, error: bool) {
        #[link(name = "user32")]
        unsafe extern "system" {
            fn MessageBoxW(
                hwnd: *mut core::ffi::c_void,
                text: *const u16,
                caption: *const u16,
                utype: u32,
            ) -> i32;
        }
        let w = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
        let (t, c) = (w(text), w("Neon"));
        const MB_ICONERROR: u32 = 0x10;
        const MB_ICONINFORMATION: u32 = 0x40;
        let icon = if error {
            MB_ICONERROR
        } else {
            MB_ICONINFORMATION
        };
        unsafe {
            MessageBoxW(core::ptr::null_mut(), t.as_ptr(), c.as_ptr(), icon);
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::Options;
    pub fn install(_: Options) -> std::io::Result<String> {
        Err(std::io::Error::other("--instalar solo funciona en Windows"))
    }
    pub fn uninstall(_: Options) -> std::io::Result<String> {
        Err(std::io::Error::other(
            "--desinstalar solo funciona en Windows",
        ))
    }
    pub fn message(text: &str, _error: bool) {
        eprintln!("{text}");
    }
}

#[cfg(windows)]
pub use imp::notification_app_id;

/// Maneja `--instalar` / `--desinstalar`. Devuelve el código de salida si
/// era uno de esos comandos (y entonces no hay que abrir la ventana).
pub fn handle_cli(args: &[String]) -> Option<i32> {
    let has = |f: &[&str]| args.iter().any(|a| f.contains(&a.as_str()));
    let opts = Options {
        silent: has(&["--silencioso", "--silent"]),
        desktop: has(&["--escritorio", "--desktop"]),
        no_start: has(&["--sin-inicio"]),
        no_menu: has(&["--sin-menu"]),
        no_explorer: has(&["--sin-explorador"]),
        purge: has(&["--borrar-datos"]),
    };
    let result = if has(&["--instalar", "--install"]) {
        imp::install(opts)
    } else if has(&["--desinstalar", "--uninstall"]) {
        imp::uninstall(opts)
    } else {
        return None;
    };
    match result {
        Ok(msg) => {
            if !opts.silent {
                imp::message(&msg, false);
            }
            Some(0)
        }
        Err(e) => {
            let msg = format!("No se pudo completar: {e}");
            if opts.silent {
                eprintln!("{msg}");
            } else {
                imp::message(&msg, true);
            }
            Some(1)
        }
    }
}

/// Carpeta en la que abrir la primera pestaña.
///
/// - `neon C:\ruta` o `neon .` → esa ruta.
/// - Sin argumento → la carpeta desde donde se lanzó (lo que hace el Explorador
///   al escribir `neon` en la barra de direcciones), salvo que sea la carpeta
///   personal, la del exe o la de sistema (menú Inicio): ahí devuelve None y
///   se restaura la sesión anterior.
pub fn start_dir(arg: Option<&str>) -> Option<String> {
    let cwd = std::env::current_dir().ok();
    if let Some(a) = arg {
        // El menú contextual en la raíz de un disco entrega `C:"` (comilla escapada).
        let mut a = a.trim_matches('"').to_string();
        if a.ends_with(':') {
            a.push('\\');
        }
        let p = std::path::PathBuf::from(a);
        let p = if p.is_relative() {
            cwd.as_ref()?.join(p)
        } else {
            p
        };
        let p = p.canonicalize().unwrap_or(p);
        return Some(clean(p));
    }
    let cwd = cwd?;
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|p| p.to_path_buf()));
    let windir = std::env::var_os("WINDIR").map(std::path::PathBuf::from);
    // Menú Inicio / barra de tareas (carpeta personal, del exe o de sistema):
    // no es "abrir en una carpeta", así que se restaura la sesión.
    let boring = Some(&cwd) == exe_dir.as_ref()
        || windir.is_some_and(|w| cwd.starts_with(w))
        || dirs::home_dir().is_some_and(|h| cwd == h)
        || cwd == std::path::Path::new("/");
    (!boring).then(|| clean(cwd))
}

/// En Windows `canonicalize` devuelve rutas `\\?\C:\...`; las dejamos normales.
fn clean(p: std::path::PathBuf) -> String {
    let s = p.to_string_lossy().into_owned();
    s.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_and_absolute_args() {
        let tmp = std::env::temp_dir().canonicalize().unwrap();
        assert_eq!(
            start_dir(Some(tmp.to_str().unwrap())).unwrap(),
            clean(tmp.clone())
        );
        let here = std::env::current_dir().unwrap().canonicalize().unwrap();
        assert_eq!(start_dir(Some(".")).unwrap(), clean(here));
        assert_eq!(clean(r"\\?\C:\Users\x".into()), r"C:\Users\x");
    }
}
