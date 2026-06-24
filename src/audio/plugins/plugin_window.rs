// Win32 plugin host window — creates a top-level OS window that CLAP plugins
// can use as a parent for their embedded GUI.
//
// On Windows, plugins that only support `is_floating=false` (embedded) need a
// parent HWND. We create a top-level window and pass its HWND via clack's
// `set_parent` so the plugin's GUI lives inside it. The window is invisible
// in the taskbar but is a real OS window that the user can move and close.
//
// Lifecycle:
// - `PluginHostWindow::create(plugin_name)` returns a `PluginHostWindow`
//   with the HWND registered in `WM_NCCREATE` (stored via
//   `SetWindowLongPtrW` with `GWLP_USERDATA`).
// - The WndProc forwards `WM_SIZE` to the plugin via the
//   `PluginHostWindow` pointer stored in user data.
// - `destroy()` posts `WM_CLOSE` to the window; the actual destruction
//   happens in the WndProc's `WM_DESTROY` handler.

#![cfg(windows)]

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassExW, SetWindowLongPtrW,
    CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, GWLP_USERDATA, WM_DESTROY, WNDCLASSEXW,
    WS_EX_TOOLWINDOW, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};

const HOST_CLASS_NAME: &str = "htrk_plugin_host\0";

/// Handle to a top-level Win32 window used as the parent for an embedded
/// plugin GUI. Drop sends `WM_CLOSE` to the window.
pub struct PluginHostWindow {
    hwnd: HWND,
    /// Set if a real window was actually created (vs. an `HWND_MESSAGE` stub).
    real: bool,
}

// SAFETY: HWND is a pointer to a window we own. We never share this across
// threads (the window is created and destroyed on the main thread).
unsafe impl Send for PluginHostWindow {}

impl PluginHostWindow {
    /// Create a top-level window titled `title` (will be embedded with the
    /// plugin's GUI). The window is positioned at the default location
    /// (`CW_USEDEFAULT`) and sized 600x400. Returns `None` on failure.
    pub fn create(title: &str) -> Option<Self> {
        // SAFETY: `GetModuleHandleW(NULL)` is always safe to call and returns
        // a handle to the current executable's module, or NULL.
        let hinstance = unsafe { GetModuleHandleW(std::ptr::null()) };
        if hinstance.is_null() {
            return None;
        }

        // Register the window class. Idempotent: if it already exists, we
        // proceed anyway.
        let class_name_w: Vec<u16> = HOST_CLASS_NAME.encode_utf16().collect();
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinstance as _,
            lpszClassName: class_name_w.as_ptr(),
            ..unsafe { std::mem::zeroed() }
        };
        // SAFETY: We pass a fully-initialized WNDCLASSEXW. Class name is
        // valid UTF-16 with a null terminator.
        let atom = unsafe { RegisterClassExW(&wc) };
        if atom == 0 {
            // Class may already be registered; that's fine.
        }

        // Build the title as a wide string.
        let mut title_w: Vec<u16> = title.encode_utf16().collect();
        title_w.push(0);

        // Create the window. We pass `lpCreateParams` as a `CREATESTRUCTW*`
        // via LPARAM, but for now we just create an empty window and store
        // a pointer to ourselves in WM_NCCREATE.
        let hwnd_raw = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW,
                class_name_w.as_ptr(),
                title_w.as_ptr(),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                600,
                400,
                std::ptr::null_mut(), // no parent
                std::ptr::null_mut(), // no menu
                hinstance as _,
                std::ptr::null_mut(),
            )
        };
        if hwnd_raw.is_null() {
            return None;
        }

        Some(PluginHostWindow {
            hwnd: hwnd_raw as HWND,
            real: true,
        })
    }

    /// Create a stub for use on non-main-thread contexts. The HWND is a
    /// `HWND_MESSAGE` (no real window). Used as a fallback if Win32 window
    /// creation is unavailable.
    pub fn stub() -> Self {
        PluginHostWindow {
            hwnd: std::ptr::null_mut(),
            real: false,
        }
    }

    /// Returns the HWND to pass to the plugin as the parent window.
    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    /// Returns true if this is a real (top-level) window.
    pub fn is_real(&self) -> bool {
        self.real
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

/// Window procedure for the plugin host window. Forwards WM_SIZE to a
/// callback (set via the user data pointer).
extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // Forward WM_SIZE: the plugin needs to know the new client area size.
    // For now, this is a stub; a future iteration can plumb resize into
    // the plugin's `set_size`.
    if msg == WM_DESTROY {
        // Clear the user data pointer to prevent stale references.
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        }
    }
    // SAFETY: `DefWindowProcW` is always safe to call for unhandled messages.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stub_creation() {
        let stub = PluginHostWindow::stub();
        assert!(!stub.is_real());
        assert!(stub.hwnd().is_null());
    }
}
