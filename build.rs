use std::{env, fs, path::PathBuf};

fn generated_icon() -> PathBuf {
    let icon_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest")).join("icons");
    fs::create_dir_all(&icon_dir).expect("create icon directory");
    let path = icon_dir.join("icon.ico");
    let size = 32u32;
    let pixel_bytes = size * size * 4;
    let mask_bytes = size * 4;
    let image_bytes = 40 + pixel_bytes + mask_bytes;
    let mut data = Vec::with_capacity((22 + image_bytes) as usize);
    data.extend_from_slice(&[0, 0, 1, 0, 1, 0]);
    data.extend_from_slice(&[32, 32, 0, 0, 1, 0, 32, 0]);
    data.extend_from_slice(&image_bytes.to_le_bytes());
    data.extend_from_slice(&22u32.to_le_bytes());
    data.extend_from_slice(&40u32.to_le_bytes());
    data.extend_from_slice(&(size as i32).to_le_bytes());
    data.extend_from_slice(&((size * 2) as i32).to_le_bytes());
    data.extend_from_slice(&1u16.to_le_bytes());
    data.extend_from_slice(&32u16.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&pixel_bytes.to_le_bytes());
    data.extend_from_slice(&[0; 16]);
    for y in (0..size).rev() {
        for x in 0..size {
            let dx = x as i32 - 16;
            let dy = y as i32 - 16;
            let inside = dx * dx + dy * dy <= 14 * 14;
            let arrow = (y < 23 && y > 7 && (x as i32 - 16).abs() <= (23 - y) as i32 / 2)
                || ((18..=24).contains(&y) && (8..=24).contains(&x));
            let (b, g, r, a) = if arrow {
                (176, 240, 255, 255)
            } else if inside {
                (54, 89, 36, 255)
            } else {
                (0, 0, 0, 0)
            };
            data.extend_from_slice(&[b, g, r, a]);
        }
    }
    data.extend(std::iter::repeat_n(0, mask_bytes as usize));
    fs::write(&path, data).expect("write generated icon");
    path
}

fn main() {
    let windows = tauri_build::WindowsAttributes::new().window_icon_path(generated_icon());
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("failed to build Tauri resources");
}
