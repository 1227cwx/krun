use crate::config::{self, Config, HotkeyConfig};
use crate::icon_loader::{IconKey, IconLoader};
use crate::layout::{Layout, LayoutInput, SettingControl, ToolButton, View};
use crate::text_input::TextInput;
use crate::{hotkey, render, search, shell, startup, tray, win};
use std::collections::HashMap;
use std::mem::{size_of, zeroed};
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{HWND, LPARAM, POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    GetMonitorInfoW, InvalidateRect, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
};
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN, RegisterHotKey, UnregisterHotKey,
    VK_CONTROL, VK_LWIN, VK_MENU, VK_SHIFT,
};
use windows_sys::Win32::UI::Shell::{DragFinish, DragQueryFileW, HDROP};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyIcon, DestroyMenu, GetClientRect, GetCursorPos,
    GetWindowLongPtrW, GetWindowRect, HICON, HMENU, HWND_TOPMOST, IsWindowVisible, KillTimer,
    LWA_ALPHA, MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MB_OKCANCEL, MF_POPUP, MF_SEPARATOR,
    MF_STRING, MessageBoxW, SW_HIDE, SW_SHOW, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSIZE, SWP_NOZORDER, SetForegroundWindow, SetLayeredWindowAttributes, SetTimer,
    SetWindowLongPtrW, SetWindowPos, ShowWindow, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu,
    WS_EX_LAYERED, WS_THICKFRAME,
};

const ITEM_LAUNCH: u32 = 5001;
const ITEM_RUN_AS: u32 = 5002;
const ITEM_OPEN_WITH: u32 = 5003;
const ITEM_REVEAL: u32 = 5004;
const ITEM_COPY_PATH: u32 = 5005;
const ITEM_RENAME: u32 = 5006;
const ITEM_DELETE: u32 = 5007;
const ITEM_NEW: u32 = 5008;
const ITEM_SORT: u32 = 5009;
const ITEM_MOVE_BASE: u32 = 5100;
const CATEGORY_NEW: u32 = 5201;
const CATEGORY_RENAME: u32 = 5202;
const CATEGORY_DELETE: u32 = 5203;
const CATEGORY_SELECT_BASE: u32 = 5300;
pub const PRIMARY_HOTKEY_ID: i32 = 1;
const SECONDARY_HOTKEY_ID: i32 = 2;
pub const FADE_TIMER_ID: usize = 9;
pub const CONTENT_TIMER_ID: usize = 10;
const FADE_DURATION: Duration = Duration::from_millis(110);
const CONTENT_DURATION: Duration = Duration::from_millis(130);

#[derive(Clone, Copy)]
struct Fade {
    from: u8,
    to: u8,
    started: Instant,
}

#[derive(Clone, Copy)]
pub enum InputMode {
    Search,
    NewCategory,
    RenameCategory(usize),
    RenameItem { category: usize, item: usize },
}

pub struct App {
    pub hwnd: HWND,
    pub config: Config,
    pub config_path: PathBuf,
    pub persistence_enabled: bool,
    pub dpi: u32,
    pub layout: Layout,
    pub view: View,
    pub search_mode: bool,
    pub query: String,
    pub search_results: Vec<search::ItemRef>,
    pub selected: Option<usize>,
    pub category_start: usize,
    pub page_offset: usize,
    pub startup_enabled: bool,
    pub hotkey_capture: bool,
    pub add_overlay_open: bool,
    pub modal_open: bool,
    pub hovered_item: Option<usize>,
    pub pressed_item: Option<usize>,
    /// True when the current selection was moved with the keyboard, so the
    /// persistent highlight is drawn. Mouse interaction relies on hover only.
    pub keyboard_selection: bool,
    /// True while the scrollbar thumb is being dragged.
    pub dragging_scrollbar: bool,
    pub input_mode: Option<InputMode>,
    text_input: Option<TextInput>,
    pub tray: Option<tray::TrayIcon>,
    pub exiting: bool,
    pub active_hotkey_id: i32,
    pub shown_at: Instant,
    fade: Option<Fade>,
    current_alpha: u8,
    accent: windows_sys::Win32::Foundation::COLORREF,
    content_started: Option<Instant>,
    icon_loader: Option<IconLoader>,
    icons: HashMap<IconKey, HICON>,
}

impl App {
    pub fn new(config: Config, config_path: PathBuf, persistence_enabled: bool) -> Self {
        let names = category_names(&config);
        let item_count = config.categories[config.active_category_index()]
            .items
            .len();
        let layout = Layout::calculate(LayoutInput {
            width: config.window.width,
            height: config.window.height,
            dpi: 96,
            view: View::Launcher,
            category_names: &names,
            category_start: 0,
            search_mode: false,
            item_count,
        });
        Self {
            hwnd: null_mut(),
            config,
            config_path,
            persistence_enabled,
            dpi: 96,
            layout,
            view: View::Launcher,
            search_mode: false,
            query: String::new(),
            search_results: Vec::new(),
            selected: None,
            category_start: 0,
            page_offset: 0,
            startup_enabled: false,
            hotkey_capture: false,
            add_overlay_open: false,
            modal_open: false,
            hovered_item: None,
            pressed_item: None,
            keyboard_selection: false,
            dragging_scrollbar: false,
            input_mode: None,
            text_input: None,
            tray: None,
            exiting: false,
            active_hotkey_id: PRIMARY_HOTKEY_ID,
            shown_at: Instant::now() - Duration::from_secs(1),
            fade: None,
            current_alpha: 255,
            accent: crate::theme::accent_color(),
            content_started: None,
            icon_loader: None,
            icons: HashMap::new(),
        }
    }

    pub fn attach(&mut self, hwnd: HWND) -> Result<(), String> {
        self.hwnd = hwnd;
        self.icon_loader = Some(IconLoader::new(hwnd));
        self.dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
        self.text_input = Some(TextInput::create(hwnd, self.dpi)?);
        self.startup_enabled = std::env::current_exe()
            .ok()
            .is_some_and(|path| startup::is_enabled(&path));
        self.tray = Some(tray::TrayIcon::add(
            hwnd,
            &hotkey::display(&self.config.hotkey),
        )?);
        self.apply_resize_style();
        self.relayout();
        Ok(())
    }

