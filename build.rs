use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let assets_dir = manifest_dir.join("assets");
    let _ = fs::create_dir_all(&assets_dir);

    let ico_path = assets_dir.join("app_icon.ico");
    let ico_bytes = build_y_logo_ico(&[16, 32, 48, 64, 128]);
    fs::write(&ico_path, ico_bytes).expect("failed to write assets/app_icon.ico");

    #[cfg(target_os = "windows")]
    {
        let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
        let rc_path = out_dir.join("app_icon.rc");
        let res_path = out_dir.join("app_icon.res");

        let ico_escaped = ico_path.to_string_lossy().replace('\\', "\\\\");
        fs::write(&rc_path, format!("1 ICON \"{ico_escaped}\"\n"))
            .expect("failed to write app_icon.rc");

        if let Some(rc_exe) = find_rc_exe() {
            let status = Command::new(rc_exe)
                .args(["/nologo", "/fo"])
                .arg(&res_path)
                .arg(&rc_path)
                .status();

            if let Ok(st) = status
                && st.success()
                && res_path.exists()
            {
                println!("cargo:rustc-link-arg-bins={}", res_path.display());
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn find_rc_exe() -> Option<PathBuf> {
    if let Ok(out) = Command::new("where.exe").arg("rc.exe").output()
        && out.status.success()
    {
        let stdout = String::from_utf8_lossy(&out.stdout);
        if let Some(first) = stdout.lines().next() {
            let p = PathBuf::from(first.trim());
            if p.exists() {
                return Some(p);
            }
        }
    }

    let kits = Path::new(r"C:\Program Files (x86)\Windows Kits\10\bin");
    let mut versions: Vec<PathBuf> = fs::read_dir(kits)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    versions.sort();
    versions.reverse();
    for ver in versions {
        let candidate = ver.join("x64").join("rc.exe");
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

/// Renders our symmetrical 4-square Y emblem on a dark obsidian rounded tile
/// into a multi-resolution Windows `.ico` (32-bit BGRA DIB frames).
fn build_y_logo_ico(sizes: &[u32]) -> Vec<u8> {
    let mut frames: Vec<(u32, Vec<u8>)> = Vec::new();
    for &sz in sizes {
        frames.push((sz, render_dib_frame(sz)));
    }

    let count = frames.len() as u16;
    let mut out = Vec::new();
    // ICONDIR header (6 bytes)
    out.extend_from_slice(&0u16.to_le_bytes()); // reserved
    out.extend_from_slice(&1u16.to_le_bytes()); // type = ICO
    out.extend_from_slice(&count.to_le_bytes());

    let mut offset = 6u32 + (frames.len() as u32) * 16;
    for (sz, data) in &frames {
        let dim = if *sz >= 256 { 0u8 } else { *sz as u8 };
        out.push(dim); // width
        out.push(dim); // height
        out.push(0); // palette
        out.push(0); // reserved
        out.extend_from_slice(&1u16.to_le_bytes()); // color planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bpp
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += data.len() as u32;
    }

    for (_, data) in frames {
        out.extend_from_slice(&data);
    }
    out
}

fn render_dib_frame(size: u32) -> Vec<u8> {
    let w = size as i32;
    let h = size as i32;
    let s = size as f32;

    // 40-byte BITMAPINFOHEADER (height is doubled for XOR + AND mask in ICO format)
    let mut buf = Vec::with_capacity(40 + (size * size * 4) as usize + ((size * size) / 8) as usize);
    buf.extend_from_slice(&40u32.to_le_bytes()); // biSize
    buf.extend_from_slice(&w.to_le_bytes()); // biWidth
    buf.extend_from_slice(&(h * 2).to_le_bytes()); // biHeight (XOR + AND)
    buf.extend_from_slice(&1u16.to_le_bytes()); // biPlanes
    buf.extend_from_slice(&32u16.to_le_bytes()); // biBitCount
    buf.extend_from_slice(&0u32.to_le_bytes()); // biCompression = BI_RGB
    buf.extend_from_slice(&(size * size * 4).to_le_bytes()); // biSizeImage
    buf.extend_from_slice(&0i32.to_le_bytes());
    buf.extend_from_slice(&0i32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());

    let radius = s * 0.22;
    let border = (s * 0.04).max(1.0);

    // Symmetrical 4-square Y layout in normalized [0..1] coordinates:
    // Square size = 0.20, centered horizontally at 0.50:
    // Top-left:  [0.18..0.38, 0.20..0.40]
    // Top-right: [0.62..0.82, 0.20..0.40]
    // Center:    [0.40..0.60, 0.42..0.62] (electric cyan #00FFFF)
    // Stem:      [0.40..0.60, 0.64..0.84] (white)
    let sq_r = 0.035_f32;

    // DIB pixels are stored bottom-up (y = h - 1 down to 0) in BGRA order
    for y_inv in 0..size {
        let y = (size - 1 - y_inv) as f32 + 0.5;
        let ny = y / s;
        for x_i in 0..size {
            let x = x_i as f32 + 0.5;
            let nx = x / s;

            // Signed distance to rounded background tile
            let bg_sd = rounded_box_sd(x - s * 0.5, y - s * 0.5, s * 0.48, s * 0.48, radius);
            if bg_sd > 0.75 {
                buf.extend_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            let bg_alpha = (0.75 - bg_sd).clamp(0.0, 1.0);

            // Base obsidian background (#0B0B0E) + hairline border (#2A2A35)
            let is_border = bg_sd > -border;
            let (mut r, mut g, mut b) = if is_border {
                (0x2A_u8, 0x2A_u8, 0x35_u8)
            } else {
                (0x0B_u8, 0x0B_u8, 0x0E_u8)
            };

            let in_top_left = rounded_box_sd(nx - 0.28, ny - 0.30, 0.10, 0.10, sq_r) <= 0.0;
            let in_top_right = rounded_box_sd(nx - 0.72, ny - 0.30, 0.10, 0.10, sq_r) <= 0.0;
            let in_center = rounded_box_sd(nx - 0.50, ny - 0.52, 0.10, 0.10, sq_r) <= 0.0;
            let in_stem = rounded_box_sd(nx - 0.50, ny - 0.74, 0.10, 0.10, sq_r) <= 0.0;

            if in_center {
                r = 0x00;
                g = 0xFF;
                b = 0xFF;
            } else if in_top_left || in_top_right || in_stem {
                r = 0xFA;
                g = 0xFA;
                b = 0xFC;
            }

            let a = (bg_alpha * 255.0).round() as u8;
            buf.extend_from_slice(&[b, g, r, a]);
        }
    }

    // AND mask (1 bit per pixel, padded to 4-byte row stride) — all 0s since we use 32-bit alpha
    let row_bytes = ((size + 31) / 32) * 4;
    buf.resize(buf.len() + (row_bytes * size) as usize, 0);
    buf
}

fn rounded_box_sd(px: f32, py: f32, bx: f32, by: f32, r: f32) -> f32 {
    let qx = px.abs() - bx + r;
    let qy = py.abs() - by + r;
    let out = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    let inside = qx.max(qy).min(0.0);
    out + inside - r
}
