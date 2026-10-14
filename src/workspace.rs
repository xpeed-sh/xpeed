use crate::connections::Connection;
use egui_tiles::{Container, ContainerKind, Linear, LinearDir, Tile, TileId, Tiles, Tree};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Pane {
    Connections,
    Files,
    Activity,
    Commands,
    Assistant,
    AppInfo,
    ServerInfo,
    ExtensionView(u64),
    ExtensionDetails(u64),
    Extensions,
    Terminal(u64),
    Welcome,
}

pub fn default_layout(terminals: &[u64], sidebar_right: bool) -> Tree<Pane> {
    let mut tiles = Tiles::default();
    let connections = tiles.insert_pane(Pane::Connections);
    let files = tiles.insert_pane(Pane::Files);
    let activity = tiles.insert_pane(Pane::Activity);
    let commands = tiles.insert_pane(Pane::Commands);
    let extensions = tiles.insert_pane(Pane::Extensions);
    let sidebar_tabs = tiles.insert_tab_tile(vec![connections, files, extensions]);
    let activity_tabs = tiles.insert_tab_tile(vec![commands, activity]);
    let sidebar = tiles.insert_container(Linear::new_binary(
        LinearDir::Vertical,
        [sidebar_tabs, activity_tabs],
        0.66,
    ));
    let terminal_panes = if terminals.is_empty() {
        vec![tiles.insert_pane(Pane::Welcome)]
    } else {
        terminals
            .iter()
            .map(|&id| tiles.insert_pane(Pane::Terminal(id)))
            .collect()
    };
    let terminal_tabs = tiles.insert_tab_tile(terminal_panes);
    let root = if sidebar_right {
        tiles.insert_container(Linear::new_binary(
            LinearDir::Horizontal,
            [terminal_tabs, sidebar],
            0.79,
        ))
    } else {
        tiles.insert_container(Linear::new_binary(
            LinearDir::Horizontal,
            [sidebar, terminal_tabs],
            0.21,
        ))
    };
    let info = tiles.insert_pane(Pane::AppInfo);
    let server = tiles.insert_pane(Pane::ServerInfo);
    let info = tiles.insert_tab_tile(vec![info]);
    let server = tiles.insert_tab_tile(vec![server]);
    let ai_tabs = tiles.insert_vertical_tile(vec![info, server]);
    let root = tiles.insert_container(Linear::new_binary(
        LinearDir::Horizontal,
        [root, ai_tabs],
        0.77,
    ));
    let mut tree = Tree::new("xpeed_workspace", root, tiles);
    tree.set_visible(files, false);
    for pane in [Pane::AppInfo, Pane::ServerInfo] {
        if let Some(id) = find(&tree, pane) {
            tree.set_visible(id, false);
        }
    }
    tree.set_visible(extensions, false);
    tree
}

/// Keep session navigation in one left host, outside terminal docking.
pub fn sidebar_layout() -> Tree<Pane> {
    let mut tiles = Tiles::default();
    let connections = tiles.insert_pane(Pane::Connections);
    let files = tiles.insert_pane(Pane::Files);
    let commands = tiles.insert_pane(Pane::Commands);
    let activity = tiles.insert_pane(Pane::Activity);
    let navigation = tiles.insert_tab_tile(vec![connections, files]);
    let records = tiles.insert_tab_tile(vec![commands, activity]);
    let root = tiles.insert_container(Linear::new_binary(
        LinearDir::Vertical,
        [navigation, records],
        0.66,
    ));
    let mut tree = Tree::new("xpeed_session_sidebar", root, tiles);
    tree.set_visible(files, false);
    tree
}

pub fn detach_sidebar(tree: &mut Tree<Pane>) {
    let panes: Vec<_> = tree
        .tiles
        .iter()
        .filter_map(|(&id, tile)| {
            matches!(
                tile,
                Tile::Pane(
                    Pane::Connections
                        | Pane::Files
                        | Pane::Commands
                        | Pane::Activity
                        | Pane::Extensions
                )
            )
            .then_some(id)
        })
        .collect();
    for id in panes {
        tree.remove_recursively(id);
    }
    refresh_visibility(tree);
}

