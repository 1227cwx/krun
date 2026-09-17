use crate::layout::Rect;
use crate::search::ItemRef;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuEntry {
    Command {
        id: u32,
        label: String,
        enabled: bool,
    },
    Separator,
    Submenu {
        label: String,
        entries: Vec<MenuEntry>,
    },
}

impl MenuEntry {
    fn selectable(&self) -> bool {
        matches!(
            self,
            Self::Command { enabled: true, .. } | Self::Submenu { .. }
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuRow {
    pub index: usize,
    pub rect: Rect,
    pub separator: bool,
    pub enabled: bool,
    pub submenu: bool,
}

#[derive(Clone, Debug)]
pub struct MenuState {
    pub root: Vec<MenuEntry>,
    pub submenu: Option<(usize, Vec<MenuEntry>)>,
    pub root_rect: Rect,
    pub submenu_rect: Option<Rect>,
    pub hovered: Option<(bool, usize)>,
    pub selected: Option<(bool, usize)>,
    pub scroll: usize,
    pub submenu_scroll: usize,
    pub target: Option<ItemRef>,
    scale: f32,
    client: Rect,
}

impl MenuState {
    pub fn new(
        entries: Vec<MenuEntry>,
        anchor: (i32, i32),
        client: Rect,
        dpi: u32,
        target: Option<ItemRef>,
    ) -> Self {
        let scale = dpi.max(96) as f32 / 96.0;
        let root_rect = panel_rect(anchor, client, &entries, scale, false);
        Self {
            root: entries,
            submenu: None,
            root_rect,
            submenu_rect: None,
            hovered: None,
            selected: None,
            scroll: 0,
            submenu_scroll: 0,
            target,
            scale,
            client,
        }
    }

    pub fn rows(&self, submenu: bool) -> Vec<MenuRow> {
        let entries = if submenu {
            self.submenu.as_ref().map(|(_, entries)| entries.as_slice())
        } else {
            Some(self.root.as_slice())
        };
        let Some(entries) = entries else {
            return Vec::new();
        };
        let rect = if submenu {
            self.submenu_rect.unwrap_or_default()
        } else {
            self.root_rect
        };
        let normal_height = scaled(34, self.scale);
        let separator_height = scaled(10, self.scale);
        let mut rows = Vec::new();
        let first = if submenu {
            self.submenu_scroll
        } else {
            self.scroll
        };
        let mut top = rect.top + scaled(6, self.scale);
        for index in first..entries.len() {
            let Some(entry) = entries.get(index) else {
                break;
            };
            let height = if matches!(entry, MenuEntry::Separator) {
                separator_height
            } else {
                normal_height
            };
            if top + height > rect.bottom - scaled(6, self.scale) {
                break;
            }
            rows.push(MenuRow {
                index,
                rect: Rect {
                    left: rect.left + scaled(4, self.scale),
                    top,
                    right: rect.right - scaled(4, self.scale),
                    bottom: top + height,
                },
                separator: matches!(entry, MenuEntry::Separator),
                enabled: entry.selectable(),
                submenu: matches!(entry, MenuEntry::Submenu { .. }),
            });
            top += height;
        }
        rows
    }

    pub fn hit(&self, x: i32, y: i32) -> Option<(bool, usize)> {
        for row in self.rows(true) {
            if row.rect.contains(x, y) && row.enabled {
                return Some((true, row.index));
            }
        }
        for row in self.rows(false) {
            if row.rect.contains(x, y) && row.enabled {
                return Some((false, row.index));
            }
        }
        None
    }

    pub fn hover(&mut self, x: i32, y: i32) -> Option<(bool, usize)> {
        let hit = self.hit(x, y);
        self.hovered = hit;
        if let Some((false, index)) = hit {
            if matches!(self.root.get(index), Some(MenuEntry::Submenu { .. })) {
                self.open_submenu(index);
            } else {
                self.submenu = None;
                self.submenu_rect = None;
            }
        }
        hit
    }

    fn open_submenu(&mut self, index: usize) {
        let Some(MenuEntry::Submenu { entries, .. }) = self.root.get(index) else {
            return;
        };
        let entries = entries.clone();
        self.submenu = Some((index, entries.clone()));
        self.submenu_scroll = 0;
        let top = self
            .rows(false)
            .into_iter()
            .find(|row| row.index == index)
            .map_or(self.root_rect.top, |row| row.rect.top);
        let mut rect = panel_rect(
            (self.root_rect.right - scaled(4, self.scale), top),
            self.client,
            &entries,
            self.scale,
            true,
        );
        let width = rect.width();
        if self.root_rect.right + width > self.client.right {
            rect.left = (self.root_rect.left - width).max(self.client.left);
            rect.right = rect.left + width;
        }
        self.submenu_rect = Some(rect);
    }

    pub fn scroll(&mut self, delta: i32) {
        let submenu = self.hovered.is_some_and(|(submenu, _)| submenu) && self.submenu.is_some();
        let visible = self.rows(submenu).len().max(1);
        let total = if submenu {
            self.submenu
                .as_ref()
                .map_or(0, |(_, entries)| entries.len())
        } else {
            self.root.len()
        };
        let offset = if submenu {
            &mut self.submenu_scroll
        } else {
            &mut self.scroll
        };
        let max = total.saturating_sub(visible);
        if delta > 0 {
            *offset = (*offset + 1).min(max);
        } else if delta < 0 {
            *offset = offset.saturating_sub(1);
        }
    }

    pub fn select_next(&mut self, delta: i32) {
        let submenu =
            self.selected.map(|(submenu, _)| submenu).unwrap_or(false) && self.submenu.is_some();
        let entries = if submenu {
            &self.submenu.as_ref().unwrap().1
        } else {
            &self.root
        };
        let selectable = entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| entry.selectable().then_some(index))
            .collect::<Vec<_>>();
        if selectable.is_empty() {
            self.selected = None;
            return;
        }
        let next = if let Some(current) = self
            .selected
            .and_then(|(selected_submenu, index)| (selected_submenu == submenu).then_some(index))
            .and_then(|index| selectable.iter().position(|candidate| *candidate == index))
        {
            if delta < 0 {
                current.saturating_sub(1)
            } else {
                (current + 1).min(selectable.len() - 1)
            }
        } else if delta < 0 {
            selectable.len() - 1
        } else {
            0
        };
        self.selected = Some((submenu, selectable[next]));
        self.ensure_selected_visible();
    }

    fn ensure_selected_visible(&mut self) {
        let Some((submenu, selected)) = self.selected else {
            return;
        };
        let visible = self.rows(submenu);
        if visible.iter().any(|row| row.index == selected) {
            return;
        }
        let offset = if submenu {
            &mut self.submenu_scroll
        } else {
            &mut self.scroll
        };
        if selected < *offset {
            *offset = selected;
        } else if let Some(last) = visible.last() {
            *offset += selected.saturating_sub(last.index);
        }
    }

    pub fn enter_submenu(&mut self) {
        let Some((false, index)) = self.selected else {
            return;
        };
        let first = match self.root.get(index) {
            Some(MenuEntry::Submenu { entries, .. }) => {
                entries.iter().position(MenuEntry::selectable)
            }
            _ => return,
        };
        self.open_submenu(index);
        self.selected = first.map(|index| (true, index));
    }

    pub fn leave_submenu(&mut self) {
        if let Some((parent, _)) = &self.submenu {
            self.selected = Some((false, *parent));
        }
    }

    pub fn selected_command(&self) -> Option<u32> {
        let (submenu, index) = self.selected?;
        self.command_at(submenu, index)
    }

    pub fn entry(&self, submenu: bool, index: usize) -> Option<&MenuEntry> {
        if submenu {
            self.submenu.as_ref()?.1.get(index)
        } else {
            self.root.get(index)
        }
    }

    pub fn command_at(&self, submenu: bool, index: usize) -> Option<u32> {
        match self.entry(submenu, index)? {
            MenuEntry::Command {
                id, enabled: true, ..
            } => Some(*id),
            _ => None,
        }
    }
}

fn scaled(value: i32, scale: f32) -> i32 {
    ((value as f32 * scale).round() as i32).max(1)
}

fn panel_rect(
    anchor: (i32, i32),
    client: Rect,
    entries: &[MenuEntry],
    scale: f32,
    _submenu: bool,
) -> Rect {
    let width = scaled(220, scale);
    let content_height = entries.iter().fold(0, |height, entry| {
        height
            + if matches!(entry, MenuEntry::Separator) {
                scaled(10, scale)
            } else {
                scaled(34, scale)
            }
    });
    let max_height = (client.height() - scaled(8, scale)).max(scaled(46, scale));
    let height = (scaled(12, scale) + content_height).min(max_height);
    let left = anchor
        .0
        .clamp(client.left, (client.right - width).max(client.left));
    let top = if anchor.1 + height <= client.bottom {
        anchor.1
    } else {
        anchor.1 - height
    };
    Rect {
        left,
        top: top.clamp(client.top, (client.bottom - height).max(client.top)),
        right: left + width,
        bottom: top.clamp(client.top, (client.bottom - height).max(client.top)) + height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(count: usize) -> Vec<MenuEntry> {
        (0..count)
            .map(|i| MenuEntry::Command {
                id: i as u32,
                label: format!("项目 {i}"),
                enabled: true,
            })
            .collect()
    }

    #[test]
    fn menu_stays_inside_client_at_common_dpis() {
        for dpi in [96, 120, 144, 192] {
            let client = Rect {
                left: 0,
                top: 0,
                right: 840 * dpi / 96,
                bottom: 520 * dpi / 96,
            };
            let menu = MenuState::new(
                entries(4),
                (client.right - 4, client.bottom - 4),
                client,
                dpi as u32,
                None,
            );
            assert!(menu.root_rect.right <= client.right);
            assert!(menu.root_rect.bottom <= client.bottom);
            assert!(menu.root_rect.left >= client.left);
            assert!(menu.root_rect.top >= client.top);
        }
    }

    #[test]
    fn long_menu_scrolls_and_hits_visible_rows() {
        let client = Rect {
            left: 0,
            top: 0,
            right: 840,
            bottom: 520,
        };
        let mut menu = MenuState::new(entries(30), (10, 10), client, 96, None);
        assert!(menu.rows(false).len() < 30);
        menu.scroll(1);
        assert_eq!(menu.rows(false)[0].index, 1);
        assert_eq!(
            menu.hit(
                menu.rows(false)[0].rect.left + 2,
                menu.rows(false)[0].rect.top + 2
            ),
            Some((false, 1))
        );
    }

    #[test]
    fn submenu_is_hit_separately() {
        let root = vec![MenuEntry::Submenu {
            label: "移动".into(),
            entries: entries(2),
        }];
        let client = Rect {
            left: 0,
            top: 0,
            right: 840,
            bottom: 520,
        };
        let mut menu = MenuState::new(root, (20, 20), client, 96, None);
        assert_eq!(menu.hover(30, 30), Some((false, 0)));
        let rect = menu.submenu_rect.unwrap();
        assert_eq!(menu.hit(rect.left + 10, rect.top + 20), Some((true, 0)));
    }

    #[test]
    fn disabled_commands_are_not_hit() {
        let root = vec![MenuEntry::Command {
            id: 1,
            label: "禁用".into(),
            enabled: false,
        }];
        let client = Rect {
            left: 0,
            top: 0,
            right: 840,
            bottom: 520,
        };
        let menu = MenuState::new(root, (20, 20), client, 96, None);
        let row = menu.rows(false)[0];
        assert_eq!(menu.hit(row.rect.left + 2, row.rect.top + 2), None);
    }
}
