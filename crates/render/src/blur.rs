//! Schneller Weichzeichner: drei Box-Blur-Durchgänge nähern einen Gauß-Blur an.

/// Weichzeichnet einen Puffer mit `channels` Kanälen pro Pixel in-place.
/// `radius` entspricht ungefähr der Standardabweichung in Pixeln.
pub fn blur(data: &mut [u8], width: usize, height: usize, channels: usize, radius: f32) {
    if radius < 0.5 || width == 0 || height == 0 {
        return;
    }
    for box_r in box_radii(radius) {
        if box_r == 0 {
            continue;
        }
        pass(data, width, height, channels, box_r, true);
        pass(data, width, height, channels, box_r, false);
    }
}

/// Box-Radien für drei Durchgänge, siehe "Fastest Gaussian Blur" (Ivan Kutskir).
fn box_radii(sigma: f32) -> [usize; 3] {
    let n = 3.0;
    let w_ideal = (12.0 * sigma * sigma / n + 1.0).sqrt();
    let mut wl = w_ideal.floor() as i32;
    if wl % 2 == 0 {
        wl -= 1;
    }
    let wu = wl + 2;
    let m_ideal = (12.0 * sigma * sigma - n * (wl * wl) as f32 - 4.0 * n * wl as f32 - 3.0 * n) / (-4.0 * wl as f32 - 4.0);
    let m = m_ideal.round() as i32;
    let mut out = [0usize; 3];
    for (i, r) in out.iter_mut().enumerate() {
        let size = if (i as i32) < m { wl } else { wu };
        *r = ((size - 1) / 2).max(0) as usize;
    }
    out
}

fn pass(data: &mut [u8], width: usize, height: usize, ch: usize, r: usize, horizontal: bool) {
    let (len, lines) = if horizontal { (width, height) } else { (height, width) };
    let stride = if horizontal { ch } else { width * ch };
    let line_step = if horizontal { width * ch } else { ch };
    let mut src = vec![0u8; len * ch];
    let div = (2 * r + 1) as u32;

    for line in 0..lines {
        let base = line * line_step;
        for i in 0..len {
            let o = base + i * stride;
            src[i * ch..i * ch + ch].copy_from_slice(&data[o..o + ch]);
        }
        for c in 0..ch {
            let at = |i: isize| -> u32 { src[(i.clamp(0, len as isize - 1) as usize) * ch + c] as u32 };
            let mut acc: u32 = (-(r as isize)..=r as isize).map(at).sum();
            for i in 0..len {
                data[base + i * stride + c] = ((acc + div / 2) / div) as u8;
                acc += at(i as isize + r as isize + 1);
                acc -= at(i as isize - r as isize);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blur_spreads_and_preserves_mass() {
        let (w, h) = (21, 21);
        let mut d = vec![0u8; w * h];
        d[10 * w + 10] = 255;
        let before: u32 = d.iter().map(|&v| v as u32).sum();
        blur(&mut d, w, h, 1, 1.5);
        let after: u32 = d.iter().map(|&v| v as u32).sum();
        assert!(d[10 * w + 10] < 255);
        assert!(d[10 * w + 12] > 0);
        assert!((before as i32 - after as i32).abs() < 40);
    }
}
