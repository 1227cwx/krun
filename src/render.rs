use crate::config::{Config, LaunchItem};
use crate::hotkey;
use crate::layout::{Layout, Rect, SettingControl, ToolButton, View};
use crate::win::wide;
use std::mem::zeroed;
use std::ptr::null_mut;
use windows_sys::Win32::Foundation::{COLORREF, HWND, RECT, SIZE};
use windows_sys::Win32::Graphics::Gdi::{
    AC_SRC_OVER, AlphaBlend, BLENDFUNCTION, BeginPaint, BitBlt, CreateCompatibleBitmap,
    CreateCompatibleDC, CreateFontW, CreatePen, CreateSolidBrush, DEFAULT_CHARSET, DEFAULT_PITCH,
    DeleteDC, DeleteObject, DrawTextW, Ellipse, EndPaint, FF_DONTCARE, FW_NORMAL, FillRect,
    GetStockObject, GetTextExtentExPointW, HDC, HFONT, HGDIOBJ, LineTo, MoveToEx, NULL_BRUSH,
    OUT_DEFAULT_PRECIS, PAINTSTRUCT, PROOF_QUALITY, PS_SOLID, SRCCOPY, SelectObject, SetBkMode,
    SetTextColor, TRANSPARENT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{DI_NORMAL, DrawIconEx, GetClientRect, HICON};

const BG: COLORREF = 0x00f5f6f7;
const TITLE_BG: COLORREF = 0x00f1f3f4;
const CATEGORY_BG: COLORREF = 0x00e9edef;
const SURFACE: COLORREF = 0x00ffffff;
const SOFT: COLORREF = 0x00f6f6f6;
const BORDER: COLORREF = 0x00d7d2cb;
const OUTER_BORDER: COLORREF = 0x009f9a92;
const CONTENT_BG: COLORREF = 0x00faf9f7;
const HOVER: COLORREF = 0x00f7f3ed;
const OVERLAY_SHADE: COLORREF = 0x00e8e5e0;
const TEXT: COLORREF = 0x00302f2d;
const MUTED: COLORREF = 0x00817e78;
const ACCENT: COLORREF = 0x008b7b1d;
const ACCENT_SOFT: COLORREF = 0x00f2eee0;
const DISABLED: COLORREF = 0x00bbb8b2;

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
    pub hovered_item: Option<usize>,
    pub add_overlay_open: bool,
    pub text_overlay_title: Option<&'static str>,
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
        match data.view {
            View::Launcher => {
                fill(memory, data.layout.content.inset(1), CONTENT_BG);
                frame(memory, data.layout.content, BORDER, 1);
            }
            View::Settings => {
                fill(memory, data.layout.settings_content.inset(1), SURFACE);
                frame(memory, data.layout.settings_content, BORDER, 1);
            }
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
            draw_icon(
                memory,
                data.layout.back_button,
                ToolButton::Back,
                TEXT,
                data.layout.scale,
            );
        } else {
            draw_icon(
                memory,
                data.layout.search_button,
                ToolButton::Search,
                MUTED,
                data.layout.scale,
            );
            draw_icon(
                memory,
                data.layout.add_button,
                ToolButton::AddItem,
                MUTED,
                data.layout.scale,
            );
            draw_icon(
                memory,
                data.layout.settings_button,
                ToolButton::Settings,
                MUTED,
                data.layout.scale,
            );
        }
        draw_icon(
            memory,
            data.layout.close_button,
            ToolButton::Close,
            MUTED,
            data.layout.scale,
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
        draw_icon(
            hdc,
            data.layout.add_category_button,
            ToolButton::AddCategory,
            ACCENT,
            data.layout.scale,
        );
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
        if data.selected == Some(global) {
            unsafe { fill(hdc, tile, ACCENT_SOFT) };
        } else if data.hovered_item == Some(global) {
            unsafe { fill(hdc, tile, HOVER) };
        }
        let size = scaled(data.layout.scale, 32);
        let x = cell.left + (cell.width() - size) / 2;
        let y = cell.top + scaled(data.layout.scale, 14);
        if !item_icon.is_null() {
            unsafe { DrawIconEx(hdc, x, y, *item_icon, size, size, 0, null_mut(), DI_NORMAL) };
        }
        let label = Rect {
            left: cell.left + scaled(data.layout.scale, 3),
            top: y + size + scaled(data.layout.scale, 8),
            right: cell.right - scaled(data.layout.scale, 3),
            bottom: cell.bottom - scaled(data.layout.scale, 3),
        };
        unsafe {
            SelectObject(hdc, small as HGDIOBJ);
            draw_wrapped_label(hdc, &item.name, label, data.layout.scale);
            SelectObject(hdc, font as HGDIOBJ);
        }
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

/// Draws a native Windows themed checkbox so it matches Explorer and Settings.
unsafe fn system_checkbox(hdc: HDC, hwnd: HWND, rect: Rect, checked: bool, disabled: bool) {
    use windows_sys::Win32::UI::Controls::{
        BP_CHECKBOX, CBS_CHECKEDDISABLED, CBS_CHECKEDNORMAL, CBS_UNCHECKEDDISABLED,
        CBS_UNCHECKEDNORMAL, CloseThemeData, DrawThemeBackground, OpenThemeData,
    };
    const VSCLASS_BUTTON: &str = "Button";

    let size = (rect.height().min(rect.width())).max(1);
    let square = Rect {
        left: rect.right - size,
        top: rect.top + (rect.height() - size) / 2,
        right: rect.right,
        bottom: rect.top + (rect.height() + size) / 2,
    };
    let class = wide(VSCLASS_BUTTON);
    let theme = unsafe { OpenThemeData(hwnd, class.as_ptr()) };
    if theme == 0 {
        unsafe { fallback_checkbox(hdc, square, checked, disabled) };
        return;
    }
    let state = match (checked, disabled) {
        (true, true) => CBS_CHECKEDDISABLED,
        (true, false) => CBS_CHECKEDNORMAL,
        (false, true) => CBS_UNCHECKEDDISABLED,
        (false, false) => CBS_UNCHECKEDNORMAL,
    };
    let native = RECT {
        left: square.left,
        top: square.top,
        right: square.right,
        bottom: square.bottom,
    };
    unsafe {
        DrawThemeBackground(theme, hdc, BP_CHECKBOX, state, &native, std::ptr::null());
        CloseThemeData(theme);
    }
}

/// Fallback box used only when the system theme service is unavailable.
unsafe fn fallback_checkbox(hdc: HDC, square: Rect, checked: bool, disabled: bool) {
    let color = if disabled {
        DISABLED
    } else if checked {
        ACCENT
    } else {
        BORDER
    };
    unsafe {
        if checked {
            fill(hdc, square, color);
            let size = square.height();
            line(
                hdc,
                square.left + size / 4,
                square.top + size / 2,
                square.left + size / 2,
                square.bottom - size / 4,
                SURFACE,
                2,
            );
            line(
                hdc,
                square.left + size / 2,
                square.bottom - size / 4,
                square.right - size / 5,
                square.top + size / 4,
                SURFACE,
                2,
            );
        } else {
            fill(hdc, square, SURFACE);
            frame(hdc, square, color, 1);
        }
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
        frame(hdc, data.layout.text_input, ACCENT, 1);
        SelectObject(hdc, font as HGDIOBJ);
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
        ToolButton::AddItem | ToolButton::AddCategory => unsafe {
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
        ToolButton::Settings => unsafe {
            ellipse_outline(hdc, cx - r, cy - r, cx + r, cy + r, color);
            fill_circle(hdc, cx, cy, 2, color);
            line(hdc, cx - r - 2, cy, cx - r + 2, cy, color, 1);
            line(hdc, cx + r - 2, cy, cx + r + 2, cy, color, 1);
            line(hdc, cx, cy - r - 2, cx, cy - r + 2, color, 1);
            line(hdc, cx, cy + r - 2, cx, cy + r + 2, color, 1);
        },
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
    let width = scaled(scale, 40);
    let height = scaled(scale, 20);
    let left = rect.right - width;
    let top = rect.top + (rect.height() - height) / 2;
    let body = Rect {
        left,
        top,
        right: left + width,
        bottom: top + height,
    };
    let track = if disabled {
        0x00e6e2dd
    } else if checked {
        accent
    } else {
        0x00c9c6c1
    };
    let knob_radius = height / 2 - scaled(scale, 3);
    let center_y = (body.top + body.bottom) / 2;
    unsafe {
        rounded_fill(hdc, body, track, height / 2);
        let knob_color = if disabled { 0x00f2f0ee } else { SURFACE };
        let knob_x = if checked {
            body.right - height / 2
        } else {
            body.left + height / 2
        };
        if disabled {
            fill_circle(hdc, knob_x, center_y, knob_radius, knob_color);
        } else {
            fill_circle(hdc, knob_x, center_y, knob_radius + 1, 0x001a1a1a);
            fill_circle(hdc, knob_x, center_y, knob_radius, knob_color);
        }
    }
}

/// Fills a rounded rectangle without relying on system theme services.
unsafe fn rounded_fill(hdc: HDC, rect: Rect, color: COLORREF, radius: i32) {
    let radius = radius.max(1).min(rect.height() / 2).min(rect.width() / 2);
    let brush = unsafe { CreateSolidBrush(color) };
    let pen = unsafe { CreatePen(PS_SOLID, 1, color) };
    let old_brush = unsafe { SelectObject(hdc, brush as HGDIOBJ) };
    let old_pen = unsafe { SelectObject(hdc, pen as HGDIOBJ) };
    unsafe {
        Ellipse(
            hdc,
            rect.left,
            rect.top,
            rect.left + radius * 2 + 1,
            rect.bottom,
        );
        Ellipse(
            hdc,
            rect.right - radius * 2 - 1,
            rect.top,
            rect.right,
            rect.bottom,
        );
        fill(
            hdc,
            Rect {
                left: rect.left + radius,
                right: rect.right - radius,
                ..rect
            },
            color,
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
