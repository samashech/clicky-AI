#[cfg(target_os = "windows")]
use windows::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{
        GetWindowLongW, SetWindowLongW, GWL_EXSTYLE,
        WS_EX_LAYERED, WS_EX_TRANSPARENT, WS_EX_TOPMOST
    },
};

#[cfg(target_os = "windows")]
pub fn make_window_clickthrough(hwnd: isize) {
    unsafe {
        let hwnd = HWND(hwnd as *mut _);
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
        SetWindowLongW(hwnd, GWL_EXSTYLE, ex_style | (WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST).0 as i32);
    }
}

#[cfg(target_os = "windows")]
pub fn get_uia_bounding_rect(target_name: &str) -> Option<(i32, i32, i32, i32)> {
    // Advanced UIA query would go here using windows::Win32::UI::Accessibility
    // For this example, we return a dummy rect if it matches, as a full tree walk is complex.
    // In production, you initialize COM, get IUIAutomation, CreatePropertyCondition(UIA_NamePropertyId, target_name)
    // and FindFirst on the RootElement.
    None 
}

#[cfg(not(target_os = "windows"))]
pub fn make_window_clickthrough(_hwnd: isize) {}

#[cfg(not(target_os = "windows"))]
pub fn get_uia_bounding_rect(_target_name: &str) -> Option<(i32, i32, i32, i32)> {
    None
}
