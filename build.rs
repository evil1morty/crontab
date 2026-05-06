use image::{ImageBuffer, Rgba, RgbaImage};
use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=tauri.conf.json");

    let icons = Path::new("icons");
    fs::create_dir_all(icons).unwrap();

    let needs = [
        ("32x32.png", 32),
        ("128x128.png", 128),
        ("128x128@2x.png", 256),
        ("icon.png", 512),
    ];
    for (name, size) in needs {
        let path = icons.join(name);
        if !path.exists() {
            make_buf(size).save(&path).unwrap();
        }
    }

    let ico = icons.join("icon.ico");
    if !ico.exists() {
        write_ico(&ico);
    }

    tauri_build::build()
}

fn make_buf(size: u32) -> RgbaImage {
    let mut img = ImageBuffer::new(size, size);
    let cx = (size as f32 - 1.0) / 2.0;
    let cy = cx;

    let r_outer = size as f32 * 0.46;
    let r_outer_sq = r_outer * r_outer;
    let r_inner = size as f32 * 0.36;
    let r_inner_sq = r_inner * r_inner;

    let violet = Rgba([0x7c, 0x5c, 0xff, 0xff]);
    let dark = Rgba([0x16, 0x18, 0x21, 0xff]);
    let arm = Rgba([0xee, 0xee, 0xf3, 0xff]);
    let clear = Rgba([0, 0, 0, 0]);

    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let d2 = dx * dx + dy * dy;
            let p = if d2 <= r_outer_sq {
                if d2 <= r_inner_sq {
                    dark
                } else {
                    violet
                }
            } else {
                clear
            };
            img.put_pixel(x, y, p);
        }
    }

    let cxi = cx.round() as i32;
    let cyi = cy.round() as i32;
    let v_len = (size as f32 * 0.26) as i32;
    let h_len = (size as f32 * 0.20) as i32;
    let thick = ((size as f32 * 0.07) as i32).max(1);

    for y in (cyi - v_len)..=cyi {
        for x in (cxi - thick / 2)..=(cxi + thick / 2) {
            put_safe(&mut img, x, y, arm);
        }
    }
    for x in cxi..=(cxi + h_len) {
        for y in (cyi - thick / 2)..=(cyi + thick / 2) {
            put_safe(&mut img, x, y, arm);
        }
    }

    img
}

fn put_safe(img: &mut RgbaImage, x: i32, y: i32, p: Rgba<u8>) {
    if x >= 0 && y >= 0 && (x as u32) < img.width() && (y as u32) < img.height() {
        img.put_pixel(x as u32, y as u32, p);
    }
}

fn write_ico(path: &Path) {
    use image::codecs::ico::IcoEncoder;
    use image::{ExtendedColorType, ImageEncoder};
    let img = make_buf(64);
    let f = fs::File::create(path).unwrap();
    let encoder = IcoEncoder::new(f);
    encoder
        .write_image(img.as_raw(), 64, 64, ExtendedColorType::Rgba8)
        .unwrap();
}
