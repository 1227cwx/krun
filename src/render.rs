use crate::config::{Config, LaunchItem};
use crate::hotkey;
use crate::layout::{Layout, Rect, SettingControl, ToolButton, View};
use crate::menu::{MenuEntry, MenuState};
use crate::win::wide;
use std::mem::zeroed;
use std::ptr::null_mut;
use windows_sys::Win32::Foundation::{COLORREF, HWND, RECT, SIZE};
use windows_sys::Win32::Graphics::Gdi::{
    AC_SRC_OVER, AlphaBlend, BLENDFUNCTION, BeginPaint, BitBlt, CreateCompatibleBitmap,
    CreateCompatibleDC, CreateFontW, CreatePen, CreateSolidBrush, DEFAULT_CHARSET, DEFAULT_PITCH,
    DeleteDC, DeleteObject, DrawTextW, Ellipse, EndPaint, FF_DONTCARE, FW_NORMAL, FillRect,
    GetStockObject, GetTextExtentExPointW, HDC, HFONT, HGDIOBJ, LineTo, MoveToEx, NULL_BRUSH,
    OUT_DEFAULT_PRECIS, PAINTSTRUCT, PROOF_QUALITY, PS_SOLID, RoundRect, SRCCOPY, SelectObject,
    SetBkMode, SetTextColor, TRANSPARENT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{DI_NORMAL, DrawIconEx, GetClientRect, HICON};

const BG: COLORREF = 0x00f5f6f7;
const TITLE_BG: COLORREF = 0x00f1f3f4;
const CATEGORY_BG: COLORREF = 0x00e9edef;
const SURFACE: COLORREF = 0x00ffffff;
const SOFT: COLORREF = 0x00f6f6f6;
const BORDER: COLORREF = 0x00d7d2cb;
const OUTER_BORDER: COLORREF = 0x009f9a92;
const HOVER: COLORREF = 0x00eae7e1;
const PRESSED: COLORREF = 0x00d5d0c7;
const OVERLAY_SHADE: COLORREF = 0x00e8e5e0;
const TEXT: COLORREF = 0x00302f2d;
const MUTED: COLORREF = 0x00817e78;
const ACCENT: COLORREF = 0x008b7b1d;
const ACCENT_SOFT: COLORREF = 0x00d8e9ec;

pub struct RenderData<'a> {
    pub config: &'a Config,
    pub layout: &'a Layout,
    pub view: View,
    pub items: &'a [(&'a LaunchItem, HICON)],
    pub active_category: usize,
    pub selected: Option<usize>,
    pub search_mode: bool,
    pub search_query: &'a str,
    pub startup_enabled: bool,
    pub hotkey_capture: bool,
    pub page_offset: usize,
    pub item_count: usize,
    pub scrollbar_hover: bool,
    pub scrollbar_active: bool,
    pub hovered_item: Option<usize>,
    pub hovered_tool: Option<ToolButton>,
    pub pressed_item: Option<usize>,
    pub keyboard_selection: bool,
    pub add_overlay_open: bool,
    pub text_overlay_title: Option<&'static str>,
    pub text_input_focused: bool,
    pub menu: Option<&'a MenuState>,
    pub content_progress: f32,
    pub accent: COLORREF,
    pub hwnd: HWND,
}

pub unsafe fn paint(hwnd: HWND, data: &RenderData<'_>) {
    let mut paint: PAINTSTRUCT = unsafe { zeroed() };
    let target = unsafe { BeginPaint(hwnd, &mut paint) };
    if target.is_null() {
        return;
    }
    let mut client = RECT::default();
    unsafe { GetClientRect(hwnd, &mut client) };
    let width = client.right - client.left;
    let height = client.bottom - client.top;
    let memory = unsafe { CreateCompatibleDC(target) };
    let bitmap = unsafe { CreateCompatibleBitmap(target, width, height) };
    if memory.is_null() || bitmap.is_null() {
        unsafe { EndPaint(hwnd, &paint) };
        return;
    }
    let old_bitmap = unsafe { SelectObject(memory, bitmap as HGDIOBJ) };
    unsafe {
        SetBkMode(memory, TRANSPARENT as i32);
        fill(memory, data.layout.client, BG);
        if data.view == View::Settings {
            fill(memory, data.layout.settings_content.inset(1), SURFACE);
            frame(memory, data.layout.settings_content, BORDER, 1);
        }
        fill(memory, data.layout.titlebar, TITLE_BG);
        fill(
            memory,
            Rect {
                top: data.layout.titlebar.bottom - 1,
                ..data.layout.titlebar
            },
            BORDER,
        );
    }

    let font = unsafe { create_font((14.0 * data.layout.scale) as i32) };
    let small = unsafe { create_font((12.0 * data.layout.scale) as i32) };
    let old_font = unsafe { SelectObject(memory, font as HGDIOBJ) };
    let title = if data.view == View::Settings {
        "设置"
    } else {
        "KRun"
    };
    unsafe {
        text(
            memory,
            title,
            data.layout.title,
            TEXT,
            0x00000004 | 0x00000020,
        );
        if data.view == View::Settings {
            draw_title_tool(
                memory,
                data.layout.back_button,
                ToolButton::Back,
                TEXT,
                data,
            );
        } else {
            draw_title_tool(
                memory,
                data.layout.search_button,
                ToolButton::Search,
                MUTED,
                data,
            );
            draw_title_tool(
                memory,
                data.layout.add_button,
                ToolButton::AddMenu,
                MUTED,
                data,
            );
            draw_title_tool(
                memory,
                data.layout.settings_button,
                ToolButton::Settings,
                MUTED,
                data,
            );
        }
        draw_title_tool(
            memory,
            data.layout.close_button,
            ToolButton::Close,
            MUTED,
            data,
        );

        match data.view {
            View::Launcher => paint_launcher(memory, data, font, small),
            View::Settings => paint_settings(memory, data, font, small),
        }
        if data.add_overlay_open {
            paint_add_overlay(memory, data, font, small);
        }
        if data.text_overlay_title.is_some() {
            paint_text_overlay(memory, data, font);
        }
        if data.content_progress < 1.0 {
            let alpha = ((1.0 - data.content_progress) * 64.0) as u8;
            translucent_fill(
                memory,
                Rect {
                    top: data.layout.titlebar.bottom,
                    ..data.layout.client
                },
                SURFACE,
                alpha,
            );
        }
        if let Some(menu) = data.menu {
            paint_menu(memory, menu, font, data.layout.scale);
        }
        frame(memory, data.layout.client, OUTER_BORDER, 1);
        frame(memory, data.layout.client.inset(1), BORDER, 1);

        SelectObject(memory, old_font);
        DeleteObject(font as HGDIOBJ);
        DeleteObject(small as HGDIOBJ);
        BitBlt(target, 0, 0, width, height, memory, 0, 0, SRCCOPY);
        SelectObject(memory, old_bitmap);
        DeleteObject(bitmap as HGDIOBJ);
        DeleteDC(memory);
        EndPaint(hwnd, &paint);
    }
}

