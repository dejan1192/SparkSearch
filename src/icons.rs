use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::time::Duration;

use eframe::egui;

use crate::index::LauncherEntry;

const ICON_SIZE: usize = 64;

#[derive(Default)]
pub struct AppIconCache {
    icons: HashMap<String, Option<egui::TextureHandle>>,
}

enum FaviconState {
    Pending,
    Loaded(egui::TextureHandle),
    Failed,
}

pub enum FaviconLookup {
    Loading,
    Ready(egui::TextureHandle),
    Unavailable,
}

pub struct FaviconCache {
    state: HashMap<String, FaviconState>,
    tx: Sender<(String, Option<egui::ColorImage>)>,
    rx: Receiver<(String, Option<egui::ColorImage>)>,
}

impl Default for FaviconCache {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self {
            state: HashMap::new(),
            tx,
            rx,
        }
    }
}

impl FaviconCache {
    pub fn lookup(&mut self, ctx: &egui::Context, shortcut: &str, url: &str) -> FaviconLookup {
        while let Ok((key, image)) = self.rx.try_recv() {
            let entry = match image {
                Some(img) => FaviconState::Loaded(ctx.load_texture(
                    format!("favicon-{key}"),
                    img,
                    egui::TextureOptions::LINEAR,
                )),
                None => FaviconState::Failed,
            };
            self.state.insert(key, entry);
        }

        let Some(host) = host_from_url(url) else {
            self.state
                .insert(shortcut.to_string(), FaviconState::Failed);
            return FaviconLookup::Unavailable;
        };
        let key = host.to_ascii_lowercase();

        match self.state.get(&key) {
            Some(FaviconState::Loaded(tex)) => return FaviconLookup::Ready(tex.clone()),
            Some(FaviconState::Pending) => return FaviconLookup::Loading,
            Some(FaviconState::Failed) => return FaviconLookup::Unavailable,
            None => {}
        }

        if let Some(image) = read_cached_favicon(&host) {
            let texture = ctx.load_texture(
                format!("favicon-{key}"),
                image,
                egui::TextureOptions::LINEAR,
            );
            self.state
                .insert(key.clone(), FaviconState::Loaded(texture.clone()));
            return FaviconLookup::Ready(texture);
        }

        self.state.insert(key.clone(), FaviconState::Pending);
        let tx = self.tx.clone();
        let ctx_clone = ctx.clone();
        thread::spawn(move || {
            let image = fetch_cached_favicon(&host);
            let _ = tx.send((key, image));
            ctx_clone.request_repaint();
        });

        FaviconLookup::Loading
    }
}

fn host_from_url(url: &str) -> Option<String> {
    let without_scheme = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let host = without_scheme
        .split(|c: char| c == '/' || c == '?' || c == '#')
        .next()?;
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

fn fetch_cached_favicon(host: &str) -> Option<egui::ColorImage> {
    let path = favicon_cache_path(host);

    if let Some(image) = read_cached_favicon(host) {
        return Some(image);
    }

    let url = format!("https://www.google.com/s2/favicons?domain={host}&sz=64");
    let response = ureq::builder()
        .timeout(Duration::from_secs(6))
        .build()
        .get(&url)
        .call()
        .ok()?;
    let mut bytes = Vec::with_capacity(4096);
    response
        .into_reader()
        .take(256 * 1024)
        .read_to_end(&mut bytes)
        .ok()?;

    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&path, &bytes);

    color_image_from_encoded_bytes(&bytes)
}

fn read_cached_favicon(host: &str) -> Option<egui::ColorImage> {
    let bytes = fs::read(favicon_cache_path(host)).ok()?;
    color_image_from_encoded_bytes(&bytes)
}

fn color_image_from_encoded_bytes(bytes: &[u8]) -> Option<egui::ColorImage> {
    let decoded = image::load_from_memory(bytes).ok()?.to_rgba8();
    let size = [decoded.width() as usize, decoded.height() as usize];
    Some(egui::ColorImage::from_rgba_unmultiplied(
        size,
        decoded.as_raw(),
    ))
}

fn favicon_cache_path(host: &str) -> PathBuf {
    let base = env::var_os("APPDATA")
        .map(PathBuf::from)
        .map(|path| path.join("Spark Run").join("favicons"))
        .unwrap_or_else(|| PathBuf::from("favicons"));
    base.join(format!("{}.png", sanitize_cache_name(host)))
}

fn sanitize_cache_name(value: &str) -> String {
    let mut output = String::new();

    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
            output.push(ch.to_ascii_lowercase());
        } else {
            output.push('_');
        }
    }

    if output.is_empty() {
        "site".to_string()
    } else {
        output
    }
}

