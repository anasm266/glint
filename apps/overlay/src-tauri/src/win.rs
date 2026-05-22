//! Win32 helpers: process tree walk + foreground-window focus.

#[cfg(windows)]
use std::mem::{size_of, zeroed};

#[cfg(windows)]
use windows_sys::Win32::Foundation::{CloseHandle, HWND, INVALID_HANDLE_VALUE, LPARAM, BOOL};
#[cfg(windows)]
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
#[cfg(windows)]
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, EnumWindows, GetWindowThreadProcessId,
    IsWindowVisible, SetForegroundWindow, ShowWindow, SW_RESTORE,
};

#[cfg(windows)]
pub fn root_codex_pid(pid: u32) -> Option<u32> {
    // Known Codex Desktop process names.
    let candidates = [
        "Codex.exe",
        "codex.exe",
        "Codex Desktop.exe",
        "openai-codex.exe",
        "openai.codex.exe",
    ];
    let map = process_map()?;

    // Walk the process tree upward from the hook's parent.
    let mut current = pid;
    for _ in 0..16 {
        let entry = map.iter().find(|e| e.pid == current)?;
        if candidates.iter().any(|c| entry.name.eq_ignore_ascii_case(c)) {
            return Some(entry.pid);
        }
        current = entry.parent_pid;
        if current == 0 {
            break;
        }
    }

    // Fall back: find any running process whose name contains "codex".
    map.iter()
        .find(|e| e.name.to_lowercase().contains("codex"))
        .map(|e| e.pid)
}

#[cfg(windows)]
pub fn root_cursor_pid(pid: u32) -> Option<u32> {
    let candidates = [
        "Cursor.exe",
        "cursor.exe",
        "Cursor Helper.exe",
        "Cursor Helper (Renderer).exe",
    ];
    let map = process_map()?;

    let mut current = pid;
    for _ in 0..16 {
        let entry = map.iter().find(|e| e.pid == current)?;
        if candidates
            .iter()
            .any(|c| entry.name.eq_ignore_ascii_case(c))
        {
            return Some(entry.pid);
        }
        current = entry.parent_pid;
        if current == 0 {
            break;
        }
    }

    map.iter()
        .find(|e| e.name.to_lowercase().contains("cursor"))
        .map(|e| e.pid)
}

#[cfg(windows)]
pub fn focus_pid(pid: u32) -> bool {
    focus_window_for_pid(pid, "Codex")
}

#[cfg(windows)]
pub fn focus_cursor(pid: u32) -> bool {
    focus_window_for_pid(pid, "Cursor")
}

#[cfg(windows)]
fn focus_window_for_pid(pid: u32, title_fragment: &str) -> bool {
    let hwnd = find_top_window_for_pid(pid)
        .or_else(|| find_window_by_title_fragment(title_fragment));
    let Some(hwnd) = hwnd else {
        return false;
    };
    unsafe {
        let _ = AllowSetForegroundWindow(pid);
        let _ = ShowWindow(hwnd, SW_RESTORE);
        SetForegroundWindow(hwnd) != 0
    }
}

#[cfg(windows)]
struct ProcEntry {
    pid: u32,
    parent_pid: u32,
    name: String,
}

#[cfg(windows)]
fn process_map() -> Option<Vec<ProcEntry>> {
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return None;
        }
        let mut entry: PROCESSENTRY32W = zeroed();
        entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        let mut out = vec![];
        if Process32FirstW(snap, &mut entry) != 0 {
            loop {
                let len = entry
                    .szExeFile
                    .iter()
                    .position(|c| *c == 0)
                    .unwrap_or(entry.szExeFile.len());
                let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
                out.push(ProcEntry {
                    pid: entry.th32ProcessID,
                    parent_pid: entry.th32ParentProcessID,
                    name,
                });
                if Process32NextW(snap, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snap);
        Some(out)
    }
}

#[cfg(windows)]
fn find_window_by_title_fragment(fragment: &str) -> Option<HWND> {
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowTextW;

    struct Search {
        fragment: Vec<u16>,
        hwnd: HWND,
    }

    let fragment_lower: String = fragment.to_lowercase();
    let mut s = Search {
        fragment: fragment_lower.encode_utf16().collect(),
        hwnd: std::ptr::null_mut(),
    };

    unsafe extern "system" fn cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let s = &mut *(lparam as *mut Search);
        let mut buf = [0u16; 256];
        let len = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        if len > 0 && IsWindowVisible(hwnd) != 0 {
            let title = String::from_utf16_lossy(&buf[..len as usize]).to_lowercase();
            let needle = String::from_utf16_lossy(&s.fragment);
            if title.contains(needle.as_str()) {
                s.hwnd = hwnd;
                return 0;
            }
        }
        1
    }

    let _ = s.fragment.len(); // ensure fragment is used
    unsafe {
        EnumWindows(Some(cb), &mut s as *mut Search as LPARAM);
    }
    if s.hwnd.is_null() { None } else { Some(s.hwnd) }
}

#[cfg(windows)]
fn find_top_window_for_pid(pid: u32) -> Option<HWND> {
    struct Search {
        pid: u32,
        hwnd: HWND,
    }
    let mut s = Search { pid, hwnd: std::ptr::null_mut() };

    unsafe extern "system" fn cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let s = &mut *(lparam as *mut Search);
        let mut owner: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut owner);
        if owner == s.pid && IsWindowVisible(hwnd) != 0 {
            s.hwnd = hwnd;
            return 0; // stop
        }
        1
    }

    unsafe {
        EnumWindows(Some(cb), &mut s as *mut Search as LPARAM);
    }
    if s.hwnd.is_null() { None } else { Some(s.hwnd) }
}

#[cfg(not(windows))]
pub fn root_codex_pid(_pid: u32) -> Option<u32> {
    None
}

#[cfg(not(windows))]
pub fn root_cursor_pid(_pid: u32) -> Option<u32> {
    None
}

#[cfg(not(windows))]
pub fn focus_pid(_pid: u32) -> bool {
    false
}

#[cfg(not(windows))]
pub fn focus_cursor(_pid: u32) -> bool {
    false
}
