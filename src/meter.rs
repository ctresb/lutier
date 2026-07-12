// Medicao de mix por stem: loudness integrada BS.1770 (K-weighting +
// gating), pico, crest, share espectral por banda e alertas de
// estridencia/mascaramento. Pico nao e loudness: um pizzicato de pico 0.6
// soa muito mais baixo que um arco sustentado de pico 0.6 - por isso a
// escala aqui e LUFS, nao pico.
use crate::engine::fft;

#[derive(Debug, Clone)]
pub struct StemStats {
    pub name: String,
    pub lufs: f64,     // loudness integrada gated (LUFS); -inf = silencio
    pub peak_db: f64,  // sample peak dbfs
    pub crest_db: f64, // pico - rms (transiente vs sustentado)
    /// share de energia 0..1 por banda: 20-250 / 250-2k / 2k-6k / 6k+
    pub bands: [f64; 4],
    pub corr: f64,   // correlacao L/R (1 mono, 0 wide, <0 anti-fase)
    pub active: f64, // fracao do tempo acima de -60dbfs (janelas de 50ms)
}

/// biquad DF1 mono: coefs normalizados (b0 b1 b2 a1 a2)
struct Biq {
    c: [f64; 5],
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}

impl Biq {
    fn new(c: [f64; 5]) -> Self {
        Biq { c, x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0 }
    }
    fn run(&mut self, x: f64) -> f64 {
        let y = self.c[0] * x + self.c[1] * self.x1 + self.c[2] * self.x2
            - self.c[3] * self.y1
            - self.c[4] * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// estagio 1 do K-weighting: shelf de +4db acima de ~1.68khz (cabeca).
/// rederivacao canonica dos coeficientes normativos do BS.1770-4: em
/// sr=48khz reproduz a tabela oficial bit-a-bit (verificado); em 997hz o
/// ganho e +0.691db - exatamente o que o offset -0.691 compensa
fn k_shelf(sr: f64) -> [f64; 5] {
    let f0 = 1681.9744509555319;
    let gdb = 3.999843853973347;
    let q = 0.7071752369554193;
    let k = (std::f64::consts::PI * f0 / sr).tan();
    let vh = 10f64.powf(gdb / 20.0);
    let vb = vh.powf(0.4996667741545416);
    let a0 = 1.0 + k / q + k * k;
    [
        (vh + vb * k / q + k * k) / a0,
        2.0 * (k * k - vh) / a0,
        (vh - vb * k / q + k * k) / a0,
        2.0 * (k * k - 1.0) / a0,
        (1.0 - k / q + k * k) / a0,
    ]
}

/// estagio 2: highpass RLB ~38hz (curva de loudness revisada);
/// b = [1, -2, 1] cru como na tabela normativa (nao normalizado por a0)
fn k_highpass(sr: f64) -> [f64; 5] {
    let f0 = 38.13547087613982;
    let q = 0.5003270373253953;
    let k = (std::f64::consts::PI * f0 / sr).tan();
    let a0 = 1.0 + k / q + k * k;
    [1.0, -2.0, 1.0, 2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0]
}

/// loudness integrada BS.1770-4: K-weighting -> blocos de 400ms com 75% de
/// overlap -> gate absoluto -70 LUFS -> gate relativo -10 LU -> media
pub fn lufs_integrated(buf: &[(f64, f64)], sr: f64) -> f64 {
    if buf.is_empty() {
        return f64::NEG_INFINITY;
    }
    let mut sl = Biq::new(k_shelf(sr));
    let mut hl = Biq::new(k_highpass(sr));
    let mut sr_ = Biq::new(k_shelf(sr));
    let mut hr = Biq::new(k_highpass(sr));
    // potencia K-weighted por sample (soma dos canais, pesos 1.0)
    let zw: Vec<f64> = buf
        .iter()
        .map(|&(l, r)| {
            let zl = hl.run(sl.run(l));
            let zr = hr.run(sr_.run(r));
            zl * zl + zr * zr
        })
        .collect();
    let block = (0.4 * sr) as usize;
    let hop = (0.1 * sr) as usize;
    if zw.len() < block || block == 0 {
        return f64::NEG_INFINITY;
    }
    // prefix sum pra media por bloco em O(1)
    let mut pre = Vec::with_capacity(zw.len() + 1);
    pre.push(0.0f64);
    for &v in &zw {
        pre.push(pre.last().unwrap() + v);
    }
    let mut powers = Vec::new();
    let mut s = 0;
    while s + block <= zw.len() {
        powers.push((pre[s + block] - pre[s]) / block as f64);
        s += hop;
    }
    let loud = |p: f64| -0.691 + 10.0 * p.max(1e-15).log10();
    let abs_gated: Vec<f64> = powers.iter().copied().filter(|&p| loud(p) > -70.0).collect();
    if abs_gated.is_empty() {
        return f64::NEG_INFINITY;
    }
    let rel_thr = loud(abs_gated.iter().sum::<f64>() / abs_gated.len() as f64) - 10.0;
    let rel_gated: Vec<f64> = abs_gated.into_iter().filter(|&p| loud(p) > rel_thr).collect();
    if rel_gated.is_empty() {
        return f64::NEG_INFINITY;
    }
    loud(rel_gated.iter().sum::<f64>() / rel_gated.len() as f64)
}

/// share de energia por banda via Welch (janelas Hann de 8192, hop 4096):
/// 20-250 (fundacao) / 250-2k (corpo) / 2k-6k (presenca) / 6k+ (ar)
pub fn band_shares(buf: &[(f64, f64)], sr: f64) -> [f64; 4] {
    const N: usize = 8192;
    if buf.len() < N {
        return [0.0; 4];
    }
    let hann: Vec<f64> = (0..N)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / N as f64).cos())
        .collect();
    let mut acc = [0.0f64; 4];
    let edges = [20.0, 250.0, 2000.0, 6000.0, sr * 0.5];
    let mut s = 0;
    while s + N <= buf.len() {
        let mut re: Vec<f64> = (0..N).map(|i| (buf[s + i].0 + buf[s + i].1) * 0.5 * hann[i]).collect();
        let mut im = vec![0.0; N];
        fft(&mut re, &mut im, false);
        for k in 1..N / 2 {
            let f = k as f64 * sr / N as f64;
            let p = re[k] * re[k] + im[k] * im[k];
            for b in 0..4 {
                if f >= edges[b] && f < edges[b + 1] {
                    acc[b] += p;
                    break;
                }
            }
        }
        s += N / 2;
    }
    let tot: f64 = acc.iter().sum();
    if tot <= 0.0 {
        return [0.0; 4];
    }
    [acc[0] / tot, acc[1] / tot, acc[2] / tot, acc[3] / tot]
}

