mod actions;
mod hotkey;
mod history;
mod icons;
mod index;
mod search;
mod shortcuts;
mod tray;

use actions::{is_terminal_command, launch, target_from_raw_command, LaunchMode};
use eframe::egui;
use history::RunHistory;
use hotkey::GlobalHotkey;
use icons::{AppIconCache, FaviconCache, FaviconLookup};
use index::{index_entries, EntryKind, LauncherEntry};
use search::{search, SearchResult};
use shortcuts::ShortcutConfig;
use tray::{TrayEvent, TrayIcon};

const RESULT_LIMIT: usize = 10;
const MAX_VISIBLE_RESULTS: usize = 6;
const RESULT_ROW_HEIGHT: f32 = 56.0;
const WINDOW_COLLAPSED_HEIGHT: f32 = 92.0;
const WINDOW_EXPANDED_HEIGHT: f32 = 440.0;
const WINDOW_WIDTH: f32 = 680.0;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Spark Run")
            .with_inner_size([WINDOW_WIDTH, WINDOW_COLLAPSED_HEIGHT])
            .with_min_inner_size([480.0, 84.0])
            .with_resizable(false)
            .with_decorations(false)
            .with_transparent(true),
        ..Default::default()
    };

    eframe::run_native(
        "Spark Run",
        options,
        Box::new(|cc| Box::new(SparkRunApp::new(cc))),
    )
}

struct SparkRunApp {
    entries: Vec<LauncherEntry>,
    query: String,
    results: Vec<SearchResult>,
    selected: usize,
    status: String,
    focus_search: bool,
    global_hotkey: Option<GlobalHotkey>,
    tray_icon: Option<TrayIcon>,
    shortcuts: ShortcutConfig,
    icon_cache: AppIconCache,
    favicon_cache: FaviconCache,
    run_history: RunHistory,
    suppress_hotkey_input_frames: u8,
    window_expanded: bool,
    center_window_frames: u8,
    exit_requested: bool,
}

