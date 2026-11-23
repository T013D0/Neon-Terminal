//! Pestañas (workspaces) y operaciones sobre su árbol de paneles:
//! dividir, cerrar, mover subárboles entre pestañas/ventanas, navegar y
//! redimensionar con el teclado.

use crate::pane::Pane;
use egui::{Rect, pos2};
use egui_tiles::{Container, LinearDir, Tile, TileId, Tiles, Tree};

/// Colores que se le pueden asignar a una pestaña (índice guardado en la sesión).
pub fn tab_colors() -> [egui::Color32; 6] {
    let p = crate::theme::pal();
    [p.red, p.orange, p.yellow, p.green, p.cyan, p.violet]
}

pub struct Workspace {
    pub tree: Tree<Pane>,
    pub focused: u64,
    /// Nombre puesto a mano (si no, se usa carpeta · rama).
    pub name: Option<String>,
    pub color: Option<u8>,
    /// Panel maximizado dentro de la pestaña (Ctrl+Shift+Enter).
    pub zoomed: Option<u64>,
}

/// Un árbol de paneles "suelto", para moverlo de una pestaña o ventana a otra.
pub enum Node {
    Pane(Pane),
    Split {
        dir: LinearDir,
        children: Vec<(f32, Node)>,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dir {
    Left,
    Right,
    Up,
    Down,
}

impl Dir {
    pub fn axis(self) -> LinearDir {
        match self {
            Dir::Left | Dir::Right => LinearDir::Horizontal,
            Dir::Up | Dir::Down => LinearDir::Vertical,
        }
    }
    /// Hacia el final del contenedor (derecha/abajo).
    pub fn forward(self) -> bool {
        matches!(self, Dir::Right | Dir::Down)
    }
}

impl Workspace {
    pub fn new(pane: Pane) -> Self {
        let focused = pane.id;
        Self::from_node(Node::Pane(pane), focused)
    }

    pub fn from_node(node: Node, focused: u64) -> Self {
        let mut tiles = Tiles::default();
        let root = insert_node(&mut tiles, node);
        let tree = Tree::new(egui::Id::new(("workspace", focused, unique())), root, tiles);
        Self {
            tree,
            focused,
            name: None,
            color: None,
            zoomed: None,
        }
    }

    pub fn pane_ids(&self) -> Vec<u64> {
        // En orden de recorrido (izquierda→derecha, arriba→abajo).
        let mut out = vec![];
        if let Some(root) = self.tree.root() {
            collect_panes(&self.tree.tiles, root, &mut out);
        }
        out
    }

    pub fn pane(&self, id: u64) -> Option<&Pane> {
        self.tree.tiles.tiles().find_map(|t| match t {
            Tile::Pane(p) if p.id == id => Some(p),
            _ => None,
        })
    }

    pub fn pane_mut(&mut self, id: u64) -> Option<&mut Pane> {
        self.tree.tiles.tiles_mut().find_map(|t| match t {
            Tile::Pane(p) if p.id == id => Some(p),
            _ => None,
        })
    }

    pub fn focused_pane(&self) -> Option<&Pane> {
        self.pane(self.focused)
    }

    pub fn focused_pane_mut(&mut self) -> Option<&mut Pane> {
        let f = self.focused;
        self.pane_mut(f)
    }

    pub fn panes(&self) -> impl Iterator<Item = &Pane> {
        self.tree.tiles.tiles().filter_map(|t| match t {
            Tile::Pane(p) => Some(p),
            _ => None,
        })
    }

    pub fn panes_mut(&mut self) -> impl Iterator<Item = &mut Pane> {
        self.tree.tiles.tiles_mut().filter_map(|t| match t {
            Tile::Pane(p) => Some(p),
            _ => None,
        })
    }

    /// Título de la pestaña: nombre manual o carpeta · rama del panel activo.
    pub fn title(&self) -> String {
        if let Some(n) = &self.name {
            return n.clone();
        }
        self.focused_pane()
            .map(|p| p.display_title())
            .unwrap_or_else(|| "terminal".into())
    }

    /// Pone `node` al lado del panel `target` (antes o después) en la dirección dada.
    pub fn insert_next_to(
        &mut self,
        target: u64,
        node: Node,
        dir: LinearDir,
        before: bool,
    ) -> bool {
        let Some(tile_id) = find_tile(&self.tree.tiles, target) else {
            return false;
        };
        let Some(old) = self.tree.tiles.remove(tile_id) else {
            return false;
        };
        let old_id = self.tree.tiles.insert_new(old);
        let new_id = insert_node(&mut self.tree.tiles, node);
        let children = if before {
            vec![new_id, old_id]
        } else {
            vec![old_id, new_id]
        };
        self.tree.tiles.insert(
            tile_id,
            Tile::Container(Container::new_linear(dir, children)),
        );
        self.zoomed = None;
        true
    }

    /// Pone `node` al lado de toda la pestaña (no de un panel): mitad y mitad.
    pub fn insert_at_root(&mut self, node: Node, dir: LinearDir, before: bool) -> bool {
        let Some(root) = self.tree.root() else {
            return false;
        };
        let new_id = insert_node(&mut self.tree.tiles, node);
        let children = if before {
            vec![new_id, root]
        } else {
            vec![root, new_id]
        };
        let c = self
            .tree
            .tiles
            .insert_new(Tile::Container(Container::new_linear(dir, children)));
        self.tree.root = Some(c);
        self.zoomed = None;
        true
    }

    /// Saca un panel del árbol (para cerrarlo o moverlo). Devuelve el panel.
    pub fn take_pane(&mut self, id: u64) -> Option<Pane> {
        let tid = find_tile(&self.tree.tiles, id)?;
        let parent = self.tree.tiles.parent_of(tid);
        let pane = match self.tree.tiles.remove(tid)? {
            Tile::Pane(p) => p,
            other => {
                self.tree.tiles.insert(tid, other);
                return None;
            }
        };
        if let Some(parent) = parent {
            if let Some(Tile::Container(c)) = self.tree.tiles.get_mut(parent) {
                c.remove_child(tid);
            }
        }
        if self.zoomed == Some(id) {
            self.zoomed = None;
        }
        // Limpia contenedores vacíos o de un solo hijo.
        self.tree.simplify(&egui_tiles::SimplificationOptions {
            all_panes_must_have_tabs: false,
            ..Default::default()
        });
        if self.tree.tiles.tiles().all(|t| !matches!(t, Tile::Pane(_))) {
            self.tree.root = None;
        }
        if self.focused == id {
            if let Some(last) = self.pane_ids().last() {
                self.focused = *last;
            }
        }
        Some(pane)
    }

    pub fn is_empty(&self) -> bool {
        self.pane_ids().is_empty()
    }

    /// Saca todo el árbol de la pestaña como un `Node`.
    pub fn into_node(mut self) -> Option<Node> {
        let root = self.tree.root()?;
        take_node(&mut self.tree.tiles, root)
    }

    /// Ajusta el tamaño del panel enfocado con el teclado.
    pub fn resize_focused(&mut self, dir: Dir) {
        let Some(mut child) = find_tile(&self.tree.tiles, self.focused) else {
            return;
        };
        while let Some(parent) = self.tree.tiles.parent_of(child) {
            if let Some(Tile::Container(Container::Linear(lin))) = self.tree.tiles.get_mut(parent) {
                if lin.dir == dir.axis() && lin.children.len() > 1 {
                    let idx = lin.children.iter().position(|c| *c == child).unwrap_or(0);
                    let sib = if idx + 1 < lin.children.len() {
                        idx + 1
                    } else {
                        idx - 1
                    };
                    let (a, b) = (lin.children[idx], lin.children[sib]);
                    let total = lin.shares[a] + lin.shares[b];
                    // → / ↓ agrandan el panel si su vecino está a la derecha/abajo.
                    let grow = dir.forward() == (sib > idx);
                    let d = total * 0.06 * if grow { 1.0 } else { -1.0 };
                    let min = total * 0.12;
                    let na = (lin.shares[a] + d).clamp(min, total - min);
                    lin.shares[a] = na;
                    lin.shares[b] = total - na;
                    return;
                }
            }
            child = parent;
        }
    }
}

/// Elige el panel vecino en una dirección, según dónde se dibujó cada uno.
pub fn neighbor(rects: &[(u64, Rect)], from: u64, dir: Dir) -> Option<u64> {
    let (_, r) = rects.iter().find(|(id, _)| *id == from)?;
    let c = r.center();
    rects
        .iter()
        .filter(|(id, _)| *id != from)
        .filter(|(_, o)| match dir {
            Dir::Left => o.max.x <= r.min.x + 1.0,
            Dir::Right => o.min.x >= r.max.x - 1.0,
            Dir::Up => o.max.y <= r.min.y + 1.0,
            Dir::Down => o.min.y >= r.max.y - 1.0,
        })
        .min_by(|(_, a), (_, b)| {
            let da = a.center().distance(c);
            let db = b.center().distance(c);
            da.total_cmp(&db)
        })
        .map(|(id, _)| *id)
}

/// Zona de un panel donde se suelta una pestaña arrastrada.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DropZone {
    Left,
    Right,
    Top,
    Bottom,
}

impl DropZone {
    /// El lado más cercano al puntero.
    pub fn at(rect: Rect, p: egui::Pos2) -> Self {
        let fx = (p.x - rect.min.x) / rect.width().max(1.0);
        let fy = (p.y - rect.min.y) / rect.height().max(1.0);
        let d = [
            (fx, DropZone::Left),
            (1.0 - fx, DropZone::Right),
            (fy, DropZone::Top),
            (1.0 - fy, DropZone::Bottom),
        ];
        d.into_iter().min_by(|a, b| a.0.total_cmp(&b.0)).unwrap().1
    }

