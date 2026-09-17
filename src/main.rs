#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod brand;
mod config;
mod config_writer;
mod hotkey;
mod icon_loader;
mod item_view;
mod layout;
mod menu;
mod render;
mod search;
mod shell;
mod startup;
mod text_input;
mod theme;
mod tray;
mod win;

use app::App;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread;
use std::time::Duration;
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HWND, LPARAM, LRESULT, RECT, WPARAM,
};
use windows_sys::Win32::Graphics::Dwm::{
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUNDSMALL, DwmSetWindowAttribute,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::Controls::WM_MOUSELEAVE;
use windows_sys::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow, SetProcessDpiAwarenessContext,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetFocus, IsWindowEnabled, MOD_NOREPEAT, RegisterHotKey, TME_LEAVE, TRACKMOUSEEVENT,
    TrackMouseEvent, VK_CONTROL, VK_DELETE, VK_DOWN, VK_ESCAPE, VK_LEFT, VK_RETURN, VK_RIGHT,
    VK_UP,
};
use windows_sys::Win32::UI::Shell::{DragAcceptFiles, HDROP};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, CS_DBLCLKS, CS_DROPSHADOW, CS_HREDRAW, CS_VREDRAW, CreateWindowExW,
    DefWindowProcW, DestroyWindow, DispatchMessageW, EN_CHANGE, FindWindowW, GetMessageW,
    GetWindowLongPtrW, GetWindowRect, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCAPTION, HTCLIENT,
    HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT, IDC_ARROW, IDCANCEL, IDOK, LoadCursorW,
    MINMAXINFO, MSG, PostMessageW, PostQuitMessage, RegisterClassExW, RegisterWindowMessageW,
    SW_HIDE, SetWindowLongPtrW, ShowWindow, TranslateMessage, WM_ACTIVATE, WM_APP, WM_CHAR,
    WM_CLOSE, WM_COMMAND, WM_CREATE, WM_DESTROY, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_DROPFILES,
    WM_EXITSIZEMOVE, WM_GETMINMAXINFO, WM_HOTKEY, WM_KEYDOWN, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCCALCSIZE, WM_NCCREATE, WM_NCHITTEST, WM_PAINT,
    WM_QUERYENDSESSION, WM_RBUTTONDOWN, WM_SIZE, WM_SYSKEYDOWN, WM_TIMER, WNDCLASSEXW,
    WS_EX_ACCEPTFILES, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_THICKFRAME,
};

const WINDOW_CLASS: &str = "QuickLaunchNativeWindow";
const WINDOW_TITLE: &str = "KRun";
const MUTEX_NAME: &str = "Local\\QuickLaunchNative-193D9D5A";
const SHOW_MESSAGE: u32 = WM_APP + 2;
static TASKBAR_MESSAGE: AtomicU32 = AtomicU32::new(0);

fn main() {
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    let mutex_name = win::wide(MUTEX_NAME);
    let mutex = unsafe { CreateMutexW(null(), 0, mutex_name.as_ptr()) };
    if mutex.is_null() {
        show_startup_error("无法创建单实例锁。");
        return;
    }
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        wake_existing();
        unsafe { CloseHandle(mutex) };
        return;
    }
    if let Err(error) = run() {
        show_startup_error(&error);
    }
    unsafe { CloseHandle(mutex) };
}

