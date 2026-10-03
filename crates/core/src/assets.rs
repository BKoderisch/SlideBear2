//! Content-adressierte Asset-Namen: gleiche Bilddaten → gleicher Dateiname, keine Duplikate.

/// FNV-1a 64 Bit, reicht zur Deduplizierung eigener Bilder.
pub fn content_hash(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// Dateiname im Asset-Ordner, z. B. `3f2a…c1.png`.
pub fn content_name(data: &[u8], ext: &str) -> String {
    let ext = ext.trim_start_matches('.').to_ascii_lowercase();
    let ext = if ext == "jpeg" { "jpg".to_string() } else { ext };
    format!("{:016x}.{ext}", content_hash(data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_names() {
        assert_eq!(content_name(b"abc", "PNG"), content_name(b"abc", "png"));
        assert_ne!(content_name(b"abc", "png"), content_name(b"abd", "png"));
        assert!(content_name(b"x", ".jpeg").ends_with(".jpg"));
    }
}
