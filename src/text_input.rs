use crate::layout::Rect;
use crate::win::wide;
use std::cell::Cell;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Graphics::Gdi::{
    CreateFontW, DEFAULT_CHARSET, DEFAULT_PITCH, DeleteObject, FF_DONTCARE, FW_NORMAL, HFONT,
    HGDIOBJ, OUT_DEFAULT_PRECIS, PROOF_QUALITY,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows_sys::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    BS_DEFPUSHBUTTON, BS_PUSHBUTTON, CreateWindowExW, ES_AUTOHSCROLL, GetParent,
    GetWindowTextLengthW, GetWindowTextW, HMENU, IDCANCEL, IDOK, PostMessageW, SW_HIDE, SW_SHOW,
    SendMessageW, SetWindowPos, SetWindowTextW, ShowWindow, WINDOW_EX_STYLE, WM_APP, WM_COMMAND,
    WM_KEYDOWN, WM_KILLFOCUS, WM_SETFOCUS, WM_SETFONT, WM_SYSKEYDOWN, WS_CHILD, WS_TABSTOP,
    WS_VISIBLE,
};

pub const EDIT_ID: usize = 7001;
pub const FOCUS_CHANGED_MESSAGE: u32 = WM_APP + 7;
const SUBCLASS_ID: usize = 1;
const EM_SETMARGINS: u32 = 0x00D3;
const EC_LEFTMARGIN: usize = 0x0001;
const EC_RIGHTMARGIN: usize = 0x0002;

pub struct TextInput {
    pub hwnd: HWND,
    confirm: HWND,
    cancel: HWND,
    font: Cell<HFONT>,
    search_font: Cell<HFONT>,
    dpi: Cell<u32>,
    dialog_mode: Cell<bool>,
}

impl TextInput {
    pub fn create(parent: HWND, dpi: u32) -> Result<Self, String> {
        let class = wide("EDIT");
        let empty = wide("");
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class.as_ptr(),
                empty.as_ptr(),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | ES_AUTOHSCROLL as u32,
                0,
                0,
                10,
                10,
                parent,
                EDIT_ID as HMENU,
                GetModuleHandleW(null()),
                null(),
            )
        };
        if hwnd.is_null() {
            return Err("无法创建文本输入框".into());
        }
        let confirm = create_button(parent, IDOK as usize, "确定", true)?;
        let cancel = create_button(parent, IDCANCEL as usize, "取消", false)?;
        let font = create_font(dpi, 14);
        let search_font = create_font(dpi, 16);
        unsafe {
            SetWindowSubclass(hwnd, Some(edit_proc), SUBCLASS_ID, 0);
            SendMessageW(hwnd, WM_SETFONT, font as usize, 1);
            SendMessageW(confirm, WM_SETFONT, font as usize, 1);
            SendMessageW(cancel, WM_SETFONT, font as usize, 1);
            ShowWindow(hwnd, SW_HIDE);
            ShowWindow(confirm, SW_HIDE);
            ShowWindow(cancel, SW_HIDE);
        }
        Ok(Self {
            hwnd,
            confirm,
            cancel,
            font: Cell::new(font),
            search_font: Cell::new(search_font),
            dpi: Cell::new(dpi.max(96)),
            dialog_mode: Cell::new(false),
        })
    }

    pub fn show(&self, rect: Rect, value: &str, buttons: Option<(Rect, Rect)>) {
        let value = wide(value);
        let dialog = buttons.is_some();
        self.dialog_mode.set(dialog);
        let font = if dialog {
            self.font.get()
        } else {
            self.search_font.get()
        };
        unsafe {
            SetWindowTextW(self.hwnd, value.as_ptr());
            SendMessageW(self.hwnd, WM_SETFONT, font as usize, 1);
            self.place(rect, dialog);
            ShowWindow(self.hwnd, SW_SHOW);
            SetFocus(self.hwnd);
            let length = GetWindowTextLengthW(self.hwnd) as usize;
            SendMessageW(self.hwnd, 0x00B1, length, length as isize);
            if let Some((confirm, cancel)) = buttons {
                place_button(self.confirm, confirm);
                place_button(self.cancel, cancel);
                ShowWindow(self.confirm, SW_SHOW);
                ShowWindow(self.cancel, SW_SHOW);
            } else {
                ShowWindow(self.confirm, SW_HIDE);
                ShowWindow(self.cancel, SW_HIDE);
            }
        }
    }

    pub fn hide(&self) {
        unsafe {
            ShowWindow(self.hwnd, SW_HIDE);
            ShowWindow(self.confirm, SW_HIDE);
            ShowWindow(self.cancel, SW_HIDE);
        };
    }

    pub fn set_rect(&self, rect: Rect, dialog: bool) {
        self.place(rect, dialog);
    }

    fn place(&self, rect: Rect, dialog: bool) {
        let geometry = input_geometry(rect, self.dpi.get(), dialog);
        unsafe {
            SetWindowPos(
                self.hwnd,
                null_mut(),
                geometry.rect.left,
                geometry.rect.top,
                geometry.rect.width(),
                geometry.rect.height(),
                0x0004,
            );
            SendMessageW(
                self.hwnd,
                EM_SETMARGINS,
                EC_LEFTMARGIN | EC_RIGHTMARGIN,
                geometry.margin_param,
            );
        }
    }

    pub fn update_dpi(&self, dpi: u32) {
        let dpi = dpi.max(96);
        if self.dpi.replace(dpi) == dpi {
            return;
        }
        let font = create_font(dpi, 14);
        let search_font = create_font(dpi, 16);
        let old_font = self.font.replace(font);
        let old_search_font = self.search_font.replace(search_font);
        let active_font = if self.dialog_mode.get() {
            font
        } else {
            search_font
        };
        unsafe {
            SendMessageW(self.hwnd, WM_SETFONT, active_font as usize, 1);
            SendMessageW(self.confirm, WM_SETFONT, font as usize, 1);
            SendMessageW(self.cancel, WM_SETFONT, font as usize, 1);
            if !old_font.is_null() {
                DeleteObject(old_font as HGDIOBJ);
            }
            if !old_search_font.is_null() {
                DeleteObject(old_search_font as HGDIOBJ);
            }
        }
    }

    pub fn set_buttons(&self, confirm: Rect, cancel: Rect) {
        unsafe {
            place_button(self.confirm, confirm);
            place_button(self.cancel, cancel);
        }
    }

    pub fn text(&self) -> String {
        let length = unsafe { GetWindowTextLengthW(self.hwnd) };
        let mut buffer = vec![0u16; length as usize + 1];
        unsafe { GetWindowTextW(self.hwnd, buffer.as_mut_ptr(), buffer.len() as i32) };
        String::from_utf16_lossy(&buffer[..length as usize])
    }
}