impl SparkRunApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_style(&cc.egui_ctx);

        let entries = index_entries();
        let loaded_history = RunHistory::load();
        let results = search(&entries, "", RESULT_LIMIT, &loaded_history.history);
        let loaded_shortcuts = ShortcutConfig::load_or_create();
        let hwnd = native_window_handle(cc);
        let (global_hotkey, hotkey_status) =
            match GlobalHotkey::register_alt_d(cc.egui_ctx.clone(), hwnd) {
                Ok(hotkey) => (Some(hotkey), "Alt+D ready".to_string()),
                Err(error) => (None, error),
            };
        let (tray_icon, tray_status) = match TrayIcon::register(cc.egui_ctx.clone(), hwnd) {
            Ok(tray_icon) => (Some(tray_icon), "tray icon ready".to_string()),
            Err(error) => (None, error),
        };
        let status = format!(
            "Indexed {} launch targets. {hotkey_status}. {tray_status}. {}. {}.",
            entries.len(),
            loaded_shortcuts.status,
            loaded_history.status
        );

        Self {
            entries,
            query: String::new(),
            results,
            selected: 0,
            status,
            focus_search: true,
            global_hotkey,
            tray_icon,
            shortcuts: loaded_shortcuts.config,
            icon_cache: AppIconCache::default(),
            favicon_cache: FaviconCache::default(),
            run_history: loaded_history.history,
            suppress_hotkey_input_frames: 0,
            window_expanded: false,
            center_window_frames: 4,
            exit_requested: false,
        }
    }

    fn refresh_results(&mut self) {
        if is_terminal_command(&self.query) {
            self.results.clear();
            self.selected = 0;
            return;
        }

        self.results = search(&self.entries, &self.query, RESULT_LIMIT, &self.run_history);
        self.selected = self.selected.min(self.results.len().saturating_sub(1));
    }

    fn selected_entry(&self) -> Option<&LauncherEntry> {
        self.results.get(self.selected).map(|result| &result.entry)
    }

    fn reload_shortcuts(&mut self) {
        let loaded_shortcuts = ShortcutConfig::load_or_create();
        self.shortcuts = loaded_shortcuts.config;

        if loaded_shortcuts
            .status
            .starts_with("Using default shortcuts")
        {
            self.status = loaded_shortcuts.status;
        }
    }

    fn launch_selected(&mut self, mode: LaunchMode, ctx: &egui::Context) {
        let target = if is_terminal_command(&self.query) {
            target_from_raw_command(&self.query, &self.shortcuts)
        } else if let Some(entry) = self.selected_entry() {
            entry.target.clone()
        } else {
            self.reload_shortcuts();
            target_from_raw_command(&self.query, &self.shortcuts)
        };

        match launch(&target, mode) {
            Ok(()) => {
                let mode_label = match mode {
                    LaunchMode::Normal => "Launched",
                    LaunchMode::Elevated => "Requested elevation for",
                };
                self.status = format!("{mode_label} {}", target.file);
                self.run_history.record(&target);
                self.query.clear();
                self.refresh_results();
                self.hide_to_background(ctx);
            }
            Err(error) => {
                self.status = error;
            }
        }
    }

    fn bring_to_foreground(&mut self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        self.focus_search = true;
        self.center_window_frames = 4;
    }

    fn hide_to_background(&mut self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        self.focus_search = true;
    }

    fn handle_global_hotkey(&mut self, ctx: &egui::Context) {
        if self
            .global_hotkey
            .as_ref()
            .is_some_and(GlobalHotkey::was_pressed)
        {
            self.suppress_hotkey_input_frames = 4;
            self.bring_to_foreground(ctx);
        }
    }

    fn handle_tray_icon(&mut self, ctx: &egui::Context) {
        let events = self
            .tray_icon
            .as_ref()
            .map(TrayIcon::drain_events)
            .unwrap_or_default();

        for event in events {
            match event {
                TrayEvent::Show => self.bring_to_foreground(ctx),
                TrayEvent::Exit => {
                    self.exit_requested = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    fn suppress_hotkey_input(&mut self, ctx: &egui::Context) {
        if self.suppress_hotkey_input_frames == 0 {
            return;
        }

        ctx.input_mut(|input| {
            input.events.retain(|event| match event {
                egui::Event::Text(text) => !text.eq_ignore_ascii_case("d"),
                egui::Event::Key {
                    key,
                    physical_key,
                    modifiers,
                    ..
                } => {
                    let is_d = *key == egui::Key::D || *physical_key == Some(egui::Key::D);
                    !(is_d && modifiers.alt)
                }
                _ => true,
            });
        });

        self.suppress_hotkey_input_frames -= 1;
    }

    fn handle_keyboard(&mut self, ctx: &egui::Context) {
        let down = ctx.input(|input| input.key_pressed(egui::Key::ArrowDown));
        let up = ctx.input(|input| input.key_pressed(egui::Key::ArrowUp));
        let enter = ctx.input(|input| input.key_pressed(egui::Key::Enter));
        let escape = ctx.input(|input| input.key_pressed(egui::Key::Escape));
        let ctrl = ctx.input(|input| input.modifiers.ctrl);

        if down && !self.results.is_empty() {
            self.selected = (self.selected + 1).min(self.results.len() - 1);
        }

        if up {
            self.selected = self.selected.saturating_sub(1);
        }

        if enter {
            let mode = if ctrl {
                LaunchMode::Elevated
            } else {
                LaunchMode::Normal
            };
            self.launch_selected(mode, ctx);
        }

        if escape {
            self.hide_to_background(ctx);
        }

        if let Some(index) = alt_number_pressed(ctx) {
            let visible = self.results.len().min(MAX_VISIBLE_RESULTS);
            if index < visible {
                self.selected = index;
                self.launch_selected(LaunchMode::Normal, ctx);
            }
        }
    }

    fn sync_window_size(&mut self, ctx: &egui::Context) {
        let should_expand =
            should_show_results(&self.query, &self.shortcuts) && !self.results.is_empty();

        if should_expand == self.window_expanded {
            return;
        }

        self.window_expanded = should_expand;
        let height = if should_expand {
            WINDOW_EXPANDED_HEIGHT
        } else {
            WINDOW_COLLAPSED_HEIGHT
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
            WINDOW_WIDTH,
            height,
        )));
    }

    fn center_window(&mut self, ctx: &egui::Context) {
        if self.center_window_frames == 0 {
            return;
        }

        let monitor = ctx.input(|input| input.viewport().monitor_size);
        let Some(monitor) = monitor else {
            return;
        };

        let x = ((monitor.x - WINDOW_WIDTH) / 2.0).max(0.0);
        let anchor_top = (monitor.y - WINDOW_EXPANDED_HEIGHT) / 2.0;
        let y = anchor_top.max(monitor.y * 0.18).max(0.0);
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(x, y)));
        self.center_window_frames -= 1;
    }
}

