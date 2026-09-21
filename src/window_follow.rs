use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetBounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[cfg(target_os = "windows")]
mod win {
    use super::TargetBounds;
    use std::sync::Mutex;
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM, RECT};
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowTextLengthW, GetWindowTextW, IsIconic, IsWindow, IsWindowVisible,
    };

    struct Cache {
        needle: String,
        hwnd: Option<isize>,
    }

    static CACHE: Mutex<Cache> = Mutex::new(Cache {
        needle: String::new(),
        hwnd: None,
    });

    fn bounds_for(hwnd: HWND) -> Option<TargetBounds> {
        unsafe {
            if !IsWindow(Some(hwnd)).as_bool()
                || !IsWindowVisible(hwnd).as_bool()
                || IsIconic(hwnd).as_bool()
            {
                return None;
            }
            let mut rect = RECT::default();
            DwmGetWindowAttribute(
                hwnd,
                DWMWA_EXTENDED_FRAME_BOUNDS,
                &mut rect as *mut _ as *mut _,
                std::mem::size_of::<RECT>() as u32,
            )
            .ok()?;
            let width = (rect.right - rect.left).max(0) as u32;
            let height = (rect.bottom - rect.top).max(0) as u32;
            (width > 0 && height > 0).then_some(TargetBounds {
                x: rect.left,
                y: rect.top,
                width,
                height,
            })
        }
    }

    fn title_matches(hwnd: HWND, needle: &str) -> bool {
        unsafe {
            let len = GetWindowTextLengthW(hwnd);
            if len <= 0 {
                return false;
            }
            let mut text = vec![0u16; len as usize + 1];
            let written = GetWindowTextW(hwnd, &mut text);
            let title = String::from_utf16_lossy(&text[..written as usize]).to_lowercase();
            title.contains(needle)
        }
    }

    fn find_hwnd(needle: &str) -> Option<HWND> {
        struct Search<'a> {
            needle: &'a str,
            result: Mutex<Option<HWND>>,
        }
        unsafe extern "system" fn visit(hwnd: HWND, state: LPARAM) -> BOOL {
            let search = &*(state.0 as *const Search<'_>);
            if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
                return BOOL(1);
            }
            if title_matches(hwnd, search.needle) {
                *search.result.lock().expect("window search lock") = Some(hwnd);
                return BOOL(0);
            }
            BOOL(1)
        }
        let search = Search {
            needle,
            result: Mutex::new(None),
        };
        unsafe {
            let _ = EnumWindows(Some(visit), LPARAM(&search as *const _ as isize));
        }
        search.result.into_inner().ok().flatten()
    }

    pub fn find_window_bounds(title_fragment: &str) -> Option<TargetBounds> {
        let needle = title_fragment.trim().to_lowercase();
        if needle.is_empty() {
            return None;
        }
        let mut cache = CACHE.lock().ok()?;
        if cache.needle != needle {
            cache.needle = needle.clone();
            cache.hwnd = None;
        }
        if let Some(raw) = cache.hwnd {
            let hwnd = HWND(raw as *mut _);
            if title_matches(hwnd, &needle) {
                if let Some(bounds) = bounds_for(hwnd) {
                    return Some(bounds);
                }
            }
            cache.hwnd = None;
        }
        let hwnd = find_hwnd(&needle)?;
        cache.hwnd = Some(hwnd.0 as isize);
        bounds_for(hwnd)
    }
}

#[cfg(target_os = "windows")]
pub fn find_window_bounds(title_fragment: &str) -> Option<TargetBounds> {
    win::find_window_bounds(title_fragment)
}

#[cfg(not(target_os = "windows"))]
pub fn find_window_bounds(_title_fragment: &str) -> Option<TargetBounds> {
    None
}