    pub fn relayout(&mut self) {
        let mut client = RECT::default();
        unsafe { GetClientRect(self.hwnd, &mut client) };
        let names = category_names(&self.config);
        let item_count = self.visible_refs().len();
        self.layout = Layout::calculate(LayoutInput {
            width: client.right - client.left,
            height: client.bottom - client.top,
            dpi: self.dpi,
            view: self.view,
            category_names: &names,
            category_start: self.category_start,
            search_mode: self.search_mode,
            item_count,
        });
        let previous_start = self.category_start;
        self.keep_active_category_visible();
        if self.category_start != previous_start {
            self.layout = Layout::calculate(LayoutInput {
                width: client.right - client.left,
                height: client.bottom - client.top,
                dpi: self.dpi,
                view: self.view,
                category_names: &names,
                category_start: self.category_start,
                search_mode: self.search_mode,
                item_count,
            });
        }
        self.clamp_page();
        self.update_text_input_rect();
    }

    fn update_text_input_rect(&self) {
        let Some(input) = &self.text_input else {
            return;
        };
        match self.input_mode {
            Some(InputMode::Search) => {
                let mut rect = self.layout.search_box;
                rect.right = self.layout.search_close_button.left;
                input.set_rect(rect);
            }
            Some(_) => {
                input.set_rect(self.layout.text_input);
                input.set_buttons(
                    self.layout.text_confirm_button,
                    self.layout.text_cancel_button,
                );
            }
            None => {}
        }
    }

    pub fn toggle(&mut self) {
        let hiding = self.fade.is_some_and(|fade| fade.to == 0);
        if unsafe { IsWindowVisible(self.hwnd) } != 0 && !hiding {
            self.hide();
        } else {
            self.show();
        }
    }

    pub fn show(&mut self) {
        let was_hidden = unsafe { IsWindowVisible(self.hwnd) } == 0;
        self.position_for_show();
        self.shown_at = Instant::now();
        if was_hidden {
            self.current_alpha = 0;
        }
        unsafe {
            ShowWindow(self.hwnd, SW_SHOW);
            SetForegroundWindow(self.hwnd);
        }
        self.relayout();
        self.redraw();
        self.start_fade(255);
    }

    pub fn hide(&mut self) {
        if unsafe { IsWindowVisible(self.hwnd) } == 0 {
            return;
        }
        self.reset_search_state();
        self.start_fade(0);
    }

    fn reset_search_state(&mut self) {
        if !self.search_mode {
            return;
        }
        self.search_mode = false;
        self.query.clear();
        self.search_results.clear();
        self.page_offset = 0;
        if matches!(self.input_mode, Some(InputMode::Search)) {
            self.input_mode = None;
        }
        if let Some(input) = &self.text_input {
            input.hide();
        }
        self.selected = None;
        self.relayout();
    }

    pub fn animation_tick(&mut self) {
        let Some(fade) = self.fade else {
            return;
        };
        let elapsed = fade.started.elapsed().as_secs_f32();
        let progress = (elapsed / FADE_DURATION.as_secs_f32()).clamp(0.0, 1.0);
        let alpha = fade.from as f32 + (fade.to as f32 - fade.from as f32) * progress;
        self.current_alpha = alpha.round() as u8;
        unsafe {
            SetLayeredWindowAttributes(self.hwnd, 0, self.current_alpha, LWA_ALPHA);
        }
        if progress >= 1.0 {
            self.fade = None;
            unsafe { KillTimer(self.hwnd, FADE_TIMER_ID) };
            if fade.to == 0 {
                self.store_window_placement();
                unsafe { ShowWindow(self.hwnd, SW_HIDE) };
                self.current_alpha = 0;
            } else {
                self.current_alpha = 255;
            }
            unsafe {
                SetLayeredWindowAttributes(self.hwnd, 0, 255, LWA_ALPHA);
                let ex_style = GetWindowLongPtrW(self.hwnd, -20) as u32;
                SetWindowLongPtrW(self.hwnd, -20, (ex_style & !WS_EX_LAYERED) as isize);
            }
        }
    }

    fn start_fade(&mut self, target: u8) {
        if self.current_alpha == target && self.fade.is_none() {
            if target == 0 {
                self.store_window_placement();
                unsafe { ShowWindow(self.hwnd, SW_HIDE) };
            }
            return;
        }
        let ex_style = unsafe { GetWindowLongPtrW(self.hwnd, -20) } as u32;
        unsafe {
            SetWindowLongPtrW(self.hwnd, -20, (ex_style | WS_EX_LAYERED) as isize);
            SetLayeredWindowAttributes(self.hwnd, 0, self.current_alpha, LWA_ALPHA);
            SetTimer(self.hwnd, FADE_TIMER_ID, 15, None);
        }
        self.fade = Some(Fade {
            from: self.current_alpha,
            to: target,
            started: Instant::now(),
        });
    }

    pub fn content_animation_tick(&mut self) {
        let Some(started) = self.content_started else {
            return;
        };
        if started.elapsed() >= CONTENT_DURATION {
            self.content_started = None;
            unsafe { KillTimer(self.hwnd, CONTENT_TIMER_ID) };
        }
        self.redraw();
    }

    fn start_content_transition(&mut self) {
        self.content_started = Some(Instant::now());
        unsafe { SetTimer(self.hwnd, CONTENT_TIMER_ID, 15, None) };
        self.redraw();
    }

    fn content_progress(&self) -> f32 {
        self.content_started
            .map(|started| {
                (started.elapsed().as_secs_f32() / CONTENT_DURATION.as_secs_f32()).clamp(0.0, 1.0)
            })
            .unwrap_or(1.0)
    }

    pub fn ignore_immediate_deactivate(&self) -> bool {
        self.shown_at.elapsed() < Duration::from_millis(350)
    }