pub fn analyze_stem(name: &str, buf: &[(f64, f64)], sr: f64) -> StemStats {
    let peak = buf.iter().map(|(l, r)| l.abs().max(r.abs())).fold(0.0f64, f64::max);
    let db = |v: f64| 20.0 * v.max(1e-15).log10();
    let n = buf.len().max(1) as f64;
    let (mut e, mut elr, mut el2, mut er2) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for &(l, r) in buf {
        e += (l * l + r * r) * 0.5;
        elr += l * r;
        el2 += l * l;
        er2 += r * r;
    }
    let rms = (e / n).sqrt();
    let corr = if el2 > 1e-12 && er2 > 1e-12 { elr / (el2 * er2).sqrt() } else { 1.0 };
    // atividade: janelas de 50ms acima de -60dbfs
    let w = (0.05 * sr) as usize;
    let mut act = 0usize;
    let mut tot = 0usize;
    if w > 0 {
        let mut s = 0;
        while s + w <= buf.len() {
            let ew: f64 = buf[s..s + w].iter().map(|(l, r)| (l * l + r * r) * 0.5).sum();
            if (ew / w as f64).sqrt() > 1e-3 {
                act += 1;
            }
            tot += 1;
            s += w;
        }
    }
    StemStats {
        name: name.to_string(),
        lufs: lufs_integrated(buf, sr),
        peak_db: db(peak),
        crest_db: db(peak) - db(rms),
        bands: band_shares(buf, sr),
        corr,
        active: if tot > 0 { act as f64 / tot as f64 } else { 0.0 },
    }
}

const BAND_NAMES: [&str; 4] = ["20-250", "250-2k", "2k-6k", "6k+"];