unsafe fn paint_launcher(hdc: HDC, data: &RenderData<'_>, font: HFONT, small: HFONT) {
    unsafe {
        fill(hdc, data.layout.categorybar, CATEGORY_BG);
        fill(
            hdc,
            Rect {
                top: data.layout.categorybar.bottom - 1,
                ..data.layout.categorybar
            },
            BORDER,
        );
    }
    for (index, rect) in &data.layout.categories {
        if *index == data.active_category {
            unsafe {
                fill(hdc, *rect, ACCENT_SOFT);
                fill(
                    hdc,
                    Rect {
                        top: rect.bottom - scaled(data.layout.scale, 2),
                        ..*rect
                    },
                    ACCENT,
                );
            }
        }
        unsafe {
            text(
                hdc,
                &elide(&data.config.categories[*index].name, 10),
                *rect,
                if *index == data.active_category {
                    TEXT
                } else {
                    MUTED
                },
                0x00000001 | 0x00000004 | 0x00000020,
            )
        };
    }
    unsafe {
        if data.layout.category_overflow {
            draw_icon(
                hdc,
                data.layout.category_left,
                ToolButton::CategoryLeft,
                MUTED,
                data.layout.scale,
            );
            draw_icon(
                hdc,
                data.layout.category_right,
                ToolButton::CategoryRight,
                MUTED,
                data.layout.scale,
            );
            draw_icon(
                hdc,
                data.layout.category_more,
                ToolButton::CategoryMore,
                MUTED,
                data.layout.scale,
            );
        }
    }

    if data.search_mode {
        unsafe {
            fill(hdc, data.layout.search_box, SURFACE);
            frame(hdc, data.layout.search_box, ACCENT, 1);
            draw_icon(
                hdc,
                data.layout.search_close_button,
                ToolButton::Close,
                MUTED,
                data.layout.scale,
            );
        }
    }

    for (local, ((item, item_icon), cell)) in
        data.items.iter().zip(data.layout.cells.iter()).enumerate()
    {
        let global = data.page_offset + local;
        let tile = cell.inset(scaled(data.layout.scale, 3));
        if data.pressed_item == Some(global) {
            unsafe { fill(hdc, tile, PRESSED) };
        } else if data.hovered_item == Some(global) {
            unsafe { fill(hdc, tile, HOVER) };
        } else if data.keyboard_selection && data.selected == Some(global) {
            unsafe { fill(hdc, tile, ACCENT_SOFT) };
        }
        let size = scaled(data.layout.scale, 30);
        let x = cell.left + (cell.width() - size) / 2;
        let y = cell.top + scaled(data.layout.scale, 8);
        if !item_icon.is_null() {
            unsafe { DrawIconEx(hdc, x, y, *item_icon, size, size, 0, null_mut(), DI_NORMAL) };
        }
        let label = Rect {
            left: cell.left + scaled(data.layout.scale, 3),
            top: y + size + scaled(data.layout.scale, 5),
            right: cell.right - scaled(data.layout.scale, 3),
            bottom: cell.bottom - scaled(data.layout.scale, 2),
        };
        unsafe {
            SelectObject(hdc, small as HGDIOBJ);
            draw_wrapped_label(hdc, &item.name, label, data.layout.scale);
            SelectObject(hdc, font as HGDIOBJ);
        }
    }

    if data.layout.scrollbar_visible
        && let Some(thumb) = data
            .layout
            .scrollbar_thumb(data.page_offset, data.item_count)
    {
        let inset = scaled(data.layout.scale, 3);
        let bar = Rect {
            left: thumb.left + inset,
            top: thumb.top,
            right: thumb.right - inset,
            bottom: thumb.bottom,
        };
        let color = if data.scrollbar_active {
            0x00736f68
        } else if data.scrollbar_hover {
            0x008b867f
        } else {
            0x00a8a39b
        };
        let radius = (bar.width() / 2).max(1);
        unsafe { rounded_fill(hdc, bar, color, radius) };
    }

    if data.items.is_empty() {
        let message = if data.search_mode && !data.search_query.is_empty() {
            "没有匹配的项目"
        } else {
            "拖入文件，或点击右上角添加"
        };
        let mut rect = data.layout.content;
        rect.top += scaled(data.layout.scale, 80);
        unsafe {
            text(
                hdc,
                message,
                rect,
                MUTED,
                0x00000001 | 0x00000004 | 0x00000020,
            )
        };
    }
}