impl AppIconCache {
    pub fn texture_for(
        &mut self,
        ctx: &egui::Context,
        entry: &LauncherEntry,
    ) -> Option<egui::TextureHandle> {
        let key = entry.target.file.to_lowercase();

        if !self.icons.contains_key(&key) {
            let icon = load_icon_image(&entry.target.file).map(|image| {
                ctx.load_texture(
                    format!("app-icon-{key}"),
                    image,
                    egui::TextureOptions::LINEAR,
                )
            });
            self.icons.insert(key.clone(), icon);
        }

        self.icons.get(&key).and_then(Clone::clone)
    }
}

#[cfg(windows)]
fn load_icon_image(path: &str) -> Option<egui::ColorImage> {
    use std::{ffi::c_void, ptr::null_mut};

    use windows_sys::Win32::{
        Graphics::Gdi::{
            CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC,
            SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HGDIOBJ, RGBQUAD,
        },
        Storage::FileSystem::FILE_ATTRIBUTE_NORMAL,
        UI::{
            Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON},
            WindowsAndMessaging::{DestroyIcon, DrawIconEx, DI_NORMAL},
        },
    };

    unsafe {
        let mut shell_info = std::mem::zeroed::<SHFILEINFOW>();
        let path = wide_null(path);
        let result = SHGetFileInfoW(
            path.as_ptr(),
            FILE_ATTRIBUTE_NORMAL,
            &mut shell_info,
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        );

        if result == 0 || shell_info.hIcon.is_null() {
            return None;
        }

        let screen_dc = GetDC(null_mut());
        if screen_dc.is_null() {
            DestroyIcon(shell_info.hIcon);
            return None;
        }

        let memory_dc = CreateCompatibleDC(screen_dc);
        if memory_dc.is_null() {
            ReleaseDC(null_mut(), screen_dc);
            DestroyIcon(shell_info.hIcon);
            return None;
        }

        let mut bitmap_info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: ICON_SIZE as i32,
                biHeight: -(ICON_SIZE as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                biSizeImage: (ICON_SIZE * ICON_SIZE * 4) as u32,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [RGBQUAD {
                rgbBlue: 0,
                rgbGreen: 0,
                rgbRed: 0,
                rgbReserved: 0,
            }],
        };

        let mut bits: *mut c_void = null_mut();
        let bitmap = CreateDIBSection(
            screen_dc,
            &mut bitmap_info,
            DIB_RGB_COLORS,
            &mut bits,
            null_mut(),
            0,
        );

        if bitmap.is_null() || bits.is_null() {
            DeleteDC(memory_dc);
            ReleaseDC(null_mut(), screen_dc);
            DestroyIcon(shell_info.hIcon);
            return None;
        }

        let old_object = SelectObject(memory_dc, bitmap as HGDIOBJ);
        let drawn = DrawIconEx(
            memory_dc,
            0,
            0,
            shell_info.hIcon,
            ICON_SIZE as i32,
            ICON_SIZE as i32,
            0,
            null_mut(),
            DI_NORMAL,
        );

        let image = if drawn != 0 {
            Some(color_image_from_bgra(bits as *const u8))
        } else {
            None
        };

        SelectObject(memory_dc, old_object);
        DeleteObject(bitmap as HGDIOBJ);
        DeleteDC(memory_dc);
        ReleaseDC(null_mut(), screen_dc);
        DestroyIcon(shell_info.hIcon);

        image
    }
}

#[cfg(not(windows))]
fn load_icon_image(_path: &str) -> Option<egui::ColorImage> {
    None
}

#[cfg(windows)]
unsafe fn color_image_from_bgra(bits: *const u8) -> egui::ColorImage {
    let bytes = std::slice::from_raw_parts(bits, ICON_SIZE * ICON_SIZE * 4);
    let has_alpha = bytes.chunks_exact(4).any(|pixel| pixel[3] != 0);
    let mut rgba = Vec::with_capacity(bytes.len());

    for pixel in bytes.chunks_exact(4) {
        let blue = pixel[0];
        let green = pixel[1];
        let red = pixel[2];
        let alpha = if has_alpha {
            pixel[3]
        } else if red != 0 || green != 0 || blue != 0 {
            255
        } else {
            0
        };

        rgba.extend_from_slice(&[red, green, blue, alpha]);
    }

    egui::ColorImage::from_rgba_unmultiplied([ICON_SIZE, ICON_SIZE], &rgba)
}

#[cfg(windows)]
fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
