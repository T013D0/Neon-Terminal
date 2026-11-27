//! Selección rápida: etiquetas sobre lo que suele copiarse de la pantalla
//! (URLs, rutas, hashes de git, IPs, UUIDs) para copiarlo con una o dos teclas.

/// Algo copiable en pantalla.
#[derive(Clone, Debug, PartialEq)]
pub struct Hint {
    pub row: usize,
    /// Columnas [c0, c1).
    pub c0: usize,
    pub c1: usize,
    pub text: String,
    pub label: String,
}

pub struct QuickSelect {
    pub hints: Vec<Hint>,
    /// Lo que se ha tecleado de la etiqueta.
    pub typed: String,
}

/// Letras de las etiquetas: primero la fila central del teclado.
const KEYS: &str = "asdfjklghqwertyuiopzxcvbnm";

impl QuickSelect {
    pub fn new(rows: &[Vec<char>]) -> Option<Self> {
        let mut hints = find(rows);
        if hints.is_empty() {
            return None;
        }
        // Lo más cercano al final de la pantalla (lo último impreso) lleva las
        // etiquetas más cortas.
        hints.sort_by_key(|h| (std::cmp::Reverse(h.row), h.c0));
        let labels = labels(hints.len());
        for (h, l) in hints.iter_mut().zip(labels) {
            h.label = l;
        }
        Some(Self {
            hints,
            typed: String::new(),
        })
    }

    /// Agrega una tecla. Devuelve el texto elegido si la etiqueta quedó completa.
    pub fn key(&mut self, ch: char) -> Option<String> {
        let ch = ch.to_ascii_lowercase();
        let next = format!("{}{ch}", self.typed);
        if !self.hints.iter().any(|h| h.label.starts_with(&next)) {
            return None;
        }
        self.typed = next;
        self.hints
            .iter()
            .find(|h| h.label == self.typed)
            .map(|h| h.text.clone())
    }
}

/// Etiquetas sin prefijos compartidos: una letra si alcanzan, si no dos.
fn labels(n: usize) -> Vec<String> {
    let keys: Vec<char> = KEYS.chars().collect();
    if n <= keys.len() {
        return keys.iter().take(n).map(|c| c.to_string()).collect();
    }
    let mut out = vec![];
    'outer: for a in &keys {
        for b in &keys {
            if out.len() == n {
                break 'outer;
            }
            out.push(format!("{a}{b}"));
        }
    }
    out
}

/// Busca lo copiable fila por fila.
pub fn find(rows: &[Vec<char>]) -> Vec<Hint> {
    let mut out = vec![];
    for (r, row) in rows.iter().enumerate() {
        let mut c = 0;
        while c < row.len() {
            if is_sep(row[c]) {
                c += 1;
                continue;
            }
            let start = c;
            while c < row.len() && !is_sep(row[c]) {
                c += 1;
            }
            let token: String = row[start..c].iter().collect();
            // Quita puntuación pegada al final (fin de frase, dos puntos…).
            let trimmed = token.trim_end_matches(['.', ',', ';', ':', '!', '?', ')', ']', '}']);
            let lead = trimmed.len() - trimmed.trim_start_matches(['(', '[', '{']).len();
            let text = &trimmed[lead..];
            if text.is_empty() || !copyable(text) {
                continue;
            }
            let c0 = start + token[..lead].chars().count();
            out.push(Hint {
                row: r,
                c0,
                c1: c0 + text.chars().count(),
                text: text.to_string(),
                label: String::new(),
            });
        }
    }
    out
}

fn is_sep(c: char) -> bool {
    c.is_whitespace() || matches!(c, '"' | '\'' | '`' | '<' | '>' | '│' | '|' | '─' | '╭' | '╰' | '╮' | '╯')
}

fn copyable(t: &str) -> bool {
    is_url(t) || is_hash(t) || is_ip(t) || is_uuid(t) || is_path(t)
}

fn is_url(t: &str) -> bool {
    ["https://", "http://", "file://", "ssh://", "git@", "ftp://"]
        .iter()
        .any(|p| t.starts_with(p) && t.len() > p.len() + 2)
}