unsafe fn paint_settings(hdc: HDC, data: &RenderData<'_>, font: HFONT, small: HFONT) {
    let descriptions = [
        ("显示快捷键", "点击后按下新的组合键"),
        ("开机启动", "登录 Windows 后在后台运行"),
        ("项目双击运行", "开启后单击选择，双击运行"),
        ("居中显示", "每次在当前屏幕居中，默认开启"),
        ("允许移动", "关闭后锁定位置；居中时不可用"),
        ("允许调整大小", "使用窗口边缘改变启动板尺寸"),
    ];
    for ((control, row, target), (title, detail)) in
        data.layout.settings_rows.iter().zip(descriptions)
    {
        unsafe {
            fill(hdc, *row, SURFACE);
            fill(
                hdc,
                Rect {
                    top: row.bottom - 1,
                    left: row.left + scaled(data.layout.scale, 12),
                    ..*row
                },
                BORDER,
            );
            text(
                hdc,
                title,
                Rect {
                    left: row.left + scaled(data.layout.scale, 16),
                    top: row.top + scaled(data.layout.scale, 12),
                    right: target.left - scaled(data.layout.scale, 16),
                    bottom: row.top + scaled(data.layout.scale, 34),
                },
                TEXT,
                0x00000004 | 0x00000020,
            );
            SelectObject(hdc, small as HGDIOBJ);
            text(
                hdc,
                detail,
                Rect {
                    left: row.left + scaled(data.layout.scale, 16),
                    top: row.top + scaled(data.layout.scale, 33),
                    right: target.left - scaled(data.layout.scale, 16),
                    bottom: row.bottom - scaled(data.layout.scale, 6),
                },
                MUTED,
                0x00000004 | 0x00000020,
            );
            SelectObject(hdc, font as HGDIOBJ);
        }
        match control {
            SettingControl::Hotkey => unsafe {
                fill(
                    hdc,
                    *target,
                    if data.hotkey_capture {
                        ACCENT_SOFT
                    } else {
                        SOFT
                    },
                );
                frame(
                    hdc,
                    *target,
                    if data.hotkey_capture {
                        data.accent
                    } else {
                        BORDER
                    },
                    1,
                );
                let label = if data.hotkey_capture {
                    "请按组合键...".to_string()
                } else {
                    hotkey::display(&data.config.hotkey)
                };
                text(
                    hdc,
                    &label,
                    *target,
                    TEXT,
                    0x00000001 | 0x00000004 | 0x00000020,
                );
            },
            SettingControl::Startup => unsafe {
                toggle(
                    hdc,
                    *target,
                    data.startup_enabled,
                    false,
                    data.layout.scale,
                    data.accent,
                )
            },
            SettingControl::DoubleClick => unsafe {
                toggle(
                    hdc,
                    *target,
                    data.config.double_click_launch,
                    false,
                    data.layout.scale,
                    data.accent,
                )
            },
            SettingControl::Centered => unsafe {
                system_checkbox(hdc, data.hwnd, *target, data.config.window.centered, false)
            },
            SettingControl::Movable => unsafe {
                toggle(
                    hdc,
                    *target,
                    data.config.window.movable,
                    data.config.window.centered,
                    data.layout.scale,
                    data.accent,
                )
            },
            SettingControl::Resizable => unsafe {
                toggle(
                    hdc,
                    *target,
                    data.config.window.resizable,
                    false,
                    data.layout.scale,
                    data.accent,
                )
            },
        }
    }
}

/// Query the theme at the window's DPI; never stretch a nominal checkbox bitmap.
unsafe fn system_checkbox(hdc: HDC, hwnd: HWND, rect: Rect, checked: bool, disabled: bool) {
    use windows_sys::Win32::UI::Controls::{
        BP_CHECKBOX, CBS_CHECKEDDISABLED, CBS_CHECKEDNORMAL, CBS_UNCHECKEDDISABLED,
        CBS_UNCHECKEDNORMAL, CloseThemeData, DrawThemeBackground, GetThemePartSize, OpenThemeData,
        TS_TRUE,
    };
    use windows_sys::Win32::UI::HiDpi::{GetDpiForWindow, OpenThemeDataForDpi};
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
    let class = wide("Button");
    let mut theme = unsafe { OpenThemeDataForDpi(hwnd, class.as_ptr(), dpi) };
    if theme == 0 {
        theme = unsafe { OpenThemeData(hwnd, class.as_ptr()) };
    }
    let state = match (checked, disabled) {
        (true, true) => CBS_CHECKEDDISABLED,
        (true, false) => CBS_CHECKEDNORMAL,
        (false, true) => CBS_UNCHECKEDDISABLED,
        (false, false) => CBS_UNCHECKEDNORMAL,
    };
    if theme != 0 {
        let mut size = SIZE::default();
        let result = unsafe {
            GetThemePartSize(
                theme,
                hdc,
                BP_CHECKBOX,
                state,
                std::ptr::null(),
                TS_TRUE,
                &mut size,
            )
        };
        if result >= 0 && size.cx > 0 && size.cy > 0 {
            let native = centered_control(rect, size.cx, size.cy);
            let result = unsafe {
                DrawThemeBackground(theme, hdc, BP_CHECKBOX, state, &native, std::ptr::null())
            };
            unsafe { CloseThemeData(theme) };
            if result >= 0 {
                return;
            }
        } else {
            unsafe { CloseThemeData(theme) };
        }
    }
    // Classic Windows fallback retains system colors, disabled state and check mark.
    use windows_sys::Win32::Graphics::Gdi::{
        DFC_BUTTON, DFCS_BUTTONCHECK, DFCS_CHECKED, DFCS_INACTIVE, DrawFrameControl,
    };
    let size = (13 * dpi / 96) as i32;
    let mut native = centered_control(rect, size, size);
    unsafe {
        DrawFrameControl(
            hdc,
            &mut native,
            DFC_BUTTON,
            DFCS_BUTTONCHECK
                | if checked { DFCS_CHECKED } else { 0 }
                | if disabled { DFCS_INACTIVE } else { 0 },
        );
    }
}

