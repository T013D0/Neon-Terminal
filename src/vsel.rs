//! Selección "virtual" para apps en pantalla alterna que usan el mouse
//! (Claude Code en pantalla completa).
//!
//! Cuando la app hace scroll, redibuja y la terminal borra la selección normal.
//! Aquí la selección vive en coordenadas del *contenido*: se detecta cuánto se
//! desplazó la pantalla comparando líneas antes y después del scroll, y se
//! guarda el texto de cada línea que pasó por la vista. Así se puede seguir
//! seleccionando más abajo (o más arriba) sin perder lo anterior.

use std::collections::HashMap;
use std::time::{Duration, Instant};

type Row = Vec<char>;

pub struct VSel {
    anchor: (i64, usize),
    head: (i64, usize),
    /// fila virtual = fila en pantalla + offset
    offset: i64,
    rows: HashMap<i64, Row>,
    last: Vec<Row>,
    wheel_at: Option<Instant>,
    pub dragging: bool,
    /// ¿Hubo scroll durante la selección? (si no, basta la selección normal)
    pub moved: bool,
}

const MAX_SHIFT: i64 = 40;

impl VSel {
    pub fn new(screen: Vec<Row>, row: usize, col: usize) -> Self {
        let mut s = Self {
            anchor: (row as i64, col),
            head: (row as i64, col),
            offset: 0,
            rows: HashMap::new(),
            last: vec![],
            wheel_at: None,
            dragging: true,
            moved: false,
        };
        s.store(&screen);
        s.last = screen;
        s
    }

    fn store(&mut self, screen: &[Row]) {
        for (i, r) in screen.iter().enumerate() {
            self.rows.insert(i as i64 + self.offset, r.clone());
        }
    }

    /// Se movió la rueda (o se hizo autoscroll): lo próximo que cambie puede ser un scroll.
    pub fn note_wheel(&mut self) {
        self.wheel_at = Some(Instant::now());
    }

    /// Actualiza con la pantalla actual y la celda bajo el mouse (fila, columna).
    pub fn update(&mut self, screen: Vec<Row>, pointer: Option<(usize, usize)>) {
        let recent = self
            .wheel_at
            .is_some_and(|t| t.elapsed() < Duration::from_millis(500));
        // Tras la rueda basta poca evidencia; si no, la app se desplazó sola
        // (por ejemplo, Claude Code escribiendo) y se pide más coincidencia.
        let min = if recent { 2 } else { 4 };
        if self.last.len() == screen.len() && self.last != screen {
            if let Some(k) = detect_shift(&self.last, &screen, min) {
                self.offset += k;
                self.moved = true;
            }
        }
        self.store(&screen);
        self.last = screen;
        if self.dragging {
            if let Some((r, c)) = pointer {
                self.head = (r as i64 + self.offset, c);
            }
        }
    }

