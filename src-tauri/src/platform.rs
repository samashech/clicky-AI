#[cfg(target_os = "windows")]
pub fn get_uia_bounding_box(target_name: &str) -> Option<(f64, f64, f64, f64)> {
    use windows::core::BSTR;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_MULTITHREADED,
    };
    use windows::Win32::System::Variant::*;
    use windows::Win32::UI::Accessibility::*;
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    struct Com;
    impl Drop for Com {
        fn drop(&mut self) {
            unsafe {
                CoUninitialize();
            }
        }
    }
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        let _com = Com;
        let automation: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
        // Limit matching to the foreground application, never the entire desktop.
        let hwnd = GetForegroundWindow();
        if hwnd.0 == 0 {
            return None;
        }
        let root = automation.ElementFromHandle(hwnd).ok()?;
        let mut variant: VARIANT = std::mem::zeroed();
        variant.Anonymous.Anonymous = std::mem::ManuallyDrop::new(VARIANT_0_0 {
            vt: VT_BSTR,
            wReserved1: 0,
            wReserved2: 0,
            wReserved3: 0,
            Anonymous: VARIANT_0_0_0 {
                bstrVal: std::mem::ManuallyDrop::new(BSTR::from(target_name)),
            },
        });
        let condition = automation.CreatePropertyConditionEx(
            UIA_NamePropertyId,
            variant.clone(),
            PropertyConditionFlags_IgnoreCase,
        );
        let _ = VariantClear(&mut variant);
        let matches = root.FindAll(TreeScope_Subtree, &condition.ok()?).ok()?;
        let count = matches.Length().ok()?;
        if count > 100 {
            return None;
        }
        let mut visible = Vec::new();
        for i in 0..count {
            let element = matches.GetElement(i).ok()?;
            if element.CurrentIsOffscreen().ok()?.as_bool()
                || !element.CurrentIsEnabled().ok()?.as_bool()
            {
                continue;
            }
            let rect = element.CurrentBoundingRectangle().ok()?;
            if rect.right > rect.left && rect.bottom > rect.top {
                visible.push((
                    rect.left as f64,
                    rect.top as f64,
                    (rect.right - rect.left) as f64,
                    (rect.bottom - rect.top) as f64,
                ));
            }
        }
        if visible.len() == 1 {
            Some(visible[0])
        } else {
            None
        }
    }
}

#[cfg(target_os = "windows")]
pub fn active_bounds() -> Option<crate::task::Bounds> {
    use windows::Win32::{
        Foundation::RECT,
        UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowRect},
    };
    unsafe {
        let hwnd = GetForegroundWindow();
        let mut r = RECT::default();
        GetWindowRect(hwnd, &mut r).ok()?;
        Some(crate::task::Bounds {
            x: r.left as f64,
            y: r.top as f64,
            width: (r.right - r.left) as f64,
            height: (r.bottom - r.top) as f64,
        })
    }
}
#[cfg(not(target_os = "windows"))]
pub fn active_bounds() -> Option<crate::task::Bounds> {
    None
}

#[cfg(not(target_os = "windows"))]
pub fn get_uia_bounding_box(_: &str) -> Option<(f64, f64, f64, f64)> {
    None
}

pub fn wayland() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
        || std::env::var("XDG_SESSION_TYPE").is_ok_and(|s| s == "wayland")
}
pub fn diagnostics() -> serde_json::Value {
    let wayland = wayland();
    let hyprland = crate::hyprland::available();
    serde_json::json!({
        "os":std::env::consts::OS,
        "session":if cfg!(windows){"windows"}else if wayland{"wayland"}else{"x11"},
        "desktop":std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default(),
        "hyprland":std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some(),
        "global_input":if wayland{"unavailable: native Wayland input observation is not integrated"}else{"backend available; runtime initialization reported separately"},
        "capture":if hyprland{"grim active-window OCR, compositor logical coordinates"}else if wayland{"unavailable: portal capture not integrated"}else{"on-demand monitor capture; runtime permission required"},
        "accessibility":if cfg!(windows){"UIA exact-name lookup"}else{"AT-SPI not integrated; OCR fallback on X11"},
        "overlay":if hyprland{"GTK layer-shell highlight and instruction panel; requires desktop Python dependencies"}else if wayland{"unverified: compositor controls positioning and stacking"}else{"click-through native window"},
        "perception_worker":"on-demand JSON stdio; Python/Pillow/pytesseract/Tesseract required",
        "verification":if hyprland{"explicit user confirmation; global clicks are not observed"}else{"platform-dependent geometric hits"},
        "cloud_screenshots":false,
        "history_storage":false
    })
}
