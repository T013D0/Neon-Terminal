//! Pegado seguro: antes de pegar, mira si el texto podría hacer daño o
//! esconder algo (lo típico de copiar un comando de una página web).

/// Motivos para pedir confirmación (vacío = se pega directo).
pub fn reasons(text: &str, runs_lines: bool) -> Vec<String> {
    let mut out = vec![];
    if has_hidden(text) {
        out.push("Tiene caracteres invisibles o que cambian la dirección del texto: lo que ves puede no ser lo que se ejecuta.".into());
    }
    let lower = text.to_lowercase();
    let squeezed: String = lower.split_whitespace().collect::<Vec<_>>().join(" ");
    for (pat, why) in DANGER {
        if squeezed.contains(pat) {
            out.push(format!("Contiene «{}»: {why}", pat.trim()));
        }
    }
    // Descargar y ejecutar en un solo paso.
    let downloads = ["curl ", "wget ", "iwr ", "irm ", "invoke-webrequest", "invoke-restmethod"];
    let executes = ["| sh", "|sh", "| bash", "|bash", "| iex", "|iex", "invoke-expression", "| pwsh", "| powershell"];
    if downloads.iter().any(|d| squeezed.contains(d)) && executes.iter().any(|e| squeezed.contains(e)) {
        out.push("Descarga algo de internet y lo ejecuta directamente.".into());
    }
    let lines = text.lines().filter(|l| !l.trim().is_empty()).count();
    if runs_lines && lines > 1 {
        out.push(format!(
            "Son {lines} líneas y esta shell las ejecutaría una tras otra al pegarlas."
        ));
    }
    out.dedup();
    out
}

const DANGER: &[(&str, &str)] = &[
    ("rm -rf", "borra carpetas completas sin preguntar"),
    ("rm -fr", "borra carpetas completas sin preguntar"),
    ("sudo ", "se ejecuta como administrador"),
    ("remove-item -recurse", "borra carpetas completas"),
    ("rd /s", "borra carpetas completas"),
    ("rmdir /s", "borra carpetas completas"),
    ("del /s", "borra archivos en todas las subcarpetas"),
    ("format ", "puede formatear un disco"),
    ("mkfs", "formatea un disco"),
    ("dd if=", "escribe directo sobre un disco"),
    (":(){", "es una bomba de procesos"),
    ("git push --force", "reescribe la historia remota"),
    ("git push -f", "reescribe la historia remota"),
    ("git reset --hard", "descarta cambios sin commit"),
    ("chmod -r 777", "abre permisos de todo"),
    ("set-executionpolicy", "cambia qué scripts puede ejecutar Windows"),
    ("reg delete", "borra claves del registro"),
];

/// Caracteres que no se ven: de ancho cero, de control y los que invierten la
/// dirección del texto (el truco de "Trojan Source").
pub fn has_hidden(text: &str) -> bool {
    text.chars().any(is_hidden)
}

fn is_hidden(c: char) -> bool {
    matches!(c,
        '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2064}'
        | '\u{2066}'..='\u{2069}' | '\u{FEFF}' | '\u{00AD}')
        || (c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
}

/// El mismo texto sin los caracteres invisibles.
pub fn clean(text: &str) -> String {
    text.chars().filter(|c| !is_hidden(*c)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_is_fine() {
        assert!(reasons("git status", true).is_empty());
        assert!(reasons("línea uno\nlínea dos", false).is_empty());
    }

    #[test]
    fn flags_risky_things() {
        assert_eq!(reasons("sudo  rm   -rf /", false).len(), 2);
        assert_eq!(reasons("curl -fsSL https://x.sh | sh", false).len(), 1);
        assert_eq!(reasons("irm https://get.x | iex", false).len(), 1);
        assert_eq!(reasons("echo a\necho b", true).len(), 1);
        let hidden = "ls\u{202E}fdp.exe";
        assert_eq!(reasons(hidden, false).len(), 1);
        assert_eq!(clean(hidden), "lsfdp.exe");
        assert!(has_hidden("a\u{1b}[31mb"));
    }
}
