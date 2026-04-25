#[cfg(windows)]
mod platform {
    use std::{
        ptr::null_mut,
        sync::{
            mpsc::{self, Receiver},
            Mutex, OnceLock,
        },
        thread,
    };

    use eframe::egui;
    use windows_sys::Win32::{
        Foundation::{GetLastError, HWND, LPARAM, LRESULT, POINT, WPARAM},
        System::{LibraryLoader::GetModuleHandleW, Threading::GetCurrentThreadId},
        UI::{
            Shell::{
                Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD,
                NIM_DELETE, NIM_SETVERSION, NIN_SELECT, NOTIFYICONDATAW, NOTIFYICON_VERSION_4,
            },
            WindowsAndMessaging::{
                AppendMenuW, CreateIcon, CreatePopupMenu, CreateWindowExW, DefWindowProcW,
                DestroyIcon, DestroyMenu, DestroyWindow, DispatchMessageW, GetCursorPos,
                GetMessageW, LoadIconW, PostMessageW, PostThreadMessageW, RegisterClassW,
                SetForegroundWindow, TrackPopupMenu, TranslateMessage, CS_HREDRAW, CS_VREDRAW,
                HICON, IDI_APPLICATION, MF_STRING, MSG, TPM_BOTTOMALIGN, TPM_NONOTIFY,
                TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_CONTEXTMENU, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN,
                WM_LBUTTONUP, WM_NULL, WM_QUIT, WM_RBUTTONDBLCLK, WM_RBUTTONDOWN, WM_RBUTTONUP,
                WM_USER, WNDCLASSW, WS_OVERLAPPED,
            },
        },
    };

    const ICON_SIZE: usize = 32;
    const TRAY_ICON_ID: u32 = 1;
    const TRAY_CALLBACK_MESSAGE: u32 = WM_USER + 42;
    const TRAY_CLASS_NAME: &str = "SparkRunTrayWindow";
    const MENU_OPEN_ID: usize = 1001;
    const MENU_EXIT_ID: usize = 1002;

    static TRAY_SENDER: OnceLock<Mutex<Option<mpsc::Sender<TrayEvent>>>> = OnceLock::new();

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum TrayEvent {
        Show,
        Exit,
    }

    pub struct TrayIcon {
        receiver: Receiver<TrayEvent>,
        thread_id: u32,
        _worker: thread::JoinHandle<()>,
    }

    impl TrayIcon {
        pub fn register(ctx: egui::Context, _app_hwnd: Option<isize>) -> Result<Self, String> {
            let (event_sender, receiver) = mpsc::channel();
            let (ready_sender, ready_receiver) = mpsc::channel();

            let worker = thread::spawn(move || unsafe {
                let thread_id = GetCurrentThreadId();
                *TRAY_SENDER
                    .get_or_init(|| Mutex::new(None))
                    .lock()
                    .expect("tray sender mutex should not be poisoned") = Some(event_sender);
                let hinstance = GetModuleHandleW(null_mut()) as _;
                let generated_icon = create_spark_icon(hinstance);
                let tray_icon = if generated_icon.is_null() {
                    LoadIconW(null_mut(), IDI_APPLICATION)
                } else {
                    generated_icon
                };
                let class_name = wide_null(TRAY_CLASS_NAME);
                let window_class = WNDCLASSW {
                    style: CS_HREDRAW | CS_VREDRAW,
                    lpfnWndProc: Some(tray_window_proc),
                    cbClsExtra: 0,
                    cbWndExtra: 0,
                    hInstance: hinstance,
                    hIcon: tray_icon,
                    hCursor: null_mut(),
                    hbrBackground: null_mut(),
                    lpszMenuName: null_mut(),
                    lpszClassName: class_name.as_ptr(),
                };

                RegisterClassW(&window_class);

                let hwnd = CreateWindowExW(
                    0,
                    class_name.as_ptr(),
                    wide_null("Spark Run").as_ptr(),
                    WS_OVERLAPPED,
                    0,
                    0,
                    0,
                    0,
                    null_mut(),
                    null_mut(),
                    hinstance,
                    null_mut(),
                );

                if hwnd.is_null() {
                    let code = GetLastError();
                    let _ = ready_sender.send(Err(format!(
                        "Could not create tray icon window. Windows error code {code}."
                    )));
                    clear_sender();
                    destroy_generated_icon(generated_icon);
                    return;
                }

                let icon_data = notify_icon_data(hwnd, tray_icon);
                if Shell_NotifyIconW(NIM_ADD, &icon_data) == 0 {
                    let code = GetLastError();
                    DestroyWindow(hwnd);
                    clear_sender();
                    destroy_generated_icon(generated_icon);
                    let _ = ready_sender.send(Err(format!(
                        "Could not add Spark to the notification area. Windows error code {code}."
                    )));
                    return;
                }

                let mut version_data = icon_data;
                version_data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
                Shell_NotifyIconW(NIM_SETVERSION, &version_data);

                let _ = ready_sender.send(Ok(thread_id));

                let mut message = std::mem::zeroed::<MSG>();
                while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                    ctx.request_repaint();
                }

                Shell_NotifyIconW(NIM_DELETE, &icon_data);
                DestroyWindow(hwnd);
                clear_sender();
                destroy_generated_icon(generated_icon);
            });

