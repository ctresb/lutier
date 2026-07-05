// Minimal WAV reader: PCM 16/24-bit and float32, mono or stereo.
// Returns (interleaved samples -1..1, sample_rate, channels).
pub fn read_wav(path: &str) -> Result<(Vec<f64>, u32, usize), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {}: {}", path, e))?;
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(format!("{}: not a WAV file", path));
    }
    let mut pos = 12usize;
    let mut fmt: Option<(u16, u16, u32, u16)> = None; // (format, channels, sr, bits)
    let mut data: Option<(usize, usize)> = None;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let sz = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let body = pos + 8;
        if id == b"fmt " && body + 16 <= bytes.len() {
            let f = u16::from_le_bytes(bytes[body..body + 2].try_into().unwrap());
            let ch = u16::from_le_bytes(bytes[body + 2..body + 4].try_into().unwrap());
            let sr = u32::from_le_bytes(bytes[body + 4..body + 8].try_into().unwrap());
            let bits = u16::from_le_bytes(bytes[body + 14..body + 16].try_into().unwrap());
            fmt = Some((f, ch, sr, bits));
        } else if id == b"data" {
            data = Some((body, sz.min(bytes.len().saturating_sub(body))));
        }
        pos = body + sz + (sz & 1);
    }
    let (format, ch, sr, bits) = fmt.ok_or(format!("{}: no fmt chunk", path))?;
    let (off, len) = data.ok_or(format!("{}: no data chunk", path))?;
    let raw = &bytes[off..off + len];
    let ch = ch.max(1) as usize;
    let mut out = Vec::new();
    match (format, bits) {
        (1, 16) => {
            for c in raw.chunks_exact(2) {
                out.push(i16::from_le_bytes([c[0], c[1]]) as f64 / 32768.0);
            }
        }
        (1, 24) => {
            for c in raw.chunks_exact(3) {
                let v = ((c[2] as i32) << 24 | (c[1] as i32) << 16 | (c[0] as i32) << 8) >> 8;
                out.push(v as f64 / 8388608.0);
            }
        }
        (3, 32) => {
            for c in raw.chunks_exact(4) {
                out.push(f32::from_le_bytes([c[0], c[1], c[2], c[3]]) as f64);
            }
        }
        (f, b) => return Err(format!("{}: unsupported wav format {}/{}bit", path, f, b)),
    }
    Ok((out, sr, ch))
}

pub fn write_wav(path: &str, samples: &[(f64, f64)], sr: u32) -> std::io::Result<()> {
    use std::io::Write;
    let n = samples.len() as u32;
    let data_len = n * 4; // 16-bit stereo
    let mut f = std::fs::File::create(path)?;
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + data_len).to_le_bytes())?;
    f.write_all(b"WAVEfmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?;
    f.write_all(&2u16.to_le_bytes())?;
    f.write_all(&sr.to_le_bytes())?;
    f.write_all(&(sr * 4).to_le_bytes())?;
    f.write_all(&4u16.to_le_bytes())?;
    f.write_all(&16u16.to_le_bytes())?;
    f.write_all(b"data")?;
    f.write_all(&data_len.to_le_bytes())?;
    let mut buf = Vec::with_capacity(samples.len() * 4);
    for &(l, r) in samples {
        let li = (l.clamp(-1.0, 1.0) * 32767.0) as i16;
        let ri = (r.clamp(-1.0, 1.0) * 32767.0) as i16;
        buf.extend_from_slice(&li.to_le_bytes());
        buf.extend_from_slice(&ri.to_le_bytes());
    }
    f.write_all(&buf)
}