    fn ordered(&self) -> ((i64, usize), (i64, usize)) {
        if (self.anchor.0, self.anchor.1) <= (self.head.0, self.head.1) {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    /// Para pintar: columnas [c0, c1) seleccionadas en la fila de pantalla `screen_row`.
    pub fn cols_on_row(&self, screen_row: usize, cols: usize) -> Option<(usize, usize)> {
        let v = screen_row as i64 + self.offset;
        let ((r0, c0), (r1, c1)) = self.ordered();
        if v < r0 || v > r1 {
            return None;
        }
        let a = if v == r0 { c0 } else { 0 };
        let b = if v == r1 { (c1 + 1).min(cols) } else { cols };
        (b > a).then_some((a, b))
    }

    /// ¿Hay algo seleccionado (no fue un clic suelto)?
    pub fn has_selection(&self) -> bool {
        self.anchor != self.head
    }

    /// Doble clic: la palabra bajo el mouse. Triple: la línea entera.
    pub fn select_word(&mut self, line: bool) {
        let (v, c) = self.anchor;
        let Some(row) = self.rows.get(&v) else { return };
        if row.is_empty() {
            return;
        }
        let c = c.min(row.len() - 1);
        let (a, b) = if line {
            let end = row.iter().rposition(|ch| !ch.is_whitespace()).unwrap_or(0);
            (0, end)
        } else {
            let word = |ch: &char| !ch.is_whitespace() && !"()[]{}<>'\"`│|,;".contains(*ch);
            if !word(&row[c]) {
                return;
            }
            let mut a = c;
            while a > 0 && word(&row[a - 1]) {
                a -= 1;
            }
            let mut b = c;
            while b + 1 < row.len() && word(&row[b + 1]) {
                b += 1;
            }
            (a, b)
        };
        self.anchor = (v, a);
        self.head = (v, b);
        self.dragging = false;
    }

    /// Celdas a pintar: (fila en pantalla, col0, col1).
    pub fn highlight(&self, rows: usize, cols: usize) -> Vec<(i32, usize, usize)> {
        if !self.has_selection() {
            return vec![];
        }
        (0..rows)
            .filter_map(|r| self.cols_on_row(r, cols).map(|(a, b)| (r as i32, a, b)))
            .collect()
    }

    pub fn text(&self) -> String {
        let ((r0, c0), (r1, c1)) = self.ordered();
        let mut out = vec![];
        for v in r0..=r1 {
            let Some(row) = self.rows.get(&v) else {
                out.push(String::new());
                continue;
            };
            let a = if v == r0 { c0.min(row.len()) } else { 0 };
            let b = if v == r1 {
                (c1 + 1).min(row.len())
            } else {
                row.len()
            };
            let s: String = row.get(a..b.max(a)).unwrap_or(&[]).iter().collect();
            out.push(s.trim_end().to_string());
        }
        out.join("\n")
    }
}

fn blank(r: &Row) -> bool {
    r.iter().all(|c| c.is_whitespace())
}

/// ¿Cuántas líneas subió el contenido? (k > 0: se ve contenido posterior)
fn detect_shift(old: &[Row], new: &[Row], min: usize) -> Option<i64> {
    let n = new.len() as i64;
    let score = |k: i64| -> usize {
        (0..n)
            .filter(|&i| {
                let j = i + k;
                j >= 0 && j < n && !blank(&new[i as usize]) && old[j as usize] == new[i as usize]
            })
            .count()
    };
    let base = score(0);
    let (mut best, mut best_k) = (0, 0);
    for k in -MAX_SHIFT..=MAX_SHIFT {
        if k == 0 {
            continue;
        }
        let s = score(k);
        if s > best {
            best = s;
            best_k = k;
        }
    }
    (best >= min && best > base).then_some(best_k)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(lines: &[&str]) -> Vec<Row> {
        lines.iter().map(|l| l.chars().collect()).collect()
    }

    #[test]
    fn detects_scroll_and_keeps_text() {
        let a = screen(&["uno", "dos", "tres", "cuatro", "cinco"]);
        let mut v = VSel::new(a, 1, 0); // empieza en "dos"
        v.note_wheel();
        // el contenido sube 2 líneas
        let b = screen(&["tres", "cuatro", "cinco", "seis", "siete"]);
        v.update(b, Some((3, 2))); // mouse sobre "seis", col 2
        assert!(v.moved);
        assert_eq!(v.text(), "dos\ntres\ncuatro\ncinco\nsei");
    }

    #[test]
    fn detects_scroll_up() {
        let a = screen(&["tres", "cuatro", "cinco", "seis"]);
        let mut v = VSel::new(a, 3, 3);
        v.note_wheel();
        let b = screen(&["uno", "dos", "tres", "cuatro"]);
        v.update(b, Some((0, 0)));
        assert!(v.moved);
        assert_eq!(v.text(), "uno\ndos\ntres\ncuatro\ncinco\nseis");
    }

    #[test]
    fn no_wheel_no_shift() {
        let a = screen(&["a1", "b2", "c3"]);
        let mut v = VSel::new(a, 0, 0);
        v.update(screen(&["b2", "c3", "d4"]), Some((2, 1)));
        assert!(!v.moved);
    }

    #[test]
    fn double_click_word_and_line() {
        let mut v = VSel::new(screen(&["  hola mundo(x)  "]), 0, 8);
        v.select_word(false);
        assert_eq!(v.text(), "mundo");
        v.select_word(true);
        assert_eq!(v.text(), "  hola mundo(x)");
    }
}
