// Win32 plugin host window — creates an OS window that CLAP plugins can use
// as a parent for their embedded GUI.
//
// Two modes:
// - `WindowMode::TopLevel`: a free-floating top-level window (no parent). Used
//   for the embedded fallback when the plugin only supports embedded mode and
//   the caller didn't provide an eframe HWND. The user can move it
//   independently of htrk.
// - `WindowMode::ChildOf(parent)`: a `WS_CHILD` window inside the eframe main
//   window (or any other parent). The plugin's child HWND is sized to fill
//   the host's client area. Resizes follow the parent's WM_SIZE events.
//
// WndProc:
// - Forwards `WM_SIZE` to the plugin by calling `MoveWindow` on the
//   plugin's child HWND (the plugin reparents itself into our HWND).
// - `WM_CLOSE` triggers `DestroyWindow`. Drop also destroys the window.

#![cfg(windows)]

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetWindow, GW_CHILD, IsWindowVisible,
    MoveWindow, RegisterClassExW, SetWindowLongPtrW, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT,
    GWLP_USERDATA, WM_DESTROY, WM_SIZE, WNDCLASSEXW, WS_CHILD, WS_EX_TOOLWINDOW,
    WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};

const HOST_CLASS_NAME: &str = "htrk_plugin_host\0";

/// How a `PluginHostWindow` is positioned in the window hierarchy.
#[derive(Clone, Copy, Debug)]
pub enum WindowMode {
    /// A free-floating top-level window with no parent. Used as a fallback
    /// when an embedded-mode plugin is loaded but no eframe HWND is
    /// available (e.g. in tests).
    TopLevel,
    /// A `WS_CHILD` window inside the given parent HWND. Used in the
    /// editor-embedded mode where the plugin lives inside htrk's main
    /// window.
    ChildOf(HWND),
}

/// Handle to a Win32 window used as the parent for an embedded plugin GUI.
/// `Drop` calls `DestroyWindow` to clean up.
pub struct PluginHostWindow {
    hwnd: HWND,
    mode: WindowMode,
    /// Set if a real window was actually created (vs. a stub).
    real: bool,
}

// SAFETY: HWND is a pointer to a window we own. We never share this across
// threads (the window is created and destroyed on the main thread).
unsafe impl Send for PluginHostWindow {}

impl PluginHostWindow {
    /// Create a window titled `title`. The window mode determines whether it
    /// is a top-level free-floating window or a child of the given parent.
    /// `width` and `height` are ignored for child windows (they get sized by
    /// the parent's layout). Returns `None` on failure.
    pub fn create(title: &str, mode: WindowMode, width: i32, height: i32) -> Option<Self> {
        // SAFETY: `GetModuleHandleW(NULL)` is always safe to call and returns
        // a handle to the current executable's module, or NULL.
        let hinstance = unsafe { GetModuleHandleW(std::ptr::null()) };
        if hinstance.is_null() {
            return None;
        }

        // Register the window class. Idempotent.
        let class_name_w: Vec<u16> = HOST_CLASS_NAME.encode_utf16().collect();
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinstance as _,
            lpszClassName: class_name_w.as_ptr(),
            ..unsafe { std::mem::zeroed() }
        };
        let _ = unsafe { RegisterClassExW(&wc) };

        let mut title_w: Vec<u16> = title.encode_utf16().collect();
        title_w.push(0);

        let (parent_hwnd, style, ex_style) = match mode {
            WindowMode::TopLevel => (std::ptr::null_mut(), WS_OVERLAPPEDWINDOW | WS_VISIBLE, WS_EX_TOOLWINDOW),
            WindowMode::ChildOf(p) => (p, WS_CHILD | WS_VISIBLE, 0u32),
        };

        // For child windows, position/size are set by the parent layout.
        // For top-level, use CW_USEDEFAULT position with caller-specified size.
        let (x, y, w, h) = match mode {
            WindowMode::TopLevel => (CW_USEDEFAULT, CW_USEDEFAULT, width, height),
            WindowMode::ChildOf(_) => (0, 0, 0, 0),
        };