fn centered_control(rect: Rect, width: i32, height: i32) -> RECT {
    let left = rect.left + (rect.width() - width) / 2;
    let top = rect.top + (rect.height() - height) / 2;
    RECT {
        left,
        top,
        right: left + width,
        bottom: top + height,
    }
}

unsafe fn paint_text_overlay(hdc: HDC, data: &RenderData<'_>, font: HFONT) {
    let Some(title) = data.text_overlay_title else {
        return;
    };
    unsafe {
        translucent_fill(hdc, data.layout.client, OVERLAY_SHADE, 92);
        let shadow = Rect {
            left: data.layout.text_overlay.left + scaled(data.layout.scale, 7),
            top: data.layout.text_overlay.top + scaled(data.layout.scale, 7),
            right: data.layout.text_overlay.right + scaled(data.layout.scale, 7),
            bottom: data.layout.text_overlay.bottom + scaled(data.layout.scale, 7),
        };
        fill(hdc, shadow, 0x00cbc6bf);
        fill(hdc, data.layout.text_overlay, SURFACE);
        frame(hdc, data.layout.text_overlay, OUTER_BORDER, 1);
        text(
            hdc,
            title,
            Rect {
                left: data.layout.text_overlay.left + scaled(data.layout.scale, 24),
                top: data.layout.text_overlay.top + scaled(data.layout.scale, 12),
                right: data.layout.text_overlay.right - scaled(data.layout.scale, 20),
                bottom: data.layout.text_overlay.top + scaled(data.layout.scale, 52),
            },
            TEXT,
            0x00000004 | 0x00000020,
        );
        fill(hdc, data.layout.text_input, SURFACE);
        frame(
            hdc,
            data.layout.text_input,
            if data.text_input_focused {
                ACCENT
            } else {
                BORDER
            },
            1,
        );
        SelectObject(hdc, font as HGDIOBJ);
    }
}

unsafe fn paint_menu(hdc: HDC, menu: &MenuState, font: HFONT, scale: f32) {
    unsafe fn panel(hdc: HDC, menu: &MenuState, submenu: bool, font: HFONT, scale: f32) {
        let rect = if submenu {
            let Some(rect) = menu.submenu_rect else {
                return;
            };
            rect
        } else {
            menu.root_rect
        };
        let shadow = Rect {
            left: rect.left + scaled(scale, 4),
            top: rect.top + scaled(scale, 4),
            right: rect.right + scaled(scale, 4),
            bottom: rect.bottom + scaled(scale, 4),
        };
        unsafe {
            fill(hdc, shadow, 0x00cbc6bf);
            fill(hdc, rect, SURFACE);
            frame(hdc, rect, BORDER, 1);
            SelectObject(hdc, font as HGDIOBJ);
        }
        for row in menu.rows(submenu) {
            let Some(entry) = menu.entry(submenu, row.index) else {
                continue;
            };
            if row.separator {
                let y = (row.rect.top + row.rect.bottom) / 2;
                unsafe {
                    fill(
                        hdc,
                        Rect {
                            left: row.rect.left + scaled(scale, 10),
                            top: y,
                            right: row.rect.right - scaled(scale, 10),
                            bottom: y + 1,
                        },
                        BORDER,
                    );
                }
                continue;
            }
            if menu.hovered == Some((submenu, row.index))
                || menu.selected == Some((submenu, row.index))
            {
                unsafe { fill(hdc, row.rect, HOVER) };
            }
            let color = if row.enabled { TEXT } else { MUTED };
            let label = match entry {
                MenuEntry::Command { label, .. } | MenuEntry::Submenu { label, .. } => label,
                MenuEntry::Separator => continue,
            };
            let label_rect = Rect {
                left: row.rect.left + scaled(scale, 12),
                right: row.rect.right - scaled(scale, 28),
                ..row.rect
            };
            unsafe { text(hdc, label, label_rect, color, 0x00000004 | 0x00000020) };
            if row.submenu {
                let cx = row.rect.right - scaled(scale, 14);
                let cy = (row.rect.top + row.rect.bottom) / 2;
                unsafe {
                    line(hdc, cx - 2, cy - 4, cx + 2, cy, color, 1);
                    line(hdc, cx + 2, cy, cx - 2, cy + 4, color, 1);
                }
            }
        }
    }
    unsafe {
        panel(hdc, menu, false, font, scale);
        panel(hdc, menu, true, font, scale);
    }
}

