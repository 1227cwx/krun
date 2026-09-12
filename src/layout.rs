#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn contains(self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }

    pub fn width(self) -> i32 {
        self.right - self.left
    }

    pub fn height(self) -> i32 {
        self.bottom - self.top
    }

    pub fn inset(self, value: i32) -> Self {
        Self {
            left: self.left + value,
            top: self.top + value,
            right: self.right - value,
            bottom: self.bottom - value,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Launcher,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolButton {
    Back,
    Search,
    AddMenu,
    Settings,
    Close,
    CategoryLeft,
    CategoryRight,
    CategoryMore,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingControl {
    Hotkey,
    Startup,
    DoubleClick,
    Centered,
    Movable,
    Resizable,
}

#[derive(Clone, Debug)]
pub struct Layout {
    pub scale: f32,
    pub client: Rect,
    pub titlebar: Rect,
    pub categorybar: Rect,
    pub content: Rect,
    pub title: Rect,
    pub drag_area: Rect,
    pub back_button: Rect,
    pub search_button: Rect,
    pub add_button: Rect,
    pub settings_button: Rect,
    pub close_button: Rect,
    pub category_left: Rect,
    pub category_right: Rect,
    pub category_more: Rect,
    pub category_overflow: bool,
    pub categories: Vec<(usize, Rect)>,
    pub search_box: Rect,
    pub search_close_button: Rect,
    pub cells: Vec<Rect>,
    pub columns: usize,
    pub visible_capacity: usize,
    pub max_page_offset: usize,
    pub scrollbar_visible: bool,
    pub scrollbar_track: Rect,
    pub settings_content: Rect,
    pub settings_rows: Vec<(SettingControl, Rect, Rect)>,
    pub add_overlay: Rect,
    pub add_file_button: Rect,
    pub add_folder_button: Rect,
    pub add_drop_zone: Rect,
    pub add_close_button: Rect,
    pub text_overlay: Rect,
    pub text_input: Rect,
    pub text_confirm_button: Rect,
    pub text_cancel_button: Rect,
}

/// Inputs describing the window state that layout depends on.
#[derive(Clone, Copy, Debug)]
pub struct LayoutInput<'a> {
    pub width: i32,
    pub height: i32,
    pub dpi: u32,
    pub view: View,
    pub category_names: &'a [String],
    pub category_start: usize,
    pub search_mode: bool,
    pub item_count: usize,
}

impl Layout {
    pub fn calculate(input: LayoutInput<'_>) -> Self {
        let LayoutInput {
            width,
            height,
            dpi,
            view,
            category_names,
            category_start,
            search_mode,
            item_count,
        } = input;
        let scale = dpi as f32 / 96.0;
        let px = |value: i32| ((value as f32) * scale).round() as i32;
        let client = Rect {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        };
        let titlebar = Rect {
            left: 0,
            top: 0,
            right: width,
            bottom: px(46),
        };
        let button_size = px(40);
        let close_button = Rect {
            left: width - button_size - px(4),
            top: px(3),
            right: width - px(4),
            bottom: px(43),
        };
        let settings_button = shift_left(close_button, button_size);
        let add_button = shift_left(settings_button, button_size);
        let search_button = shift_left(add_button, button_size);
        let back_button = Rect {
            left: px(6),
            top: px(3),
            right: px(46),
            bottom: px(43),
        };
        let title = if view == View::Settings {
            Rect {
                left: px(52),
                top: 0,
                right: px(220),
                bottom: px(46),
            }
        } else {
            Rect {
                left: px(18),
                top: 0,
                right: px(180),
                bottom: px(46),
            }
        };
        let drag_area = Rect {
            left: title.right,
            top: 0,
            right: if view == View::Settings {
                close_button.left
            } else {
                search_button.left
            },
            bottom: px(46),
        };

        let categorybar = Rect {
            left: 0,
            top: px(46),
            right: width,
            bottom: px(88),
        };
        let mini = px(34);
        let category_more = Rect {
            left: width - mini - px(8),
            top: px(50),
            right: width - px(8),
            bottom: px(84),
        };
        let total_tab_width = category_names
            .iter()
            .map(|name| {
                let chars = name.chars().count().min(10) as i32;
                px((chars * 16 + 30).clamp(72, 156)) + px(2)
            })
            .sum::<i32>();
        let category_overflow = total_tab_width.saturating_sub(px(2)) > width - px(22);
        let category_right = shift_left(category_more, mini);
        let category_left = shift_left(category_right, mini);
        let available_right = if category_overflow {
            category_left.left - px(8)
        } else {
            width - px(8)
        };
        let mut categories = Vec::new();
        let mut left = px(14);
        for (index, name) in category_names.iter().enumerate().skip(category_start) {
            let chars = name.chars().count().min(10) as i32;
            let tab_width = px((chars * 16 + 30).clamp(72, 156));
            if left + tab_width > available_right {
                break;
            }
            categories.push((
                index,
                Rect {
                    left,
                    top: px(48),
                    right: left + tab_width,
                    bottom: px(86),
                },
            ));
            left += tab_width + px(2);
        }

        let search_box = Rect {
            left: px(18),
            top: px(98),
            right: width - px(18),
            bottom: px(130),
        };
        let search_close_button = Rect {
            left: search_box.right - px(32),
            top: search_box.top,
            right: search_box.right,
            bottom: search_box.bottom,
        };
        let content_top = if search_mode { px(132) } else { px(90) };
        let content_full = Rect {
            left: px(8),
            top: content_top,
            right: width - px(8),
            bottom: height - px(8),
        };
        let min_cell_width = px(88).max(1);
        let cell_height = px(94).max(1);
        let scrollbar_width = px(10);

        // Reserve space for the scrollbar only when the items really overflow.
        let full_columns = (content_full.width() / min_cell_width).max(1) as usize;
        let full_rows = (content_full.height() / cell_height).max(1) as usize;
        let scrollbar_visible = view == View::Launcher && item_count > full_columns * full_rows;
        let content = if scrollbar_visible {
            Rect {
                right: content_full.right - scrollbar_width,
                ..content_full
            }
        } else {
            content_full
        };

        let columns = (content.width() / min_cell_width).max(1) as usize;
        let cell_width = (content.width() / columns as i32).max(1);
        let rows = (content.height() / cell_height).max(1) as usize;
        let visible_capacity = columns * rows;
        let max_page_offset = if view == View::Launcher {
            item_count.saturating_sub(visible_capacity)
        } else {
            0
        };
        let scrollbar_track = Rect {
            left: content_full.right - scrollbar_width,
            top: content_full.top,
            right: content_full.right,
            bottom: content_full.bottom,
        };
        let cells = (0..visible_capacity)
            .map(|index| {
                let column = index % columns;
                let row = index / columns;
                let left = content.left + column as i32 * cell_width;
                let top = content.top + row as i32 * cell_height;
                Rect {
                    left,
                    top,
                    right: left + cell_width,
                    bottom: top + cell_height,
                }
            })
            .collect();

        let mut settings_rows = Vec::new();
        // Start below the title bar and keep six rows inside the smallest window.
        let settings_content = Rect {
            left: px(24),
            top: px(50),
            right: width - px(24),
            bottom: height - px(14),
        };
        if view == View::Settings {
            let row_height = px(54);
            let control_right = settings_content.right - px(20);
            for (index, control) in [
                SettingControl::Hotkey,
                SettingControl::Startup,
                SettingControl::DoubleClick,
                SettingControl::Centered,
                SettingControl::Movable,
                SettingControl::Resizable,
            ]
            .into_iter()
            .enumerate()
            {
                let top = settings_content.top + index as i32 * row_height;
                let row = Rect {
                    left: settings_content.left,
                    top,
                    right: settings_content.right,
                    bottom: top + row_height,
                };
                let target = match control {
                    SettingControl::Hotkey => Rect {
                        left: control_right - px(190),
                        top: top + px(11),
                        right: control_right,
                        bottom: top + px(43),
                    },
                    _ => Rect {
                        left: control_right - px(40),
                        top: top + px(17),
                        right: control_right,
                        bottom: top + px(37),
                    },
                };
                settings_rows.push((control, row, target));
            }
        }

        let overlay_width = px(500).min(width - px(48));
        let overlay_height = px(300).min(height - px(48));
        let overlay_left = (width - overlay_width) / 2;
        let overlay_top = (height - overlay_height) / 2;
        let add_overlay = Rect {
            left: overlay_left,
            top: overlay_top,
            right: overlay_left + overlay_width,
            bottom: overlay_top + overlay_height,
        };
        let add_close_button = Rect {
            left: add_overlay.right - px(46),
            top: add_overlay.top + px(8),
            right: add_overlay.right - px(8),
            bottom: add_overlay.top + px(46),
        };
        let button_gap = px(12);
        let button_width = (add_overlay.width() - px(56) - button_gap) / 2;
        let add_file_button = Rect {
            left: add_overlay.left + px(22),
            top: add_overlay.top + px(72),
            right: add_overlay.left + px(22) + button_width,
            bottom: add_overlay.top + px(118),
        };
        let add_folder_button = Rect {
            left: add_file_button.right + button_gap,
            top: add_file_button.top,
            right: add_overlay.right - px(22),
            bottom: add_file_button.bottom,
        };
        let add_drop_zone = Rect {
            left: add_overlay.left + px(22),
            top: add_overlay.top + px(140),
            right: add_overlay.right - px(22),
            bottom: add_overlay.bottom - px(22),
        };

        let text_width = px(460).min(width - px(48));
        let text_height = px(190).min(height - px(48));
        let text_left = (width - text_width) / 2;
        let text_top = (height - text_height) / 2;
        let text_overlay = Rect {
            left: text_left,
            top: text_top,
            right: text_left + text_width,
            bottom: text_top + text_height,
        };
        let text_input = Rect {
            left: text_overlay.left + px(24),
            top: text_overlay.top + px(64),
            right: text_overlay.right - px(24),
            bottom: text_overlay.top + px(100),
        };
        let text_confirm_button = Rect {
            left: text_overlay.right - px(202),
            top: text_overlay.bottom - px(58),
            right: text_overlay.right - px(112),
            bottom: text_overlay.bottom - px(20),
        };
        let text_cancel_button = Rect {
            left: text_overlay.right - px(102),
            top: text_confirm_button.top,
            right: text_overlay.right - px(12),
            bottom: text_confirm_button.bottom,
        };

        Self {
            scale,
            client,
            titlebar,
            categorybar,
            content,
            title,
            drag_area,
            back_button,
            search_button,
            add_button,
            settings_button,
            close_button,
            category_left,
            category_right,
            category_more,
            category_overflow,
            categories,
            search_box,
            search_close_button,
            cells,
            columns,
            visible_capacity,
            max_page_offset,
            scrollbar_visible,
            scrollbar_track,
            settings_content,
            settings_rows,
            add_overlay,
            add_file_button,
            add_folder_button,
            add_drop_zone,
            add_close_button,
            text_overlay,
            text_input,
            text_confirm_button,
            text_cancel_button,
        }
    }

    pub fn tool_at(&self, x: i32, y: i32, view: View) -> Option<ToolButton> {
        let common = [(self.close_button, ToolButton::Close)];
        for (rect, button) in common {
            if rect.contains(x, y) {
                return Some(button);
            }
        }
        if view == View::Settings {
            return self.back_button.contains(x, y).then_some(ToolButton::Back);
        }
        let mut buttons = vec![
            (self.search_button, ToolButton::Search),
            (self.add_button, ToolButton::AddMenu),
            (self.settings_button, ToolButton::Settings),
        ];
        if self.category_overflow {
            buttons.extend([
                (self.category_left, ToolButton::CategoryLeft),
                (self.category_right, ToolButton::CategoryRight),
                (self.category_more, ToolButton::CategoryMore),
            ]);
        }
        buttons
            .into_iter()
            .find_map(|(rect, button)| rect.contains(x, y).then_some(button))
    }

    pub fn category_at(&self, x: i32, y: i32) -> Option<usize> {
        self.categories
            .iter()
            .find_map(|(index, rect)| rect.contains(x, y).then_some(*index))
    }

    pub fn item_at(&self, x: i32, y: i32) -> Option<usize> {
        self.cells.iter().position(|cell| cell.contains(x, y))
    }

    pub fn setting_at(&self, x: i32, y: i32) -> Option<SettingControl> {
        self.settings_rows
            .iter()
            .find_map(|(control, row, _)| row.contains(x, y).then_some(*control))
    }

    /// Geometry of the draggable scrollbar thumb, or `None` when everything fits.
    pub fn scrollbar_thumb(&self, offset: usize, item_count: usize) -> Option<Rect> {
        if !self.scrollbar_visible
            || self.visible_capacity == 0
            || item_count <= self.visible_capacity
        {
            return None;
        }
        let track = self.scrollbar_track;
        let track_height = track.height();
        if track_height <= 0 {
            return None;
        }
        let minimum = (self.scale * 28.0).round() as i32;
        let thumb_height = ((track_height as i64 * self.visible_capacity as i64)
            / item_count.max(1) as i64)
            .max(minimum as i64)
            .min(track_height as i64) as i32;
        let travel = (track_height - thumb_height).max(0);
        let max_offset = self.max_page_offset.max(1) as i64;
        let top = track.top + (travel as i64 * offset as i64 / max_offset) as i32;
        Some(Rect {
            left: track.left,
            top,
            right: track.right,
            bottom: top + thumb_height,
        })
    }

    /// Maps a track position to the closest page offset.
    pub fn scrollbar_offset_at(&self, y: i32, item_count: usize) -> usize {
        let Some(thumb) = self.scrollbar_thumb(self.max_page_offset, item_count) else {
            return 0;
        };
        let track = self.scrollbar_track;
        let travel = (track.height() - thumb.height()).max(1);
        let relative = (y - track.top - thumb.height() / 2).clamp(0, travel);
        ((relative as i64 * self.max_page_offset as i64) / travel as i64) as usize
    }
}

fn shift_left(rect: Rect, amount: i32) -> Rect {
    Rect {
        left: rect.left - amount,
        right: rect.right - amount,
        ..rect
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reclaimed_category_space_and_top_menu_hit_at_all_dpis() {
        // Eleven 72px tabs now fit (previously the extra plus forced overflow).
        let names = vec!["常用".to_owned(); 11];
        for dpi in [96, 120, 144, 192] {
            let px = |n: i32| (n as f32 * dpi as f32 / 96.0).round() as i32;
            let layout = Layout::calculate(LayoutInput {
                width: px(840),
                height: px(520),
                dpi,
                view: View::Launcher,
                category_names: &names,
                category_start: 0,
                search_mode: false,
                item_count: 0,
            });
            assert!(!layout.category_overflow);
            assert_eq!(layout.categories.len(), 11);
            assert_eq!(layout.tool_at(px(815), px(67), View::Launcher), None);
            assert_eq!(layout.category_at(px(815), px(67)), Some(10));
            assert_eq!(
                layout.tool_at(px(735), px(23), View::Launcher),
                Some(ToolButton::AddMenu)
            );
        }
    }

    #[test]
    fn dynamic_tabs_overflow_without_disappearing_controls() {
        let names = (0..20)
            .map(|index| format!("分类 {index}"))
            .collect::<Vec<_>>();
        let layout = Layout::calculate(LayoutInput {
            width: 840,
            height: 520,
            dpi: 96,
            view: View::Launcher,
            category_names: &names,
            category_start: 0,
            search_mode: false,
            item_count: 0,
        });
        assert!(layout.categories.len() < names.len());
        assert!(layout.category_overflow);
        assert_eq!(layout.category_more.right, 832);
        assert_eq!(
            layout.tool_at(815, 67, View::Launcher),
            Some(ToolButton::CategoryMore)
        );
    }

    #[test]
    fn one_single_category_hides_overflow_navigation() {
        let names = vec!["常用".to_string()];
        let layout = Layout::calculate(LayoutInput {
            width: 840,
            height: 520,
            dpi: 96,
            view: View::Launcher,
            category_names: &names,
            category_start: 0,
            search_mode: false,
            item_count: 0,
        });
        assert!(!layout.category_overflow);
        let center = (
            (layout.category_more.left + layout.category_more.right) / 2,
            (layout.category_more.top + layout.category_more.bottom) / 2,
        );
        assert_ne!(
            layout.tool_at(center.0, center.1, View::Launcher),
            Some(ToolButton::CategoryMore)
        );
    }

    #[test]
    fn search_moves_grid_below_search_box() {
        let names = vec!["常用".to_string()];
        let normal = Layout::calculate(LayoutInput {
            width: 840,
            height: 520,
            dpi: 96,
            view: View::Launcher,
            category_names: &names,
            category_start: 0,
            search_mode: false,
            item_count: 0,
        });
        let search = Layout::calculate(LayoutInput {
            width: 840,
            height: 520,
            dpi: 96,
            view: View::Launcher,
            category_names: &names,
            category_start: 0,
            search_mode: true,
            item_count: 0,
        });
        assert!(search.content.top > normal.content.top);
        assert!(search.content.top > search.search_box.bottom);
    }

    #[test]
    fn settings_have_all_controls() {
        let layout = Layout::calculate(LayoutInput {
            width: 840,
            height: 520,
            dpi: 96,
            view: View::Settings,
            category_names: &[],
            category_start: 0,
            search_mode: false,
            item_count: 0,
        });
        assert_eq!(layout.settings_rows.len(), 6);
        assert!(layout.settings_rows.last().unwrap().1.bottom <= layout.client.bottom);
    }

    #[test]
    fn settings_fit_inside_the_smallest_window() {
        for dpi in [96u32, 120, 144, 192] {
            // The minimum window size is expressed in 96-DPI logical units and
            // Windows enforces it in physical pixels.
            let width = 620 * dpi as i32 / 96;
            let height = 400 * dpi as i32 / 96;
            let layout = Layout::calculate(LayoutInput {
                width,
                height,
                dpi,
                view: View::Settings,
                category_names: &[],
                category_start: 0,
                search_mode: false,
                item_count: 0,
            });
            let last = layout.settings_rows.last().unwrap().1;
            assert!(
                last.bottom <= layout.client.bottom,
                "settings rows overflow at dpi {dpi}: {} > {}",
                last.bottom,
                layout.client.bottom
            );
        }
    }

    #[test]
    fn add_overlay_stays_inside_window() {
        let layout = Layout::calculate(LayoutInput {
            width: 840,
            height: 520,
            dpi: 96,
            view: View::Launcher,
            category_names: &["常用".into()],
            category_start: 0,
            search_mode: false,
            item_count: 0,
        });
        assert!(
            layout
                .client
                .contains(layout.add_overlay.left, layout.add_overlay.top)
        );
        assert!(layout.add_overlay.right <= layout.client.right);
        assert!(layout.add_overlay.bottom <= layout.client.bottom);
        assert!(layout.add_drop_zone.top > layout.add_file_button.bottom);
    }

    #[test]
    fn dialog_input_has_compact_height_and_clear_spacing() {
        for dpi in [96u32, 120, 144, 192] {
            let width = 840 * dpi as i32 / 96;
            let height = 520 * dpi as i32 / 96;
            let layout = Layout::calculate(LayoutInput {
                width,
                height,
                dpi,
                view: View::Launcher,
                category_names: &["常用".into()],
                category_start: 0,
                search_mode: false,
                item_count: 0,
            });
            let expected = (36 * dpi as i32 + 48) / 96;
            assert_eq!(layout.text_input.height(), expected);
            assert!(layout.text_input.top > layout.text_overlay.top);
            assert!(layout.text_input.bottom < layout.text_confirm_button.top);
        }
    }

    #[test]
    fn scrollbar_only_appears_when_items_overflow() {
        let names = vec!["常用".to_string()];
        let few = Layout::calculate(LayoutInput {
            width: 840,
            height: 520,
            dpi: 96,
            view: View::Launcher,
            category_names: &names,
            category_start: 0,
            search_mode: false,
            item_count: 3,
        });
        assert!(!few.scrollbar_visible);
        assert_eq!(few.max_page_offset, 0);
        assert!(few.scrollbar_thumb(0, 3).is_none());
    }

    #[test]
    fn scrollbar_and_offset_are_bounded_when_items_overflow() {
        let names = vec!["常用".to_string()];
        let reference = Layout::calculate(LayoutInput {
            width: 840,
            height: 520,
            dpi: 96,
            view: View::Launcher,
            category_names: &names,
            category_start: 0,
            search_mode: false,
            item_count: 0,
        });
        let item_count = reference.visible_capacity + 7;
        let layout = Layout::calculate(LayoutInput {
            width: 840,
            height: 520,
            dpi: 96,
            view: View::Launcher,
            category_names: &names,
            category_start: 0,
            search_mode: false,
            item_count,
        });
        assert!(layout.scrollbar_visible);
        assert!(layout.max_page_offset > 0);
        assert_eq!(layout.max_page_offset, item_count - layout.visible_capacity);

        let thumb = layout.scrollbar_thumb(0, item_count).unwrap();
        assert!(thumb.top >= layout.scrollbar_track.top);
        assert!(thumb.bottom <= layout.scrollbar_track.bottom);
        let end = layout
            .scrollbar_thumb(layout.max_page_offset, item_count)
            .unwrap();
        assert!(end.bottom <= layout.scrollbar_track.bottom);
    }
}