        let hwnd_raw = unsafe {
            CreateWindowExW(
                ex_style,
                class_name_w.as_ptr(),
                title_w.as_ptr(),
                style,
                x, y, w, h,
                parent_hwnd,
                std::ptr::null_mut(),
                hinstance as _,
                std::ptr::null_mut(),
            )
        };
        if hwnd_raw.is_null() {
            return None;
        }

        Some(PluginHostWindow {
            hwnd: hwnd_raw as HWND,
            mode,
            real: true,
        })
    }

    /// Create a stub (no real window). Used for non-Windows or test contexts.
    pub fn stub() -> Self {
        PluginHostWindow {
            hwnd: std::ptr::null_mut(),
            mode: WindowMode::TopLevel,
            real: false,
        }
    }

    /// Returns the HWND to pass to the plugin as the parent window.
    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    /// Returns the window mode (top-level or child-of).
    pub fn mode(&self) -> WindowMode {
        self.mode
    }

    /// Returns true if this is a real (top-level or child) window.
    pub fn is_real(&self) -> bool {
        self.real
    }

    /// Resize the window. Used by the UI when the editor panel is resized
    /// (only meaningful for child windows).
    pub fn resize(&self, width: i32, height: i32) {
        if !self.real || self.hwnd.is_null() {
            return;
        }
        // SAFETY: We own this HWND.
        unsafe {
            MoveWindow(self.hwnd, 0, 0, width, height, 1);
        }
    }

    /// Returns true if the OS still considers this window visible.
    /// Used by the UI to detect when the user X-closed the window externally.
    pub fn is_visible(&self) -> bool {
        if !self.real || self.hwnd.is_null() {
            return false;
        }
        // SAFETY: The HWND is valid (we own it).
        unsafe { IsWindowVisible(self.hwnd) != 0 }
    }
}

impl Drop for PluginHostWindow {
    fn drop(&mut self) {
        if self.real && !self.hwnd.is_null() {
            // SAFETY: We own this HWND. `DestroyWindow` posts WM_DESTROY.
            unsafe {
                DestroyWindow(self.hwnd);
            }
        }
    }
}

/// Window procedure for the plugin host window. Forwards `WM_SIZE` to the
/// plugin's child HWND via `MoveWindow`.
extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let handled: LRESULT = match msg {
        WM_SIZE => {
            // On resize, find the plugin's child HWND and resize it to fill
            // our client area. CLAP plugins reparent their GUI into our
            // HWND via set_parent, so the child window is the plugin's UI.
            let width = (lparam & 0xFFFF) as i32;
            let height = ((lparam >> 16) & 0xFFFF) as i32;
            // SAFETY: GetWindow and MoveWindow are safe to call for our HWND.
            unsafe {
                let child = GetWindow(hwnd, GW_CHILD);
                if !child.is_null() {
                    MoveWindow(child, 0, 0, width, height, 1);
                }
            }
            0
        }
        WM_DESTROY => {
            // Clear the user data pointer to prevent stale references.
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            }
            0
        }
        _ => -1, // sentinel: fall through to DefWindowProcW
    };
    if handled != -1 {
        return handled;
    }
    // SAFETY: `DefWindowProcW` is always safe to call for unhandled messages.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// Extract the HWND of the eframe main window (if running on Windows).
/// Returns `None` if the platform doesn't expose one (e.g. Linux/macOS or
/// a platform where `raw-window-handle 0.6` isn't available).
pub fn get_eframe_hwnd(frame: &eframe::Frame) -> Option<HWND> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let handle = frame.window_handle().ok()?;
    match handle.as_raw() {
        RawWindowHandle::Win32(w) => Some(w.hwnd.get() as *mut _),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stub_creation() {
        let stub = PluginHostWindow::stub();
        assert!(!stub.is_real());
        assert!(stub.hwnd().is_null());
        assert!(!stub.is_visible());
    }

    #[test]
    fn test_create_top_level_window() {
        let win = PluginHostWindow::create("test", WindowMode::TopLevel, 320, 240);
        assert!(win.is_some());
        let win = win.unwrap();
        assert!(win.is_real());
        // The window is WS_VISIBLE so it should report visible.
        assert!(win.is_visible());
    }
}
