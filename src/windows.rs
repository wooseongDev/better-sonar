use std::{ffi::OsStr, mem::size_of_val, path::Path, thread};

use anyhow::{Context, Result};
use auto_launch::AutoLaunch;
use gpui::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::{
    Win32::{
        Foundation::{
            CloseHandle, ERROR_ALREADY_EXISTS, ERROR_SUCCESS, GetLastError, HANDLE, HWND, SetLastError, WAIT_OBJECT_0,
        },
        Graphics::Dwm::{DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DwmSetWindowAttribute},
        System::{
            Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW},
            Threading::{CreateEventW, CreateMutexW, INFINITE, SetEvent, WaitForSingleObject},
        },
        UI::WindowsAndMessaging::{IsIconic, SW_HIDE, SW_RESTORE, SW_SHOW, ShowWindow},
    },
    core::w,
};

use crate::app::Event;

pub const AUTOSTART_ARG: &str = "--autostart";
pub const PRODUCT_NAME: &str = "Better Sonar";

#[derive(Clone, Copy)]
pub enum InstallerKind {
    Nsis,
    Msi,
}

pub fn is_autostart() -> bool {
    std::env::args_os().any(|arg| arg == OsStr::new(AUTOSTART_ARG))
}

pub fn taskbar_uses_light_theme() -> bool {
    let mut light = 0u32;
    let mut bytes = size_of_val(&light) as u32;
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some((&raw mut light).cast()),
            Some(&raw mut bytes),
        )
    };
    result == ERROR_SUCCESS && light != 0
}

pub fn set_titlebar_colors(window: &Window, background: u32, foreground: u32) {
    let Some(hwnd) = hwnd(window) else {
        return;
    };
    for (attribute, rgb) in [(DWMWA_CAPTION_COLOR, background), (DWMWA_TEXT_COLOR, foreground)] {
        let colorref = rgb.swap_bytes() >> 8;
        // Older Windows versions use the system title bar when these attributes are unavailable.
        let _ = unsafe {
            DwmSetWindowAttribute(
                hwnd,
                attribute,
                (&raw const colorref).cast(),
                size_of_val(&colorref) as u32,
            )
        };
    }
}

pub fn autostart(path: &Path, installed: bool) -> Result<AutoLaunch> {
    let app_name = if installed { PRODUCT_NAME } else { "Better Sonar (dev)" };
    let path = path.to_str().context("잘못된 실행 파일 경로입니다")?;
    Ok(AutoLaunch::new(app_name, &format!("\"{path}\""), &[AUTOSTART_ARG]))
}

pub fn set_autostart(autostart: &AutoLaunch, enabled: bool) -> Result<()> {
    if enabled {
        autostart.enable()?;
    } else {
        autostart.disable()?;
    }
    Ok(())
}

pub struct Instance(HANDLE);

impl Instance {
    pub fn acquire(sender: async_channel::Sender<Event>, show: bool) -> Result<Option<Self>> {
        let event = unsafe { CreateEventW(None, false, false, w!("Local\\BetterSonar.OpenWindow"))? };
        let event = Self(event);
        unsafe { SetLastError(ERROR_SUCCESS) };
        let mutex = unsafe { CreateMutexW(None, false, w!("Local\\BetterSonar.Instance"))? };
        let already_running = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        let mutex = Self(mutex);
        if already_running {
            if show {
                unsafe { SetEvent(event.0)? };
            }
            return Ok(None);
        }

        thread::Builder::new()
            .name("open-window".into())
            .spawn(move || event.receive(sender))?;
        Ok(Some(mutex))
    }

    fn receive(self, sender: async_channel::Sender<Event>) {
        while unsafe { WaitForSingleObject(self.0, INFINITE) } == WAIT_OBJECT_0 {
            if sender.send_blocking(Event::Show).is_err() {
                break;
            }
        }
    }
}

// A Win32 event handle can be waited on or closed from a different thread.
unsafe impl Send for Instance {}

impl Drop for Instance {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn hwnd(window: &Window) -> Option<HWND> {
    let RawWindowHandle::Win32(handle) = HasWindowHandle::window_handle(window).ok()?.as_raw() else {
        return None;
    };
    Some(HWND(handle.hwnd.get() as _))
}

pub fn hide(window: &Window) {
    if let Some(hwnd) = hwnd(window) {
        let _ = unsafe { ShowWindow(hwnd, SW_HIDE) };
    }
}

pub fn show(window: &Window) {
    if let Some(hwnd) = hwnd(window) {
        let command = if unsafe { IsIconic(hwnd) }.as_bool() {
            SW_RESTORE
        } else {
            SW_SHOW
        };
        let _ = unsafe { ShowWindow(hwnd, command) };
    }
    window.activate_window();
}

pub fn installer_kind(exe: &Path) -> Option<InstallerKind> {
    let contents = std::fs::read_to_string(exe.with_file_name("installer-kind")).ok()?;
    match contents.trim() {
        "nsis" => Some(InstallerKind::Nsis),
        "msi" => Some(InstallerKind::Msi),
        _ => None,
    }
}