impl eframe::App for SparkRunApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_global_hotkey(ctx);
        self.handle_tray_icon(ctx);

        if ctx.input(|input| input.viewport().close_requested()) && !self.exit_requested {
            self.hide_to_background(ctx);
        }

        self.suppress_hotkey_input(ctx);
        self.handle_keyboard(ctx);

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(egui::Color32::TRANSPARENT))
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(egui::Color32::from_rgb(20, 22, 27))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(38, 42, 50)))
                    .rounding(12.0)
                    .inner_margin(egui::Margin::symmetric(14.0, 10.0))
                    .show(ui, |ui| {
                        draw_window_chrome(ui, ctx);

                        if draw_search_box(
                            ui,
                            &mut self.query,
                            &self.shortcuts,
                            &mut self.favicon_cache,
                            &mut self.focus_search,
                        ) {
                            self.refresh_results();
                        }

                        if should_show_results(&self.query, &self.shortcuts)
                            && !self.results.is_empty()
                        {
                            ui.add_space(10.0);
                            let mut pending_launch: Option<usize> = None;
                            egui::ScrollArea::vertical()
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    ui.spacing_mut().item_spacing.y = 2.0;
                                    for index in 0..self.results.len().min(MAX_VISIBLE_RESULTS) {
                                        let selected = index == self.selected;
                                        let result = self.results[index].clone();
                                        let response = draw_result(
                                            ui,
                                            &result,
                                            selected,
                                            index,
                                            &mut self.icon_cache,
                                            ctx,
                                        );

                                        if response.clicked() {
                                            self.selected = index;
                                        }

                                        if response.double_clicked() {
                                            pending_launch = Some(index);
                                        }
                                    }
                                });

                            if let Some(index) = pending_launch {
                                self.selected = index;
                                self.launch_selected(LaunchMode::Normal, ctx);
                            }
                        }

                        self.sync_window_size(ctx);
                        self.center_window(ctx);
                    });
            });

        ctx.request_repaint_after(std::time::Duration::from_secs(1));
    }
}

fn draw_result(
    ui: &mut egui::Ui,
    result: &SearchResult,
    selected: bool,
    index: usize,
    icon_cache: &mut AppIconCache,
    ctx: &egui::Context,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), RESULT_ROW_HEIGHT),
        egui::Sense::click(),
    );
    let painter = ui.painter_at(rect);

    let bg = if selected {
        egui::Color32::from_rgb(32, 36, 44)
    } else if response.hovered() {
        egui::Color32::from_rgb(26, 29, 35)
    } else {
        egui::Color32::TRANSPARENT
    };
    painter.rect_filled(rect, 8.0, bg);

    if selected {
        let bar = egui::Rect::from_min_max(
            rect.left_top() + egui::vec2(4.0, 9.0),
            egui::pos2(rect.left() + 7.0, rect.bottom() - 9.0),
        );
        painter.rect_filled(bar, 1.5, egui::Color32::from_rgb(82, 156, 255));
    }

    let icon_center = egui::pos2(rect.left() + 40.0, rect.center().y);
    if let Some(texture) = icon_cache.texture_for(ctx, &result.entry) {
        let icon_rect = egui::Rect::from_center_size(icon_center, egui::vec2(32.0, 32.0));
        painter.image(
            texture.id(),
            icon_rect,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    } else {
        paint_entry_icon(&painter, icon_center, &result.entry);
    }

    let text_left = icon_center.x + 26.0;
    let title_color = egui::Color32::from_rgb(236, 238, 244);
    let subtitle_color = egui::Color32::from_rgb(138, 144, 158);

    painter.text(
        egui::pos2(text_left, rect.center().y - 10.0),
        egui::Align2::LEFT_CENTER,
        &result.entry.name,
        egui::FontId::proportional(15.0),
        title_color,
    );
    painter.text(
        egui::pos2(text_left, rect.center().y + 10.0),
        egui::Align2::LEFT_CENTER,
        subtitle_for(&result.entry),
        egui::FontId::proportional(12.0),
        subtitle_color,
    );

    if index < 9 {
        let pill = egui::Rect::from_min_max(
            egui::pos2(rect.right() - 62.0, rect.center().y - 11.0),
            egui::pos2(rect.right() - 12.0, rect.center().y + 11.0),
        );
        painter.rect_filled(pill, 4.0, egui::Color32::from_rgb(46, 50, 60));
        painter.text(
            pill.center(),
            egui::Align2::CENTER_CENTER,
            format!("Alt+{}", index + 1),
            egui::FontId::proportional(11.0),
            egui::Color32::from_rgb(186, 192, 205),
        );
    }

    response
}