impl Drop for TextInput {
    fn drop(&mut self) {
        let font = self.font.get();
        if !font.is_null() {
            unsafe { DeleteObject(font as HGDIOBJ) };
        }
        let search_font = self.search_font.get();
        if !search_font.is_null() {
            unsafe { DeleteObject(search_font as HGDIOBJ) };
        }
    }
}

struct InputGeometry {
    rect: Rect,
    margin_param: isize,
}

fn input_geometry(rect: Rect, dpi: u32, dialog: bool) -> InputGeometry {
    let dpi = dpi.max(96) as i32;
    let scale = |value: i32| (value * dpi + 48) / 96;
    let margin = if dialog { scale(10) } else { scale(8) }.clamp(0, u16::MAX as i32);
    let height = if dialog {
        scale(24).min(rect.height()).max(1)
    } else {
        (rect.height() - scale(6)).max(1)
    };
    let top = rect.top + (rect.height() - height) / 2;
    InputGeometry {
        rect: Rect {
            left: rect.left + 1,
            top,
            right: rect.right - 1,
            bottom: top + height,
        },
        margin_param: ((margin as u32) | ((margin as u32) << 16)) as isize,
    }
}

fn create_button(
    parent: HWND,
    id: usize,
    label: &str,
    default_button: bool,
) -> Result<HWND, String> {
    let class = wide("BUTTON");
    let label = wide(label);
    let style = if default_button {
        BS_DEFPUSHBUTTON
    } else {
        BS_PUSHBUTTON
    };
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class.as_ptr(),
            label.as_ptr(),
            WS_CHILD | WS_TABSTOP | style as u32,
            0,
            0,
            10,
            10,
            parent,
            id as HMENU,
            GetModuleHandleW(null()),
            null(),
        )
    };
    if hwnd.is_null() {
        Err("无法创建输入操作按钮".into())
    } else {
        Ok(hwnd)
    }
}

unsafe fn place_button(hwnd: HWND, rect: Rect) {
    unsafe {
        SetWindowPos(
            hwnd,
            null_mut(),
            rect.left,
            rect.top,
            rect.width(),
            rect.height(),
            0x0004,
        );
    }
}

unsafe extern "system" fn edit_proc(
    hwnd: HWND,
    message: u32,
    wparam: usize,
    lparam: isize,
    _subclass_id: usize,
    _data: usize,
) -> isize {
    if message == WM_SETFOCUS || message == WM_KILLFOCUS {
        let parent = unsafe { GetParent(hwnd) };
        unsafe { PostMessageW(parent, FOCUS_CHANGED_MESSAGE, 0, 0) };
    }
    if message == WM_KEYDOWN || message == WM_SYSKEYDOWN {
        let command = match wparam as u32 {
            13 => Some(IDOK),
            27 => Some(IDCANCEL),
            _ => None,
        };
        if let Some(command) = command {
            let parent = unsafe { GetParent(hwnd) };
            unsafe { SendMessageW(parent, WM_COMMAND, command as usize, 0) };
            return 0;
        }
    }
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

fn create_font(dpi: u32, size: i32) -> HFONT {
    let face = wide("Microsoft YaHei UI");
    let height = (size * dpi.max(96) as i32 / 96).max(12);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialog_input_is_vertically_centered_at_common_dpis() {
        for dpi in [96, 120, 144, 192] {
            let scale = |value: i32| (value * dpi + 48) / 96;
            let outer = Rect {
                left: scale(24),
                top: scale(64),
                right: scale(436),
                bottom: scale(100),
            };
            let geometry = input_geometry(outer, dpi as u32, true);
            assert_eq!(geometry.rect.height(), scale(24));
            let top_space = geometry.rect.top - outer.top;
            let bottom_space = outer.bottom - geometry.rect.bottom;
            assert!((top_space - bottom_space).abs() <= 1);
            let margin = geometry.margin_param as u32 & 0xffff;
            assert_eq!(margin, scale(10) as u32);
            assert_eq!(geometry.margin_param as u32 >> 16, margin);
        }
    }

    #[test]
    fn search_input_keeps_its_existing_height_ratio() {
        let outer = Rect {
            left: 18,
            top: 98,
            right: 822,
            bottom: 130,
        };
        let geometry = input_geometry(outer, 96, false);
        assert_eq!(geometry.rect.height(), 26);
        assert_eq!(geometry.rect.top, 101);
        assert_eq!(geometry.margin_param as u32 & 0xffff, 8);
    }
}
