# Spark Run

A small Rust proof of concept for a Windows "Power Run" style launcher.

## What it does

- Opens a compact command-palette window.
- Keeps running in the background when hidden.
- Indexes Start Menu shortcuts and executables on `PATH`.
- Fuzzy-filters entries as you type.
- Runs the selected item with `Enter`.
- Runs the selected item elevated with `Ctrl+Enter`.
- Hides after successfully launching a command.
- Hides with `Esc` or the window close button.
- Shows again and focuses search with global `Alt+D`.
- Falls back to launching the typed command if no indexed item is selected.
- Opens configurable `!` shortcuts.

## Shortcut Config

Spark loads shortcuts from `shortcuts.conf` in the current directory when that file exists. If it does not exist, Spark creates one at:

```text
%APPDATA%\Spark Run\shortcuts.conf
```

The format is simple:

```text
!g = "https://google.com/"
!g.search = "https://www.google.com/search?q={query}"
!gpt = "https://chatgpt.com/"
!gpt.search = "https://chatgpt.com/?q={query}"
```

Type `!g` to open the base URL. Type `!g rust egui` to use the optional `.search` URL, with `{query}` replaced by URL-encoded search text.

When a configured shortcut is followed by a space, Spark shows the shortcut as a compact chip while you type the remaining text. Pressing `Backspace` from an empty chip field swaps back to the textual shortcut so you can delete it normally.

Default shortcuts:

- `!g` opens Google.
- `!d` opens DuckDuckGo.
- `!git` opens GitHub.
- `!y` opens YouTube.
- `!gpt` opens ChatGPT; `!gpt text` passes text to ChatGPT.
- `!claude` opens Claude.

## Run

Install Rust first if `cargo` is not available:

```powershell
winget install Rustlang.Rustup
```

Then restart your terminal and run:

```powershell
cargo run
```

## Next useful steps

- Add a small settings file for pinned commands.
- Add command arguments in saved entries.
- Package it as an `.msi` or portable `.exe`.
