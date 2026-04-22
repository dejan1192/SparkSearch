# Spark Run — Context for Claude

## What this is

A small, Windows-only "PowerToys Run"-style launcher written in Rust on top of
`eframe`/`egui`. Invoked with a global `Alt+D` hotkey, it shows a compact
command palette that indexes Start Menu shortcuts, `PATH` executables, and a
fixed list of built-in commands, fuzzy-filters as the user types, and launches
the selected target with `Enter` (or elevated with `Ctrl+Enter`).

It also supports URL/search shortcuts defined in `shortcuts.conf` (`!g`,
`!gpt`, `!gpt rust egui`, etc.), a tray icon for Show/Exit, and a persisted
run-history that boosts frequently-used results.

Stage: proof-of-concept. Single binary, no installer, no unit-test coverage
beyond what exists for `shortcuts`/`actions`.

## Layout

- [Cargo.toml](Cargo.toml) — `eframe 0.27`, `raw-window-handle 0.6`,
  `windows-sys 0.59` (Foundation, Graphics_Gdi, Storage_FileSystem,
  System_SystemInformation, System_Threading, UI_Input_KeyboardAndMouse,
  UI_Shell, UI_WindowsAndMessaging).
- [src/main.rs](src/main.rs) — `SparkRunApp`, all drawing, keyboard handling,
  window sizing, and the custom chrome (no OS titlebar).
- [src/index.rs](src/index.rs) — builds the `LauncherEntry` list: built-ins,
  Start Menu recursion, `PATH` scan. `EntryKind` = `BuiltIn | StartMenu |
  PathExecutable`.
- [src/search.rs](src/search.rs) — fuzzy scorer. Takes `&RunHistory` to boost
  recently-/frequently-launched entries.
- [src/actions.rs](src/actions.rs) — `LaunchTarget`, `launch` (ShellExecuteW),
  `target_from_raw_command` (resolves `!shortcuts`), `is_terminal_command`
  helper.
- [src/shortcuts.rs](src/shortcuts.rs) — `shortcuts.conf` loader and `!key`
  URL expansion (`{query}` placeholder).
- [src/hotkey.rs](src/hotkey.rs) — global `Alt+D` via `RegisterHotKey` + a
  hidden window-message hook.
- [src/tray.rs](src/tray.rs) — `Shell_NotifyIconW` tray with Show/Exit.
- [src/icons.rs](src/icons.rs) — `AppIconCache` uses `SHGetFileInfoW` + GDI
  `DrawIconEx` to extract app icons into `egui::ColorImage` textures.
  `FaviconCache` is a **stub** — `texture_for` always returns `None`.
- [src/history.rs](src/history.rs) — `RunHistory`, persisted launch counts
  for result boosting.
- [shortcuts.conf](shortcuts.conf) — user-editable URL shortcut map.

## UI invariants (recently tuned — don't regress these)

- Window uses `with_decorations(false)` + `with_transparent(true)` +
  `App::clear_color` returning `[0, 0, 0, 0]` so the rounded inner
  `Frame` (rounding 12.0, fill `rgb(20,22,27)`, 1px stroke `rgb(38,42,50)`)
  floats with true-transparent corners.
- `draw_window_chrome` paints a 6px drag strip with a small center dot and
  forwards `ViewportCommand::StartDrag` — that is the only way to move the
  window.
- Result row = `RESULT_ROW_HEIGHT` (56px) with: blue left-accent bar when
  selected, real app icon (via `AppIconCache`) with colored-initial fallback
  (`paint_entry_icon`), title + subtitle, and an `Alt+N` pill on the right.
- Subtitle format: `<kind label> — <shortened target path>` from
  `subtitle_for` / `shorten_path`.
- `Alt+1..Alt+9` (see `alt_number_pressed`) select and launch the
  corresponding visible row. `MAX_VISIBLE_RESULTS = 6`.
- Search trailing area: magnifier icon always visible, live clock
  (HH:MM AM/PM from `GetLocalTime`) fades via
  `ctx.animate_bool_with_time(..., 0.18)` — visible only when the query is
  empty. The clock label is skipped entirely when alpha ≤ 0.01 so layout
  doesn't jitter.
- Placeholder text: `"Spark something up…"` (both plain and shortcut-chip
  search boxes).
- `ctx.request_repaint_after(1s)` keeps the clock ticking.

## Build / run

```powershell
cargo run
```

Cargo lives at `%USERPROFILE%\.cargo\bin\cargo.exe`. In this environment it is
not on `PATH`, so scripts must invoke the full path.

## Known loose ends

- `FaviconCache` is a stub. The plumbing (field on `SparkRunApp`, param on
  `draw_search_box`) is in place, but `texture_for` returns `None`. When we
  wire it up, the shortcut chip (`draw_shortcut_chip`) should prefer the
  favicon texture over the letter/color rendering.
- `status` field is set throughout but never drawn. Either surface it
  (status bar / toast) or remove it.
- `center_window_frames` is decremented-and-used logic for re-centering on
  Show; double-check it works after you add monitor-DPI handling.
- No tests for the UI/chrome; `actions.rs` and `shortcuts.rs` have tests —
  keep adding there when touching that logic.

## How to continue development

1. **Before editing**, skim `main.rs` top-to-bottom — drawing, key handling,
   and window-size sync are interleaved.
2. **Build with the full path**: `& "$env:USERPROFILE\.cargo\bin\cargo.exe"
   build --message-format=short`. The project is Windows-only; `#[cfg(windows)]`
   branches exist (e.g. `local_time_string`, `native_window_handle`,
   `load_icon_image`) — keep the non-Windows fallbacks compiling even if
   degraded, because `cargo check` on other platforms is useful.
3. **UI changes** belong in `main.rs`. Prefer extending a `draw_*` helper
   over adding another. If you add state that persists across frames, hang it
   off `SparkRunApp`, not `egui::Memory`.
4. **New indexable sources** (e.g. UWP apps, recent files) go in `index.rs`
   as a new `EntryKind` variant + push path. Update `entry_icon_palette`,
   `subtitle_for`, and `AppIconCache` lookup as needed.
5. **New `windows-sys` APIs** require adding their feature to
   `Cargo.toml`. `SYSTEMTIME` lives in `Win32_Foundation`, `GetLocalTime` in
   `Win32_System_SystemInformation` — use that as a template.
6. **Keep the floating-pill look**: any new top-level widget must live
   inside the inner rounded `Frame` in `update`. Don't re-enable window
   decorations or set an opaque clear color.
7. **Don't silently swallow `Result`s** from Win32 calls — the app already
   surfaces `hotkey_status` / `tray_status` in `self.status`; new
   registrations should do the same.