/// Hash de git (7 a 40 caracteres hex, con letras y números).
fn is_hash(t: &str) -> bool {
    (7..=64).contains(&t.len())
        && t.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        && t.chars().any(|c| c.is_ascii_digit())
        && t.chars().any(|c| c.is_ascii_alphabetic())
}

fn is_ip(t: &str) -> bool {
    let host = t.rsplit_once(':').map_or(t, |(h, port)| {
        if port.chars().all(|c| c.is_ascii_digit()) && !port.is_empty() {
            h
        } else {
            t
        }
    });
    let parts: Vec<&str> = host.split('.').collect();
    parts.len() == 4
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.len() <= 3 && p.parse::<u16>().is_ok_and(|n| n <= 255))
}

fn is_uuid(t: &str) -> bool {
    let parts: Vec<&str> = t.split('-').collect();
    parts.len() == 5
        && parts.iter().map(|p| p.len()).eq([8, 4, 4, 4, 12])
        && parts.iter().all(|p| p.chars().all(|c| c.is_ascii_hexdigit()))
}

/// Rutas: con separador (`src/app.rs`, `C:\x`, `./a`) o un archivo con
/// extensión (`Cargo.toml`, `app.rs:120`).
fn is_path(t: &str) -> bool {
    if t.len() < 3 || t.starts_with("--") || t.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return false;
    }
    let base = t.trim_end_matches(|c: char| c.is_ascii_digit() || c == ':');
    let has_sep = base.contains('/') || base.contains('\\');
    let ext = base.rsplit_once('.').is_some_and(|(name, ext)| {
        name.chars().any(|c| c.is_alphabetic())
            && (1..=8).contains(&ext.len())
            && ext.chars().all(|c| c.is_ascii_alphanumeric())
            && ext.chars().any(|c| c.is_ascii_alphabetic())
    });
    let sane = base
        .chars()
        .all(|c| c.is_alphanumeric() || "/\\._-~:@+".contains(c));
    sane && (ext || (has_sep && base.chars().filter(|c| c.is_alphanumeric()).count() >= 2))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(lines: &[&str]) -> Vec<Vec<char>> {
        lines.iter().map(|l| l.chars().collect()).collect()
    }

    fn texts(lines: &[&str]) -> Vec<String> {
        find(&rows(lines)).into_iter().map(|h| h.text).collect()
    }

    #[test]
    fn finds_common_things() {
        let t = texts(&[
            "commit 9fb8cc5a1 (HEAD -> main) see https://github.com/x/y.",
            "  modified:   src/app.rs:120",
            "error at C:\\dev\\Neon\\src\\ui.rs, ping 192.168.1.10:8080",
            "id 123e4567-e89b-12d3-a456-426614174000 done in 3.5s",
        ]);
        assert_eq!(
            t,
            [
                "9fb8cc5a1",
                "https://github.com/x/y",
                "src/app.rs:120",
                "C:\\dev\\Neon\\src\\ui.rs",
                "192.168.1.10:8080",
                "123e4567-e89b-12d3-a456-426614174000",
            ]
        );
    }

    #[test]
    fn ignores_plain_words_and_numbers() {
        assert!(texts(&["hola mundo 1234 3.14 v0.2.7 -- --flag deadbeef"]).is_empty());
    }

    #[test]
    fn columns_and_labels() {
        let mut q = QuickSelect::new(&rows(&["ver (src/a.rs) y", "abc1234"])).unwrap();
        // El de más abajo lleva la primera etiqueta.
        assert_eq!(q.hints[0].text, "abc1234");
        assert_eq!(q.hints[0].label, "a");
        let h = &q.hints[1];
        assert_eq!((h.row, h.c0, h.c1), (0, 5, 13));
        assert_eq!(q.key('x'), None);
        assert_eq!(q.key('S').as_deref(), Some("src/a.rs"));
    }

    #[test]
    fn two_letter_labels() {
        let l = labels(30);
        assert_eq!(l.len(), 30);
        assert_eq!(l[0], "aa");
        let set: std::collections::HashSet<_> = l.iter().collect();
        assert_eq!(set.len(), 30);
    }
}
