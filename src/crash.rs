//! Reporte de errores: si Neon se cae, deja lo que pasó en `crash.log` (en la
//! carpeta de datos). Al abrir de nuevo ofrece copiarlo o reportarlo. Nada se
//! envía solo a internet.

use std::path::PathBuf;

fn log_path() -> PathBuf {
    crate::shell::data_dir().join("crash.log")
}

/// Instala el gancho que escribe el reporte cuando algo entra en pánico
/// (en cualquier hilo).
pub fn install() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let msg = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "(sin mensaje)".into());
        let place = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_default();
        let thread = std::thread::current().name().unwrap_or("sin nombre").to_string();
        let report = format!(
            "Neon {}\nSistema: {} {}\nFecha: {}\nHilo: {thread}\nDónde: {place}\nError: {msg}\n\n{}\n",
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS,
            std::env::consts::ARCH,
            now(),
            std::backtrace::Backtrace::force_capture()
        );
        let path = log_path();
        if let Some(d) = path.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        // Si hay varios pánicos seguidos se agregan, no se pisan.
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            let _ = f.write_all(report.as_bytes());
        }
        previous(info);
    }));
}

/// Reporte que quedó de la vez anterior (si Neon se cayó). Lo mueve a
/// `crash-anterior.log` para no avisar dos veces.
pub fn take_pending() -> Option<String> {
    let path = log_path();
    let text = std::fs::read_to_string(&path).ok()?;
    let _ = std::fs::rename(&path, path.with_file_name("crash-anterior.log"));
    // Recortado: el aviso no necesita megas de texto.
    let text = if text.len() > 20_000 { text[..20_000].to_string() } else { text };
    (!text.trim().is_empty()).then_some(text)
}

pub fn folder() -> PathBuf {
    crate::shell::data_dir()
}

/// Página para abrir un issue con título ya puesto (el detalle va al portapapeles).
pub fn issue_url(report: &str) -> String {
    let first = report
        .lines()
        .find_map(|l| l.strip_prefix("Error: "))
        .unwrap_or("Neon se cerró inesperadamente");
    let title: String = format!("Error: {first}").chars().take(90).collect();
    let enc: String = title
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect();
    format!(
        "https://github.com/{}/issues/new?title={enc}",
        crate::update::DEFAULT_REPO
    )
}

fn now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Fecha UTC simple (sin dependencias).
    let days = secs / 86_400;
    let (h, m, s) = (secs % 86_400 / 3600, secs % 3600 / 60, secs % 60);
    let (y, mo, d) = civil(days as i64);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{m:02}:{s:02} UTC")
}

/// Días desde 1970 → (año, mes, día). Algoritmo de Howard Hinnant.
fn civil(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_and_urls() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(20_419), (2025, 11, 27));
        let u = issue_url("Neon 1.0.0\nError: índice fuera de rango\n");
        assert!(u.starts_with("https://github.com/T013D0/Neon-Terminal/issues/new?title=Error%3A%20"));
        assert!(!u.contains(' '));
    }
}
