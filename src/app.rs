use crate::config::{Config, HotkeyConfig, LaunchItem};
use crate::config_writer::ConfigWriter;
use crate::icon_loader::{IconKey, IconLoader};
use crate::item_view::Scope;
use crate::layout::{Layout, LayoutInput, SettingControl, ToolButton, View};
use crate::menu::{MenuEntry, MenuState};
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
    GetFocus, GetKeyState, MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN, RegisterHotKey,
    UnregisterHotKey, VK_CONTROL, VK_LWIN, VK_MENU, VK_SHIFT,
};
use windows_sys::Win32::UI::Shell::{DragFinish, DragQueryFileW, HDROP};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DestroyIcon, GetClientRect, GetCursorPos, GetWindowLongPtrW, GetWindowRect, HICON,
    HWND_TOPMOST, IsWindowVisible, KillTimer, LWA_ALPHA, MB_ICONERROR, MB_ICONINFORMATION, MB_OK,
    MB_OKCANCEL, MessageBoxW, SW_HIDE, SW_SHOW, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSIZE, SWP_NOZORDER, SetForegroundWindow, SetLayeredWindowAttributes, SetTimer,
    SetWindowLongPtrW, SetWindowPos, ShowWindow, WS_EX_LAYERED, WS_THICKFRAME,
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
const ITEM_CHANGE_ICON: u32 = 5010;
const ITEM_RESET_ICON: u32 = 5011;
const ITEM_MOVE_BASE: u32 = 0x1000_0000;
const CATEGORY_NEW: u32 = 5201;
const CATEGORY_SELECT_BASE: u32 = 0x2000_0000;
const CATEGORY_CONTEXT_BASE: u32 = 0x3000_0000;
pub const PRIMARY_HOTKEY_ID: i32 = 1;
const SECONDARY_HOTKEY_ID: i32 = 2;
pub const FADE_TIMER_ID: usize = 9;
pub const CONTENT_TIMER_ID: usize = 10;
pub const SAVE_TIMER_ID: usize = 11;
const FADE_DURATION: Duration = Duration::from_millis(110);
const CONTENT_DURATION: Duration = Duration::from_millis(130);
/// Writes are coalesced for this long so a burst of edits produces one file.
const SAVE_DEBOUNCE_MS: u32 = 350;

#[derive(Clone, Copy)]
struct Fade {
    from: u8,
    to: u8,
    started: Instant,
}