fn should_show_results(query: &str, shortcuts: &ShortcutConfig) -> bool {
    let query = query.trim();

    if query.is_empty() {
        return false;
    }

    if is_terminal_command(query) {
        return false;
    }

    if shortcuts.shortcut_prefix(query).is_some() {
        return true;
    }

    query.chars().count() >= 2
}

fn draw_window_chrome(ui: &mut egui::Ui, ctx: &egui::Context) {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 6.0),
        egui::Sense::click_and_drag(),
    );

    if response.drag_started() {
        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }

    ui.painter().rect_filled(
        egui::Rect::from_center_size(rect.center(), egui::vec2(46.0, 2.0)),
        1.0,
        egui::Color32::from_rgb(50, 54, 64),
    );
}

fn paint_entry_icon(painter: &egui::Painter, center: egui::Pos2, entry: &LauncherEntry) {
    let (fill, accent) = entry_icon_palette(entry.kind);
    let bg = egui::Rect::from_center_size(center, egui::vec2(34.0, 34.0));
    painter.rect_filled(bg, 17.0, fill);

    let ring = egui::Rect::from_center_size(center, egui::vec2(24.0, 24.0));
    painter.rect_filled(
        ring,
        12.0,
        egui::Color32::from_rgba_unmultiplied(0, 0, 0, 38),
    );

    let initial = entry
        .name
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string());

    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        initial,
        egui::FontId::proportional(14.0),
        accent,
    );
}

fn entry_icon_palette(kind: EntryKind) -> (egui::Color32, egui::Color32) {
    match kind {
        EntryKind::BuiltIn => (
            egui::Color32::from_rgb(70, 128, 224),
            egui::Color32::from_rgb(240, 246, 255),
        ),
        EntryKind::StartMenu => (
            egui::Color32::from_rgb(56, 160, 165),
            egui::Color32::from_rgb(236, 252, 252),
        ),
        EntryKind::PathExecutable => (
            egui::Color32::from_rgb(210, 128, 70),
            egui::Color32::from_rgb(255, 244, 232),
        ),
    }
}

fn subtitle_for(entry: &LauncherEntry) -> String {
    let label = match entry.kind {
        EntryKind::BuiltIn => "Built-in command",
        EntryKind::StartMenu => "Start Menu shortcut",
        EntryKind::PathExecutable => "Executable on PATH",
    };

    let target = shorten_path(&entry.target.file, 48);
    if target.is_empty() {
        label.to_string()
    } else {
        format!("{label} — {target}")
    }
}

fn shorten_path(path: &str, max: usize) -> String {
    if path.chars().count() <= max {
        return path.to_string();
    }
    let tail: String = path.chars().rev().take(max.saturating_sub(1)).collect();
    let tail: String = tail.chars().rev().collect();
    format!("…{tail}")
}

fn alt_number_pressed(ctx: &egui::Context) -> Option<usize> {
    ctx.input(|input| {
        if !input.modifiers.alt {
            return None;
        }

        const KEYS: [egui::Key; 9] = [
            egui::Key::Num1,
            egui::Key::Num2,
            egui::Key::Num3,
            egui::Key::Num4,
            egui::Key::Num5,
            egui::Key::Num6,
            egui::Key::Num7,
            egui::Key::Num8,
            egui::Key::Num9,
        ];

        KEYS.iter().position(|key| input.key_pressed(*key))
    })
}

fn configure_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.visuals = egui::Visuals::dark();
    style.visuals.window_rounding = 8.0.into();
    style.visuals.widgets.active.rounding = 6.0.into();
    style.visuals.widgets.hovered.rounding = 6.0.into();
    style.visuals.widgets.inactive.rounding = 6.0.into();
    ctx.set_style(style);
}

#[cfg(windows)]
fn native_window_handle(cc: &eframe::CreationContext<'_>) -> Option<isize> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    match cc.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
        _ => None,
    }
}

#[cfg(not(windows))]
fn native_window_handle(_cc: &eframe::CreationContext<'_>) -> Option<isize> {
    None
}