            let thread_id = ready_receiver
                .recv()
                .map_err(|_| "Could not start tray icon listener.".to_string())??;

            Ok(Self {
                receiver,
                thread_id,
                _worker: worker,
            })
        }

        pub fn drain_events(&self) -> Vec<TrayEvent> {
            self.receiver.try_iter().collect()
        }
    }

    impl Drop for TrayIcon {
        fn drop(&mut self) {
            unsafe {
                PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0);
            }
        }
    }

    unsafe extern "system" fn tray_window_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if message == TRAY_CALLBACK_MESSAGE && is_tray_icon_message(wparam, lparam) {
            match tray_event_kind(lparam) {
                NativeTrayEvent::Show => {
                    show_and_notify();
                    return 0;
                }
                NativeTrayEvent::Menu => {
                    match show_context_menu(hwnd) {
                        MENU_OPEN_ID => show_and_notify(),
                        MENU_EXIT_ID => request_app_exit(),
                        _ => {}
                    }
                    return 0;
                }
                NativeTrayEvent::Ignore => {}
            }
        }

        DefWindowProcW(hwnd, message, wparam, lparam)
    }

    fn is_tray_icon_message(wparam: WPARAM, lparam: LPARAM) -> bool {
        let legacy_id = wparam as u32;
        let version_4_id = ((lparam as u32) >> 16) & 0xffff;
        legacy_id == TRAY_ICON_ID || version_4_id == TRAY_ICON_ID
    }

    unsafe fn notify_icon_data(hwnd: HWND, icon: HICON) -> NOTIFYICONDATAW {
        let mut data = std::mem::zeroed::<NOTIFYICONDATAW>();
        data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = hwnd;
        data.uID = TRAY_ICON_ID;
        data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP;
        data.uCallbackMessage = TRAY_CALLBACK_MESSAGE;
        data.hIcon = icon;
        write_tip(&mut data.szTip, "Spark Run");
        data
    }

    fn write_tip(destination: &mut [u16], value: &str) {
        for (index, unit) in value
            .encode_utf16()
            .take(destination.len().saturating_sub(1))
            .enumerate()
        {
            destination[index] = unit;
        }
    }

    enum NativeTrayEvent {
        Show,
        Menu,
        Ignore,
    }

    fn tray_event_kind(lparam: LPARAM) -> NativeTrayEvent {
        const NIN_KEYSELECT: u32 = NIN_SELECT + 1;
        let event = (lparam as u32) & 0xffff;

        match event {
            WM_LBUTTONDOWN | WM_LBUTTONUP | WM_LBUTTONDBLCLK | NIN_SELECT | NIN_KEYSELECT => {
                NativeTrayEvent::Show
            }
            WM_RBUTTONDOWN | WM_RBUTTONUP | WM_RBUTTONDBLCLK | WM_CONTEXTMENU => {
                NativeTrayEvent::Menu
            }
            _ => NativeTrayEvent::Ignore,
        }
    }

    unsafe fn show_context_menu(hwnd: HWND) -> usize {
        let menu = CreatePopupMenu();
        if menu.is_null() {
            return 0;
        }

        let open_label = wide_null("Open Spark");
        let exit_label = wide_null("Exit");
        AppendMenuW(menu, MF_STRING, MENU_OPEN_ID, open_label.as_ptr());
        AppendMenuW(menu, MF_STRING, MENU_EXIT_ID, exit_label.as_ptr());

        let mut point = POINT { x: 0, y: 0 };
        GetCursorPos(&mut point);

        SetForegroundWindow(hwnd);
        let command = TrackPopupMenu(
            menu,
            TPM_RIGHTBUTTON | TPM_RETURNCMD | TPM_NONOTIFY | TPM_BOTTOMALIGN,
            point.x,
            point.y,
            0,
            hwnd,
            null_mut(),
        ) as usize;
        PostMessageW(hwnd, WM_NULL, 0, 0);
        DestroyMenu(menu);
        command
    }

    unsafe fn show_and_notify() {
        notify_tray_event(TrayEvent::Show);
    }

    unsafe fn request_app_exit() {
        notify_tray_event(TrayEvent::Exit);
    }

    fn notify_tray_event(event: TrayEvent) {
        if let Some(sender) = TRAY_SENDER
            .get()
            .and_then(|sender| sender.lock().ok())
            .and_then(|sender| sender.clone())
        {
            let _ = sender.send(event);
        }
    }

    fn clear_sender() {
        if let Some(sender) = TRAY_SENDER.get() {
            if let Ok(mut sender) = sender.lock() {
                *sender = None;
            }
        }
    }

    fn wide_null(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    unsafe fn create_spark_icon(hinstance: windows_sys::Win32::Foundation::HINSTANCE) -> HICON {
        let rgba = crate::branding::spark_icon_rgba(ICON_SIZE as u32);
        let mut and_bits = vec![0xff_u8; ICON_SIZE * ICON_SIZE / 8];
        let mut xor_bits = vec![0_u8; ICON_SIZE * ICON_SIZE * 4];

        for y in 0..ICON_SIZE {
            for x in 0..ICON_SIZE {
                let src = (y * ICON_SIZE + x) * 4;
                let r = rgba[src];
                let g = rgba[src + 1];
                let b = rgba[src + 2];
                let a = rgba[src + 3];

                if a < 16 {
                    continue;
                }

                set_mask_bit(&mut and_bits, x, y, false);
                let dst = ((ICON_SIZE - 1 - y) * ICON_SIZE + x) * 4;
                xor_bits[dst] = b;
                xor_bits[dst + 1] = g;
                xor_bits[dst + 2] = r;
                xor_bits[dst + 3] = a;
            }
        }

        CreateIcon(
            hinstance,
            ICON_SIZE as i32,
            ICON_SIZE as i32,
            1,
            32,
            and_bits.as_ptr(),
            xor_bits.as_ptr(),
        )
    }

    fn set_mask_bit(bits: &mut [u8], x: usize, y: usize, transparent: bool) {
        let row = ICON_SIZE - 1 - y;
        let index = row * (ICON_SIZE / 8) + x / 8;
        let mask = 0x80 >> (x % 8);

        if transparent {
            bits[index] |= mask;
        } else {
            bits[index] &= !mask;
        }
    }

    unsafe fn destroy_generated_icon(icon: HICON) {
        if !icon.is_null() {
            DestroyIcon(icon);
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use eframe::egui;
    use std::sync::mpsc::{self, Receiver};

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum TrayEvent {
        Show,
        Exit,
    }

    pub struct TrayIcon {
        receiver: Receiver<TrayEvent>,
    }

    impl TrayIcon {
        pub fn register(_ctx: egui::Context, _app_hwnd: Option<isize>) -> Result<Self, String> {
            let (_sender, receiver) = mpsc::channel();
            Err("Tray icons are only supported on Windows.".to_string()).map(|_| Self { receiver })
        }

        pub fn drain_events(&self) -> Vec<TrayEvent> {
            self.receiver.try_iter().collect()
        }
    }
}

pub use platform::{TrayEvent, TrayIcon};
