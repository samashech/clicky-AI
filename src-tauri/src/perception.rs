use crate::task::{Bounds, UIElement};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Deserialize;
use std::{io::Cursor, path::PathBuf, process::Stdio, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Response {
    elements: Vec<UIElement>,
    error: Option<String>,
}

pub async fn ocr(
    script: PathBuf,
    image: Vec<u8>,
    origin: (f64, f64),
) -> Result<Vec<UIElement>, String> {
    if image.len() > 8 * 1024 * 1024 {
        return Err("Capture exceeds local image budget".into());
    }
    let python = crate::voice::python();
    let mut command = tokio::process::Command::new(python);
    command
        .arg(script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let mut child = command.spawn().map_err(|_| {
        "Local OCR requires Python with Pillow, pytesseract, and Tesseract".to_string()
    })?;
    let payload = serde_json::to_vec(&serde_json::json!({"image_base64":STANDARD.encode(image)}))
        .map_err(|e| e.to_string())?;
    let work = async {
        let mut input = child.stdin.take().ok_or("Worker stdin unavailable")?;
        input.write_all(&payload).await.map_err(|e| e.to_string())?;
        drop(input);
        let mut output = Vec::new();
        child
            .stdout
            .take()
            .ok_or("Worker stdout unavailable")?
            .take(512 * 1024 + 1)
            .read_to_end(&mut output)
            .await
            .map_err(|e| e.to_string())?;
        if output.len() > 512 * 1024 {
            return Err("OCR response exceeds budget".into());
        }
        let exit = child.wait().await.map_err(|e| e.to_string())?;
        if !exit.success() {
            return Err("OCR worker exited unsuccessfully".into());
        }
        let mut response: Response =
            serde_json::from_slice(&output).map_err(|_| "Invalid OCR contract".to_string())?;
        if let Some(error) = response.error {
            return Err(format!(
                "OCR unavailable: {error}. Check diagnostics and dependencies."
            ));
        }
        if response.elements.len() > 500 {
            return Err("Too many OCR elements".into());
        }
        for e in &mut response.elements {
            e.bounds.x += origin.0;
            e.bounds.y += origin.1;
            if !e.bounds.valid() || !(0.0..=1.0).contains(&e.confidence) {
                return Err("Invalid OCR bounds".into());
            }
        }
        Ok(response.elements)
    };
    tokio::time::timeout(Duration::from_secs(20), work)
        .await
        .map_err(|_| "OCR timeout; worker terminated".to_string())?
}

pub fn capture(point: (f64, f64)) -> Result<(Vec<u8>, (f64, f64)), String> {
    if crate::platform::wayland() {
        return Err("Native Wayland capture is not yet integrated. No XWayland screenshot is used as a full desktop capture.".into());
    }
    let active = crate::platform::active_bounds().filter(|r| r.valid());
    let point = active
        .map(|r| (r.x + r.width / 2., r.y + r.height / 2.))
        .unwrap_or(point);
    let screen = screenshots::Screen::from_point(point.0 as i32, point.1 as i32)
        .map_err(|e| e.to_string())?;
    let monitor = Bounds {
        x: screen.display_info.x as f64,
        y: screen.display_info.y as f64,
        width: screen.display_info.width as f64,
        height: screen.display_info.height as f64,
    };
    let roi = active.unwrap_or(monitor);
    let x = roi.x.max(monitor.x);
    let y = roi.y.max(monitor.y);
    let right = (roi.x + roi.width).min(monitor.x + monitor.width);
    let bottom = (roi.y + roi.height).min(monitor.y + monitor.height);
    if right <= x || bottom <= y {
        return Err("Active window is outside the pointer monitor".into());
    }
    #[cfg(windows)]
    let image = screen
        .capture_area_ignore_area_check(
            (x - monitor.x) as i32,
            (y - monitor.y) as i32,
            (right - x) as u32,
            (bottom - y) as u32,
        )
        .map_err(|e| e.to_string())?;
    #[cfg(not(windows))]
    let image = screen
        .capture_area(
            (x - monitor.x) as i32,
            (y - monitor.y) as i32,
            (right - x) as u32,
            (bottom - y) as u32,
        )
        .map_err(|e| e.to_string())?;
    let mut bytes = Cursor::new(Vec::new());
    image
        .write_to(&mut bytes, screenshots::image::ImageOutputFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok((bytes.into_inner(), (x, y)))
}

pub fn native(hint: &str) -> Option<UIElement> {
    crate::platform::get_uia_bounding_box(hint).map(|(x, y, width, height)| UIElement {
        id: "uia-target".into(),
        role: "control".into(),
        name: hint.into(),
        bounds: Bounds {
            x,
            y,
            width,
            height,
        },
        confidence: 1.,
        source: "uia".into(),
        actionable: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_oversized_frame_before_spawning() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(ocr(
            PathBuf::from("missing-worker"),
            vec![0; 8 * 1024 * 1024 + 1],
            (0., 0.),
        ));
        assert!(result.unwrap_err().contains("budget"));
    }
    #[test]
    fn python_worker_invalid_image_exits_with_typed_error() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../omni_server/main.py");
        let result = rt.block_on(ocr(script, vec![0, 1, 2], (-1920., 0.)));
        assert!(result.unwrap_err().contains("OCR unavailable"));
    }
}