    pub fn paint(&mut self) {
        let refs = self.visible_refs();
        let end = (self.page_offset + self.layout.visible_capacity).min(refs.len());
        let page = if self.page_offset < end {
            &refs[self.page_offset..end]
        } else {
            &[]
        };
        let icon_size = ((32i64 * self.dpi as i64) / 96).clamp(32, 80) as i32;
        let mut visible = Vec::with_capacity(page.len());
        for reference in page {
            let item = self.config.categories[reference.category].items[reference.item].clone();
            let icon = self.icon_for(&item.path, icon_size);
            visible.push((item, icon));
        }
        let borrowed = visible
            .iter()
            .map(|(item, icon)| (item, *icon))
            .collect::<Vec<_>>();
        let data = render::RenderData {
            config: &self.config,
            layout: &self.layout,
            view: self.view,
            items: &borrowed,
            active_category: self.config.active_category_index(),
            selected: self.selected,
            search_mode: self.search_mode,
            search_query: &self.query,
            startup_enabled: self.startup_enabled,
            hotkey_capture: self.hotkey_capture,
            page_offset: self.page_offset,
            item_count: self.visible_refs().len(),
            hovered_item: self.hovered_item,
            pressed_item: self.pressed_item,
            keyboard_selection: self.keyboard_selection,
            add_overlay_open: self.add_overlay_open,
            text_overlay_title: self.input_title(),
            content_progress: self.content_progress(),
            accent: self.accent,
            hwnd: self.hwnd,
        };
        unsafe { render::paint(self.hwnd, &data) };
    }

    pub fn mouse_down(&mut self, x: i32, y: i32) {
        if self.hit_scrollbar(x, y) {
            return;
        }
        let item = self.item_under(x, y);
        self.pressed_item = item;
        self.hovered_item = item;
        self.keyboard_selection = false;
        self.left_click(x, y);
    }

    /// Handles a press on the scrollbar track or thumb. Returns true when the
    /// press was consumed, so the item grid should not also handle it.
    fn hit_scrollbar(&mut self, x: i32, y: i32) -> bool {
        if self.view != View::Launcher || !self.layout.scrollbar_visible {
            return false;
        }
        let item_count = self.visible_refs().len();
        let Some(thumb) = self.layout.scrollbar_thumb(self.page_offset, item_count) else {
            return false;
        };
        if thumb.contains(x, y) {
            self.dragging_scrollbar = true;
            return true;
        }
        if self.layout.scrollbar_track.contains(x, y) {
            self.dragging_scrollbar = true;
            self.page_offset = self.layout.scrollbar_offset_at(y, item_count);
            self.clamp_page();
            self.redraw();
            return true;
        }
        false
    }

    pub fn mouse_up(&mut self) {
        self.dragging_scrollbar = false;
        if self.pressed_item.take().is_some() {
            self.redraw();
        }
    }

    pub fn mouse_drag(&mut self, y: i32) {
        if !self.dragging_scrollbar {
            return;
        }
        self.page_offset = self
            .layout
            .scrollbar_offset_at(y, self.visible_refs().len());
        self.clamp_page();
        self.redraw();
    }

    fn item_under(&self, x: i32, y: i32) -> Option<usize> {
        if self.view != View::Launcher || self.add_overlay_open {
            return None;
        }
        let local = self.layout.item_at(x, y)?;
        let global = self.page_offset + local;
        (global < self.visible_refs().len()).then_some(global)
    }

    pub fn left_click(&mut self, x: i32, y: i32) {
        if self.add_overlay_open {
            self.handle_add_overlay_click(x, y);
            return;
        }
        if self.input_mode.is_some() && !matches!(self.input_mode, Some(InputMode::Search)) {
            self.handle_text_overlay_click(x, y);
            return;
        }
        if let Some(button) = self.layout.tool_at(x, y, self.view) {
            self.handle_tool(button);
            return;
        }
        if self.view == View::Settings {
            if let Some(control) = self.layout.setting_at(x, y) {
                self.handle_setting(control);
            }
            return;
        }
        if self.search_mode {
            if self.layout.search_close_button.contains(x, y) {
                self.close_search();
                return;
            }
            if self.layout.search_box.contains(x, y) {
                if let Some(input) = &self.text_input {
                    unsafe {
                        windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus(input.hwnd)
                    };
                }
                return;
            }
        }
        if let Some(category) = self.layout.category_at(x, y) {
            self.select_category(category);
            return;
        }
        if let Some(local) = self.layout.item_at(x, y) {
            let global = self.page_offset + local;
            if global < self.visible_refs().len() {
                self.selected = Some(global);
                self.keyboard_selection = false;
                self.redraw();
                if should_launch_on_single_click(self.config.double_click_launch) {
                    self.run_selected();
                }
                return;
            }
        }
        self.selected = None;
        self.redraw();
    }

    pub fn double_click(&mut self, x: i32, y: i32) {
        if self.view != View::Launcher
            || self.add_overlay_open
            || (self.input_mode.is_some() && !matches!(self.input_mode, Some(InputMode::Search)))
        {
            return;
        }
        let Some(local) = self.layout.item_at(x, y) else {
            return;
        };
        let global = self.page_offset + local;
        if global < self.visible_refs().len() {
            self.selected = Some(global);
            self.keyboard_selection = false;
            self.redraw();
            if self.config.double_click_launch {
                self.run_selected();
            }
        }
    }

    pub fn mouse_move(&mut self, x: i32, y: i32) {
        let hovered = if self.view == View::Launcher
            && !self.add_overlay_open
            && (self.input_mode.is_none() || matches!(self.input_mode, Some(InputMode::Search)))
        {
            self.layout.item_at(x, y).and_then(|local| {
                let global = self.page_offset + local;
                (global < self.visible_refs().len()).then_some(global)
            })
        } else {
            None
        };
        if hovered != self.hovered_item {
            self.hovered_item = hovered;
            self.redraw();
        }
    }

    pub fn mouse_leave(&mut self) {
        let had_hover = self.hovered_item.take().is_some();
        let had_press = self.pressed_item.take().is_some();
        if had_hover || had_press {
            self.redraw();
        }
    }