unsafe fn paint_add_overlay(hdc: HDC, data: &RenderData<'_>, font: HFONT, small: HFONT) {
    unsafe {
        translucent_fill(hdc, data.layout.client, OVERLAY_SHADE, 92);
        let shadow = Rect {
            left: data.layout.add_overlay.left + scaled(data.layout.scale, 7),
            top: data.layout.add_overlay.top + scaled(data.layout.scale, 7),
            right: data.layout.add_overlay.right + scaled(data.layout.scale, 7),
            bottom: data.layout.add_overlay.bottom + scaled(data.layout.scale, 7),
        };
        fill(hdc, shadow, 0x00cbc6bf);
        fill(hdc, data.layout.add_overlay, SURFACE);
        frame(hdc, data.layout.add_overlay, OUTER_BORDER, 1);
        text(
            hdc,
            "添加项目",
            Rect {
                left: data.layout.add_overlay.left + scaled(data.layout.scale, 22),
                top: data.layout.add_overlay.top + scaled(data.layout.scale, 10),
                right: data.layout.add_close_button.left,
                bottom: data.layout.add_overlay.top + scaled(data.layout.scale, 54),
            },
            TEXT,
            0x00000004 | 0x00000020,
        );
        draw_icon(
            hdc,
            data.layout.add_close_button,
            ToolButton::Close,
            MUTED,
            data.layout.scale,
        );
        fill(hdc, data.layout.add_file_button, ACCENT);
        fill(hdc, data.layout.add_folder_button, SOFT);
        frame(hdc, data.layout.add_folder_button, BORDER, 1);
        text(
            hdc,
            "选择文件",
            data.layout.add_file_button,
            SURFACE,
            0x00000001 | 0x00000004 | 0x00000020,
        );
        text(
            hdc,
            "选择文件夹",
            data.layout.add_folder_button,
            TEXT,
            0x00000001 | 0x00000004 | 0x00000020,
        );
        frame(hdc, data.layout.add_drop_zone, BORDER, 1);
        SelectObject(hdc, small as HGDIOBJ);
        let mut drop_text = data.layout.add_drop_zone;
        drop_text.top += scaled(data.layout.scale, 34);
        text(
            hdc,
            "也可以把程序、快捷方式、文件或文件夹拖到这里",
            drop_text,
            MUTED,
            0x00000001 | 0x00000004 | 0x00000020,
        );
        SelectObject(hdc, font as HGDIOBJ);
    }
}

unsafe fn draw_title_tool(
    hdc: HDC,
    rect: Rect,
    kind: ToolButton,
    color: COLORREF,
    data: &RenderData<'_>,
) {
    let hovered = data.hovered_tool == Some(kind);
    if hovered {
        let background = if kind == ToolButton::Close {
            0x0042_42c7
        } else {
            HOVER
        };
        unsafe { fill(hdc, rect, background) };
    }
    let icon_color = if hovered && kind == ToolButton::Close {
        SURFACE
    } else {
        color
    };
    unsafe { draw_icon(hdc, rect, kind, icon_color, data.layout.scale) };
}

unsafe fn draw_icon(hdc: HDC, rect: Rect, kind: ToolButton, color: COLORREF, scale: f32) {
    let cx = (rect.left + rect.right) / 2;
    let cy = (rect.top + rect.bottom) / 2;
    let r = scaled(scale, 7);
    match kind {
        ToolButton::Search => unsafe {
            ellipse_outline(hdc, cx - r, cy - r, cx + r, cy + r, color);
            line(
                hdc,
                cx + r - 1,
                cy + r - 1,
                cx + r + 5,
                cy + r + 5,
                color,
                1,
            );
        },
        ToolButton::AddMenu => unsafe {
            line(hdc, cx - r, cy, cx + r, cy, color, 1);
            line(hdc, cx, cy - r, cx, cy + r, color, 1);
        },
        ToolButton::Close => unsafe {
            line(hdc, cx - r, cy - r, cx + r, cy + r, color, 1);
            line(hdc, cx + r, cy - r, cx - r, cy + r, color, 1);
        },
        ToolButton::Back | ToolButton::CategoryLeft => unsafe {
            line(hdc, cx + r / 2, cy - r, cx - r / 2, cy, color, 1);
            line(hdc, cx - r / 2, cy, cx + r / 2, cy + r, color, 1);
        },
        ToolButton::CategoryRight => unsafe {
            line(hdc, cx - r / 2, cy - r, cx + r / 2, cy, color, 1);
            line(hdc, cx + r / 2, cy, cx - r / 2, cy + r, color, 1);
        },
        ToolButton::CategoryMore => unsafe {
            fill_circle(hdc, cx - r, cy, 2, color);
            fill_circle(hdc, cx, cy, 2, color);
            fill_circle(hdc, cx + r, cy, 2, color);
        },
        ToolButton::Settings => unsafe { draw_gear(hdc, rect, color, scale) },
    }
}

/// Draws a Windows 11 style toggle switch filling the given control rect.
unsafe fn toggle(
    hdc: HDC,
    rect: Rect,
    checked: bool,
    disabled: bool,
    scale: f32,
    accent: COLORREF,
) {
    let Some(graphics) = (unsafe { SmoothGraphics::new(hdc) }) else {
        return;
    };
    let x = rect.right as f32 - 40.0 * scale;
    let y = (rect.top + rect.bottom) as f32 / 2.0 - 10.0 * scale;
    let track = if disabled {
        0x00e6e2dd
    } else if checked {
        accent
    } else {
        0x00c9c6c1
    };
    unsafe {
        graphics.capsule(x, y, 40.0 * scale, 20.0 * scale, track);
        graphics.ellipse(
            x + if checked { 23.0 } else { 3.0 } * scale,
            y + 3.0 * scale,
            14.0 * scale,
            14.0 * scale,
            if disabled { 0x00f2f0ee } else { SURFACE },
        );
    }
}