/// alertas acionaveis: estridencia, mascaramento, stem sumido
pub fn alerts(stems: &[StemStats]) -> Vec<String> {
    let mut out = Vec::new();
    // referencia = stem mais alto que nao seja o MASTER (o master e a soma)
    let loudest = stems
        .iter()
        .filter(|s| s.name != "MASTER")
        .map(|s| s.lufs)
        .filter(|v| v.is_finite())
        .fold(f64::NEG_INFINITY, f64::max);
    for s in stems {
        if !s.lufs.is_finite() || s.active < 0.05 || s.name == "MASTER" {
            continue;
        }
        // estridencia: presenca (2-6k) dominando um stem que esta alto na mix
        if s.bands[2] > 0.35 && s.lufs > loudest - 12.0 {
            out.push(format!(
                "ESTRIDENCIA {}: {:.0}% da energia em 2-6khz e esta a {:.1} LU do stem mais alto",
                s.name,
                s.bands[2] * 100.0,
                loudest - s.lufs
            ));
        }
        // sumido: ativo mas 18+ LU abaixo do mais alto
        if loudest - s.lufs > 18.0 {
            out.push(format!(
                "SUMIDO {}: {:.1} LU abaixo do stem mais alto (ativo {:.0}% do tempo)",
                s.name,
                loudest - s.lufs,
                s.active * 100.0
            ));
        }
    }
    // mascaramento: dois stems altos com a mesma banda dominante.
    // so compara pares do mesmo grupo (synth x synth, canal x canal):
    // synth x canal-que-o-contem e stem x MASTER sao redundancia, nao briga
    for i in 0..stems.len() {
        for j in i + 1..stems.len() {
            let (a, b) = (&stems[i], &stems[j]);
            if a.name == "MASTER" || b.name == "MASTER" {
                continue;
            }
            if a.name.starts_with('[') != b.name.starts_with('[') {
                continue;
            }
            if !a.lufs.is_finite() || !b.lufs.is_finite() {
                continue;
            }
            let da = a.bands.iter().enumerate().max_by(|x, y| x.1.partial_cmp(y.1).unwrap());
            let db_ = b.bands.iter().enumerate().max_by(|x, y| x.1.partial_cmp(y.1).unwrap());
            if let (Some((ba, va)), Some((bb, vb))) = (da, db_) {
                if ba == bb
                    && *va > 0.5
                    && *vb > 0.5
                    && (a.lufs - b.lufs).abs() < 6.0
                    && a.lufs > loudest - 12.0
                    && b.lufs > loudest - 12.0
                {
                    out.push(format!(
                        "MASCARAMENTO {} x {}: ambos dominados por {} ({:.0}%/{:.0}%) a {:.1} LU um do outro",
                        a.name,
                        b.name,
                        BAND_NAMES[ba],
                        va * 100.0,
                        vb * 100.0,
                        (a.lufs - b.lufs).abs()
                    ));
                }
            }
        }
    }
    out
}

/// tabela do relatorio (stems + master no fim se presente)
pub fn print_report(stems: &[StemStats]) {
    println!(
        "{:<18} {:>8} {:>8} {:>7} {:>7} | {:>6} {:>6} {:>6} {:>6} | {:>5} {:>5}",
        "stem", "LUFS", "pico db", "crest", "ativo", "20-250", "250-2k", "2k-6k", "6k+", "corr", ""
    );
    for s in stems {
        let lufs = if s.lufs.is_finite() { format!("{:.1}", s.lufs) } else { "-inf".into() };
        println!(
            "{:<18} {:>8} {:>8.1} {:>7.1} {:>6.0}% | {:>5.0}% {:>5.0}% {:>5.0}% {:>5.0}% | {:>5.2} {:>5}",
            s.name,
            lufs,
            s.peak_db,
            s.crest_db,
            s.active * 100.0,
            s.bands[0] * 100.0,
            s.bands[1] * 100.0,
            s.bands[2] * 100.0,
            s.bands[3] * 100.0,
            s.corr,
            ""
        );
    }
    let warn = alerts(stems);
    if warn.is_empty() {
        println!("\nsem alertas de mix");
    } else {
        println!();
        for w in &warn {
            println!("! {}", w);
        }
    }
}
