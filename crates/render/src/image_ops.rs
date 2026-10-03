//! Bilder laden, auf den Rahmen einpassen und filtern.

use std::path::Path;

use image::RgbaImage;
use image::imageops::{self, FilterType};
use slidebear_core::scene::{Filter, ImageFit};
use tiny_skia::{IntSize, Pixmap};

use crate::blur::blur;

/// Ein fertiges Bild samt Versatz relativ zur linken oberen Rahmenecke.
pub struct Placed {
    pub pixmap: Pixmap,
    pub offset_x: i32,
    pub offset_y: i32,
}

pub fn prepare(path: &Path, fw: u32, fh: u32, fit: ImageFit, filters: &[Filter], scale: f32) -> Option<Placed> {
    let src = image::open(path).ok()?.to_rgba8();
    let (iw, ih) = (src.width() as f32, src.height() as f32);
    if iw < 1.0 || ih < 1.0 {
        return None;
    }

    let (mut img, ox, oy) = match fit {
        ImageFit::Stretch => (imageops::resize(&src, fw, fh, FilterType::Triangle), 0, 0),
        ImageFit::Cover => {
            let s = (fw as f32 / iw).max(fh as f32 / ih);
            let (rw, rh) = (((iw * s).round() as u32).max(fw), ((ih * s).round() as u32).max(fh));
            let resized = imageops::resize(&src, rw, rh, FilterType::Triangle);
            let cx = (rw - fw) / 2;
            let cy = (rh - fh) / 2;
            (imageops::crop_imm(&resized, cx, cy, fw, fh).to_image(), 0, 0)
        }
        ImageFit::Contain => {
            let s = (fw as f32 / iw).min(fh as f32 / ih);
            let (rw, rh) = (((iw * s).round() as u32).max(1), ((ih * s).round() as u32).max(1));
            let resized = imageops::resize(&src, rw, rh, FilterType::Triangle);
            (resized, (fw as i32 - rw as i32) / 2, (fh as i32 - rh as i32) / 2)
        }
    };

    for f in filters {
        apply_filter(&mut img, f, scale);
    }

    Some(Placed { pixmap: to_pixmap(img)?, offset_x: ox, offset_y: oy })
}

pub fn apply_filter(img: &mut RgbaImage, filter: &Filter, scale: f32) {
    match *filter {
        Filter::Darken { amount } => {
            let k = 1.0 - amount.clamp(0.0, 1.0);
            for p in img.pixels_mut() {
                for c in &mut p.0[..3] {
                    *c = (*c as f32 * k).round() as u8;
                }
            }
        }
        Filter::Tint { color, amount } => {
            let a = amount.clamp(0.0, 1.0);
            let t = [color.r, color.g, color.b];
            for p in img.pixels_mut() {
                for (c, t) in p.0[..3].iter_mut().zip(t) {
                    *c = (*c as f32 * (1.0 - a) + t as f32 * a).round() as u8;
                }
            }
        }
        Filter::Grayscale => {
            for p in img.pixels_mut() {
                let [r, g, b, _] = p.0;
                let l = (0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32).round() as u8;
                p.0[..3].copy_from_slice(&[l, l, l]);
            }
        }
        Filter::Blur { radius } => {
            let (w, h) = (img.width() as usize, img.height() as usize);
            blur(img.as_mut(), w, h, 4, radius * scale);
        }
    }
}

fn to_pixmap(img: RgbaImage) -> Option<Pixmap> {
    let (w, h) = img.dimensions();
    let mut data = img.into_raw();
    for px in data.as_chunks_mut::<4>().0 {
        let a = px[3] as u16;
        if a < 255 {
            for c in &mut px[..3] {
                *c = ((*c as u16 * a + 127) / 255) as u8;
            }
        }
    }
    Pixmap::from_vec(data, IntSize::from_wh(w, h)?)
}