// Windows GDI+ provides 8x8 coverage antialiasing at fractional DPI without
// a GUI runtime, a bundled dependency or scaling a low-resolution bitmap.
use windows_sys::Win32::Graphics::GdiPlus::*;

struct SmoothGraphics {
    graphics: *mut GpGraphics,
    token: usize,
}

impl SmoothGraphics {
    unsafe fn new(hdc: HDC) -> Option<Self> {
        let input = GdiplusStartupInput {
            GdiplusVersion: 1,
            ..unsafe { zeroed() }
        };
        let mut token = 0;
        if unsafe { GdiplusStartup(&mut token, &input, null_mut()) } != 0 {
            return None;
        }
        let mut graphics = null_mut();
        if unsafe { GdipCreateFromHDC(hdc, &mut graphics) } != 0 {
            unsafe { GdiplusShutdown(token) };
            return None;
        }
        unsafe {
            GdipSetSmoothingMode(graphics, SmoothingModeAntiAlias8x8);
        }
        Some(Self { graphics, token })
    }

    unsafe fn ellipse(&self, x: f32, y: f32, w: f32, h: f32, color: COLORREF) {
        let mut brush = null_mut();
        if unsafe { GdipCreateSolidFill(argb(color), &mut brush) } == 0 {
            unsafe {
                GdipFillEllipse(self.graphics, brush.cast(), x, y, w, h);
                GdipDeleteBrush(brush.cast());
            }
        }
    }

    unsafe fn capsule(&self, x: f32, y: f32, w: f32, h: f32, color: COLORREF) {
        let mut path = null_mut();
        if unsafe { GdipCreatePath(FillModeAlternate, &mut path) } != 0 {
            return;
        }
        let mut brush = null_mut();
        unsafe {
            GdipAddPathArc(path, x, y, h, h, 90.0, 180.0);
            GdipAddPathArc(path, x + w - h, y, h, h, 270.0, 180.0);
            GdipClosePathFigure(path);
            if GdipCreateSolidFill(argb(color), &mut brush) == 0 {
                GdipFillPath(self.graphics, brush.cast(), path);
                GdipDeleteBrush(brush.cast());
            }
            GdipDeletePath(path);
        }
    }
}

impl Drop for SmoothGraphics {
    fn drop(&mut self) {
        unsafe {
            GdipDeleteGraphics(self.graphics);
            GdiplusShutdown(self.token);
        }
    }
}

fn argb(color: COLORREF) -> u32 {
    0xff000000 | ((color & 0xff) << 16) | (color & 0xff00) | ((color >> 16) & 0xff)
}

// Eight flat-topped teeth, using the same polar profile as docs/app.js.
fn gear_points(cx: f32, cy: f32, scale: f32) -> Vec<PointF> {
    (0..8)
        .flat_map(|tooth| {
            [
                (-22.5_f32, 7.0_f32),
                (-13.0, 7.0),
                (-10.0, 9.0),
                (10.0, 9.0),
                (13.0, 7.0),
            ]
            .map(move |(offset, radius)| {
                let angle = (tooth as f32 * 45.0 + offset - 90.0).to_radians();
                PointF {
                    X: cx + radius * scale * angle.cos(),
                    Y: cy + radius * scale * angle.sin(),
                }
            })
        })
        .collect()
}

unsafe fn draw_gear(hdc: HDC, rect: Rect, color: COLORREF, scale: f32) {
    let Some(graphics) = (unsafe { SmoothGraphics::new(hdc) }) else {
        return;
    };
    let cx = (rect.left + rect.right) as f32 / 2.0;
    let cy = (rect.top + rect.bottom) as f32 / 2.0;
    let points = gear_points(cx, cy, scale);
    let mut path = null_mut();
    if unsafe { GdipCreatePath(FillModeAlternate, &mut path) } != 0 {
        return;
    }
    let mut pen = null_mut();
    unsafe {
        GdipAddPathPolygon(path, points.as_ptr(), points.len() as i32);
        GdipAddPathEllipse(
            path,
            cx - 3.0 * scale,
            cy - 3.0 * scale,
            6.0 * scale,
            6.0 * scale,
        );
        if GdipCreatePen1(argb(color), scale, UnitPixel, &mut pen) == 0 {
            GdipSetPenLineJoin(pen, LineJoinRound);
            GdipDrawPath(graphics.graphics, pen, path);
            GdipDeletePen(pen);
        }
        GdipDeletePath(path);
    }
}

/// Fills a rounded rectangle using GDI's own rounded-rectangle primitive,
/// which produces clean, symmetric corners at any size.
unsafe fn rounded_fill(hdc: HDC, rect: Rect, color: COLORREF, radius: i32) {
    if rect.width() <= 0 || rect.height() <= 0 {
        return;
    }
    let diameter = radius.max(1).min(rect.height() / 2).min(rect.width() / 2) * 2;
    let brush = unsafe { CreateSolidBrush(color) };
    let pen = unsafe { CreatePen(PS_SOLID, 1, color) };
    let old_brush = unsafe { SelectObject(hdc, brush as HGDIOBJ) };
    let old_pen = unsafe { SelectObject(hdc, pen as HGDIOBJ) };
    unsafe {
        RoundRect(
            hdc,
            rect.left,
            rect.top,
            rect.right,
            rect.bottom,
            diameter,
            diameter,
        );
        SelectObject(hdc, old_pen);
        SelectObject(hdc, old_brush);
        DeleteObject(pen as HGDIOBJ);
        DeleteObject(brush as HGDIOBJ);
    }
}