fn wake_existing() {
    let class = win::wide(WINDOW_CLASS);
    for _ in 0..20 {
        let existing = unsafe { FindWindowW(class.as_ptr(), null()) };
        if !existing.is_null() {
            unsafe { PostMessageW(existing, SHOW_MESSAGE, 0, 0) };
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn run() -> Result<(), String> {
    let config_path = config::config_path().map_err(|error| format!("配置路径无效：{error}"))?;
    let (config, persistence_enabled) = match config::load(&config_path) {
        Ok(config) => (config, true),
        Err(error) => {
            show_startup_error(&format!("{error}\n\n已使用默认配置，原文件未被覆盖。"));
            (config::Config::default(), false)
        }
    };
    let mut app = Box::new(App::new(config, config_path, persistence_enabled));

    unsafe {
        let instance = GetModuleHandleW(null());
        let class_name = win::wide(WINDOW_CLASS);
        let mut class: WNDCLASSEXW = zeroed();
        class.cbSize = size_of::<WNDCLASSEXW>() as u32;
        class.style = CS_HREDRAW | CS_VREDRAW | CS_DBLCLKS | CS_DROPSHADOW;
        class.lpfnWndProc = Some(window_proc);
        class.hInstance = instance;
        class.hIcon = crate::brand::load_icon(32);
        class.hIconSm = crate::brand::load_icon(16);
        class.hCursor = LoadCursorW(null_mut(), IDC_ARROW);
        class.hbrBackground = 6usize as *mut _;
        class.lpszClassName = class_name.as_ptr();
        if RegisterClassExW(&class) == 0 {
            return Err(format!("注册窗口失败（系统错误 {}）。", GetLastError()));
        }

        let taskbar = RegisterWindowMessageW(win::wide("TaskbarCreated").as_ptr());
        TASKBAR_MESSAGE.store(taskbar, Ordering::Relaxed);
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_ACCEPTFILES | WS_EX_TOOLWINDOW,
            class_name.as_ptr(),
            win::wide(WINDOW_TITLE).as_ptr(),
            WS_POPUP | WS_THICKFRAME,
            0,
            0,
            app.config.window.width,
            app.config.window.height,
            null_mut(),
            null_mut(),
            instance,
            app.as_mut() as *mut App as *const _,
        );
        if hwnd.is_null() {
            return Err(format!("创建窗口失败（系统错误 {}）。", GetLastError()));
        }
        let corner = DWMWCP_ROUNDSMALL;
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            (&corner as *const i32).cast(),
            size_of_val(&corner) as u32,
        );
        if RegisterHotKey(
            hwnd,
            app::PRIMARY_HOTKEY_ID,
            app.config.hotkey.modifiers | MOD_NOREPEAT,
            app.config.hotkey.key,
        ) == 0
        {
            app.error(&format!(
                "全局快捷键 {} 已被其他程序占用。仍可通过通知区域图标打开。",
                hotkey::display(&app.config.hotkey)
            ));
        }
        ShowWindow(hwnd, SW_HIDE);

        let mut message: MSG = zeroed();
        loop {
            let result = GetMessageW(&mut message, null_mut(), 0, 0);
            if result == 0 {
                break;
            }
            if result == -1 {
                return Err("读取 Windows 消息失败。".into());
            }
            if app.menu_open()
                && message.hwnd == app.text_input_hwnd()
                && message.message == WM_CHAR
            {
                continue;
            }
            if message.hwnd == app.text_input_hwnd()
                && (message.message == WM_KEYDOWN || message.message == WM_SYSKEYDOWN)
            {
                if app.menu_open() && app.menu_key(message.wParam as u32) {
                    continue;
                }
                if handle_edit_key(&mut app, message.wParam as u32) {
                    continue;
                }
            }
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    Ok(())
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let create = lparam as *const CREATESTRUCTW;
        let app = unsafe { (*create).lpCreateParams as *mut App };
        unsafe { SetWindowLongPtrW(hwnd, -21, app as isize) };
        return 1;
    }
    let app = unsafe { GetWindowLongPtrW(hwnd, -21) as *mut App };
    if app.is_null() {
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    }
    let app = unsafe { &mut *app };

    if message == crate::text_input::FOCUS_CHANGED_MESSAGE {
        app.redraw_input_overlay();
        return 0;
    }
    if message == crate::icon_loader::ICON_READY_MESSAGE {
        app.icons_ready();
        return 0;
    }
    if message == TASKBAR_MESSAGE.load(Ordering::Relaxed) {
        app.restore_tray();
        return 0;
    }

    match message {
        WM_CREATE => {
            unsafe { DragAcceptFiles(hwnd, 1) };
            if let Err(error) = app.attach(hwnd) {
                app.error(&error);
                return -1;
            }
            0
        }
        WM_NCCALCSIZE if wparam != 0 => 0,
        WM_NCHITTEST => hit_test(hwnd, app, lparam),
        WM_GETMINMAXINFO => {
            let info = lparam as *mut MINMAXINFO;
            if !info.is_null() {
                let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96) as i32;
                unsafe {
                    (*info).ptMinTrackSize.x = 620 * dpi / 96;
                    (*info).ptMinTrackSize.y = 400 * dpi / 96;
                }
            }
            0
        }
        WM_PAINT => {
            app.paint();
            0
        }
        WM_HOTKEY if wparam == app.active_hotkey_id as usize => {
            app.toggle();
            0
        }
        SHOW_MESSAGE => {
            app.show();
            0
        }
        WM_LBUTTONDOWN => {
            let (x, y) = app::client_point(lparam);
            app.mouse_down(x, y);
            0
        }
        WM_LBUTTONUP => {
            app.mouse_up();
            0
        }
        WM_LBUTTONDBLCLK => {
            let (x, y) = app::client_point(lparam);
            app.double_click(x, y);
            0
        }
        WM_RBUTTONDOWN => {
            let (x, y) = app::client_point(lparam);
            app.right_click(x, y);
            0
        }
        WM_MOUSEMOVE => {
            let (x, y) = app::client_point(lparam);
            if app.dragging_scrollbar {
                app.mouse_drag(y);
            } else {
                app.mouse_move(x, y);
            }
            let mut track = TRACKMOUSEEVENT {
                cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            unsafe { TrackMouseEvent(&mut track) };
            0
        }
        WM_MOUSELEAVE => {
            app.mouse_leave();
            0
        }
        WM_MOUSEWHEEL => {
            app.wheel(((wparam >> 16) as i16) as i32);
            0
        }
        WM_DROPFILES => {
            app.add_dropped(wparam as HDROP);
            0
        }
        WM_CHAR => {
            if unsafe { GetFocus() } != app.text_input_hwnd()
                && let Some(character) = char::from_u32(wparam as u32)
            {
                app.character(character);
            }
            0
        }
        WM_COMMAND => {
            let control = wparam & 0xffff;
            let notification = (wparam >> 16) as u32;
            if control == crate::text_input::EDIT_ID && notification == EN_CHANGE {
                app.text_changed();
            } else if control == IDOK as usize {
                app.confirm_text_input();
            } else if control == IDCANCEL as usize {
                if matches!(app.input_mode, Some(crate::app::InputMode::Search)) {
                    app.close_search();
                } else {
                    app.cancel_text_input();
                }
            }
            0
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            handle_key(app, wparam as u32);
            0
        }
        WM_TIMER if wparam == app::FADE_TIMER_ID => {
            app.animation_tick();
            0
        }
        WM_TIMER if wparam == app::CONTENT_TIMER_ID => {
            app.content_animation_tick();
            0
        }
        WM_TIMER if wparam == app::SAVE_TIMER_ID => {
            app.save_timer_tick();
            0
        }
        crate::config_writer::SAVE_ERROR_MESSAGE => {
            app.report_save_error();
            0
        }
        WM_SIZE => {
            app.dismiss_menu();
            app.relayout();
            0
        }
        WM_EXITSIZEMOVE => {
            app.store_window_placement();
            0
        }
        WM_DPICHANGED => {
            app.dismiss_menu();
            app.dpi_changed(((wparam >> 16) & 0xffff) as u32);
            app::apply_dpi_rect(hwnd, lparam as *const RECT);
            app.relayout();
            0
        }
        WM_DISPLAYCHANGE => {
            app.dismiss_menu();
            app.relayout();
            0
        }
        WM_ACTIVATE if (wparam & 0xffff) == 0 => {
            app.dismiss_menu();
            if unsafe { IsWindowEnabled(hwnd) } != 0
                && !app::should_stay_visible()
                && !app.ignore_immediate_deactivate()
                && !app.modal_open
                && !app.exiting
            {
                app.hide();
            }
            0
        }
        tray::TRAY_MESSAGE => {
            app.tray_event(tray::event_from_lparam(lparam));
            0
        }
        WM_QUERYENDSESSION => {
            // Windows may terminate the process after logoff/shutdown without
            // delivering WM_DESTROY, so persist while the session still exists.
            app.store_window_placement();
            app.save_for_session_end();
            1
        }
        WM_CLOSE => {
            if app.exiting {
                unsafe { DestroyWindow(hwnd) };
            } else {
                app.hide();
            }
            0
        }
        WM_DESTROY => {
            app.unregister_hotkey();
            app.flush_save();
            unsafe {
                SetWindowLongPtrW(hwnd, -21, 0);
                PostQuitMessage(0);
            }
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

fn handle_edit_key(app: &mut App, key: u32) -> bool {
    if key == VK_ESCAPE as u32 {
        if matches!(app.input_mode, Some(crate::app::InputMode::Search)) {
            app.close_search();
        } else {
            app.cancel_text_input();
        }
        true
    } else if key == VK_RETURN as u32 {
        app.confirm_text_input();
        true
    } else if key == VK_UP as u32 {
        app.move_selection(-(app.layout.columns as isize));
        true
    } else if key == VK_DOWN as u32 {
        app.move_selection(app.layout.columns as isize);
        true
    } else {
        false
    }
}

fn handle_key(app: &mut App, key: u32) {
    if app.menu_key(key) {
        return;
    }
    let editing = unsafe { GetFocus() } == app.text_input_hwnd();
    if editing {
        if key == VK_ESCAPE as u32 {
            if matches!(app.input_mode, Some(crate::app::InputMode::Search)) {
                app.close_search();
            } else {
                app.cancel_text_input();
            }
        } else if key == VK_RETURN as u32 {
            app.confirm_text_input();
        } else if key == VK_UP as u32 {
            app.move_selection(-(app.layout.columns as isize));
        } else if key == VK_DOWN as u32 {
            app.move_selection(app.layout.columns as isize);
        }
        return;
    }
    if app.hotkey_capture {
        app.capture_hotkey(key);
    } else if key == b'F' as u32 && key_down(VK_CONTROL as i32) {
        app.enter_search();
    } else if key == VK_ESCAPE as u32 {
        app.escape();
    } else if key == VK_RETURN as u32 {
        app.run_selected();
    } else if key == VK_DELETE as u32 {
        app.delete_selected();
    } else if key == VK_LEFT as u32 {
        app.move_selection(-1);
    } else if key == VK_RIGHT as u32 {
        app.move_selection(1);
    } else if key == VK_UP as u32 {
        app.move_selection(-(app.layout.columns as isize));
    } else if key == VK_DOWN as u32 {
        app.move_selection(app.layout.columns as isize);
    }
}

fn hit_test(hwnd: HWND, app: &App, lparam: LPARAM) -> LRESULT {
    let (x, y) = app::client_point(lparam);
    let mut window = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut window) };
    let local_x = x - window.left;
    let local_y = y - window.top;
    if app.can_resize() {
        let edge = 7 * unsafe { GetDpiForWindow(hwnd) }.max(96) as i32 / 96;
        let left = x < window.left + edge;
        let right = x >= window.right - edge;
        let top = y < window.top + edge;
        let bottom = y >= window.bottom - edge;
        match (left, right, top, bottom) {
            (true, _, true, _) => return HTTOPLEFT as isize,
            (_, true, true, _) => return HTTOPRIGHT as isize,
            (true, _, _, true) => return HTBOTTOMLEFT as isize,
            (_, true, _, true) => return HTBOTTOMRIGHT as isize,
            (true, _, _, _) => return HTLEFT as isize,
            (_, true, _, _) => return HTRIGHT as isize,
            (_, _, true, _) => return HTTOP as isize,
            (_, _, _, true) => return HTBOTTOM as isize,
            _ => {}
        }
    }
    if app.hit_test_drag(local_x, local_y) {
        HTCAPTION as isize
    } else {
        HTCLIENT as isize
    }
}

fn key_down(key: i32) -> bool {
    unsafe {
        (windows_sys::Win32::UI::Input::KeyboardAndMouse::GetKeyState(key) as u16 & 0x8000) != 0
    }
}

fn show_startup_error(message: &str) {
    let title = win::wide(WINDOW_TITLE);
    let message = win::wide(message);
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(
            null_mut(),
            message.as_ptr(),
            title.as_ptr(),
            0x00000010,
        )
    };
}