    pub fn right_click(&mut self, x: i32, y: i32) {
        if self.view != View::Launcher
            || self.add_overlay_open
            || (self.input_mode.is_some() && !matches!(self.input_mode, Some(InputMode::Search)))
        {
            return;
        }
        if let Some(category) = self.layout.category_at(x, y) {
            self.show_category_menu(category);
            return;
        }
        if let Some(local) = self.layout.item_at(x, y) {
            let global = self.page_offset + local;
            if global < self.visible_refs().len() {
                self.selected = Some(global);
                self.keyboard_selection = false;
                self.show_item_menu(global);
                self.redraw();
            }
        }
    }

    pub fn wheel(&mut self, delta: i32) {
        if self.view != View::Launcher || self.layout.max_page_offset == 0 {
            return;
        }
        let step = self.layout.columns.max(1);
        if delta < 0 {
            self.page_offset = (self.page_offset + step).min(self.layout.max_page_offset);
        } else {
            self.page_offset = self.page_offset.saturating_sub(step);
        }
        self.clamp_page();
        self.redraw();
    }

    pub fn add_dropped(&mut self, drop: HDROP) {
        let count = unsafe { DragQueryFileW(drop, u32::MAX, null_mut(), 0) };
        let mut paths = Vec::new();
        for index in 0..count {
            let length = unsafe { DragQueryFileW(drop, index, null_mut(), 0) };
            let mut buffer = vec![0u16; length as usize + 1];
            let copied =
                unsafe { DragQueryFileW(drop, index, buffer.as_mut_ptr(), buffer.len() as u32) };
            if copied > 0 {
                paths.push(PathBuf::from(String::from_utf16_lossy(
                    &buffer[..copied as usize],
                )));
            }
        }
        unsafe { DragFinish(drop) };
        if !paths.is_empty() {
            self.add_overlay_open = false;
        }
        self.add_paths(paths);
    }

