//! Where the text should go: a snapshot of the focused window, taken the moment the shortcut is
//! pressed, and the means to bring that window back if something else took focus meanwhile.

use std::time::{Duration, Instant};
use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, RECT};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
use windows::Win32::Security::{
    GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, TokenIntegrityLevel, TOKEN_MANDATORY_LABEL,
    TOKEN_QUERY,
};
use windows::Win32::System::Threading::{
    AttachThreadInput, GetCurrentProcess, GetCurrentThreadId, OpenProcess, OpenProcessToken, QueryFullProcessImageNameW,
    PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, GetAncestor, GetClassNameW, GetForegroundWindow, GetGUIThreadInfo,
    GetWindowThreadProcessId, IsIconic, IsWindow, SetForegroundWindow, ShowWindow, GA_ROOT, GUITHREADINFO, SW_RESTORE,
};
use super::keys;

/// The window (and control) that had keyboard focus.
#[derive(Debug, Clone, Default)]
pub struct Target {
    pub hwnd: isize,
    /// The focused control inside it, when the app uses real child windows (classic Win32).
    pub focus_hwnd: isize,
    pub pid: u32,
    /// Executable file name, lowercase ("slack.exe").
    pub exe: String,
    pub window_class: String,
    pub focus_class: String,
    /// The app runs with higher rights (as administrator), so Windows blocks typing into it.
    pub elevated: bool,
}

pub fn hwnd(raw: isize) -> HWND {
    HWND(raw as *mut core::ffi::c_void)
}

fn class_name(h: HWND) -> String {
    let mut buf = [0u16; 128];
    // SAFETY: writes into the local buffer.
    let len = unsafe { GetClassNameW(h, &mut buf) };
    String::from_utf16_lossy(&buf[..len.max(0) as usize])
}

/// Closes a process or token handle when dropped.
struct Owned(HANDLE);

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: the handle was opened by us and is closed once.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn exe_name(process: HANDLE) -> Option<String> {
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    // SAFETY: the buffer and its length are passed together.
    unsafe { QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len) }.ok()?;
    let path = String::from_utf16_lossy(&buf[..len as usize]);
    path.rsplit(['\\', '/']).next().map(|n| n.to_lowercase())
}

/// The mandatory integrity level of a process (Medium = 0x2000, High = 0x3000), if readable.
fn integrity(process: HANDLE) -> Option<u32> {
    // SAFETY: the token handle is closed by `Owned`; the buffer is sized by the first call.
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(process, TOKEN_QUERY, &mut token).ok()?;
        let token = Owned(token);
        let mut needed = 0u32;
        let _ = GetTokenInformation(token.0, TokenIntegrityLevel, None, 0, &mut needed);
        if needed == 0 {
            return None;
        }
        let mut buf = vec![0u8; needed as usize];
        GetTokenInformation(token.0, TokenIntegrityLevel, Some(buf.as_mut_ptr().cast()), needed, &mut needed).ok()?;
        let label = &*(buf.as_ptr() as *const TOKEN_MANDATORY_LABEL);
        let count = *GetSidSubAuthorityCount(label.Label.Sid);
        if count == 0 {
            return None;
        }
        Some(*GetSidSubAuthority(label.Label.Sid, (count - 1) as u32))
    }
}

fn own_integrity() -> Option<u32> {
    // SAFETY: the pseudo handle of this process needs no closing.
    integrity(unsafe { GetCurrentProcess() })
}

/// Snapshot the window that has keyboard focus right now.
pub fn snapshot() -> Option<Target> {
    // SAFETY: read-only queries about other windows; handles are closed by `Owned`.
    unsafe {
        let foreground = GetForegroundWindow();
        if foreground.0.is_null() {
            return None;
        }
        let mut pid = 0u32;
        let thread_id = GetWindowThreadProcessId(foreground, Some(&mut pid));
        let mut info = GUITHREADINFO { cbSize: std::mem::size_of::<GUITHREADINFO>() as u32, ..Default::default() };
        let focus = if GetGUIThreadInfo(thread_id, &mut info).is_ok() && !info.hwndFocus.0.is_null() {
            info.hwndFocus
        } else {
            foreground
        };
        let (exe, elevated) = match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(process) => {
                let process = Owned(process);
                let exe = exe_name(process.0).unwrap_or_default();
                // A process whose token we cannot read runs with rights above ours.
                let elevated = match (integrity(process.0), own_integrity()) {
                    (Some(theirs), Some(ours)) => theirs > ours,
                    (None, Some(ours)) => ours < 0x3000,
                    _ => false,
                };
                (exe, elevated)
            }
            Err(_) => (String::new(), own_integrity().is_some_and(|ours| ours < 0x3000)),
        };
        Some(Target {
            hwnd: foreground.0 as isize,
            focus_hwnd: focus.0 as isize,
            pid,
            exe,
            window_class: class_name(foreground),
            focus_class: class_name(focus),
            elevated,
        })
    }
}

impl Target {
    pub fn still_exists(&self) -> bool {
        // SAFETY: plain query.
        unsafe { IsWindow(Some(hwnd(self.hwnd))).as_bool() }
    }

    /// The desktop, the taskbar or Start: there is no text field to type into.
    pub fn is_shell(&self) -> bool {
        matches!(
            self.window_class.as_str(),
            "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "Windows.UI.Core.CoreWindow"
        ) && matches!(self.exe.as_str(), "explorer.exe" | "searchhost.exe" | "startmenuexperiencehost.exe" | "")
    }