unsafe fn create_font(height: i32) -> HFONT {
    let face = wide("Microsoft YaHei UI");
    unsafe {
        CreateFontW(
            -height,
            0,
            0,
            0,
            FW_NORMAL as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET.into(),
            OUT_DEFAULT_PRECIS.into(),
            0,
            PROOF_QUALITY.into(),
            (DEFAULT_PITCH | FF_DONTCARE).into(),
            face.as_ptr(),
        )
    }
}

unsafe fn translucent_fill(hdc: HDC, rect: Rect, color: COLORREF, alpha: u8) {
    let source = unsafe { CreateCompatibleDC(hdc) };
    let bitmap = unsafe { CreateCompatibleBitmap(hdc, 1, 1) };
    if source.is_null() || bitmap.is_null() {
        return;
    }
    let old_bitmap = unsafe { SelectObject(source, bitmap as HGDIOBJ) };
    unsafe {
        fill(
            source,
            Rect {
                left: 0,
                top: 0,
                right: 1,
                bottom: 1,
            },
            color,
        );
        AlphaBlend(
            hdc,
            rect.left,
            rect.top,
            rect.width(),
            rect.height(),
            source,
            0,
            0,
            1,
            1,
            BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: alpha,
                AlphaFormat: 0,
            },
        );
        SelectObject(source, old_bitmap);
        DeleteObject(bitmap as HGDIOBJ);
        DeleteDC(source);
    }
}

unsafe fn fill(hdc: HDC, rect: Rect, color: COLORREF) {
    let brush = unsafe { CreateSolidBrush(color) };
    let native = RECT {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    };
    unsafe {
        FillRect(hdc, &native, brush);
        DeleteObject(brush as HGDIOBJ);
    }
}

unsafe fn frame(hdc: HDC, rect: Rect, color: COLORREF, width: i32) {
    unsafe {
        fill(
            hdc,
            Rect {
                bottom: rect.top + width,
                ..rect
            },
            color,
        );
        fill(
            hdc,
            Rect {
                top: rect.bottom - width,
                ..rect
            },
            color,
        );
        fill(
            hdc,
            Rect {
                right: rect.left + width,
                ..rect
            },
            color,
        );
        fill(
            hdc,
            Rect {
                left: rect.right - width,
                ..rect
            },
            color,
        );
    }
}

unsafe fn line(hdc: HDC, x1: i32, y1: i32, x2: i32, y2: i32, color: COLORREF, width: i32) {
    let pen = unsafe { CreatePen(PS_SOLID, width, color) };
    let old = unsafe { SelectObject(hdc, pen as HGDIOBJ) };
    unsafe {
        MoveToEx(hdc, x1, y1, null_mut());
        LineTo(hdc, x2, y2);
        SelectObject(hdc, old);
        DeleteObject(pen as HGDIOBJ);
    }
}

unsafe fn ellipse_outline(hdc: HDC, left: i32, top: i32, right: i32, bottom: i32, color: COLORREF) {
    let pen = unsafe { CreatePen(PS_SOLID, 1, color) };
    let old_pen = unsafe { SelectObject(hdc, pen as HGDIOBJ) };
    let old_brush = unsafe { SelectObject(hdc, GetStockObject(NULL_BRUSH)) };
    unsafe {
        Ellipse(hdc, left, top, right, bottom);
        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_pen);
        DeleteObject(pen as HGDIOBJ);
    }
}

unsafe fn fill_circle(hdc: HDC, x: i32, y: i32, radius: i32, color: COLORREF) {
    let brush = unsafe { CreateSolidBrush(color) };
    let old = unsafe { SelectObject(hdc, brush as HGDIOBJ) };
    unsafe {
        Ellipse(hdc, x - radius, y - radius, x + radius + 1, y + radius + 1);
        SelectObject(hdc, old);
        DeleteObject(brush as HGDIOBJ);
    }
}

unsafe fn text(hdc: HDC, value: &str, rect: Rect, color: COLORREF, flags: u32) {
    let value = wide(value);
    let mut native = RECT {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    };
    unsafe {
        SetTextColor(hdc, color);
        DrawTextW(hdc, value.as_ptr(), -1, &mut native, flags);
    }
}

fn elide(value: &str, max: usize) -> String {
    let mut chars = value.chars();
    let prefix = chars.by_ref().take(max).collect::<String>();
    if chars.next().is_some() {
        format!("{prefix}…")
    } else {
        prefix
    }
}

/// Draws a name centered on as many lines as needed (up to `MAX_LABEL_LINES`),
/// wrapping by measured pixel width so no character is ever clipped.
unsafe fn draw_wrapped_label(hdc: HDC, value: &str, rect: Rect, scale: f32) {
    const MAX_LABEL_LINES: usize = 3;
    let line_height = scaled(scale, 15);
    for (index, line) in wrap_text(hdc, value, rect.width(), MAX_LABEL_LINES)
        .into_iter()
        .enumerate()
    {
        let top = rect.top + index as i32 * line_height;
        if top + line_height > rect.bottom + line_height {
            break;
        }
        unsafe {
            text(
                hdc,
                &line,
                Rect {
                    top,
                    bottom: top + line_height,
                    ..rect
                },
                TEXT,
                0x00000001 | 0x00000004 | 0x00000020,
            );
        }
    }
}

