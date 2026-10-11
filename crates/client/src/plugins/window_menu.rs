//! Keeps Windows' window menu off the Alt key. A windowed window has a title
//! bar and its menu, and Windows takes Alt and a letter as that menu's
//! shortcut: the menu takes the keyboard, the game never sees the letter let
//! go, and a key pressed while help is held (`help::KEYCODE_HELP`, Right
//! Alt) stays down. Borderless and fullscreen windows have no menu. Once the
//! window exists its messages are hooked, and the request to open the menu
//! from the keyboard is dropped; Alt+F4 and Alt+Tab go another way and keep
//! working.

use bevy::{
    ecs::system::NonSendMarker,
    prelude::*,
    window::{PrimaryWindow, RawHandleWrapper},
};
use raw_window_handle::RawWindowHandle;
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    UI::{
        Shell::{DefSubclassProc, SetWindowSubclass},
        WindowsAndMessaging::{SC_KEYMENU, WM_SYSCOMMAND},
    },
};

pub struct WindowMenuPlugin;

impl Plugin for WindowMenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, hook);
    }
}

/// This hook's id among the window's subclasses.
const HOOK: usize = 0x414c54;

/// Hooks the primary window as it gets its handle, on the main thread, the
/// thread that owns the window, as a hook requires.
fn hook(windows: Query<&RawHandleWrapper, (With<PrimaryWindow>, Added<RawHandleWrapper>)>, _main: NonSendMarker) {
    for handle in &windows {
        let RawWindowHandle::Win32(win32) = handle.get_window_handle() else { continue };
        // SAFETY: the handle is the live primary window's, and this runs on
        // the thread that owns it.
        let hooked = unsafe { SetWindowSubclass(win32.hwnd.get() as HWND, Some(drop_keyboard_menu), HOOK, 0) };
        if hooked == 0 {
            warn!("could not keep the window menu off the Alt key");
        }
    }
}

/// Drops the menu Alt and a key would open; passes everything else on.
unsafe extern "system" fn drop_keyboard_menu(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, _: usize, _: usize) -> LRESULT {
    // A system command's low four bits are Windows' own.
    if msg == WM_SYSCOMMAND && wparam & 0xFFF0 == SC_KEYMENU as usize {
        return 0;
    }
    // SAFETY: passing the message on, as Windows hands it to a subclass.
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}