fn draw_search_box(
    ui: &mut egui::Ui,
    query: &mut String,
    shortcuts: &ShortcutConfig,
    favicon_cache: &mut FaviconCache,
    focus_search: &mut bool,
) -> bool {
    let Some(shortcut) = shortcuts.shortcut_prefix(query).map(str::to_string) else {
        return draw_plain_search_box(ui, query, focus_search);
    };

    if ui
        .ctx()
        .memory(|memory| memory.has_focus(search_input_id()))
    {
        *focus_search = true;
    }

    draw_shortcut_search_box(ui, query, &shortcut, shortcuts, favicon_cache, focus_search)
}

fn search_input_id() -> egui::Id {
    egui::Id::new("spark_search_input")
}

fn set_search_cursor_end(ctx: &egui::Context, value: &str) {
    use egui::text::{CCursor, CCursorRange};

    let id = search_input_id();
    let mut state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
    let cursor = CCursor::new(value.chars().count());
    state.cursor.set_char_range(Some(CCursorRange::one(cursor)));
    egui::TextEdit::store_state(ctx, id, state);
    ctx.memory_mut(|memory| memory.request_focus(id));
}

fn draw_plain_search_box(ui: &mut egui::Ui, query: &mut String, focus_search: &mut bool) -> bool {
    let mut changed = false;

    egui::Frame::none()
        .fill(egui::Color32::from_rgb(30, 32, 37))
        .rounding(10.0)
        .inner_margin(egui::Margin::symmetric(16.0, 10.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let right_reserve = 88.0;
                let text_width = (ui.available_width() - right_reserve).max(120.0);

                let response = ui.add_sized(
                    [text_width, 32.0],
                    egui::TextEdit::singleline(query)
                        .id(search_input_id())
                        .hint_text("Spark something up…")
                        .font(egui::TextStyle::Heading)
                        .frame(false),
                );

                if *focus_search {
                    response.request_focus();
                    *focus_search = false;
                }

                changed = response.changed();

                draw_search_trailing(ui, query.is_empty());
            });
        });

    changed
}

fn draw_shortcut_search_box(
    ui: &mut egui::Ui,
    query: &mut String,
    shortcut: &str,
    shortcuts: &ShortcutConfig,
    favicon_cache: &mut FaviconCache,
    focus_search: &mut bool,
) -> bool {
    let mut changed = false;
    let mut text_after_shortcut = query
        .split_once(char::is_whitespace)
        .map(|(_, value)| value.trim_start().to_string())
        .unwrap_or_default();
    let was_empty = text_after_shortcut.is_empty();
    let input_id = search_input_id();
    let has_focus = ui.ctx().memory(|memory| memory.has_focus(input_id));
    let (backspace, ctrl_backspace) = ui.ctx().input(|input| {
        let backspace = input.key_pressed(egui::Key::Backspace);
        (backspace, backspace && input.modifiers.ctrl)
    });

    if has_focus && was_empty && ctrl_backspace {
        query.clear();
        *focus_search = true;
        set_search_cursor_end(ui.ctx(), query);
        return true;
    }

    if has_focus && was_empty && backspace {
        *query = delete_last_char(shortcut);
        *focus_search = true;
        set_search_cursor_end(ui.ctx(), query);
        return true;
    }

    egui::Frame::none()
        .fill(egui::Color32::from_rgb(30, 32, 37))
        .rounding(10.0)
        .inner_margin(egui::Margin::symmetric(10.0, 8.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let chip_ctx = ui.ctx().clone();
                draw_shortcut_chip(ui, shortcut, shortcuts, favicon_cache, &chip_ctx);

                let right_reserve = 88.0;
                let text_width = (ui.available_width() - right_reserve).max(100.0);

                let response = ui.add_sized(
                    [text_width, 32.0],
                    egui::TextEdit::singleline(&mut text_after_shortcut)
                        .id(input_id)
                        .hint_text("Spark something up…")
                        .font(egui::TextStyle::Heading)
                        .frame(false),
                );

                if *focus_search {
                    response.request_focus();
                    *focus_search = false;
                }

                if response.changed() {
                    *query = format!("{shortcut} {text_after_shortcut}");
                    changed = true;
                }

                draw_search_trailing(ui, text_after_shortcut.is_empty());
            });
        });

    changed
}

