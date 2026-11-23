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
        if recent && self.last.len() == screen.len() && self.last != screen {
            if let Some(k) = detect_shift(&self.last, &screen) {
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
fn detect_shift(old: &[Row], new: &[Row]) -> Option<i64> {
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
    (best >= 2 && best > base).then_some(best_k)
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
}