    pub fn input_title(&self) -> Option<&'static str> {
        match self.input_mode {
            Some(InputMode::NewCategory) => Some("新建分类"),
            Some(InputMode::RenameCategory(_)) => Some("重命名分类"),
            Some(InputMode::RenameItem { .. }) => Some("重命名项目"),
            _ => None,
        }
    }

    pub fn text_input_hwnd(&self) -> HWND {
        self.text_input
            .as_ref()
            .map(|input| input.hwnd)
            .unwrap_or(null_mut())
    }

    pub fn enter_search(&mut self) {
        if self.view != View::Launcher || self.add_overlay_open {
            return;
        }
        self.search_mode = true;
        self.input_mode = Some(InputMode::Search);
        self.query.clear();
        self.refresh_search();
        self.start_content_transition();
        if let Some(input) = &self.text_input {
            let mut rect = self.layout.search_box;
            rect.right = self.layout.search_close_button.left;
            input.show(rect, "", None);
        }
    }

    pub fn close_search(&mut self) {
        self.search_mode = false;
        self.query.clear();
        self.search_results.clear();
        self.selected = None;
        self.page_offset = 0;
        self.input_mode = None;
        if let Some(input) = &self.text_input {
            input.hide();
        }
        self.relayout();
        self.start_content_transition();
    }

    pub fn text_changed(&mut self) {
        let Some(input) = &self.text_input else {
            return;
        };
        if matches!(self.input_mode, Some(InputMode::Search)) {
            self.query = input.text();
            self.refresh_search();
        }
    }

    pub fn confirm_text_input(&mut self) {
        let Some(mode) = self.input_mode else {
            return;
        };
        if matches!(mode, InputMode::Search) {
            self.run_selected();
            return;
        }
        let value = self
            .text_input
            .as_ref()
            .map(TextInput::text)
            .unwrap_or_default();
        let value = value.trim().to_string();
        if value.is_empty() {
            return;
        }
        match mode {
            InputMode::NewCategory => {
                let id = self.config.unique_category_id(&value);
                self.config
                    .categories
                    .push(crate::config::Category::new(id.clone(), value));
                self.config.active_category = id;
                self.category_start = 0;
                self.page_offset = 0;
            }
            InputMode::RenameCategory(category) => {
                if let Some(target) = self.config.categories.get_mut(category) {
                    target.name = value;
                }
            }
            InputMode::RenameItem { category, item } => {
                if let Some(target) = self
                    .config
                    .categories
                    .get_mut(category)
                    .and_then(|category| category.items.get_mut(item))
                {
                    target.name = value;
                }
            }
            InputMode::Search => {}
        }
        self.cancel_text_input();
        self.save();
        self.relayout();
        self.redraw();
    }

    pub fn cancel_text_input(&mut self) {
        self.input_mode = None;
        if let Some(input) = &self.text_input {
            input.hide();
        }
        self.relayout();
        self.redraw();
    }

    fn open_text_input(&mut self, mode: InputMode, value: &str) {
        self.add_overlay_open = false;
        self.input_mode = Some(mode);
        self.relayout();
        self.start_content_transition();
        if let Some(input) = &self.text_input {
            input.show(
                self.layout.text_input,
                value,
                Some((
                    self.layout.text_confirm_button,
                    self.layout.text_cancel_button,
                )),
            );
        }
        self.redraw();
    }

    pub fn character(&mut self, character: char) {
        if self.view != View::Launcher || character.is_control() {
            return;
        }
        self.search_mode = true;
        self.input_mode = Some(InputMode::Search);
        self.query = character.to_string();
        self.refresh_search();
        self.start_content_transition();
        if let Some(input) = &self.text_input {
            let mut rect = self.layout.search_box;
            rect.right = self.layout.search_close_button.left;
            input.show(rect, &self.query, None);
        }
    }

    pub fn escape(&mut self) {
        if self.add_overlay_open {
            self.add_overlay_open = false;
            self.redraw();
        } else if self.hotkey_capture {
            self.hotkey_capture = false;
            self.redraw();
        } else if self.view == View::Settings {
            self.view = View::Launcher;
            self.relayout();
            self.redraw();
        } else if self.search_mode {
            self.close_search();
        } else {
            self.hide();
        }
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.view != View::Launcher {
            return;
        }
        let count = self.visible_refs().len();
        if count == 0 {
            self.selected = None;
            return;
        }
        let current = self.selected.unwrap_or(0) as isize;
        let selected = (current + delta).clamp(0, count as isize - 1) as usize;
        self.selected = Some(selected);
        self.keyboard_selection = true;
        if selected < self.page_offset {
            self.page_offset = selected;
        } else if selected >= self.page_offset + self.layout.visible_capacity {
            self.page_offset = selected + 1 - self.layout.visible_capacity;
        }
        self.redraw();
    }

    pub fn run_selected(&mut self) {
        let refs = self.visible_refs();
        let Some(reference) = self.selected.and_then(|index| refs.get(index).copied()) else {
            return;
        };
        let item = self.config.categories[reference.category].items[reference.item].clone();
        match shell::launch(self.hwnd, &item) {
            Ok(()) => self.hide(),
            Err(error) => self.error(&error),
        }
    }

    pub fn delete_selected(&mut self) {
        let refs = self.visible_refs();
        let Some(reference) = self.selected.and_then(|index| refs.get(index).copied()) else {
            return;
        };
        self.config.categories[reference.category]
            .items
            .remove(reference.item);
        self.selected = None;
        self.refresh_search();
        self.save();
    }

    pub fn capture_hotkey(&mut self, key: u32) {
        if !self.hotkey_capture || hotkey::is_modifier(key) {
            return;
        }
        let candidate = HotkeyConfig {
            modifiers: current_modifiers(),
            key,
        };
        if !hotkey::is_valid(&candidate) {
            self.error("快捷键需要包含 Ctrl、Alt、Shift 或 Win，并搭配字母、数字或 F1-F24。");
            return;
        }
        let candidate_id = if self.active_hotkey_id == PRIMARY_HOTKEY_ID {
            SECONDARY_HOTKEY_ID
        } else {
            PRIMARY_HOTKEY_ID
        };
        if unsafe {
            RegisterHotKey(
                self.hwnd,
                candidate_id,
                candidate.modifiers | hotkey::MOD_NOREPEAT_VALUE,
                candidate.key,
            )
        } == 0
        {
            self.error("这个快捷键已被其他程序占用，请换一个组合。");
            return;
        }
        unsafe { UnregisterHotKey(self.hwnd, self.active_hotkey_id) };
        self.active_hotkey_id = candidate_id;
        self.config.hotkey = candidate;
        self.hotkey_capture = false;
        if let Some(tray) = &mut self.tray {
            tray.update_tip(&hotkey::display(&self.config.hotkey));
        }
        self.save();
        self.redraw();
    }

    pub fn unregister_hotkey(&self) {
        unsafe { UnregisterHotKey(self.hwnd, self.active_hotkey_id) };
    }

    pub fn tray_event(&mut self, event: u32) {
        match event {
            0x0203 => self.toggle(),
            0x0205 => match tray::show_menu(self.hwnd) {
                tray::CMD_SHOW => self.toggle(),
                tray::CMD_OPEN_DIRECTORY => {
                    if let Some(directory) = self.config_path.parent()
                        && let Err(error) = shell::open_directory(self.hwnd, directory)
                    {
                        self.error(&error);
                    }
                }
                tray::CMD_EXIT => {
                    self.prepare_exit();
                    unsafe {
                        windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow(self.hwnd)
                    };
                }
                _ => {}
            },
            _ => {}
        }
    }

    pub fn restore_tray(&mut self) {
        if let Some(tray) = &mut self.tray {
            tray.restore(&hotkey::display(&self.config.hotkey));
        }
    }

    pub fn prepare_exit(&mut self) {
        self.store_window_placement();
        self.save();
        self.tray.take();
        self.exiting = true;
    }

    pub fn store_window_placement(&mut self) {
        let mut rect = RECT::default();
        unsafe { GetWindowRect(self.hwnd, &mut rect) };
        if rect.right <= rect.left || rect.bottom <= rect.top {
            return;
        }
        self.dpi = unsafe { GetDpiForWindow(self.hwnd) }.max(96);
        self.config.window.width = unscale(rect.right - rect.left, self.dpi).clamp(620, 1600);
        self.config.window.height = unscale(rect.bottom - rect.top, self.dpi).clamp(400, 1200);
        if !self.config.window.centered {
            self.config.window.x = rect.left;
            self.config.window.y = rect.top;
        }
        self.save();
    }

    pub fn hit_test_drag(&self, x: i32, y: i32) -> bool {
        !self.add_overlay_open
            && !self.config.window.centered
            && self.config.window.movable
            && self.layout.drag_area.contains(x, y)
    }

    pub fn can_resize(&self) -> bool {
        self.config.window.resizable
    }

    pub fn error(&self, message: &str) {
        message_box(self.hwnd, "KRun", message, MB_ICONERROR);
    }

    pub fn info(&self, message: &str) {
        message_box(self.hwnd, "KRun", message, MB_ICONINFORMATION);
    }

    fn handle_text_overlay_click(&mut self, x: i32, y: i32) {
        if self.layout.text_confirm_button.contains(x, y) {
            self.confirm_text_input();
        } else if self.layout.text_cancel_button.contains(x, y)
            || !self.layout.text_overlay.contains(x, y)
        {
            self.cancel_text_input();
        }
    }

    fn handle_add_overlay_click(&mut self, x: i32, y: i32) {
        if self.layout.add_close_button.contains(x, y) || !self.layout.add_overlay.contains(x, y) {
            self.add_overlay_open = false;
            self.redraw();
            return;
        }
        let folder_mode = if self.layout.add_file_button.contains(x, y) {
            Some(false)
        } else if self.layout.add_folder_button.contains(x, y) {
            Some(true)
        } else {
            None
        };
        if let Some(folders) = folder_mode {
            self.modal_open = true;
            let result = shell::choose_paths(self.hwnd, folders);
            self.modal_open = false;
            match result {
                Ok(paths) => {
                    if !paths.is_empty() {
                        self.add_overlay_open = false;
                        self.add_paths(paths);
                    }
                }
                Err(error) => self.error(&error),
            }
            unsafe { SetForegroundWindow(self.hwnd) };
            self.redraw();
        }
    }

    fn handle_tool(&mut self, button: ToolButton) {
        match button {
            ToolButton::Close => self.hide(),
            ToolButton::Back => {
                self.view = View::Launcher;
                self.hotkey_capture = false;
                self.relayout();
                self.start_content_transition();
            }
            ToolButton::Search => {
                if self.search_mode {
                    if let Some(input) = &self.text_input {
                        unsafe {
                            windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus(input.hwnd)
                        };
                    }
                } else {
                    self.enter_search();
                }
            }
            ToolButton::AddItem => {
                self.add_overlay_open = true;
                self.selected = None;
                self.start_content_transition();
            }
            ToolButton::Settings => {
                self.close_search();
                self.view = View::Settings;
                self.hotkey_capture = false;
                self.startup_enabled = std::env::current_exe()
                    .ok()
                    .is_some_and(|path| startup::is_enabled(&path));
                self.relayout();
                self.start_content_transition();
            }
            ToolButton::AddCategory => self.create_category(),
            ToolButton::CategoryLeft => {
                self.category_start = self.category_start.saturating_sub(1);
                self.relayout();
                self.redraw();
            }
            ToolButton::CategoryRight => {
                if self.category_start + 1 < self.config.categories.len() {
                    self.category_start += 1;
                    self.relayout();
                    self.redraw();
                }
            }
            ToolButton::CategoryMore => self.show_all_categories_menu(),
        }
    }

    fn handle_setting(&mut self, control: SettingControl) {
        match control {
            SettingControl::Hotkey => {
                self.hotkey_capture = true;
                self.redraw();
            }
            SettingControl::Startup => {
                let Some(executable) = std::env::current_exe().ok() else {
                    self.error("无法确定当前程序路径。");
                    return;
                };
                let enabled = !self.startup_enabled;
                match startup::set_enabled(&executable, enabled) {
                    Ok(()) => self.startup_enabled = enabled,
                    Err(error) => self.error(&error),
                }
                self.redraw();
            }
            SettingControl::DoubleClick => {
                self.config.double_click_launch = !self.config.double_click_launch;
                self.save();
                self.redraw();
            }
            SettingControl::Centered => {
                self.config.window.centered = !self.config.window.centered;
                if self.config.window.centered {
                    self.config.window.movable = false;
                    self.position_for_show();
                } else {
                    self.store_window_placement();
                }
                self.save();
                self.redraw();
            }
            SettingControl::Movable => {
                if !self.config.window.centered {
                    self.config.window.movable = !self.config.window.movable;
                    self.save();
                    self.redraw();
                }
            }
            SettingControl::Resizable => {
                self.config.window.resizable = !self.config.window.resizable;
                self.apply_resize_style();
                self.save();
                self.redraw();
            }
        }
    }

    fn position_for_show(&mut self) {
        let mut cursor = POINT::default();
        unsafe { GetCursorPos(&mut cursor) };
        let anchor = if self.config.window.centered {
            cursor
        } else {
            POINT {
                x: self.config.window.x,
                y: self.config.window.y,
            }
        };
        let monitor = unsafe { MonitorFromPoint(anchor, MONITOR_DEFAULTTONEAREST) };
        let mut info: MONITORINFO = unsafe { zeroed() };
        info.cbSize = size_of::<MONITORINFO>() as u32;
        unsafe { GetMonitorInfoW(monitor, &mut info) };
        let dpi = self.dpi.max(96);
        let max_width = info.rcWork.right - info.rcWork.left;
        let max_height = info.rcWork.bottom - info.rcWork.top;
        let width = scale(self.config.window.width, dpi).clamp(scale(620, dpi), max_width);
        let height = scale(self.config.window.height, dpi).clamp(scale(400, dpi), max_height);
        let (desired_x, desired_y) = if self.config.window.centered {
            (
                info.rcWork.left + (max_width - width) / 2,
                info.rcWork.top + (max_height - height) / 2,
            )
        } else {
            (self.config.window.x, self.config.window.y)
        };
        let x = desired_x.clamp(info.rcWork.left, info.rcWork.right - width);
        let y = desired_y.clamp(info.rcWork.top, info.rcWork.bottom - height);
        unsafe { SetWindowPos(self.hwnd, HWND_TOPMOST, x, y, width, height, 0) };
    }

    fn apply_resize_style(&self) {
        let current = unsafe { GetWindowLongPtrW(self.hwnd, -16) } as u32;
        let style = if self.config.window.resizable {
            current | WS_THICKFRAME
        } else {
            current & !WS_THICKFRAME
        };
        unsafe {
            SetWindowLongPtrW(self.hwnd, -16, style as isize);
            SetWindowPos(
                self.hwnd,
                null_mut(),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
        }
    }

    fn select_category(&mut self, category: usize) {
        self.view = View::Launcher;
        self.search_mode = false;
        self.query.clear();
        self.search_results.clear();
        self.selected = None;
        self.hovered_item = None;
        self.pressed_item = None;
        self.keyboard_selection = false;
        self.page_offset = 0;
        self.config.active_category = self.config.categories[category].id.clone();
        self.save();
        self.relayout();
        self.start_content_transition();
    }

    fn create_category(&mut self) {
        self.open_text_input(InputMode::NewCategory, "");
    }

    fn add_paths(&mut self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        let active = self.config.active_category_index();
        let mut added = 0usize;
        for path in paths {
            let normalized = path.to_string_lossy().to_lowercase();
            let duplicate = self.config.categories.iter().any(|category| {
                category
                    .items
                    .iter()
                    .any(|item| shell::expand_environment(&item.path).to_lowercase() == normalized)
            });
            if !duplicate {
                self.config.categories[active]
                    .items
                    .push(shell::item_from_path(path));
                added += 1;
            }
        }
        if added == 0 {
            self.info("所选项目已经存在。");
        } else {
            self.save();
        }
        self.refresh_search();
    }

    fn refresh_search(&mut self) {
        self.search_results = if self.search_mode && !self.query.trim().is_empty() {
            search::results(&self.config, &self.query)
        } else {
            Vec::new()
        };
        self.page_offset = 0;
        self.selected = (!self.visible_refs().is_empty()).then_some(0);
        self.hovered_item = None;
        self.pressed_item = None;
        self.keyboard_selection = false;
        self.relayout();
        self.redraw();
    }

    fn visible_refs(&self) -> Vec<search::ItemRef> {
        if self.search_mode {
            if self.query.trim().is_empty() {
                self.config
                    .categories
                    .iter()
                    .enumerate()
                    .flat_map(|(category, value)| {
                        (0..value.items.len()).map(move |item| search::ItemRef { category, item })
                    })
                    .collect()
            } else {
                self.search_results.clone()
            }
        } else {
            let category = self.config.active_category_index();
            (0..self.config.categories[category].items.len())
                .map(|item| search::ItemRef { category, item })
                .collect()
        }
    }

    fn clamp_page(&mut self) {
        self.page_offset = self.page_offset.min(self.layout.max_page_offset);
    }

    fn keep_active_category_visible(&mut self) {
        if self.view != View::Launcher {
            return;
        }
        if !self.layout.category_overflow {
            self.category_start = 0;
            return;
        }
        let active = self.config.active_category_index();
        let active_visible = self
            .layout
            .categories
            .iter()
            .any(|(index, _)| *index == active);
        if !active_visible {
            self.category_start = active;
        }
    }

    fn icon_for(&mut self, path: &str, size: i32) -> HICON {
        let key = IconKey {
            path: shell::expand_environment(path),
            size,
        };
        if let Some(icon) = self.icons.get(&key) {
            return *icon;
        }
        if let Some(loader) = &mut self.icon_loader {
            loader.request(key);
        }
        null_mut()
    }

    pub fn icons_ready(&mut self) {
        let Some(loader) = &mut self.icon_loader else {
            return;
        };
        for item in loader.drain() {
            let (key, icon) = item.into_parts();
            if let Some(previous) = self.icons.insert(key, icon)
                && !previous.is_null()
            {
                unsafe { DestroyIcon(previous) };
            }
        }
        self.redraw();
    }

    fn show_all_categories_menu(&mut self) {
        let menu = unsafe { CreatePopupMenu() };
        if menu.is_null() {
            return;
        }
        for (index, category) in self.config.categories.iter().enumerate() {
            append(menu, CATEGORY_SELECT_BASE + index as u32, &category.name);
        }
        let command = popup(self.hwnd, menu);
        unsafe { DestroyMenu(menu) };
        if command >= CATEGORY_SELECT_BASE {
            let index = (command - CATEGORY_SELECT_BASE) as usize;
            if index < self.config.categories.len() {
                self.category_start = index;
                self.select_category(index);
            }
        }
    }

    fn show_item_menu(&mut self, visible_index: usize) {
        let refs = self.visible_refs();
        let Some(reference) = refs.get(visible_index).copied() else {
            return;
        };
        let menu = unsafe { CreatePopupMenu() };
        let move_menu = unsafe { CreatePopupMenu() };
        if menu.is_null() || move_menu.is_null() {
            return;
        }
        append(menu, ITEM_LAUNCH, "运行");
        append(menu, ITEM_RUN_AS, "以管理员身份运行");
        append(menu, ITEM_OPEN_WITH, "打开方式...");
        unsafe { AppendMenuW(menu, MF_SEPARATOR, 0, null()) };
        append(menu, ITEM_REVEAL, "打开文件所在位置");
        append(menu, ITEM_COPY_PATH, "复制完整路径");
        unsafe { AppendMenuW(menu, MF_SEPARATOR, 0, null()) };
        for (index, category) in self.config.categories.iter().enumerate() {
            append(move_menu, ITEM_MOVE_BASE + index as u32, &category.name);
        }
        let label = win::wide("移动到分类");
        unsafe { AppendMenuW(menu, MF_POPUP, move_menu as usize, label.as_ptr()) };
        unsafe { AppendMenuW(menu, MF_SEPARATOR, 0, null()) };
        append(menu, ITEM_NEW, "新建项目");
        append(menu, ITEM_SORT, "按名称排序");
        unsafe { AppendMenuW(menu, MF_SEPARATOR, 0, null()) };
        append(menu, ITEM_RENAME, "重命名");
        append(menu, ITEM_DELETE, "删除");
        let command = popup(self.hwnd, menu);
        unsafe { DestroyMenu(menu) };
        match command {
            ITEM_LAUNCH => self.run_selected(),
            ITEM_RUN_AS => {
                let item = self.config.categories[reference.category].items[reference.item].clone();
                if let Err(error) = shell::run_as_admin(self.hwnd, &item) {
                    self.error(&error);
                } else {
                    self.hide();
                }
            }
            ITEM_OPEN_WITH => {
                let path = self.config.categories[reference.category].items[reference.item]
                    .path
                    .clone();
                if let Err(error) = shell::open_with(self.hwnd, &path) {
                    self.error(&error);
                }
            }
            ITEM_COPY_PATH => {
                let path = self.config.categories[reference.category].items[reference.item]
                    .path
                    .clone();
                if let Err(error) = shell::copy_path(self.hwnd, &path) {
                    self.error(&error);
                }
            }
            ITEM_RENAME => {
                let initial = self.config.categories[reference.category].items[reference.item]
                    .name
                    .clone();
                self.open_text_input(
                    InputMode::RenameItem {
                        category: reference.category,
                        item: reference.item,
                    },
                    &initial,
                );
            }
            ITEM_REVEAL => {
                let path = self.config.categories[reference.category].items[reference.item]
                    .path
                    .clone();
                if let Err(error) = shell::reveal(self.hwnd, &path) {
                    self.error(&error);
                }
            }
            ITEM_DELETE => self.delete_selected(),
            ITEM_NEW => {
                self.add_overlay_open = true;
                self.start_content_transition();
            }
            ITEM_SORT => {
                self.config.categories[reference.category]
                    .items
                    .sort_by_key(|item| item.name.to_lowercase());
                self.selected = None;
                self.save();
                self.start_content_transition();
            }
            command if command >= ITEM_MOVE_BASE => {
                let destination = (command - ITEM_MOVE_BASE) as usize;
                if destination < self.config.categories.len() && destination != reference.category {
                    let item = self.config.categories[reference.category]
                        .items
                        .remove(reference.item);
                    self.config.categories[destination].items.push(item);
                    self.selected = None;
                    self.save();
                    self.refresh_search();
                }
            }
            _ => {}
        }
    }

    fn show_category_menu(&mut self, category: usize) {
        let menu = unsafe { CreatePopupMenu() };
        if menu.is_null() {
            return;
        }
        append(menu, CATEGORY_NEW, "新建分类");
        append(menu, CATEGORY_RENAME, "重命名分类");
        append(menu, CATEGORY_DELETE, "删除分类");
        let command = popup(self.hwnd, menu);
        unsafe { DestroyMenu(menu) };
        match command {
            CATEGORY_NEW => self.create_category(),
            CATEGORY_RENAME => {
                let initial = self.config.categories[category].name.clone();
                self.open_text_input(InputMode::RenameCategory(category), &initial);
            }
            CATEGORY_DELETE => self.delete_category(category),
            _ => {}
        }
    }

    fn delete_category(&mut self, category: usize) {
        if self.config.categories.len() == 1 {
            self.info("至少需要保留一个分类。");
            return;
        }
        let name = self.config.categories[category].name.clone();
        if message_box_result(
            self.hwnd,
            "删除分类",
            &format!("删除分类“{name}”？其中的项目会移动到相邻分类。"),
        ) != 1
        {
            return;
        }
        let active_id = self.config.active_category.clone();
        let removed = self.config.categories.remove(category);
        let destination = category.min(self.config.categories.len() - 1);
        self.config.categories[destination]
            .items
            .extend(removed.items);
        if active_id == removed.id {
            self.config.active_category = self.config.categories[destination].id.clone();
        }
        self.category_start = self.category_start.min(self.config.categories.len() - 1);
        self.page_offset = 0;
        self.selected = None;
        self.save();
        self.relayout();
        self.redraw();
    }

    fn save(&self) {
        if self.persistence_enabled
            && let Err(error) = config::save(&self.config_path, &self.config)
        {
            self.error(&error);
        }
    }

    fn redraw(&self) {
        unsafe { InvalidateRect(self.hwnd, null(), 0) };
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.tray.take();
        for icon in self.icons.values().copied() {
            if !icon.is_null() {
                unsafe { DestroyIcon(icon) };
            }
        }
    }
}