fn valid_sidebar(tree: &Tree<Pane>) -> bool {
    if tree.tiles.len() > 64 {
        return false;
    }
    let Some(root) = tree.root else {
        return false;
    };
    let mut visited = std::collections::HashSet::new();
    let mut pending = vec![root];
    let mut panes = Vec::new();
    while let Some(id) = pending.pop() {
        if !visited.insert(id) {
            return false;
        }
        match tree.tiles.get(id) {
            Some(Tile::Container(container)) => pending.extend(container.children().copied()),
            Some(Tile::Pane(
                pane @ (Pane::Connections | Pane::Files | Pane::Commands | Pane::Activity),
            )) => panes.push(*pane),
            Some(Tile::Pane(Pane::ExtensionView(_))) => {}
            _ => return false,
        }
    }
    panes.len() == 4
        && [
            Pane::Connections,
            Pane::Files,
            Pane::Commands,
            Pane::Activity,
        ]
        .iter()
        .all(|pane| panes.contains(pane))
}

pub fn find(tree: &Tree<Pane>, pane: Pane) -> Option<TileId> {
    tree.tiles.iter().find_map(|(&id, tile)| {
        if *tile == Tile::Pane(pane) {
            Some(id)
        } else {
            None
        }
    })
}
/// Add a view beside an existing tab without changing terminal splits or the active tab.
pub fn add_view(tree: &mut Tree<Pane>, anchor: Pane, pane: Pane) {
    if find(tree, pane).is_some() {
        return;
    }
    let Some(anchor) = find(tree, anchor) else {
        return;
    };
    let parent = tree.tiles.parent_of(anchor);
    let new = tree.tiles.insert_pane(pane);
    if let Some(Tile::Container(Container::Tabs(tabs))) =
        parent.and_then(|id| tree.tiles.get_mut(id))
    {
        tabs.children.push(new);
    } else if let Some(old) = tree.tiles.remove(anchor) {
        let old = tree.tiles.insert_new(old);
        tree.tiles
            .insert(anchor, Tile::Container(Container::new_tabs(vec![old, new])));
    }
}

pub fn panel_members(tree: &Tree<Pane>, pane: Pane) -> Vec<Pane> {
    let Some(tile) = find(tree, pane) else {
        return vec![];
    };
    if let Some(Tile::Container(Container::Tabs(tabs))) =
        tree.tiles.parent_of(tile).and_then(|id| tree.tiles.get(id))
    {
        tabs.children
            .iter()
            .filter_map(|id| {
                if let Some(Tile::Pane(pane)) = tree.tiles.get(*id) {
                    Some(*pane)
                } else {
                    None
                }
            })
            .collect()
    } else {
        vec![pane]
    }
}

pub fn join_panel(tree: &mut Tree<Pane>, anchor: Pane, pane: Pane) {
    if panel_members(tree, anchor).contains(&pane) {
        return;
    }
    let visible = [anchor, pane]
        .into_iter()
        .any(|pane| find(tree, pane).is_some_and(|id| tree.tiles.is_visible(id)));
    if let Some(id) = find(tree, pane) {
        tree.remove_recursively(id);
    }
    add_view(tree, anchor, pane);
    for member in panel_members(tree, anchor) {
        if let Some(id) = find(tree, member) {
            tree.set_visible(id, visible);
        }
    }
    refresh_visibility(tree);
}

/// A panel is an independent vertical sibling; tabs are opt-in via add_view.
pub fn add_panel(tree: &mut Tree<Pane>, anchor: Pane, pane: Pane) {
    if find(tree, pane).is_some() {
        return;
    }
    let Some(anchor) = find(tree, anchor) else {
        return;
    };
    let group = tree
        .tiles
        .parent_of(anchor)
        .filter(|id| {
            matches!(
                tree.tiles.get(*id),
                Some(Tile::Container(Container::Tabs(_)))
            )
        })
        .unwrap_or(anchor);
    let new = tree.tiles.insert_pane(pane);
    let panel = tree.tiles.insert_tab_tile(vec![new]);
    if let Some(old) = tree.tiles.remove(group) {
        let old = tree.tiles.insert_new(old);
        tree.tiles.insert(
            group,
            Tile::Container(Container::new_vertical(vec![old, panel])),
        );
    }
}

