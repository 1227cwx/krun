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
    AddItem,
    Settings,
    Close,
    AddCategory,
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
    pub add_category_button: Rect,
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

impl Layout {
    pub fn calculate(
        width: i32,
        height: i32,
        dpi: u32,
        view: View,
        category_names: &[String],
        category_start: usize,
        search_mode: bool,
    ) -> Self {
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
        let add_category_button = Rect {
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
        let category_overflow = total_tab_width > add_category_button.left - px(22);
        let category_more = shift_left(add_category_button, mini);
        let category_right = shift_left(category_more, mini);
        let category_left = shift_left(category_right, mini);
        let available_right = if category_overflow {
            category_left.left - px(8)
        } else {
            add_category_button.left - px(8)
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
        let content_top = if search_mode { px(140) } else { px(100) };
        let content = Rect {
            left: px(16),
            top: content_top,
            right: width - px(16),
            bottom: height - px(16),
        };
        let cell_width = px(92).max(1);
        let cell_height = px(88).max(1);
        let columns = (content.width() / cell_width).max(1) as usize;
        let rows = (content.height() / cell_height).max(1) as usize;
        let visible_capacity = columns * rows;
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
        let settings_content = Rect {
            left: px(24),
            top: px(58),
            right: width - px(24),
            bottom: height - px(18),
        };
        if view == View::Settings {
            let row_height = px(60);
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
                        top: top + px(14),
                        right: control_right,
                        bottom: top + px(46),
                    },
                    SettingControl::Centered => Rect {
                        left: control_right - px(20),
                        top: top + px(20),
                        right: control_right,
                        bottom: top + px(40),
                    },
                    _ => Rect {
                        left: control_right - px(40),
                        top: top + px(20),
                        right: control_right,
                        bottom: top + px(40),
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
            bottom: text_overlay.top + px(106),
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
            add_category_button,
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
            (self.add_button, ToolButton::AddItem),
            (self.settings_button, ToolButton::Settings),
            (self.add_category_button, ToolButton::AddCategory),
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
    fn dynamic_tabs_overflow_without_disappearing_controls() {
        let names = (0..20)
            .map(|index| format!("分类 {index}"))
            .collect::<Vec<_>>();
        let layout = Layout::calculate(840, 520, 96, View::Launcher, &names, 0, false);
        assert!(layout.categories.len() < names.len());
        assert!(layout.category_overflow);
        assert!(layout.category_more.right <= layout.add_category_button.left);
        assert_eq!(
            layout.tool_at(815, 67, View::Launcher),
            Some(ToolButton::AddCategory)
        );
    }

    #[test]
    fn one_single_category_hides_overflow_navigation() {
        let names = vec!["常用".to_string()];
        let layout = Layout::calculate(840, 520, 96, View::Launcher, &names, 0, false);
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
        let normal = Layout::calculate(840, 520, 96, View::Launcher, &names, 0, false);
        let search = Layout::calculate(840, 520, 96, View::Launcher, &names, 0, true);
        assert!(search.content.top > normal.content.top);
        assert!(search.content.top > search.search_box.bottom);
    }

    #[test]
    fn settings_have_all_controls() {
        let layout = Layout::calculate(840, 520, 96, View::Settings, &[], 0, false);
        assert_eq!(layout.settings_rows.len(), 6);
        assert!(layout.settings_rows.last().unwrap().1.bottom <= layout.client.bottom);
    }

    #[test]
    fn add_overlay_stays_inside_window() {
        let layout = Layout::calculate(840, 520, 96, View::Launcher, &["常用".into()], 0, false);
        assert!(
            layout
                .client
                .contains(layout.add_overlay.left, layout.add_overlay.top)
        );
        assert!(layout.add_overlay.right <= layout.client.right);
        assert!(layout.add_overlay.bottom <= layout.client.bottom);
        assert!(layout.add_drop_zone.top > layout.add_file_button.bottom);
    }
}
