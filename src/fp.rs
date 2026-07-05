// Audio fingerprints for golden regression tests (tier3 §2.1):
// sr, n_samples, per-1024-block rms_l/rms_r/peak, xxh64 of the full i16 PCM.

pub fn xxh64(data: &[u8], seed: u64) -> u64 {
    const P1: u64 = 0x9E3779B185EBCA87;
    const P2: u64 = 0xC2B2AE3D27D4EB4F;
    const P3: u64 = 0x165667B19E3779F9;
    const P4: u64 = 0x85EBCA77C2B2AE63;
    const P5: u64 = 0x27D4EB2F165667C5;
    let mut h: u64;
    let len = data.len();
    let mut i = 0usize;
    let read8 = |d: &[u8], i: usize| u64::from_le_bytes(d[i..i + 8].try_into().unwrap());
    let read4 = |d: &[u8], i: usize| u32::from_le_bytes(d[i..i + 4].try_into().unwrap()) as u64;
    if len >= 32 {
        let mut v1 = seed.wrapping_add(P1).wrapping_add(P2);
        let mut v2 = seed.wrapping_add(P2);
        let mut v3 = seed;
        let mut v4 = seed.wrapping_sub(P1);
        while i + 32 <= len {
            let round = |acc: u64, x: u64| {
                acc.wrapping_add(x.wrapping_mul(P2)).rotate_left(31).wrapping_mul(P1)
            };
            v1 = round(v1, read8(data, i));
            v2 = round(v2, read8(data, i + 8));
            v3 = round(v3, read8(data, i + 16));
            v4 = round(v4, read8(data, i + 24));
            i += 32;
        }
        h = v1
            .rotate_left(1)
            .wrapping_add(v2.rotate_left(7))
            .wrapping_add(v3.rotate_left(12))
            .wrapping_add(v4.rotate_left(18));
        let merge = |h: u64, v: u64| {
            let v = v.wrapping_mul(P2).rotate_left(31).wrapping_mul(P1);
            (h ^ v).wrapping_mul(P1).wrapping_add(P4)
        };
        h = merge(h, v1);
        h = merge(h, v2);
        h = merge(h, v3);
        h = merge(h, v4);
    } else {
        h = seed.wrapping_add(P5);
    }
    h = h.wrapping_add(len as u64);
    while i + 8 <= len {
        let k = read8(data, i).wrapping_mul(P2).rotate_left(31).wrapping_mul(P1);
        h = (h ^ k).rotate_left(27).wrapping_mul(P1).wrapping_add(P4);
        i += 8;
    }
    if i + 4 <= len {
        h = (h ^ read4(data, i).wrapping_mul(P1)).rotate_left(23).wrapping_mul(P2).wrapping_add(P3);
        i += 4;
    }
    while i < len {
        h = (h ^ (data[i] as u64).wrapping_mul(P5)).rotate_left(11).wrapping_mul(P1);
        i += 1;
    }
    h ^= h >> 33;
    h = h.wrapping_mul(P2);
    h ^= h >> 29;
    h = h.wrapping_mul(P3);
    h ^= h >> 32;
    h
}

pub fn fingerprint(buf: &[(f64, f64)], sr: u32) -> String {
    let mut pcm = Vec::with_capacity(buf.len() * 4);
    for &(l, r) in buf {
        pcm.extend_from_slice(&((l.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
        pcm.extend_from_slice(&((r.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    let hash = xxh64(&pcm, 0);
    let mut out = format!("sr {}\nn {}\nhash {:016x}\n", sr, buf.len(), hash);
    for (bi, chunk) in buf.chunks(1024).enumerate() {
        let n = chunk.len() as f64;
        let rms_l = (chunk.iter().map(|s| s.0 * s.0).sum::<f64>() / n).sqrt();
        let rms_r = (chunk.iter().map(|s| s.1 * s.1).sum::<f64>() / n).sqrt();
        let peak = chunk.iter().map(|s| s.0.abs().max(s.1.abs())).fold(0.0f64, f64::max);
        out.push_str(&format!("b {} {:.6} {:.6} {:.6}\n", bi, rms_l as f32, rms_r as f32, peak as f32));
    }
    out
}

/// human-readable diff between two fingerprints: which blocks changed and by how much
pub fn diff(old: &str, new: &str) -> String {
    let parse = |s: &str| -> Vec<(f64, f64, f64)> {
        s.lines()
            .filter(|l| l.starts_with("b "))
            .map(|l| {
                let p: Vec<&str> = l.split_whitespace().collect();
                (p[2].parse().unwrap_or(0.0), p[3].parse().unwrap_or(0.0), p[4].parse().unwrap_or(0.0))
            })
            .collect()
    };
    let (a, b) = (parse(old), parse(new));
    let mut out = String::new();
    for i in 0..a.len().max(b.len()) {
        match (a.get(i), b.get(i)) {
            (Some(x), Some(y)) => {
                let d = 20.0 * ((y.0.max(1e-9)) / (x.0.max(1e-9))).log10();
                if d.abs() > 0.5 {
                    out.push_str(&format!("block {}: rms {:+.1}db\n", i, d));
                }
            }
            _ => out.push_str(&format!("block {}: length changed\n", i)),
        }
    }
    if out.is_empty() {
        out = "(rms profile unchanged; only bit-level diff)".into();
    }
    out
}