    /// A classic Win32 edit control, which takes text directly (no clipboard, no keystrokes).
    pub fn is_classic_edit(&self) -> bool {
        self.focus_hwnd != 0 && is_edit_class(&self.focus_class)
    }
}

/// Whether `class` is a classic edit control's window class.
pub fn is_edit_class(class: &str) -> bool {
    let c = class.to_ascii_lowercase();
    c == "edit" || c.starts_with("richedit") || c == "richeditd2dpt"
}

/// The focused window right now (its top-level ancestor).
pub fn foreground() -> isize {
    // SAFETY: plain queries.
    unsafe {
        let f = GetForegroundWindow();
        if f.0.is_null() {
            return 0;
        }
        GetAncestor(f, GA_ROOT).0 as isize
    }
}

/// Bring `target`'s window back to the front, with its focus. Windows only lets the app the user
/// is working with change the foreground window, so this borrows the current foreground thread's
/// input state for the switch, as accessibility tools do.
pub fn activate(target: &Target) -> bool {
    let window = hwnd(target.hwnd);
    if foreground() == target.hwnd {
        return true;
    }
    // SAFETY: plain window management calls; the input attachment is always undone.
    unsafe {
        if IsIconic(window).as_bool() {
            let _ = ShowWindow(window, SW_RESTORE);
        }
        // A key event of our own counts as recent input, which Windows weighs when it decides
        // whether a foreground change is allowed.
        keys::tap(keys::VK_DUMMY);
        let current = GetForegroundWindow();
        let current_thread = if current.0.is_null() { 0 } else { GetWindowThreadProcessId(current, None) };
        let me = GetCurrentThreadId();
        let attached = current_thread != 0 && current_thread != me && AttachThreadInput(me, current_thread, true).as_bool();
        let _ = BringWindowToTop(window);
        let _ = SetForegroundWindow(window);
        if attached {
            let _ = AttachThreadInput(me, current_thread, false);
        }
    }
    let deadline = Instant::now() + Duration::from_millis(400);
    while Instant::now() < deadline {
        if foreground() == target.hwnd {
            // Give the app a moment to put the caret back in its field.
            std::thread::sleep(Duration::from_millis(60));
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

/// The work area (screen minus taskbar) of the monitor showing `window`, in physical pixels,
/// and that monitor's scale factor.
pub fn work_area(window: isize) -> Option<(RECT, f64)> {
    // SAFETY: plain queries into local structs.
    unsafe {
        let monitor = MonitorFromWindow(hwnd(window), MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return None;
        }
        let (mut dpi_x, mut dpi_y) = (96u32, 96u32);
        let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
        Some((info.rcWork, dpi_x.max(96) as f64 / 96.0))
    }
}

/// A friendly name for the pill: "Slack", "VS Code", or the executable without ".exe".
pub fn app_label(exe: &str) -> Option<String> {
    let name = exe.trim_end_matches(".exe");
    if name.is_empty() {
        return None;
    }
    let known = match name {
        "chrome" => "Chrome",
        "msedge" => "Edge",
        "firefox" => "Firefox",
        "brave" => "Brave",
        "opera" => "Opera",
        "arc" => "Arc",
        "code" => "VS Code",
        "cursor" => "Cursor",
        "windsurf" => "Windsurf",
        "devenv" => "Visual Studio",
        "winword" => "Word",
        "excel" => "Excel",
        "powerpnt" => "PowerPoint",
        "outlook" | "olk" => "Outlook",
        "onenote" => "OneNote",
        "slack" => "Slack",
        "teams" | "ms-teams" => "Teams",
        "discord" => "Discord",
        "whatsapp" | "whatsapp.root" => "WhatsApp",
        "telegram" => "Telegram",
        "signal" => "Signal",
        "zoom" => "Zoom",
        "notion" => "Notion",
        "obsidian" => "Obsidian",
        "notepad" => "Notepad",
        "notepad++" => "Notepad++",
        "windowsterminal" | "wt" => "Terminal",
        "cmd" | "conhost" | "openconsole" => "Command Prompt",
        "powershell" | "pwsh" => "PowerShell",
        "explorer" => "File Explorer",
        "talkr" => "Talkr",
        "mstsc" | "msrdc" => "Remote Desktop",
        "thunderbird" => "Thunderbird",
        "figma" => "Figma",
        "chatgpt" => "ChatGPT",
        "claude" => "Claude",
        "applicationframehost" => return None,
        _ => "",
    };
    if !known.is_empty() {
        return Some(known.to_string());
    }
    let mut chars = name.chars();
    let first = chars.next()?;
    Some(first.to_uppercase().chain(chars).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels() {
        assert_eq!(app_label("slack.exe").as_deref(), Some("Slack"));
        assert_eq!(app_label("code.exe").as_deref(), Some("VS Code"));
        assert_eq!(app_label("myeditor.exe").as_deref(), Some("Myeditor"));
        assert_eq!(app_label(""), None);
        assert_eq!(app_label("applicationframehost.exe"), None);
    }

    #[test]
    fn edit_classes() {
        assert!(is_edit_class("Edit"));
        assert!(is_edit_class("RICHEDIT50W"));
        assert!(is_edit_class("RichEditD2DPT"));
        assert!(!is_edit_class("Chrome_RenderWidgetHostHWND"));
    }

    #[test]
    fn snapshot_of_this_session_does_not_panic() {
        // Headless CI may have no foreground window at all; either way it must not crash.
        if let Some(t) = snapshot() {
            assert!(t.hwnd != 0);
        }
    }
}