#[derive(Clone, Copy)]
enum CategorySwitch {
    Hover,
    Explicit,
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
    pub hovered_tool: Option<ToolButton>,
    pub menu: Option<MenuState>,
    pub scrollbar_hover: bool,
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
    /// Icons keyed by physical pixel size and expanded filesystem path.
    icons: HashMap<i32, HashMap<String, HICON>>,
    /// Expanded paths keyed by the raw value from launcher.json. This cache is
    /// populated on first use so repeated frames perform no string expansion.
    expanded_paths: HashMap<String, String>,
    writer: Option<ConfigWriter>,
    save_pending: bool,
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
            hovered_tool: None,
            menu: None,
            scrollbar_hover: false,
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
            expanded_paths: HashMap::new(),
            writer: None,
            save_pending: false,
        }
    }

    pub fn attach(&mut self, hwnd: HWND) -> Result<(), String> {
        self.hwnd = hwnd;
        self.icon_loader = Some(IconLoader::new(hwnd));
        self.writer = Some(ConfigWriter::new(hwnd));
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
        // Warm the first page while the window is still hidden so showing the
        // launcher never has to wait for shell icon lookups.
        self.preload_visible_icons();
        Ok(())
    }

    pub fn menu_open(&self) -> bool {
        self.menu.is_some()
    }

    pub fn menu_key(&mut self, key: u32) -> bool {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
            VK_DOWN, VK_ESCAPE, VK_LEFT, VK_RETURN, VK_RIGHT, VK_UP,
        };
        if self.menu.is_none() {
            return false;
        }
        if key == VK_ESCAPE as u32 {
            self.close_menu();
        } else if key == VK_UP as u32 {
            self.menu.as_mut().unwrap().select_next(-1);
            self.redraw();
        } else if key == VK_DOWN as u32 {
            self.menu.as_mut().unwrap().select_next(1);
            self.redraw();
        } else if key == VK_RIGHT as u32 {
            self.menu.as_mut().unwrap().enter_submenu();
            self.redraw();
        } else if key == VK_LEFT as u32 {
            self.menu.as_mut().unwrap().leave_submenu();
            self.redraw();
        } else if key == VK_RETURN as u32 {
            let submenu_selected = self
                .menu
                .as_ref()
                .and_then(|menu| menu.selected)
                .is_some_and(|(submenu, index)| {
                    !submenu
                        && matches!(
                            self.menu.as_ref().unwrap().entry(false, index),
                            Some(MenuEntry::Submenu { .. })
                        )
                });
            if submenu_selected {
                self.menu.as_mut().unwrap().enter_submenu();
                self.redraw();
                return true;
            }
            let (command, target) = {
                let menu = self.menu.as_ref().unwrap();
                (menu.selected_command(), menu.target)
            };
            self.menu = None;
            self.modal_open = false;
            if let Some(command) = command {
                self.execute_menu_command(command, target);
                self.restore_search_input();
            } else {
                self.restore_search_input();
                self.redraw();
            }
        }
        true
    }

    pub fn dpi_changed(&mut self, dpi: u32) {
        self.dpi = dpi.max(96);
        if let Some(input) = &self.text_input {
            input.update_dpi(self.dpi);
        }
    }

    pub fn relayout(&mut self) {
        let mut client = RECT::default();
        unsafe { GetClientRect(self.hwnd, &mut client) };
        let names = category_names(&self.config);
        let item_count = self.visible_count();
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
                input.set_rect(rect, false);
            }
            Some(_) => {
                input.set_rect(self.layout.text_input, true);
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
        self.menu = None;
        self.modal_open = false;
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
        let count = self.visible_count();
        let end = self
            .page_offset
            .saturating_add(self.layout.visible_capacity)
            .min(count);
        // Only the references for the page actually on screen are materialised.
        let mut page = Vec::new();
        self.scope().write_page(
            &self.config,
            self.page_offset,
            end.saturating_sub(self.page_offset),
            &mut page,
        );
        let icon_size = ((32i64 * self.dpi as i64) / 96).clamp(32, 80) as i32;
        let mut icons = Vec::with_capacity(page.len());
        let (config, icon_cache, expanded_paths, icon_loader) = (
            &self.config,
            &self.icons,
            &mut self.expanded_paths,
            &mut self.icon_loader,
        );
        for reference in &page {
            let Some(item) = config
                .categories
                .get(reference.category)
                .and_then(|category| category.items.get(reference.item))
            else {
                icons.push(null_mut());
                continue;
            };
            icons.push(icon_for_item(
                item,
                icon_size,
                icon_cache,
                expanded_paths,
                icon_loader.as_mut(),
            ));
        }
        let borrowed = self.borrowed_items(&page, &icons);
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
            item_count: count,
            scrollbar_hover: self.scrollbar_hover,
            scrollbar_active: self.dragging_scrollbar,
            hovered_item: self.hovered_item,
            hovered_tool: self.hovered_tool,
            pressed_item: self.pressed_item,
            keyboard_selection: self.keyboard_selection,
            add_overlay_open: self.add_overlay_open,
            text_overlay_title: self.input_title(),
            text_input_focused: self
                .text_input
                .as_ref()
                .is_some_and(|input| unsafe { GetFocus() } == input.hwnd),
            menu: self.menu.as_ref(),
            content_progress: self.content_progress(),
            accent: self.accent,
            hwnd: self.hwnd,
        };
        unsafe { render::paint(self.hwnd, &data) };
    }

    fn borrowed_items<'a>(
        &'a self,
        page: &[search::ItemRef],
        icons: &'a [HICON],
    ) -> Vec<(&'a LaunchItem, HICON)> {
        page.iter()
            .zip(icons.iter())
            .filter_map(|(reference, icon)| {
                self.item_at(reference.category, reference.item)
                    .map(|item| (item, *icon))
            })
            .collect()
    }

    /// Asks the background loader for any icon that is not cached yet.
    fn request_icons(&mut self, requested: &mut Vec<(usize, usize)>, size: i32) {
        if requested.is_empty() {
            return;
        }
        let keys = requested
            .drain(..)
            .filter_map(|(category, item)| self.item_at(category, item))
            .flat_map(|item| {
                let mut sources = vec![item.path.as_str()];
                if !item.icon_path.trim().is_empty() {
                    sources.insert(0, item.icon_path.as_str());
                }
                sources.into_iter().map(move |source| IconKey {
                    path: shell::expand_environment(source),
                    size,
                })
            })
            .collect::<Vec<_>>();
        let Some(loader) = &mut self.icon_loader else {
            return;
        };
        for key in keys {
            loader.request(key);
        }
    }

    pub fn mouse_down(&mut self, x: i32, y: i32) {
        if self.menu_click(x, y) {
            return;
        }
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
        let item_count = self.visible_count();
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
        self.page_offset = self.layout.scrollbar_offset_at(y, self.visible_count());
        self.clamp_page();
        self.redraw();
    }

    fn item_under(&self, x: i32, y: i32) -> Option<usize> {
        if self.view != View::Launcher || self.add_overlay_open {
            return None;
        }
        let local = self.layout.item_at(x, y)?;
        let global = self.page_offset + local;
        (global < self.visible_count()).then_some(global)
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
            self.select_category(category, CategorySwitch::Explicit);
            return;
        }
        if let Some(local) = self.layout.item_at(x, y) {
            let global = self.page_offset + local;
            if global < self.visible_count() {
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
        if self.menu.is_some() {
            return;
        }
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
        if global < self.visible_count() {
            self.selected = Some(global);
            self.keyboard_selection = false;
            self.redraw();
            if self.config.double_click_launch {
                self.run_selected();
            }
        }
    }

    pub fn mouse_move(&mut self, x: i32, y: i32) {
        if let Some(menu) = &mut self.menu {
            let previous = menu.hovered;
            menu.hover(x, y);
            if menu.hovered != previous {
                self.redraw();
            }
            return;
        }
        let interactions_enabled = !self.add_overlay_open
            && (self.input_mode.is_none() || matches!(self.input_mode, Some(InputMode::Search)));
        let hovered_category = (self.view == View::Launcher && interactions_enabled)
            .then(|| self.layout.category_at(x, y))
            .flatten();
        if let Some(category) = hovered_category
            && category != self.config.active_category_index()
        {
            self.select_category(category, CategorySwitch::Hover);
            return;
        }

        let hovered_tool = interactions_enabled
            .then(|| self.layout.tool_at(x, y, self.view))
            .flatten()
            .filter(|tool| {
                matches!(
                    tool,
                    ToolButton::Back
                        | ToolButton::Search
                        | ToolButton::AddMenu
                        | ToolButton::Settings
                        | ToolButton::Close
                )
            });
        let scrollbar_hover = self.scrollbar_contains(x, y);
        let hovered = if !scrollbar_hover && self.view == View::Launcher && interactions_enabled {
            self.layout.item_at(x, y).and_then(|local| {
                let global = self.page_offset + local;
                (global < self.visible_count()).then_some(global)
            })
        } else {
            None
        };
        if hovered != self.hovered_item
            || hovered_tool != self.hovered_tool
            || scrollbar_hover != self.scrollbar_hover
        {
            self.hovered_item = hovered;
            self.hovered_tool = hovered_tool;
            self.scrollbar_hover = scrollbar_hover;
            self.redraw();
        }
    }

    /// True when the point is inside the scrollbar track or thumb.
    fn scrollbar_contains(&self, x: i32, y: i32) -> bool {
        self.view == View::Launcher
            && self.layout.scrollbar_visible
            && self.layout.scrollbar_track.contains(x, y)
    }

    pub fn mouse_leave(&mut self) {
        let had_hover = self.hovered_item.take().is_some();
        let had_tool = self.hovered_tool.take().is_some();
        let had_press = self.pressed_item.take().is_some();
        let had_scroll = std::mem::take(&mut self.scrollbar_hover);
        if had_hover || had_tool || had_press || had_scroll {
            self.redraw();
        }
    }

    pub fn right_click(&mut self, x: i32, y: i32) {
        if self.menu.is_some() {
            self.close_menu();
            return;
        }
        if self.view != View::Launcher
            || self.add_overlay_open
            || (self.input_mode.is_some() && !matches!(self.input_mode, Some(InputMode::Search)))
        {
            return;
        }
        if let Some(category) = self.layout.category_at(x, y) {
            self.show_category_menu(category, (x, y));
            return;
        }
        if let Some(local) = self.layout.item_at(x, y) {
            let global = self.page_offset + local;
            if global < self.visible_count() {
                self.selected = Some(global);
                self.keyboard_selection = false;
                self.show_item_menu(global, (x, y));
                self.redraw();
            }
        }
    }

    pub fn wheel(&mut self, delta: i32) {
        if let Some(menu) = &mut self.menu {
            menu.scroll(if delta < 0 { 1 } else { -1 });
            self.redraw();
            return;
        }
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
        if self.menu.is_some() || self.view != View::Launcher || character.is_control() {
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
        let count = self.visible_count();
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
        let Some(reference) = self.selected.and_then(|index| self.visible_ref(index)) else {
            return;
        };
        let Some(item) = self.item_at(reference.category, reference.item).cloned() else {
            return;
        };
        match shell::launch(self.hwnd, &item) {
            Ok(()) => self.hide(),
            Err(error) => self.error(&error),
        }
    }

    pub fn delete_selected(&mut self) {
        let reference = self.selected.and_then(|index| self.visible_ref(index));
        let Some(reference) = reference else {
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
        self.flush_save();
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
            ToolButton::AddMenu => self.show_add_menu(),
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

    fn open_menu_at(
        &mut self,
        entries: Vec<MenuEntry>,
        anchor: (i32, i32),
        target: Option<search::ItemRef>,
    ) {
        self.menu = Some(MenuState::new(
            entries,
            anchor,
            self.layout.client,
            self.dpi,
            target,
        ));
        if self.search_mode
            && let Some(input) = &self.text_input
        {
            input.hide();
        }
        self.modal_open = true;
        self.hovered_item = None;
        self.pressed_item = None;
        self.redraw();
    }

    pub fn dismiss_menu(&mut self) {
        if self.menu.take().is_some() {
            self.modal_open = false;
            self.redraw();
        }
    }

    fn restore_search_input(&self) {
        if self.search_mode
            && !self.add_overlay_open
            && matches!(self.input_mode, Some(InputMode::Search))
            && let Some(input) = &self.text_input
        {
            let mut rect = self.layout.search_box;
            rect.right = self.layout.search_close_button.left;
            input.show(rect, &self.query, None);
        }
    }

    fn close_menu(&mut self) {
        if self.menu.take().is_some() {
            self.modal_open = false;
            self.restore_search_input();
            self.redraw();
        }
    }

    fn menu_click(&mut self, x: i32, y: i32) -> bool {
        let Some(menu) = &self.menu else { return false };
        let hit = menu.hit(x, y);
        if let Some((false, index)) = hit
            && matches!(menu.entry(false, index), Some(MenuEntry::Submenu { .. }))
        {
            self.menu.as_mut().unwrap().hover(x, y);
            self.redraw();
            return true;
        }
        let command = hit.and_then(|(submenu, index)| menu.command_at(submenu, index));
        let target = menu.target;
        self.menu = None;
        self.modal_open = false;
        if let Some(command) = command {
            self.execute_menu_command(command, target);
            self.restore_search_input();
        } else {
            self.restore_search_input();
            self.redraw();
        }
        true
    }

    fn execute_menu_command(&mut self, command: u32, target: Option<search::ItemRef>) {
        if command == ITEM_NEW || command == CATEGORY_NEW {
            self.apply_add_command(command);
        } else if (ITEM_LAUNCH..=ITEM_RESET_ICON).contains(&command)
            || (ITEM_MOVE_BASE..CATEGORY_SELECT_BASE).contains(&command)
        {
            if let Some(target) = target {
                self.execute_item_command(target, command);
            }
        } else if command >= CATEGORY_CONTEXT_BASE {
            self.execute_category_command(command);
        } else if command >= CATEGORY_SELECT_BASE {
            let index = (command - CATEGORY_SELECT_BASE) as usize;
            if index < self.config.categories.len() {
                self.category_start = index;
                self.select_category(index, CategorySwitch::Explicit);
            }
        }
    }

    fn show_add_menu(&mut self) {
        let entries = vec![
            command(ITEM_NEW, "添加项目", true),
            command(CATEGORY_NEW, "新建分类", true),
        ];
        self.menu = Some(MenuState::new(
            entries,
            (
                self.layout.add_button.right - scale(220, self.dpi),
                self.layout.add_button.bottom,
            ),
            self.layout.client,
            self.dpi,
            None,
        ));
        if self.search_mode
            && let Some(input) = &self.text_input
        {
            input.hide();
        }
        self.modal_open = true;
        self.redraw();
    }

    fn apply_add_command(&mut self, command: u32) {
        match command {
            ITEM_NEW => {
                self.add_overlay_open = true;
                self.selected = None;
                self.start_content_transition();
            }
            CATEGORY_NEW => self.create_category(),
            _ => {}
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

    fn select_category(&mut self, category: usize, switch: CategorySwitch) {
        self.view = View::Launcher;
        self.search_mode = false;
        self.query.clear();
        self.search_results.clear();
        if matches!(self.input_mode, Some(InputMode::Search)) {
            self.input_mode = None;
            if let Some(input) = &self.text_input {
                input.hide();
            }
        }
        self.selected = None;
        self.hovered_item = None;
        let previous_tool = self.hovered_tool.take();
        self.pressed_item = None;
        self.keyboard_selection = false;
        self.page_offset = 0;
        self.config.active_category = self.config.categories[category].id.clone();
        self.save();
        self.relayout();
        self.preload_visible_icons();
        match switch {
            CategorySwitch::Hover => {
                self.content_started = None;
                unsafe { KillTimer(self.hwnd, CONTENT_TIMER_ID) };
                if let Some(tool) = previous_tool {
                    self.redraw_tool(tool);
                }
                self.redraw_launcher();
            }
            CategorySwitch::Explicit => self.start_content_transition(),
        }
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
        self.selected = (self.visible_count() > 0).then_some(0);
        self.hovered_item = None;
        self.pressed_item = None;
        self.keyboard_selection = false;
        self.relayout();
        self.redraw();
    }

    /// References currently shown, resolved lazily so looking up a single index
    /// or the total count never allocates a vector as large as the category.
    fn scope(&self) -> Scope<'_> {
        Scope::new(
            &self.config,
            self.search_mode,
            &self.query,
            &self.search_results,
        )
    }

    fn visible_count(&self) -> usize {
        self.scope().len(&self.config)
    }

    fn visible_ref(&self, index: usize) -> Option<search::ItemRef> {
        self.scope().get(&self.config, index)
    }

    fn item_at(&self, category: usize, item: usize) -> Option<&LaunchItem> {
        self.config
            .categories
            .get(category)
            .and_then(|category| category.items.get(item))
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

    pub fn icons_ready(&mut self) {
        let Some(loader) = &mut self.icon_loader else {
            return;
        };
        for item in loader.drain() {
            let (key, icon) = item.into_parts();
            let replaced = self
                .icons
                .entry(key.size)
                .or_default()
                .insert(key.path.clone(), icon);
            if let Some(previous) = replaced
                && !previous.is_null()
            {
                unsafe { DestroyIcon(previous) };
            }
        }
        self.redraw();
    }

    /// Warms the first page of the active category so the very first show has
    /// its icons already in the cache.
    fn preload_visible_icons(&mut self) {
        let icon_size = ((32i64 * self.dpi as i64) / 96).clamp(32, 80) as i32;
        let mut page = Vec::new();
        self.scope()
            .write_page(&self.config, 0, self.layout.visible_capacity, &mut page);
        let mut missing: Vec<(usize, usize)> = Vec::new();
        for reference in &page {
            let Some(item) = self.item_at(reference.category, reference.item) else {
                continue;
            };
            let source = item.icon_source();
            let expanded = self
                .expanded_paths
                .get(source)
                .map(String::as_str)
                .unwrap_or(source);
            let cached = self
                .icons
                .get(&icon_size)
                .is_some_and(|cache| cache.contains_key(expanded));
            if !cached {
                missing.push((reference.category, reference.item));
            }
        }
        self.request_icons(&mut missing, icon_size);
    }

    fn show_all_categories_menu(&mut self) {
        let entries = self
            .config
            .categories
            .iter()
            .enumerate()
            .map(|(index, category)| {
                command(CATEGORY_SELECT_BASE + index as u32, &category.name, true)
            })
            .collect();
        self.open_menu_at(
            entries,
            (
                self.layout.category_more.left,
                self.layout.category_more.bottom,
            ),
            None,
        );
    }

    fn show_item_menu(&mut self, visible_index: usize, anchor: (i32, i32)) {
        let Some(reference) = self.visible_ref(visible_index) else {
            return;
        };
        let has_custom_icon = !self.config.categories[reference.category].items[reference.item]
            .icon_path
            .is_empty();
        let move_entries = self
            .config
            .categories
            .iter()
            .enumerate()
            .map(|(index, category)| {
                command(
                    ITEM_MOVE_BASE + index as u32,
                    &category.name,
                    index != reference.category,
                )
            })
            .collect();
        let entries = vec![
            command(ITEM_LAUNCH, "运行", true),
            command(ITEM_RUN_AS, "以管理员身份运行", true),
            command(ITEM_OPEN_WITH, "打开方式...", true),
            MenuEntry::Separator,
            command(ITEM_CHANGE_ICON, "修改图标...", true),
            command(ITEM_RESET_ICON, "恢复默认图标", has_custom_icon),
            MenuEntry::Separator,
            command(ITEM_REVEAL, "打开文件所在位置", true),
            command(ITEM_COPY_PATH, "复制完整路径", true),
            MenuEntry::Separator,
            MenuEntry::Submenu {
                label: "移动到分类".into(),
                entries: move_entries,
            },
            MenuEntry::Separator,
            command(ITEM_NEW, "新建项目", true),
            command(ITEM_SORT, "按名称排序", true),
            MenuEntry::Separator,
            command(ITEM_RENAME, "重命名", true),
            command(ITEM_DELETE, "删除", true),
        ];
        self.open_menu_at(entries, anchor, Some(reference));
    }

    fn execute_item_command(&mut self, reference: search::ItemRef, command: u32) {
        match command {
            ITEM_LAUNCH => {
                let item = self.config.categories[reference.category].items[reference.item].clone();
                match shell::launch(self.hwnd, &item) {
                    Ok(()) => self.hide(),
                    Err(error) => self.error(&error),
                }
            }
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
            ITEM_CHANGE_ICON => self.change_item_icon(reference),
            ITEM_RESET_ICON => self.reset_item_icon(reference),
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
            ITEM_DELETE => {
                self.config.categories[reference.category]
                    .items
                    .remove(reference.item);
                self.selected = None;
                self.refresh_search();
                self.save();
            }
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

    fn change_item_icon(&mut self, reference: search::ItemRef) {
        self.modal_open = true;
        let result = shell::choose_icon_file(self.hwnd);
        self.modal_open = false;
        match result {
            Ok(Some(path)) => {
                let value = path.to_string_lossy().into_owned();
                if let Some(item) = self
                    .config
                    .categories
                    .get_mut(reference.category)
                    .and_then(|category| category.items.get_mut(reference.item))
                {
                    item.icon_path = value;
                    self.save();
                    self.preload_visible_icons();
                    self.redraw();
                }
            }
            Ok(None) => {}
            Err(error) => self.error(&error),
        }
        unsafe { SetForegroundWindow(self.hwnd) };
    }

    fn reset_item_icon(&mut self, reference: search::ItemRef) {
        if let Some(item) = self
            .config
            .categories
            .get_mut(reference.category)
            .and_then(|category| category.items.get_mut(reference.item))
            && !item.icon_path.is_empty()
        {
            item.icon_path.clear();
            self.save();
            self.preload_visible_icons();
            self.redraw();
        }
    }

    fn show_category_menu(&mut self, category: usize, anchor: (i32, i32)) {
        let base = CATEGORY_CONTEXT_BASE + category as u32 * 3;
        let entries = vec![
            command(base, "新建分类", true),
            command(base + 1, "重命名分类", true),
            MenuEntry::Separator,
            command(base + 2, "删除分类", self.config.categories.len() > 1),
        ];
        self.open_menu_at(entries, anchor, None);
    }

    fn execute_category_command(&mut self, command: u32) {
        if command < CATEGORY_CONTEXT_BASE {
            return;
        }
        let offset = command - CATEGORY_CONTEXT_BASE;
        let category = (offset / 3) as usize;
        if category >= self.config.categories.len() {
            return;
        }
        match offset % 3 {
            0 => self.create_category(),
            1 => {
                let initial = self.config.categories[category].name.clone();
                self.open_text_input(InputMode::RenameCategory(category), &initial);
            }
            2 => self.delete_category(category),
            _ => {}
        }
    }

    fn delete_category(&mut self, category: usize) {
        if self.config.categories.len() == 1 {
            self.info("至少需要保留一个分类。");
            return;
        }
        let destination = category.saturating_sub(1);
        let name = self.config.categories[category].name.clone();
        let destination_name = if category == 0 {
            self.config.categories[1].name.clone()
        } else {
            self.config.categories[destination].name.clone()
        };
        if message_box_result(
            self.hwnd,
            "删除分类",
            &format!(
                "确定删除分类“{name}”？\n\n分类中的应用不会被删除，将移动到“{destination_name}”。"
            ),
        ) != windows_sys::Win32::UI::WindowsAndMessaging::IDOK
        {
            return;
        }
        apply_category_deletion(&mut self.config, category);
        self.category_start = self.category_start.min(self.config.categories.len() - 1);
        self.page_offset = 0;
        self.selected = None;
        self.search_mode = false;
        self.query.clear();
        self.search_results.clear();
        if matches!(self.input_mode, Some(InputMode::Search)) {
            self.input_mode = None;
            if let Some(input) = &self.text_input {
                input.hide();
            }
        }
        self.save();
        self.relayout();
        self.redraw();
    }

    /// Queues the current configuration to be written shortly. Bursts of edits
    /// collapse into one write, and the window is never blocked on disk I/O.
    fn save(&mut self) {
        if !self.persistence_enabled || self.writer.is_none() {
            return;
        }
        self.save_pending = true;
        unsafe { SetTimer(self.hwnd, SAVE_TIMER_ID, SAVE_DEBOUNCE_MS, None) };
    }

    /// Hands the current configuration to the worker without waiting for it.
    /// Only the clone happens on this thread; JSON encoding and the file write
    /// stay on the worker.
    fn enqueue_save(&mut self) {
        if !self.persistence_enabled {
            return;
        }
        let Some(writer) = self.writer.as_mut() else {
            return;
        };
        let config = self.config.clone();
        writer.write(self.config_path.clone(), config);
        self.save_pending = false;
    }

    /// Called when the debounce timer elapses. The worker thread stays alive so
    /// later edits keep being written.
    pub fn save_timer_tick(&mut self) {
        // SetTimer arms a repeating timer; drop it and re-arm on the next edit.
        unsafe { KillTimer(self.hwnd, SAVE_TIMER_ID) };
        if self.save_pending {
            self.enqueue_save();
        }
    }

    /// Persists synchronously without stopping the writer. Windows can cancel a
    /// session shutdown after WM_QUERYENDSESSION, so later saves must still work.
    pub fn save_for_session_end(&mut self) {
        unsafe { KillTimer(self.hwnd, SAVE_TIMER_ID) };
        if !self.persistence_enabled {
            return;
        }
        let config = self.config.clone();
        if let Some(writer) = &self.writer {
            writer.write_and_wait(self.config_path.clone(), config);
        }
        self.save_pending = false;
        self.report_save_error();
    }

    /// Writes any pending change and drains the queue. Used only on the way out,
    /// because it stops the writer for good.
    pub fn flush_save(&mut self) {
        unsafe { KillTimer(self.hwnd, SAVE_TIMER_ID) };
        if self.save_pending {
            self.enqueue_save();
        }
        if let Some(writer) = self.writer.as_mut() {
            writer.finish();
        }
        self.report_save_error();
    }

    pub fn report_save_error(&mut self) {
        let message = self.writer.as_ref().and_then(ConfigWriter::take_error);
        if let Some(message) = message {
            self.error(&message);
        }
    }

    pub fn redraw_input_overlay(&self) {
        if self.input_title().is_some() {
            let rect = RECT {
                left: self.layout.text_input.left,
                top: self.layout.text_input.top,
                right: self.layout.text_input.right,
                bottom: self.layout.text_input.bottom,
            };
            unsafe { InvalidateRect(self.hwnd, &rect, 0) };
        }
    }

    fn redraw_tool(&self, tool: ToolButton) {
        let rect = match tool {
            ToolButton::Back => self.layout.back_button,
            ToolButton::Search => self.layout.search_button,
            ToolButton::AddMenu => self.layout.add_button,
            ToolButton::Settings => self.layout.settings_button,
            ToolButton::Close => self.layout.close_button,
            ToolButton::CategoryLeft | ToolButton::CategoryRight | ToolButton::CategoryMore => {
                return;
            }
        };
        let rect = RECT {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        };
        unsafe { InvalidateRect(self.hwnd, &rect, 0) };
    }

    fn redraw_launcher(&self) {
        let rect = RECT {
            left: self.layout.categorybar.left,
            top: self.layout.categorybar.top,
            right: self.layout.client.right,
            bottom: self.layout.client.bottom,
        };
        unsafe { InvalidateRect(self.hwnd, &rect, 0) };
    }

    fn redraw(&self) {
        unsafe { InvalidateRect(self.hwnd, null(), 0) };
    }
}

fn icon_for_item(
    item: &LaunchItem,
    size: i32,
    icons: &HashMap<i32, HashMap<String, HICON>>,
    expanded_paths: &mut HashMap<String, String>,
    mut loader: Option<&mut IconLoader>,
) -> HICON {
    if !item.icon_path.trim().is_empty() {
        let custom = icon_for_path(
            &item.icon_path,
            size,
            icons,
            expanded_paths,
            loader.as_deref_mut(),
        );
        if !custom.is_null() {
            return custom;
        }
    }
    icon_for_path(&item.path, size, icons, expanded_paths, loader)
}

/// Looks an icon up by expanded filesystem path. Expansion is cached by raw
/// configured path, so repeated frames do no string allocation or expansion.
fn icon_for_path(
    path: &str,
    size: i32,
    icons: &HashMap<i32, HashMap<String, HICON>>,
    expanded_paths: &mut HashMap<String, String>,
    loader: Option<&mut IconLoader>,
) -> HICON {
    let expanded = expanded_paths
        .entry(path.to_owned())
        .or_insert_with(|| shell::expand_environment(path));
    if let Some(icon) = icons.get(&size).and_then(|cache| cache.get(expanded)) {
        return *icon;
    }
    if let Some(loader) = loader {
        loader.request(IconKey {
            path: expanded.clone(),
            size,
        });
    }
    null_mut()
}

impl Drop for App {
    fn drop(&mut self) {
        self.tray.take();
        if self.writer.is_some() {
            self.flush_save();
        }
        for cache in self.icons.values() {
            for icon in cache.values().copied() {
                if !icon.is_null() {
                    unsafe { DestroyIcon(icon) };
                }
            }
        }
    }
}

fn command(id: u32, label: &str, enabled: bool) -> MenuEntry {
    MenuEntry::Command {
        id,
        label: label.into(),
        enabled,
    }
}

fn apply_category_deletion(config: &mut Config, category: usize) -> Option<usize> {
    if config.categories.len() <= 1 || category >= config.categories.len() {
        return None;
    }
    let active_id = config.active_category.clone();
    let removed = config.categories.remove(category);
    let destination = category.saturating_sub(1).min(config.categories.len() - 1);
    config.categories[destination].items.extend(removed.items);
    if active_id == removed.id {
        config.active_category = config.categories[destination].id.clone();
    }
    Some(destination)
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
    use super::*;

    #[test]
    fn combined_add_menu_routes_items_categories_and_cancel() {
        let mut app = App::new(Config::default(), std::path::PathBuf::new(), false);
        app.apply_add_command(0);
        assert!(!app.add_overlay_open);
        assert!(app.input_mode.is_none());
        app.apply_add_command(ITEM_NEW);
        assert!(app.add_overlay_open);
        assert!(app.input_mode.is_none());
        app.add_overlay_open = false;
        app.apply_add_command(CATEGORY_NEW);
        assert!(!app.add_overlay_open);
        assert!(matches!(app.input_mode, Some(InputMode::NewCategory)));
    }

    #[test]
    fn single_click_only_launches_when_double_click_is_disabled() {
        assert!(!should_launch_on_single_click(true));
        assert!(should_launch_on_single_click(false));
    }

    #[test]
    fn visible_scope_tracks_category_selection_and_search() {
        let mut config = Config::default();
        config.categories[0].items = (0..40)
            .map(|item| LaunchItem {
                name: format!("项目{item}"),
                ..LaunchItem::default()
            })
            .collect();
        config
            .categories
            .push(crate::config::Category::new("tools", "工具"));
        config.categories[1].items = (0..5)
            .map(|item| LaunchItem {
                name: format!("工具{item}"),
                ..LaunchItem::default()
            })
            .collect();

        let mut app = App::new(config, std::path::PathBuf::new(), false);
        assert_eq!(app.visible_count(), 40);
        assert_eq!(
            app.visible_ref(39),
            Some(search::ItemRef {
                category: 0,
                item: 39
            })
        );
        assert_eq!(app.visible_ref(40), None);

        app.config.active_category = "tools".into();
        assert_eq!(app.visible_count(), 5);

        // An empty query searches every category, not just the active one.
        app.search_mode = true;
        assert_eq!(app.visible_count(), 45);
        assert_eq!(
            app.visible_ref(40),
            Some(search::ItemRef {
                category: 1,
                item: 0
            })
        );

        // A real query uses the pre-computed result list.
        app.query = "工具".into();
        app.search_results = search::results(&app.config, &app.query);
        assert_eq!(app.visible_count(), app.search_results.len());
        assert_eq!(app.visible_ref(0), app.search_results.first().copied());
    }

    #[test]
    fn hovering_category_switches_without_clicking() {
        let mut config = Config::default();
        config
            .categories
            .push(crate::config::Category::new("tools", "工具"));
        let mut app = App::new(config, std::path::PathBuf::new(), false);
        let (_, rect) = app.layout.categories[1];

        app.search_mode = true;
        app.input_mode = Some(InputMode::Search);
        app.query = "示例".into();
        app.mouse_move((rect.left + rect.right) / 2, (rect.top + rect.bottom) / 2);

        assert_eq!(app.config.active_category, "tools");
        assert!(!app.search_mode);
        assert!(app.input_mode.is_none());
        assert!(app.query.is_empty());
        assert_eq!(app.selected, None);
        assert_eq!(app.hovered_tool, None);
        assert!(app.content_started.is_none());
    }

    #[test]
    fn explicit_category_switch_keeps_content_transition() {
        let mut config = Config::default();
        config
            .categories
            .push(crate::config::Category::new("tools", "工具"));
        let mut app = App::new(config, std::path::PathBuf::new(), false);

        app.select_category(1, CategorySwitch::Explicit);

        assert_eq!(app.config.active_category, "tools");
        assert!(app.content_started.is_some());
    }

    #[test]
    fn toolbar_hover_is_tracked_and_cleared_on_leave() {
        let mut app = App::new(Config::default(), std::path::PathBuf::new(), false);
        let rect = app.layout.search_button;

        app.mouse_move((rect.left + rect.right) / 2, (rect.top + rect.bottom) / 2);
        assert_eq!(app.hovered_tool, Some(ToolButton::Search));

        app.mouse_leave();
        assert_eq!(app.hovered_tool, None);
    }

    #[test]
    fn deleting_categories_moves_items_to_expected_neighbor() {
        let item = |name: &str| LaunchItem {
            id: name.into(),
            name: name.into(),
            path: format!("C:\\{name}.exe"),
            ..LaunchItem::default()
        };
        let mut first = crate::config::Category::new("first", "第一");
        first.items.push(item("a"));
        let mut middle = crate::config::Category::new("middle", "第二");
        middle.items.extend([item("b"), item("c")]);
        let mut last = crate::config::Category::new("last", "第三");
        last.items.push(item("d"));
        let mut config = Config {
            categories: vec![first, middle, last],
            active_category: "middle".into(),
            ..Config::default()
        };

        assert_eq!(apply_category_deletion(&mut config, 1), Some(0));
        assert_eq!(
            config.categories[0]
                .items
                .iter()
                .map(|item| item.name.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b", "c"]
        );
        assert_eq!(config.active_category, "first");

        assert_eq!(apply_category_deletion(&mut config, 0), Some(0));
        assert_eq!(
            config.categories[0]
                .items
                .iter()
                .map(|item| item.name.as_str())
                .collect::<Vec<_>>(),
            vec!["d", "a", "b", "c"]
        );
        assert_eq!(apply_category_deletion(&mut config, 0), None);
    }

    #[test]
    fn custom_icon_source_falls_back_to_target() {
        let mut item = LaunchItem {
            path: "C:\\App.exe".into(),
            ..LaunchItem::default()
        };
        assert_eq!(item.icon_source(), item.path);
        item.icon_path = "C:\\App.ico".into();
        assert_eq!(item.icon_source(), item.icon_path);
        item.icon_path.clear();
        assert_eq!(item.icon_source(), item.path);
    }

    #[test]
    fn menu_commands_keep_fixed_target_reference() {
        let target = search::ItemRef {
            category: 2,
            item: 5,
        };
        let menu = MenuState::new(
            vec![command(ITEM_RENAME, "重命名", true)],
            (10, 10),
            crate::layout::Rect {
                left: 0,
                top: 0,
                right: 840,
                bottom: 520,
            },
            96,
            Some(target),
        );
        assert_eq!(menu.target, Some(target));
        assert_eq!(menu.command_at(false, 0), Some(ITEM_RENAME));
    }

    #[test]
    fn failed_icon_is_a_negative_cache_entry() {
        let path = "%KRUN_MISSING_PATH%\\missing.exe";
        let expanded = shell::expand_environment(path);
        let mut icons = HashMap::new();
        icons
            .entry(32)
            .or_insert_with(HashMap::new)
            .insert(expanded, null_mut());
        let mut expanded_paths = HashMap::new();

        assert!(icon_for_path(path, 32, &icons, &mut expanded_paths, None).is_null());
        assert_eq!(expanded_paths.len(), 1);
    }

    #[test]
    fn page_count_saturates_when_offset_is_stale() {
        let count = 3usize;
        let page_offset = 50usize;
        let capacity = 24usize;
        let end = page_offset.saturating_add(capacity).min(count);
        assert_eq!(end.saturating_sub(page_offset), 0);
    }
}