/// Breaks `value` into lines that each fit `max_width` pixels.
///
/// Uses `GetTextExtentExPointW` so break points match what GDI renders.
/// Prefers breaking after a space, then after `-`, `_`, `.`, `/`, and only
/// breaks inside a token when no separator fits. If the text still overflows
/// the last line, it is truncated with an ellipsis instead of being clipped.
fn wrap_text(hdc: HDC, value: &str, max_width: i32, max_lines: usize) -> Vec<String> {
    if value.is_empty() {
        return Vec::new();
    }
    if max_width <= 0 {
        return vec![value.to_string()];
    }
    let mut remaining: Vec<u16> = value.encode_utf16().collect();
    let mut lines: Vec<String> = Vec::new();
    while !remaining.is_empty() && lines.len() < max_lines {
        let fit = unsafe { text_fit(hdc, &remaining, max_width) }.max(1) as usize;
        let mut take = fit.min(remaining.len());
        if take < remaining.len() && is_high_surrogate(remaining[take - 1]) {
            take = take.saturating_sub(1).max(1);
        }
        if take < remaining.len()
            && let Some(separator) = remaining[..take]
                .iter()
                .rposition(|unit| is_break_character(*unit))
            && separator > 0
        {
            take = separator + 1;
        }
        let line = String::from_utf16_lossy(&remaining[..take]);
        lines.push(line.trim_end().to_string());
        remaining.drain(..take);
        while remaining.first().is_some_and(|unit| *unit == b' ' as u16) {
            remaining.remove(0);
        }
    }
    if !remaining.is_empty()
        && let Some(last) = lines.last_mut()
    {
        truncate_with_ellipsis(hdc, last, remaining, max_width);
    }
    lines
}

/// Fits `last` plus as much of `overflow` as possible, ending with an ellipsis.
fn truncate_with_ellipsis(hdc: HDC, last: &mut String, overflow: Vec<u16>, max_width: i32) {
    let mut combined: Vec<u16> = last.encode_utf16().collect();
    combined.extend_from_slice(&overflow);
    let ellipsis: Vec<u16> = "…".encode_utf16().collect();
    let budget = max_width.max(1);
    let fit = unsafe { text_fit(hdc, &combined, budget) }.max(0) as usize;
    let mut take = fit.min(combined.len());
    while take > 0 {
        let mut candidate = combined[..take].to_vec();
        candidate.extend_from_slice(&ellipsis);
        if unsafe { text_fit(hdc, &candidate, budget) } as usize >= candidate.len() {
            *last = String::from_utf16_lossy(&candidate);
            return;
        }
        take -= 1;
    }
    *last = "…".to_string();
}

fn is_break_character(unit: u16) -> bool {
    matches!(unit, 0x20 | 0x2D | 0x5F | 0x2E | 0x2F | 0x5C | 0x2B | 0x28)
}

/// Number of UTF-16 units that fit inside `max_width` at the current font.
unsafe fn text_fit(hdc: HDC, units: &[u16], max_width: i32) -> i32 {
    let mut fit = 0i32;
    let mut size = SIZE { cx: 0, cy: 0 };
    let ok = unsafe {
        GetTextExtentExPointW(
            hdc,
            units.as_ptr(),
            units.len() as i32,
            max_width,
            &mut fit,
            std::ptr::null_mut(),
            &mut size,
        )
    };
    if ok == 0 { units.len() as i32 } else { fit }
}

fn is_high_surrogate(unit: u16) -> bool {
    (0xD800..0xDC00).contains(&unit)
}

fn scaled(scale: f32, value: i32) -> i32 {
    (scale * value as f32).round() as i32
}

#[cfg(test)]
mod tests {
    use super::is_break_character;

    #[test]
    fn separators_are_preferred_break_points() {
        for unit in *b" -_./\\+(" {
            assert!(is_break_character(unit as u16));
        }
    }

    #[test]
    fn cjk_and_letters_are_not_break_points() {
        assert!(!is_break_character('中' as u16));
        assert!(!is_break_character('A' as u16));
        assert!(!is_break_character('9' as u16));
        assert!(!is_break_character('a' as u16));
    }
}

#[cfg(test)]
mod control_tests {
    use super::*;

    #[test]
    fn gear_has_eight_distinct_teeth_and_scales_symmetrically() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let points = gear_points(20.0 * scale, 20.0 * scale, scale);
            assert_eq!(points.len(), 40);
            let mut tips = 0;
            for (i, p) in points.iter().enumerate() {
                let radius =
                    ((p.X - 20.0 * scale).powi(2) + (p.Y - 20.0 * scale).powi(2)).sqrt() / scale;
                assert!((7.0 - 0.001..=9.0 + 0.001).contains(&radius));
                if radius > 8.0 {
                    tips += 1;
                }
                let opposite = &points[(i + 20) % 40];
                assert!((p.X + opposite.X - 40.0 * scale).abs() < 0.001);
                assert!((p.Y + opposite.Y - 40.0 * scale).abs() < 0.001);
            }
            assert_eq!(tips, 16); // Two outer corners per flat tooth.
        }
    }

    #[test]
    fn themed_checkbox_preserves_true_size_and_control_center() {
        for dpi in [96, 120, 144, 192] {
            let rect = Rect {
                left: 0,
                top: 0,
                right: 40 * dpi / 96,
                bottom: 20 * dpi / 96,
            };
            let size = 13 * dpi / 96;
            let native = centered_control(rect, size, size);
            assert_eq!(native.right - native.left, size);
            assert_eq!(native.bottom - native.top, size);
            assert!((native.left + native.right - rect.width()).abs() <= 1);
            assert!((native.top + native.bottom - rect.height()).abs() <= 1);
        }
    }
}