    /// Mitad del panel que ocuparía lo que se suelta (para dibujarlo).
    pub fn preview(self, r: Rect) -> Rect {
        let c = r.center();
        match self {
            DropZone::Left => Rect::from_min_max(r.min, pos2(c.x, r.max.y)),
            DropZone::Right => Rect::from_min_max(pos2(c.x, r.min.y), r.max),
            DropZone::Top => Rect::from_min_max(r.min, pos2(r.max.x, c.y)),
            DropZone::Bottom => Rect::from_min_max(pos2(r.min.x, c.y), r.max),
        }
    }

    pub fn split(self) -> (LinearDir, bool) {
        match self {
            DropZone::Left => (LinearDir::Horizontal, true),
            DropZone::Right => (LinearDir::Horizontal, false),
            DropZone::Top => (LinearDir::Vertical, true),
            DropZone::Bottom => (LinearDir::Vertical, false),
        }
    }
}

pub fn find_tile(tiles: &Tiles<Pane>, pane_id: u64) -> Option<TileId> {
    tiles.iter().find_map(|(tid, t)| match t {
        Tile::Pane(p) if p.id == pane_id => Some(*tid),
        _ => None,
    })
}

fn collect_panes(tiles: &Tiles<Pane>, id: TileId, out: &mut Vec<u64>) {
    match tiles.get(id) {
        Some(Tile::Pane(p)) => out.push(p.id),
        Some(Tile::Container(c)) => {
            for ch in c.children() {
                collect_panes(tiles, *ch, out);
            }
        }
        None => {}
    }
}

/// Saca un subárbol completo de `tiles` (los tiles quedan eliminados).
pub fn take_node(tiles: &mut Tiles<Pane>, id: TileId) -> Option<Node> {
    match tiles.remove(id)? {
        Tile::Pane(p) => Some(Node::Pane(p)),
        Tile::Container(c) => {
            let (dir, kids, shares): (LinearDir, Vec<TileId>, Vec<f32>) = match &c {
                Container::Linear(l) => (
                    l.dir,
                    l.children.clone(),
                    l.children.iter().map(|ch| l.shares[*ch]).collect(),
                ),
                other => {
                    let k = other.children_vec();
                    let n = k.len();
                    (LinearDir::Horizontal, k, vec![1.0; n])
                }
            };
            let children: Vec<(f32, Node)> = kids
                .into_iter()
                .zip(shares)
                .filter_map(|(ch, s)| take_node(tiles, ch).map(|n| (s, n)))
                .collect();
            match children.len() {
                0 => None,
                1 => children.into_iter().next().map(|(_, n)| n),
                _ => Some(Node::Split { dir, children }),
            }
        }
    }
}

/// Inserta un subárbol y devuelve el id de su raíz.
pub fn insert_node(tiles: &mut Tiles<Pane>, node: Node) -> TileId {
    match node {
        Node::Pane(p) => tiles.insert_pane(p),
        Node::Split { dir, children } => {
            let mut ids = vec![];
            let mut shares = vec![];
            for (s, n) in children {
                ids.push(insert_node(tiles, n));
                shares.push(s);
            }
            let mut lin = egui_tiles::Linear::new(dir, ids.clone());
            for (id, s) in ids.iter().zip(shares) {
                lin.shares.set_share(*id, s);
            }
            tiles.insert_new(Tile::Container(Container::Linear(lin)))
        }
    }
}

/// Primer panel de un nodo (para enfocarlo después de moverlo).
pub fn first_pane_id(node: &Node) -> Option<u64> {
    match node {
        Node::Pane(p) => Some(p.id),
        Node::Split { children, .. } => children.first().and_then(|(_, n)| first_pane_id(n)),
    }
}

fn unique() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(1);
    N.fetch_add(1, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::vec2;

    #[test]
    fn neighbor_picks_the_closest_pane_in_direction() {
        let a = Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 100.0));
        let b = Rect::from_min_size(pos2(100.0, 0.0), vec2(100.0, 50.0));
        let c = Rect::from_min_size(pos2(100.0, 50.0), vec2(100.0, 50.0));
        let rects = [(1, a), (2, b), (3, c)];
        assert_eq!(neighbor(&rects, 1, Dir::Right), Some(2).or(Some(3)));
        assert_eq!(neighbor(&rects, 2, Dir::Down), Some(3));
        assert_eq!(neighbor(&rects, 3, Dir::Up), Some(2));
        assert_eq!(neighbor(&rects, 2, Dir::Left), Some(1));
        assert_eq!(neighbor(&rects, 1, Dir::Left), None);
    }

    #[test]
    fn drop_zone_uses_nearest_edge() {
        let r = Rect::from_min_size(pos2(0.0, 0.0), vec2(200.0, 100.0));
        assert_eq!(DropZone::at(r, pos2(10.0, 50.0)), DropZone::Left);
        assert_eq!(DropZone::at(r, pos2(190.0, 50.0)), DropZone::Right);
        assert_eq!(DropZone::at(r, pos2(100.0, 5.0)), DropZone::Top);
        assert_eq!(DropZone::at(r, pos2(100.0, 95.0)), DropZone::Bottom);
        assert_eq!(DropZone::Left.split(), (LinearDir::Horizontal, true));
    }
}
