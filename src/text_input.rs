use crate::layout::Rect;
use crate::win::wide;
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
    GetWindowTextLengthW, GetWindowTextW, HMENU, IDCANCEL, IDOK, SW_HIDE, SW_SHOW, SendMessageW,
    SetWindowPos, SetWindowTextW, ShowWindow, WINDOW_EX_STYLE, WM_COMMAND, WM_KEYDOWN, WM_SETFONT,
    WM_SYSKEYDOWN, WS_CHILD, WS_TABSTOP, WS_VISIBLE,
};

pub const EDIT_ID: usize = 7001;
const SUBCLASS_ID: usize = 1;

pub struct TextInput {
    pub hwnd: HWND,
    confirm: HWND,
    cancel: HWND,
    font: HFONT,
    search_font: HFONT,
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
            font,
            search_font,
        })
    }

    pub fn show(&self, rect: Rect, value: &str, buttons: Option<(Rect, Rect)>) {
        let value = wide(value);
        let font = if buttons.is_some() {
            self.font
        } else {
            self.search_font
        };
        unsafe {
            SetWindowTextW(self.hwnd, value.as_ptr());
            SendMessageW(self.hwnd, WM_SETFONT, font as usize, 1);
            self.place(rect);
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

    pub fn set_rect(&self, rect: Rect) {
        self.place(rect);
    }

    fn place(&self, rect: Rect) {
        let inset = 8;
        unsafe {
            SetWindowPos(
                self.hwnd,
                null_mut(),
                rect.left + inset,
                rect.top + 3,
                (rect.width() - inset * 2).max(1),
                (rect.height() - 6).max(1),
                0x0004,
            );
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
        if !self.font.is_null() {
            unsafe { DeleteObject(self.font as HGDIOBJ) };
        }
        if !self.search_font.is_null() {
            unsafe { DeleteObject(self.search_font as HGDIOBJ) };
        }
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
