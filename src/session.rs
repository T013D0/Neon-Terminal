//! Restaurar la sesión: al cerrar se guardan ventanas, pestañas, divisiones,
//! carpetas, nombres y colores; al abrir Neon normalmente, se recrean.

use crate::pane::Pane;
use crate::profiles::ProfileKind;
use egui_tiles::{Container, LinearDir, Tile, TileId, Tiles};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct Session {
    pub version: u32,
    pub font_size: f32,
    pub windows: Vec<WinState>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WinState {
    pub pos: Option<[f32; 2]>,
    pub size: Option<[f32; 2]>,
    pub maximized: bool,
    pub active: usize,
    pub tabs: Vec<TabState>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TabState {
    pub name: Option<String>,
    pub color: Option<u8>,
    /// Índice (en orden de recorrido) del panel enfocado.
    pub focused: usize,
    pub layout: LayoutNode,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum LayoutNode {
    Pane {
        cwd: String,
        profile: ProfileKind,
    },
    Split {
        horizontal: bool,
        children: Vec<(f32, LayoutNode)>,
    },
}

fn path() -> PathBuf {
    crate::shell::data_dir().join("session.json")
}

pub fn load() -> Option<Session> {
    let text = std::fs::read_to_string(path()).ok()?;
    let s: Session = serde_json::from_str(&text).ok()?;
    (!s.windows.is_empty()).then_some(s)
}

pub fn save(s: &Session) {
    let p = path();
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    // Escribe aparte y renombra, para no dejar un archivo a medias.
    let tmp = p.with_extension("json.tmp");
    if let Ok(json) = serde_json::to_string_pretty(s) {
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(&tmp, &p);
        }
    }
}

/// Describe el árbol de paneles a partir de `id`.
pub fn layout_of(tiles: &Tiles<Pane>, id: TileId) -> Option<LayoutNode> {
    match tiles.get(id)? {
        Tile::Pane(p) => Some(LayoutNode::Pane {
            cwd: p.cwd.clone(),
            profile: p.profile,
        }),
        Tile::Container(c) => {
            let (horizontal, kids, shares): (bool, Vec<TileId>, Vec<f32>) = match c {
                Container::Linear(l) => (
                    l.dir == LinearDir::Horizontal,
                    l.children.clone(),
                    l.children.iter().map(|ch| l.shares[*ch]).collect(),
                ),
                other => {
                    let k = other.children_vec();
                    let n = k.len();
                    (true, k, vec![1.0; n])
                }
            };
            let children: Vec<(f32, LayoutNode)> = kids
                .into_iter()
                .zip(shares)
                .filter_map(|(k, s)| layout_of(tiles, k).map(|l| (s, l)))
                .collect();
            match children.len() {
                0 => None,
                1 => children.into_iter().next().map(|(_, l)| l),
                _ => Some(LayoutNode::Split {
                    horizontal,
                    children,
                }),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_roundtrips_through_json() {
        let s = Session {
            version: 1,
            font_size: 15.0,
            windows: vec![WinState {
                pos: Some([10.0, 20.0]),
                size: Some([800.0, 600.0]),
                maximized: false,
                active: 0,
                tabs: vec![TabState {
                    name: Some("api".into()),
                    color: Some(2),
                    focused: 1,
                    layout: LayoutNode::Split {
                        horizontal: true,
                        children: vec![
                            (
                                1.0,
                                LayoutNode::Pane {
                                    cwd: "/a".into(),
                                    profile: ProfileKind::Bash,
                                },
                            ),
                            (
                                2.0,
                                LayoutNode::Pane {
                                    cwd: "/b".into(),
                                    profile: ProfileKind::Wsl,
                                },
                            ),
                        ],
                    },
                }],
            }],
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: Session = serde_json::from_str(&json).unwrap();
        assert_eq!(back.windows[0].tabs[0].layout, s.windows[0].tabs[0].layout);
        assert_eq!(back.windows[0].tabs[0].name.as_deref(), Some("api"));
    }
}