/// Migrate the former shared right tab group while preserving terminal IDs/splits.
pub fn migrate_right_tabs(tree: &mut Tree<Pane>) {
    let anchor = find(tree, Pane::Assistant);
    let parent = anchor.and_then(|id| tree.tiles.parent_of(id));
    let shared = parent.and_then(|id| tree.tiles.get(id)).is_some_and(
        |tile| matches!(tile, Tile::Container(Container::Tabs(tabs)) if tabs.children.len() > 1),
    );
    if shared {
        let active = match tree.tiles.get(parent.unwrap()) {
            Some(Tile::Container(Container::Tabs(tabs))) => tabs.active,
            _ => None,
        };
        let children = match tree.tiles.get(parent.unwrap()) {
            Some(Tile::Container(Container::Tabs(tabs))) => tabs.children.clone(),
            _ => vec![],
        };
        let panels: Vec<_> = children
            .into_iter()
            .map(|id| {
                if active.is_some_and(|active| active != id) {
                    tree.set_visible(id, false);
                }
                tree.tiles.insert_tab_tile(vec![id])
            })
            .collect();
        tree.tiles.insert(
            parent.unwrap(),
            Tile::Container(Container::new_vertical(panels)),
        );
    }
    refresh_visibility(tree);
}

pub fn ensure_right_views(tree: &mut Tree<Pane>) {
    if find(tree, Pane::AppInfo).is_none() {
        if find(tree, Pane::Assistant).is_some() {
            add_panel(tree, Pane::Assistant, Pane::AppInfo);
        } else if let Some(root) = tree.root {
            let pane = tree.tiles.insert_pane(Pane::AppInfo);
            let panel = tree.tiles.insert_tab_tile(vec![pane]);
            tree.root = Some(tree.tiles.insert_horizontal_tile(vec![root, panel]));
            tree.set_visible(pane, false);
        }
    }
    for pane in [Pane::AppInfo, Pane::ServerInfo] {
        if find(tree, pane).is_none() {
            add_panel(tree, Pane::AppInfo, pane);
            if let Some(id) = find(tree, pane) {
                tree.set_visible(id, false);
            }
        }
    }
    refresh_visibility(tree);
}
pub fn refresh_visibility(tree: &mut Tree<Pane>) {
    fn visit(tree: &mut Tree<Pane>, id: TileId) -> bool {
        let children = match tree.tiles.get(id) {
            Some(Tile::Container(c)) => c.children().copied().collect::<Vec<_>>(),
            _ => return tree.tiles.is_visible(id),
        };
        let mut visible = false;
        for child in children {
            visible |= visit(tree, child);
        }
        tree.set_visible(id, visible);
        visible
    }
    if let Some(root) = tree.root {
        visit(tree, root);
    }
}

pub fn add_terminal(
    tree: &mut Tree<Pane>,
    id: u64,
    focused: Option<u64>,
    split: Option<ContainerKind>,
) {
    let target = focused
        .and_then(|id| find(tree, Pane::Terminal(id)))
        .or_else(|| find(tree, Pane::Welcome));
    let new_pane = tree.tiles.insert_pane(Pane::Terminal(id));
    if let Some(target) = target {
        if tree.tiles.get(target) == Some(&Tile::Pane(Pane::Welcome)) {
            tree.tiles.remove(new_pane);
            tree.tiles.insert(target, Tile::Pane(Pane::Terminal(id)));
        } else {
            let parent = tree.tiles.parent_of(target);
            let group = parent
                .filter(|p| {
                    matches!(
                        tree.tiles.get(*p),
                        Some(Tile::Container(Container::Tabs(_)))
                    )
                })
                .unwrap_or(target);
            if let Some(kind) = split {
                if let Some(old_tile) = tree.tiles.remove(group) {
                    let old_id = tree.tiles.insert_new(old_tile);
                    let new_tabs = tree.tiles.insert_tab_tile(vec![new_pane]);
                    tree.tiles.insert(
                        group,
                        Tile::Container(Container::new(kind, vec![old_id, new_tabs])),
                    );
                }
            } else if let Some(Tile::Container(Container::Tabs(tabs))) = tree.tiles.get_mut(group) {
                tabs.children.push(new_pane);
                tabs.active = Some(new_pane);
            } else if let Some(old_tile) = tree.tiles.remove(group) {
                let old_id = tree.tiles.insert_new(old_tile);
                tree.tiles.insert(
                    group,
                    Tile::Container(Container::new_tabs(vec![old_id, new_pane])),
                );
            }
        }
    } else if let Some(root) = tree.root {
        let tabs = tree.tiles.insert_tab_tile(vec![new_pane]);
        tree.root = Some(tree.tiles.insert_horizontal_tile(vec![root, tabs]));
    } else {
        tree.root = Some(tree.tiles.insert_tab_tile(vec![new_pane]));
    }
    tree.make_active(|_, tile| *tile == Tile::Pane(Pane::Terminal(id)));
}

