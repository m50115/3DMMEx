//! `stream://` URI scheme handler.
//!
//! Registers a custom WKWebView protocol so the frontend can load rendered
//! frames as `<img src="stream://localhost/frame/{scene}/{frame}">`.
//!
//! This eliminates the Phase 6a bottleneck (PNG encode + base64 + IPC):
//!   Before: GPU(3ms) + PNG-encode(37ms) + base64 + IPC JSON = ~40ms/frame (~25fps)
//!   After:  GPU(3ms) + BMP-encode(<1ms) + binary URI response = ~4ms/frame (~250fps theoretical)

use tauri::{http, Manager, Runtime, UriSchemeContext};

use crate::commands::{encode_rgba_to_bmp, render_to_rgba};
use crate::state::AppState;

/// Handler registered via `tauri::Builder::register_uri_scheme_protocol("stream", ...)`.
///
/// URL format: `stream://localhost/frame/{scene_idx}/{frame}`
/// Returns a 24-bit BMP image with Content-Type: image/bmp.
pub fn handle<R: Runtime>(
    ctx: UriSchemeContext<'_, R>,
    request: http::Request<Vec<u8>>,
) -> http::Response<Vec<u8>> {
    let path = request.uri().path(); // e.g. "/frame/0/5"
    let segments: Vec<&str> = path.trim_start_matches('/').split('/').collect();

    // Expect: ["frame", "{scene_idx}", "{frame}"] with optional "/{w}/{h}"
    // Full path: /frame/{scene}/{frame}/{w}/{h}
    if segments.len() < 3 || segments[0] != "frame" {
        return error_response(400, b"bad path: expected /frame/{scene}/{frame}");
    }

    let scene_idx: usize = match segments[1].parse() {
        Ok(v) => v,
        Err(_) => return error_response(400, b"bad scene index"),
    };
    let frame: i32 = match segments[2].parse() {
        Ok(v) => v,
        Err(_) => return error_response(400, b"bad frame index"),
    };

    // Optional w/h path segments; default to 640×480 for backwards compat.
    let width: u32 = segments.get(3).and_then(|s| s.parse().ok()).unwrap_or(640);
    let height: u32 = segments.get(4).and_then(|s| s.parse().ok()).unwrap_or(480);

    let app = ctx.app_handle();
    let state = app.state::<AppState>();

    let t0 = std::time::Instant::now();
    let rgba = match render_to_rgba(state.inner(), scene_idx, frame, width, height) {
        Ok(r) => r,
        Err(e) => {
            // Return a blank dark-gray frame instead of HTTP 500 so the frontend
            // <img> doesn't fire onError. The error is logged to stderr.
            eprintln!("[RENDER] scene={scene_idx} frame={frame} err={e}");
            vec![30u8; (width * height * 4) as usize] // dark gray RGBA
        }
    };
    let bmp = encode_rgba_to_bmp(&rgba, width, height);
    eprintln!(
        "[PERF-6b] scene={scene_idx} frame={frame} {width}x{height} total={}ms",
        t0.elapsed().as_millis()
    );

    http::Response::builder()
        .header("Content-Type", "image/bmp")
        .header("Cache-Control", "no-store")
        .body(bmp)
        .unwrap()
}

fn error_response(status: u16, body: &[u8]) -> http::Response<Vec<u8>> {
    http::Response::builder()
        .status(status)
        .header("Content-Type", "text/plain")
        .body(body.to_vec())
        .unwrap()
}