fn should_launch_on_single_click(double_click_launch: bool) -> bool {
    !double_click_launch
}

fn category_names(config: &Config) -> Vec<String> {
    config
        .categories
        .iter()
        .map(|category| category.name.clone())
        .collect()
}

fn current_modifiers() -> u32 {
    let mut modifiers = 0;
    if key_down(VK_CONTROL as i32) {
        modifiers |= MOD_CONTROL;
    }
    if key_down(VK_MENU as i32) {
        modifiers |= MOD_ALT;
    }
    if key_down(VK_SHIFT as i32) {
        modifiers |= MOD_SHIFT;
    }
    if key_down(VK_LWIN as i32) || key_down(0x5C) {
        modifiers |= MOD_WIN;
    }
    modifiers
}

fn key_down(key: i32) -> bool {
    unsafe { (GetKeyState(key) as u16 & 0x8000) != 0 }
}

fn ctrl_down() -> bool {
    key_down(VK_CONTROL as i32)
}

fn popup(hwnd: HWND, menu: HMENU) -> u32 {
    let mut point = POINT::default();
    unsafe {
        GetCursorPos(&mut point);
        SetForegroundWindow(hwnd);
        TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            point.x,
            point.y,
            0,
            hwnd,
            null(),
        ) as u32
    }
}