#[derive(Serialize, Deserialize)]
pub struct Snapshot {
    #[serde(default)]
    pub layout_version: u32,
    pub tree: Tree<Pane>,
    pub sources: BTreeMap<u64, Connection>,
    #[serde(default)]
    pub sidebar_tree: Option<Tree<Pane>>,
}
impl Snapshot {
    fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|p| p.join("xpeed").join("workspace.json"))
    }
    pub fn load() -> Option<Self> {
        let mut snapshot: Self = Self::path()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())?;
        // Reject inconsistent structures before giving persisted data to the dock renderer.
        if snapshot.tree.tiles.len() > 512 || snapshot.sources.len() > 64 {
            return None;
        }
        let root = snapshot.tree.root?;
        let mut visited = std::collections::HashSet::new();
        let mut pending = vec![root];
        while let Some(id) = pending.pop() {
            if !visited.insert(id) {
                return None;
            }
            match snapshot.tree.tiles.get(id)? {
                Tile::Container(container) => pending.extend(container.children().copied()),
                Tile::Pane(Pane::Terminal(id)) if !snapshot.sources.contains_key(id) => {
                    return None;
                }
                _ => {}
            }
        }
        if snapshot
            .sidebar_tree
            .as_ref()
            .is_some_and(|tree| !valid_sidebar(tree))
        {
            snapshot.sidebar_tree = None;
        }
        if snapshot.layout_version < 2 {
            migrate_right_tabs(&mut snapshot.tree);
            snapshot.layout_version = 2;
        }
        Some(snapshot)
    }
    pub fn save(&self) -> Result<(), String> {
        let path =
            Self::path().ok_or(xpeed_i18n::t("No se encontró la carpeta de configuración"))?;
        std::fs::create_dir_all(path.parent().ok_or(xpeed_i18n::t("Ruta inválida"))?)
            .map_err(|e| e.to_string())?;
        let temp = path.with_extension("tmp");
        std::fs::write(
            &temp,
            serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        std::fs::rename(temp, path).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn separate_monitor_is_migrated_into_server_panel_without_duplicates() {
        let mut tree = default_layout(&[1], false);
        add_panel(&mut tree, Pane::AppInfo, Pane::ExtensionView(9));
        let monitor = find(&tree, Pane::ExtensionView(9)).unwrap();
        tree.set_visible(monitor, true);
        join_panel(&mut tree, Pane::ServerInfo, Pane::ExtensionView(9));
        join_panel(&mut tree, Pane::ServerInfo, Pane::ExtensionView(9));
        assert_eq!(
            panel_members(&tree, Pane::ServerInfo),
            vec![Pane::ServerInfo, Pane::ExtensionView(9)]
        );
        assert!(
            tree.tiles
                .is_visible(find(&tree, Pane::ServerInfo).unwrap())
        );
        assert_eq!(
            tree.tiles
                .iter()
                .filter(|(_, tile)| **tile == Tile::Pane(Pane::ExtensionView(9)))
                .count(),
            1
        );
        assert!(find(&tree, Pane::Terminal(1)).is_some());
    }
    #[test]
    fn legacy_right_tabs_become_panels_and_explicit_tab_groups_remain_opt_in() {
        let mut tree = default_layout(&[7], false);
        add_panel(&mut tree, Pane::AppInfo, Pane::Assistant);
        for pane in [Pane::AppInfo, Pane::ServerInfo] {
            tree.remove_recursively(find(&tree, pane).unwrap());
            add_view(&mut tree, Pane::Assistant, pane);
        }
        let terminal = find(&tree, Pane::Terminal(7));
        migrate_right_tabs(&mut tree);
        let parents: std::collections::HashSet<_> =
            [Pane::Assistant, Pane::AppInfo, Pane::ServerInfo]
                .into_iter()
                .map(|pane| tree.tiles.parent_of(find(&tree, pane).unwrap()))
                .collect();
        assert_eq!(parents.len(), 3);
        assert_eq!(find(&tree, Pane::Terminal(7)), terminal);
        add_view(&mut tree, Pane::Assistant, Pane::ExtensionView(10));
        ensure_right_views(&mut tree);
        assert_eq!(
            tree.tiles.parent_of(find(&tree, Pane::Assistant).unwrap()),
            tree.tiles
                .parent_of(find(&tree, Pane::ExtensionView(10)).unwrap())
        );
        add_panel(&mut tree, Pane::Assistant, Pane::ExtensionView(11));
        assert_ne!(
            tree.tiles.parent_of(find(&tree, Pane::Assistant).unwrap()),
            tree.tiles
                .parent_of(find(&tree, Pane::ExtensionView(11)).unwrap())
        );
    }
    #[test]
    fn right_views_migrate_without_changing_terminals_and_extensions_can_dock() {
        let mut tree = default_layout(&[9], false);
        let terminal = find(&tree, Pane::Terminal(9)).unwrap();
        for pane in [Pane::AppInfo, Pane::ServerInfo] {
            tree.remove_recursively(find(&tree, pane).unwrap());
        }
        ensure_right_views(&mut tree);
        ensure_right_views(&mut tree);
        let parent = tree.tiles.parent_of(find(&tree, Pane::AppInfo).unwrap());
        assert_ne!(
            tree.tiles.parent_of(find(&tree, Pane::ServerInfo).unwrap()),
            parent
        );
        assert_eq!(find(&tree, Pane::Terminal(9)), Some(terminal));
        add_view(&mut tree, Pane::AppInfo, Pane::ExtensionView(77));
        add_view(&mut tree, Pane::AppInfo, Pane::ExtensionView(77));
        assert_eq!(
            tree.tiles
                .iter()
                .filter(|(_, tile)| **tile == Tile::Pane(Pane::ExtensionView(77)))
                .count(),
            1
        );
        let restored: Tree<Pane> =
            serde_json::from_slice(&serde_json::to_vec(&tree).unwrap()).unwrap();
        assert_eq!(tree, restored);
        let mut sidebar = sidebar_layout();
        add_view(&mut sidebar, Pane::Connections, Pane::ExtensionView(78));
        assert!(valid_sidebar(&sidebar));
    }
    #[test]
    fn older_layouts_move_navigation_outside_terminal_docking() {
        let mut tree = default_layout(&[1], true);
        add_terminal(&mut tree, 2, Some(1), Some(ContainerKind::Vertical));
        let terminal = find(&tree, Pane::Terminal(1)).unwrap();
        let parent = tree.tiles.parent_of(terminal);
        detach_sidebar(&mut tree);
        assert_eq!(find(&tree, Pane::Terminal(1)), Some(terminal));
        assert_eq!(tree.tiles.parent_of(terminal), parent);
        assert!(find(&tree, Pane::Terminal(2)).is_some());
        assert!(find(&tree, Pane::Assistant).is_none());
        assert!(find(&tree, Pane::AppInfo).is_some());
        for pane in [
            Pane::Connections,
            Pane::Files,
            Pane::Commands,
            Pane::Activity,
            Pane::Extensions,
        ] {
            assert!(find(&tree, pane).is_none());
        }
        let mut sidebar = sidebar_layout();
        assert!(valid_sidebar(&sidebar));
        let root = sidebar.root.unwrap();
        sidebar.tiles.insert(root, Tile::Pane(Pane::Terminal(1)));
        assert!(!valid_sidebar(&sidebar));
    }
    #[test]
    fn nested_splits_and_tabs_keep_all_panels_and_session_ids() {
        let mut tree = default_layout(&[1], false);
        add_terminal(&mut tree, 2, Some(1), Some(ContainerKind::Horizontal));
        add_terminal(&mut tree, 3, Some(2), Some(ContainerKind::Vertical));
        add_terminal(&mut tree, 4, Some(3), None);
        for pane in [
            Pane::Connections,
            Pane::Files,
            Pane::Activity,
            Pane::Terminal(1),
            Pane::Terminal(2),
            Pane::Terminal(3),
            Pane::Terminal(4),
        ] {
            assert!(find(&tree, pane).is_some());
        }
        let json = serde_json::to_vec(&tree).unwrap();
        let restored: Tree<Pane> = serde_json::from_slice(&json).unwrap();
        assert_eq!(tree, restored);
        let source = find(&tree, Pane::Connections).unwrap();
        let terminal = find(&tree, Pane::Terminal(3)).unwrap();
        let container = tree.tiles.parent_of(terminal).unwrap();
        tree.move_tile_to_container(source, container, 0, true);
        assert_eq!(tree.tiles.parent_of(source), Some(container));
        assert!(find(&tree, Pane::Terminal(1)).is_some());
    }
}
