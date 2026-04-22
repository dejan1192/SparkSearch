#[cfg(windows)]
mod platform {
    use std::{
        ptr::null_mut,
        sync::mpsc::{self, Receiver},
        thread,
    };

    use eframe::egui;
    use windows_sys::Win32::{
        Foundation::GetLastError,
        System::Threading::GetCurrentThreadId,
        UI::{
            Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey, MOD_ALT},
            WindowsAndMessaging::{
                BringWindowToTop, GetMessageW, PostThreadMessageW, SetForegroundWindow, ShowWindow,
                MSG, SW_RESTORE, SW_SHOW, WM_HOTKEY, WM_QUIT,
            },
        },
    };

    const ALT_D_HOTKEY_ID: i32 = 1;
    const VK_D: u32 = b'D' as u32;

    pub struct GlobalHotkey {
        receiver: Receiver<()>,
        thread_id: u32,
        _worker: thread::JoinHandle<()>,
    }

    impl GlobalHotkey {
        pub fn register_alt_d(ctx: egui::Context, hwnd: Option<isize>) -> Result<Self, String> {
            let (event_sender, receiver) = mpsc::channel();
            let (ready_sender, ready_receiver) = mpsc::channel();

            let worker = thread::spawn(move || unsafe {
                let thread_id = GetCurrentThreadId();

                if RegisterHotKey(null_mut(), ALT_D_HOTKEY_ID, MOD_ALT, VK_D) == 0 {
                    let code = GetLastError();
                    let _ = ready_sender.send(Err(format!(
                        "Could not register Alt+D global hotkey. Windows error code {code}."
                    )));
                    return;
                }

                let _ = ready_sender.send(Ok(thread_id));

                let mut message = std::mem::zeroed::<MSG>();
                while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {
                    if message.message == WM_HOTKEY && message.wParam == ALT_D_HOTKEY_ID as usize {
                        if let Some(hwnd) = hwnd {
                            let hwnd = hwnd as _;
                            ShowWindow(hwnd, SW_SHOW);
                            ShowWindow(hwnd, SW_RESTORE);
                            BringWindowToTop(hwnd);
                            SetForegroundWindow(hwnd);
                        }

                        let _ = event_sender.send(());
                        ctx.request_repaint();
                    }
                }

                UnregisterHotKey(null_mut(), ALT_D_HOTKEY_ID);
            });

            let thread_id = ready_receiver
                .recv()
                .map_err(|_| "Could not start Alt+D global hotkey listener.".to_string())??;

            Ok(Self {
                receiver,
                thread_id,
                _worker: worker,
            })
        }

        pub fn was_pressed(&self) -> bool {
            let mut pressed = false;

            while self.receiver.try_recv().is_ok() {
                pressed = true;
            }

            pressed
        }
    }

    impl Drop for GlobalHotkey {
        fn drop(&mut self) {
            unsafe {
                PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0);
            }
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use eframe::egui;

    pub struct GlobalHotkey;

    impl GlobalHotkey {
        pub fn register_alt_d(_ctx: egui::Context, _hwnd: Option<isize>) -> Result<Self, String> {
            Err("Global hotkeys are only supported on Windows.".to_string())
        }

        pub fn was_pressed(&self) -> bool {
            false
        }
    }
}

pub use platform::GlobalHotkey;