fn append(menu: HMENU, command: u32, label: &str) {
    let label = win::wide(label);
    unsafe { AppendMenuW(menu, MF_STRING, command as usize, label.as_ptr()) };
}

fn message_box(hwnd: HWND, title: &str, message: &str, icon: u32) {
    let title = win::wide(title);
    let message = win::wide(message);
    unsafe { MessageBoxW(hwnd, message.as_ptr(), title.as_ptr(), MB_OK | icon) };
}

fn message_box_result(hwnd: HWND, title: &str, message: &str) -> i32 {
    let title = win::wide(title);
    let message = win::wide(message);
    unsafe {
        MessageBoxW(
            hwnd,
            message.as_ptr(),
            title.as_ptr(),
            MB_OKCANCEL | 0x00000030,
        )
    }
}

fn scale(value: i32, dpi: u32) -> i32 {
    ((value as i64 * dpi as i64) / 96) as i32
}

fn unscale(value: i32, dpi: u32) -> i32 {
    ((value as i64 * 96) / dpi.max(1) as i64) as i32
}

pub fn should_stay_visible() -> bool {
    ctrl_down()
}

pub fn client_point(lparam: LPARAM) -> (i32, i32) {
    win::point_from_lparam(lparam)
}

pub fn apply_dpi_rect(hwnd: HWND, rect: *const RECT) {
    if rect.is_null() {
        return;
    }
    let rect = unsafe { *rect };
    unsafe {
        SetWindowPos(
            hwnd,
            null_mut(),
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            SWP_NOZORDER | SWP_NOACTIVATE,
        )
    };
}

#[cfg(test)]
mod tests {
    use super::should_launch_on_single_click;

    #[test]
    fn single_click_only_launches_when_double_click_is_disabled() {
        assert!(!should_launch_on_single_click(true));
        assert!(should_launch_on_single_click(false));
    }
}