fn draw_search_trailing(ui: &mut egui::Ui, query_is_empty: bool) {
    let clock_alpha = ui.ctx().animate_bool_with_time(
        egui::Id::new("spark_clock_alpha"),
        query_is_empty,
        0.18,
    );

    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        draw_magnifier_icon(ui, 18.0);
        if clock_alpha > 0.01 {
            ui.add_space(10.0);
            let alpha_byte = (clock_alpha * 255.0).round().clamp(0.0, 255.0) as u8;
            ui.label(
                egui::RichText::new(local_time_string())
                    .color(egui::Color32::from_rgba_unmultiplied(
                        188, 194, 208, alpha_byte,
                    ))
                    .size(13.0),
            );
        }
    });
}

fn draw_magnifier_icon(ui: &mut egui::Ui, size: f32) {
    let (rect, _response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let color = egui::Color32::from_rgb(170, 178, 195);
    let stroke = egui::Stroke::new(1.8, color);
    let circle_center = rect.center() - egui::vec2(size * 0.12, size * 0.12);
    let radius = size * 0.33;
    painter.circle_stroke(circle_center, radius, stroke);
    let offset = radius * 0.72;
    let handle_start = circle_center + egui::vec2(offset, offset);
    let handle_end = circle_center + egui::vec2(offset + size * 0.22, offset + size * 0.22);
    painter.line_segment([handle_start, handle_end], stroke);
}

#[cfg(windows)]
fn local_time_string() -> String {
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;

    let mut st: SYSTEMTIME = unsafe { std::mem::zeroed() };
    unsafe { GetLocalTime(&mut st) };
    let hour24 = st.wHour as u32;
    let minute = st.wMinute as u32;
    let (display_hour, suffix) = match hour24 {
        0 => (12, "AM"),
        1..=11 => (hour24, "AM"),
        12 => (12, "PM"),
        _ => (hour24 - 12, "PM"),
    };
    format!("{display_hour:02}:{minute:02} {suffix}")
}

#[cfg(not(windows))]
fn local_time_string() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let secs_of_day = secs % 86_400;
    let hour24 = (secs_of_day / 3600) as u32;
    let minute = ((secs_of_day % 3600) / 60) as u32;
    let (display_hour, suffix) = match hour24 {
        0 => (12, "AM"),
        1..=11 => (hour24, "AM"),
        12 => (12, "PM"),
        _ => (hour24 - 12, "PM"),
    };
    format!("{display_hour:02}:{minute:02} {suffix}")
}

fn draw_shortcut_chip(
    ui: &mut egui::Ui,
    shortcut: &str,
    shortcuts: &ShortcutConfig,
    favicon_cache: &mut FaviconCache,
    ctx: &egui::Context,
) {
    let size = egui::vec2(42.0, 38.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter_at(rect);

    let lookup = shortcuts
        .target_for_shortcut(shortcut)
        .map(|url| favicon_cache.lookup(ctx, shortcut, url))
        .unwrap_or(FaviconLookup::Unavailable);

    match lookup {
        FaviconLookup::Ready(tex) => {
            painter.rect_filled(
                rect.shrink(2.0),
                6.0,
                egui::Color32::from_rgb(240, 242, 247),
            );
            let icon_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(26.0, 26.0));
            painter.image(
                tex.id(),
                icon_rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
        FaviconLookup::Loading => {
            painter.rect_filled(
                rect.shrink(2.0),
                6.0,
                egui::Color32::from_rgb(44, 48, 57),
            );
            let time = ui.input(|input| input.time);
            paint_spinner(&painter, rect.center(), time);
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
        FaviconLookup::Unavailable => {
            painter.rect_filled(
                rect.shrink(2.0),
                6.0,
                egui::Color32::from_rgb(44, 48, 57),
            );
        }
    }

    response.on_hover_text(shortcut);
}

fn paint_spinner(painter: &egui::Painter, center: egui::Pos2, time: f64) {
    let radius = 9.0;
    let rotation = (time * 4.5) as f32;
    let arc_span = std::f32::consts::TAU * 0.72;
    let segments = 28;
    let stroke = egui::Stroke::new(2.0, egui::Color32::from_rgb(140, 180, 240));

    let mut previous: Option<egui::Pos2> = None;
    for step in 0..=segments {
        let t = step as f32 / segments as f32;
        let angle = rotation + t * arc_span;
        let point = center + egui::vec2(angle.cos(), angle.sin()) * radius;
        if let Some(previous) = previous {
            painter.line_segment([previous, point], stroke);
        }
        previous = Some(point);
    }
}

fn delete_last_char(value: &str) -> String {
    let mut value = value.to_string();
    value.pop();
    value
}
