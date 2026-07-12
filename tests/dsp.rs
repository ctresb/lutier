// DSP unit tests (tier3 §2.2) driven through the public render path.
use lutier::engine::fft;
use lutier::render::render_song;

fn render_patch(synth: &str, score: &str) -> Vec<(f64, f64)> {
    render_song(synth, score, 44100.0, 1).expect("render").buf
}

fn spectrum_db(mono: &[f64]) -> Vec<f64> {
    let n = 16384.min(mono.len()).next_power_of_two() / 2;
    let mut re: Vec<f64> = mono[..n].to_vec();
    let mut im = vec![0.0; n];
    // hann window
    for (i, v) in re.iter_mut().enumerate() {
        *v *= 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / n as f64).cos();
    }
    fft(&mut re, &mut im, false);
    (0..n / 2)
        .map(|i| 20.0 * (re[i].hypot(im[i]) / n as f64).max(1e-12).log10())
        .collect()
}

#[test]
fn polyblep_saw_aliasing_below_minus_50db() {
    // saw at ~3khz (f#7 = 2960hz): aliased components must sit well below the fundamental
    let buf = render_patch(
        "synth t { poly 1 gain 0db voice { out saw(freq: note, gain: 0.8) } }",
        "tempo 120\ntrack a t\n0 f#7 4 1.0\n",
    );
    let mono: Vec<f64> = buf.iter().skip(4410).map(|s| 0.5 * (s.0 + s.1)).collect();
    let spec = spectrum_db(&mono);
    let bin_hz = 44100.0 / (spec.len() as f64 * 2.0);
    let f0 = 2959.96;
    let fund_bin = (f0 / bin_hz) as usize;
    let fund = spec[fund_bin - 2..fund_bin + 3].iter().cloned().fold(f64::MIN, f64::max);
    // max energy at non-harmonic bins between harmonics 1..6
    let mut worst = f64::MIN;
    for (i, &db) in spec.iter().enumerate().skip(10) {
        let f = i as f64 * bin_hz;
        if f > 20000.0 {
            break;
        }
        let h = f / f0;
        if (h - h.round()).abs() > 0.25 {
            worst = worst.max(db);
        }
    }
    assert!(
        worst < fund - 50.0,
        "aliasing floor {:.1}db vs fundamental {:.1}db (< -50db required)",
        worst,
        fund
    );
}

#[test]
fn svf_stable_no_nan() {
    let buf = render_patch(
        "synth t { poly 1 voice { let sw = env { 20hz -> 20khz in 2s curve exp } out lowpass(noise(gain: 1), cutoff: sw, q: 0.9) } }",
        "tempo 120\ntrack a t\n0 c4 3 1.0\n",
    );
    assert!(buf.iter().all(|s| s.0.is_finite() && s.1.is_finite()), "NaN/inf in filter sweep");
}

#[test]
fn limiter_never_exceeds_ceiling() {
    // +12db input into the master limiter: output must respect -1dbfs ceiling
    let buf = render_patch(
        "synth t { poly 1 voice { out sine(freq: note, gain: 4.0) } } master { limiter(ceiling: -1db, lookahead: 5ms, release: 60ms) }",
        "tempo 120\ntrack a t\n0 a4 2 1.0\n",
    );
    let ceil = 10f64.powf(-1.0 / 20.0) + 1e-6;
    let peak = buf.iter().map(|s| s.0.abs().max(s.1.abs())).fold(0.0f64, f64::max);
    assert!(peak <= ceil, "limiter output peak {} exceeds ceiling {}", peak, ceil);
}

#[test]
fn delay_period_matches_declared() {
    // impulse through delay_fx at 100ms: autocorrelation peak at ~4410 samples
    let buf = render_patch(
        "synth t { poly 1 kill after 3s voice { out noise(gain: 1) * env { 1 -> 0 in 2ms curve lin } } bus { delay_fx(time: 100ms, feedback: 60%, mix: 100%) } }",
        "tempo 120\ntrack a t\n0 c4 0.1 1.0\n",
    );
    let mono: Vec<f64> = buf.iter().map(|s| 0.5 * (s.0 + s.1)).collect();
    let n = 44100.min(mono.len());
    let mut best = (0usize, f64::MIN);
    for lag in 2000..8000 {
        let mut acc = 0.0;
        for i in 0..n - lag {
            acc += mono[i] * mono[i + lag];
        }
        if acc > best.1 {
            best = (lag, acc);
        }
    }
    assert!(
        (best.0 as i64 - 4410).abs() <= 2,
        "delay period {} samples, expected 4410±2",
        best.0
    );
}

#[test]
fn env_reaches_target_in_time() {
    // 1 -> 0 in 500ms exp: at 500ms the env is at target (engine snaps at segment end)
    let buf = render_patch(
        "synth t { poly 1 voice { out env { 1 -> 0 in 500ms curve lin } * sine(freq: 100hz, gain: 0) + env { 1 -> 0 in 500ms curve lin } } }",
        "tempo 120\ntrack a t\n0 c4 1 1.0\n",
    );
    // normalization scales everything; compare relative decay shape instead
    let v0 = buf[100].0.abs();
    let v_mid = buf[11025].0.abs(); // 250ms
    let v_end = buf[22500].0.abs(); // ~510ms
    assert!(v_mid < v0 * 0.6 && v_mid > v0 * 0.4, "linear env midpoint off: {} vs start {}", v_mid, v0);
    assert!(v_end < v0 * 0.01, "env not at target after declared time: {}", v_end);
}

#[test]
fn modal2_peaks_at_declared_ratios() {
    // 440hz fundamental, modes at 0.5 / 1.0 / 2.0: spectral peaks within 1 bin
    let buf = render_patch(
        "synth b { poly 1 kill after 6s voice { let m = [(0.5, 6s, 0.9), (1.0, 5s, 1.0), (2.0, 3s, 0.8)] out modal2(freq: 440hz, modes: m, doublet: 0.05%, strike: 0.35, hard: 0.8) } }",
        "tempo 120\ntrack a b\n0 a4 4 1.0\n",
    );
    let mono: Vec<f64> = buf.iter().skip(2205).map(|s| 0.5 * (s.0 + s.1)).collect();
    let spec = spectrum_db(&mono);
    let bin_hz = 44100.0 / (spec.len() as f64 * 2.0);
    for ratio in [0.5, 1.0, 2.0] {
        let f = 440.0 * ratio;
        let bin = (f / bin_hz).round() as usize;
        let peak = spec[bin - 1..bin + 2].iter().cloned().fold(f64::MIN, f64::max);
        // between declared modes (offset by ~25%) the spectrum must sit well below
        let off_bin = (f * 1.25 / bin_hz).round() as usize;
        let off = spec[off_bin - 1..off_bin + 2].iter().cloned().fold(f64::MIN, f64::max);
        assert!(
            peak > off + 20.0,
            "mode at ratio {} not prominent: peak {:.1}db vs offpeak {:.1}db",
            ratio, peak, off
        );
    }
}

#[test]
fn nwave_pulse_is_asymmetric() {
    // shock front: steepest rise slope must dwarf the steepest fall slope
    let buf = render_patch(
        "synth s { poly 1 kill after 1s voice { out nwave(dur: 3ms, sharp: 0.9, reflect: 0ms, air: 15khz) } }",
        "tempo 120\ntrack a s\n0 c4 0.5 1.0\n",
    );
    let mono: Vec<f64> = buf.iter().take(2205).map(|s| 0.5 * (s.0 + s.1)).collect();
    let mut max_rise = 0.0f64;
    let mut max_fall = 0.0f64;
    for w in mono.windows(2) {
        let d = w[1] - w[0];
        if d > max_rise {
            max_rise = d;
        }
        if -d > max_fall {
            max_fall = -d;
        }
    }
    assert!(
        max_rise > 4.0 * max_fall,
        "nwave not asymmetric: rise {:.4} fall {:.4}",
        max_rise, max_fall
    );
    let peak = mono.iter().cloned().fold(0.0f64, f64::max);
    assert!(peak > 0.1, "nwave produced no pulse");
}

#[test]
fn modal2_doublet_beats() {
    // one mode with a 0.5% doublet at 880hz: amplitude envelope of the partial
    // must show beating (deep periodic dips) rather than smooth exponential decay
    let buf = render_patch(
        "synth b { poly 1 kill after 6s voice { let m = [(1.0, 8s, 1.0)] out modal2(freq: 880hz, modes: m, doublet: 0.5%, strike: 0.4, hard: 0.9) } }",
        "tempo 120\ntrack a b\n0 a5 4 1.0\n",
    );
    let mono: Vec<f64> = buf.iter().map(|s| 0.5 * (s.0 + s.1)).collect();
    // rms in 50ms windows over 3s
    let win = 2205;
    let rms: Vec<f64> = (0..(3 * 44100 / win))
        .map(|i| {
            let seg = &mono[i * win..(i + 1) * win];
            (seg.iter().map(|v| v * v).sum::<f64>() / win as f64).sqrt().max(1e-12)
        })
        .collect();
    // beating: some later window must come back up over a previous local minimum
    let mut rises = 0;
    for w in rms.windows(2).skip(2) {
        if w[1] > w[0] * 1.15 {
            rises += 1;
        }
    }
    assert!(rises >= 2, "no beating detected in doublet mode (rises={})", rises);
}

#[test]
fn convolve_impulse_reproduces_ir_modes() {
    // impulse through convolve(ir: body) must ring at the body's mode frequencies
    let buf = render_patch(
        "synth body { poly 1 kill after 1s voice { let m = [(1.0, 0.3s, 1.0), (2.5, 0.2s, 0.6)] out modal2(freq: 500hz, modes: m, doublet: 0%, strike: 0.4, hard: 0.9) } }\n\
         synth hit { poly 1 kill after 2s voice { out env { 1 -> 0 in 1ms curve lin } } bus { convolve(ir: body, dur: 400ms, mix: 100%) } }",
        "tempo 120\ntrack a hit\n0 c4 1 1.0\n",
    );
    let mono: Vec<f64> = buf.iter().map(|s| 0.5 * (s.0 + s.1)).collect();
    let spec = spectrum_db(&mono);
    let bin_hz = 44100.0 / (spec.len() as f64 * 2.0);
    for f in [500.0, 1250.0] {
        let bin = (f / bin_hz).round() as usize;
        let peak = spec[bin - 2..bin + 3].iter().cloned().fold(f64::MIN, f64::max);
        let off_bin = (f * 1.3 / bin_hz).round() as usize;
        let off = spec[off_bin - 2..off_bin + 3].iter().cloned().fold(f64::MIN, f64::max);
        assert!(
            peak > off + 15.0,
            "convolved output missing IR mode at {}hz: {:.1}db vs {:.1}db",
            f, peak, off
        );
    }
    // tail: the convolved sound must ring past the 1-sample input
    let late = &mono[(0.15 * 44100.0) as usize..(0.25 * 44100.0) as usize];
    let rms = (late.iter().map(|v| v * v).sum::<f64>() / late.len() as f64).sqrt();
    assert!(rms > 1e-4, "no convolution tail (rms {:.2e})", rms);
}

fn dominant_hz(mono: &[f64], skip: usize) -> f64 {
    let seg: Vec<f64> = mono.iter().skip(skip).take(8192).cloned().collect();
    let mut spec = spectrum_db(&seg);
    let bin_hz = 44100.0 / (spec.len() as f64 * 2.0);
    spec[0] = f64::MIN; // ignore DC
    let (mut best, mut best_db) = (0usize, f64::MIN);
    for (i, &db) in spec.iter().enumerate().skip(2) {
        if db > best_db {
            best = i;
            best_db = db;
        }
    }
    best as f64 * bin_hz
}

#[test]
fn bow_speaks_at_pitch_with_harmonics() {
    let buf = render_patch(
        "synth b { poly 1 kill after 5s voice { let v = env { 0 -> 1 in 100ms curve exp sustain 1 release -> 0 in 100ms } out bow(freq: hz(note), pressure: 0.55, velocity: v, position: 0.12) } }",
        "tempo 120\ntrack a b\n0 a3 3 1.0\n",
    );
    let mono: Vec<f64> = buf.iter().map(|s| 0.5 * (s.0 + s.1)).collect();
    let f0 = dominant_hz(&mono, 44100);
    // dominant partial is some harmonic of 220; check 220 divides it (±3%)
    let h = (f0 / 220.0).round().max(1.0);
    assert!(
        (f0 / h - 220.0).abs() < 220.0 * 0.03,
        "bow off pitch: dominant {:.1}hz not near a 220hz harmonic",
        f0
    );
    // Helmholtz motion is saw-like: harmonics 2..4 must carry real energy
    let seg: Vec<f64> = mono.iter().skip(44100).take(8192).cloned().collect();
    let spec = spectrum_db(&seg);
    let bin_hz = 44100.0 / (spec.len() as f64 * 2.0);
    let level = |f: f64| -> f64 {
        let b = (f / bin_hz).round() as usize;
        spec[b - 1..b + 2].iter().cloned().fold(f64::MIN, f64::max)
    };
    for hh in [2.0, 3.0] {
        assert!(
            level(220.0 * hh) > level(220.0) - 30.0,
            "bow spectrum not saw-like: h{} too weak",
            hh
        );
    }
}

#[test]
fn reed_produces_odd_harmonics() {
    let buf = render_patch(
        "synth c { poly 1 kill after 4s voice { let p = env { 0 -> 0.85 in 80ms curve exp sustain 0.85 release -> 0 in 100ms } out reed(freq: hz(note), pressure: p, stiffness: 0.5) } }",
        "tempo 120\ntrack a c\n0 d3 3 1.0\n",
    );
    let mono: Vec<f64> = buf.iter().map(|s| 0.5 * (s.0 + s.1)).collect();
    let seg: Vec<f64> = mono.iter().skip(44100).take(8192).cloned().collect();
    let spec = spectrum_db(&seg);
    let bin_hz = 44100.0 / (spec.len() as f64 * 2.0);
    let f0 = 146.83;
    let level = |f: f64| -> f64 {
        let b = (f / bin_hz).round() as usize;
        spec[b - 1..b + 2].iter().cloned().fold(f64::MIN, f64::max)
    };
    // closed tube: h2 must sit well below h1 and h3
    let (h1, h2, h3) = (level(f0), level(2.0 * f0), level(3.0 * f0));
    assert!(h1 > -60.0, "clarinet not speaking (h1 {:.1}db)", h1);
    assert!(h2 < h1 - 15.0, "clarinet h2 {:.1}db not suppressed vs h1 {:.1}db", h2, h1);
    assert!(h2 < h3 - 5.0, "clarinet h2 {:.1}db not below h3 {:.1}db", h2, h3);
}

#[test]
fn flute_speaks_fundamental() {
    let buf = render_patch(
        "synth f { poly 1 kill after 4s voice { let p = env { 0 -> 1.0 in 60ms curve exp sustain 1.0 release -> 0 in 100ms } out flute(freq: hz(note), pressure: p, breath: 0.02) } }",
        "tempo 120\ntrack a f\n0 a4 3 1.0\n",
    );
    let mono: Vec<f64> = buf.iter().map(|s| 0.5 * (s.0 + s.1)).collect();
    let f0 = dominant_hz(&mono, 44100);
    assert!(
        (f0 - 440.0).abs() < 440.0 * 0.03,
        "flute dominant {:.1}hz, expected ~440hz fundamental",
        f0
    );
}

// ---------- SOTA nodes (leslie / hall / brass / voz / bow elasto-plastic) ----------

/// autocorrelation-based f0 (robust when the fundamental is not the loudest bin)
fn f0_autocorr(mono: &[f64], skip: usize, lo_hz: f64, hi_hz: f64) -> f64 {
    let seg: Vec<f64> = mono.iter().skip(skip).take(16384).cloned().collect();
    let n = seg.len();
    let (mut best, mut best_v) = (0usize, f64::MIN);
    for lag in (44100.0 / hi_hz) as usize..(44100.0 / lo_hz) as usize {
        let mut acc = 0.0;
        let mut i = 0;
        while i + lag < n {
            acc += seg[i] * seg[i + lag];
            i += 2;
        }
        if acc > best_v {
            best = lag;
            best_v = acc;
        }
    }
    if best == 0 { 0.0 } else { 44100.0 / best as f64 }
}

#[test]
fn leslie_modulates_periodically_and_decorrelates() {
    // sustained additive tone through leslie at tremolo speed: the left
    // channel envelope must fluctuate at the horn rotation rate (~6.8hz),
    // and L/R must decorrelate (opposite mic phases) - the OPPOSITE of the
    // static detuned beating the old chorus produced.
    let buf = render_patch(
        "synth o { poly 1 voice { let t = sine(freq: note, gain: 0.5) + sine(freq: note + 19st, gain: 0.3) let a = adsr(attack: 5ms, decay: 10ms, sustain: 1, release: 50ms) out t * a } bus { leslie(speed: 1, depth: 1) } }",
        "tempo 120\ntrack a o\n0 a4 6 0.9\n",
    );
    // envelope: rms in 5ms hops on L, skip 1s of spin-up
    let hop = 220usize;
    let l: Vec<f64> = buf.iter().skip(44100).map(|s| s.0).collect();
    let r: Vec<f64> = buf.iter().skip(44100).map(|s| s.1).collect();
    let env: Vec<f64> = l
        .chunks(hop)
        .map(|c| (c.iter().map(|v| v * v).sum::<f64>() / c.len() as f64).sqrt())
        .collect();
    let er = 44100.0 / hop as f64; // envelope rate
    let m = env.iter().sum::<f64>() / env.len() as f64;
    let e: Vec<f64> = env.iter().map(|v| v - m).collect();
    let (mut best, mut best_v) = (0usize, f64::MIN);
    for lag in (er / 12.0) as usize..(er / 3.0) as usize {
        let acc: f64 = (0..e.len() - lag).map(|i| e[i] * e[i + lag]).sum();
        if acc > best_v {
            best = lag;
            best_v = acc;
        }
    }
    let am_hz = er / best as f64;
    assert!(
        (am_hz - 6.8).abs() < 1.5,
        "leslie AM rate {:.2}hz, expected ~6.8hz (horn tremolo)",
        am_hz
    );
    // L/R decorrelation
    let n = l.len().min(r.len());
    let (ml, mr) = (l.iter().sum::<f64>() / n as f64, r.iter().sum::<f64>() / n as f64);
    let num: f64 = (0..n).map(|i| (l[i] - ml) * (r[i] - mr)).sum();
    let den = ((0..n).map(|i| (l[i] - ml).powi(2)).sum::<f64>()
        * (0..n).map(|i| (r[i] - mr).powi(2)).sum::<f64>())
        .sqrt();
    let corr = num / den.max(1e-12);
    assert!(corr < 0.9, "leslie L/R correlation {:.2} too high (no rotation?)", corr);
}

#[test]
fn hall_has_early_reflections_and_matching_decay() {
    // impulse into the SDN hall: geometric early reflections must arrive
    // within 5..80ms, and the tail must decay in the same order as `decay:`
    let buf = render_patch(
        "synth i { poly 1 kill after 4s voice { out noise(gain: 1) * env { 1 -> 0 in 2ms curve lin } } bus { hall(size: 0.7, decay: 2s, damp: 5khz, mix: 100%) } }",
        "tempo 120\ntrack a i\n0 c4 0.1 1.0\n",
    );
    let mono: Vec<f64> = buf.iter().map(|s| 0.5 * (s.0 + s.1)).collect();
    let peak = mono.iter().map(|v| v.abs()).fold(0.0f64, f64::max);
    // early reflections window: 5..80ms after the hit
    let er: f64 = mono[220..3528].iter().map(|v| v.abs()).fold(0.0, f64::max);
    assert!(er > peak * 0.05, "no early reflections in the SDN output");
    // decay: rms at 0.3s vs 1.5s should drop substantially but not be silent
    let rms = |a: usize, b: usize| -> f64 {
        (mono[a..b].iter().map(|v| v * v).sum::<f64>() / (b - a) as f64).sqrt()
    };
    let early = rms(13230, 17640); // 0.3..0.4s
    let late = rms(66150, 70560); // 1.5..1.6s
    assert!(late < early, "hall not decaying");
    let drop_db = 20.0 * (late / early.max(1e-12)).log10();
    assert!(
        drop_db < -8.0 && drop_db > -45.0,
        "hall decay {:.1}db over 1.2s inconsistent with decay: 2s",
        drop_db
    );
}

#[test]
fn brass_pitch_and_centroid_rise_with_pressure() {
    let patch = "synth t { poly 1 kill after 4s voice { let s = env { 0 -> 1 in 40ms curve exp sustain 1 release -> 0 in 100ms curve exp } out brass(freq: hz(note), pressure: s * (0.45 + velocity * 0.75), lip: 1.0, bell: 1.4khz, rasp: 0.6) } }";
    let quiet = render_patch(patch, "tempo 120\ntrack a t\n0 c3 3 0.25\n");
    let loud = render_patch(patch, "tempo 120\ntrack a t\n0 c3 3 1.0\n");
    let mq: Vec<f64> = quiet.iter().map(|s| 0.5 * (s.0 + s.1)).collect();
    let ml: Vec<f64> = loud.iter().map(|s| 0.5 * (s.0 + s.1)).collect();
    // pitch: within 3% of c3 (130.81hz)
    let f0 = f0_autocorr(&ml, 44100, 60.0, 400.0);
    assert!(
        (f0 - 130.81).abs() < 130.81 * 0.03,
        "brass f0 {:.1}hz, expected ~130.8hz",
        f0
    );
    // brassiness signature: HARMONIC centroid rises with blowing pressure
    // (full-band centroid is polluted by the normalized noise floor)
    let harm_centroid = |mono: &[f64]| -> f64 {
        let seg: Vec<f64> = mono.iter().skip(44100).take(8192).cloned().collect();
        let spec = spectrum_db(&seg);
        let bin_hz = 44100.0 / (spec.len() as f64 * 2.0);
        let mut num = 0.0;
        let mut den = 0.0;
        for k in 1..=12 {
            let f = 130.81 * k as f64;
            let b = (f / bin_hz).round() as usize;
            let db = spec[b - 1..b + 2].iter().cloned().fold(f64::MIN, f64::max);
            let a = 10f64.powf(db / 20.0);
            num += f * a;
            den += a;
        }
        num / den.max(1e-12)
    };
    let cq = harm_centroid(&mq);
    let cl = harm_centroid(&ml);
    assert!(
        cl > cq * 1.1,
        "brass harmonic centroid must rise with pressure: quiet {:.0}hz loud {:.0}hz",
        cq,
        cl
    );
}

#[test]
fn voz_formants_present() {
    // tenor 'a' at a3: F1 ~650hz region must dominate the inter-formant
    // valley (~1500hz) by a clear margin, and f0 must track the note
    let buf = render_patch(
        "synth v { poly 1 voice { let a = adsr(attack: 100ms, decay: 50ms, sustain: 0.9, release: 200ms) out voz(freq: hz(note), vowel: 0, tipo: tenor, ens: 4, vib: 0.15, jitter: 0.08) * a } }",
        "tempo 120\ntrack a v\n0 a3 4 0.9\n",
    );
    let mono: Vec<f64> = buf.iter().map(|s| 0.5 * (s.0 + s.1)).collect();
    let f0 = f0_autocorr(&mono, 44100, 120.0, 400.0);
    assert!((f0 - 220.0).abs() < 220.0 * 0.04, "voz f0 {:.1}hz, expected ~220hz", f0);
    let seg: Vec<f64> = mono.iter().skip(44100).take(8192).cloned().collect();
    let spec = spectrum_db(&seg);
    let bin_hz = 44100.0 / (spec.len() as f64 * 2.0);
    let band_max = |lo: f64, hi: f64| -> f64 {
        let (a, b) = ((lo / bin_hz) as usize, (hi / bin_hz) as usize);
        spec[a..b].iter().cloned().fold(f64::MIN, f64::max)
    };
    let f1 = band_max(520.0, 800.0);
    let valley = band_max(1350.0, 1600.0);
    assert!(
        f1 > valley + 8.0,
        "voz F1 region {:.1}db must exceed the 1.5khz valley {:.1}db by >8db",
        f1,
        valley
    );
}

#[test]
fn bow_slip_noise_gives_texture_not_floor() {
    // sustained bowed note: the 2-6khz band must FLUCTUATE (slip events),
    // not sit at a constant hiss floor
    let buf = render_patch(
        "synth b { poly 1 kill after 5s voice { let v = env { 0 -> 1 in 100ms curve exp sustain 1 release -> 0 in 100ms } out bow(freq: hz(note), pressure: 0.55, velocity: v, position: 0.12, noise: 0.25) } }",
        "tempo 120\ntrack a b\n0 a3 4 1.0\n",
    );
    let mono: Vec<f64> = buf.iter().map(|s| 0.5 * (s.0 + s.1)).collect();
    // crude 2-6khz bandpass: difference of one-pole lowpasses
    let mut lp6 = 0.0;
    let mut lp2 = 0.0;
    let k6 = 1.0 - (-2.0 * std::f64::consts::PI * 6000.0 / 44100.0f64).exp();
    let k2 = 1.0 - (-2.0 * std::f64::consts::PI * 2000.0 / 44100.0f64).exp();
    let band: Vec<f64> = mono
        .iter()
        .map(|&x| {
            lp6 += k6 * (x - lp6);
            lp2 += k2 * (x - lp2);
            lp6 - lp2
        })
        .collect();
    // rms per 100ms window over the sustained 1..3.5s
    let dbs: Vec<f64> = band[44100..154350]
        .chunks(4410)
        .map(|c| {
            let r = (c.iter().map(|v| v * v).sum::<f64>() / c.len() as f64).sqrt();
            20.0 * r.max(1e-9).log10()
        })
        .collect();
    let mean = dbs.iter().sum::<f64>() / dbs.len() as f64;
    let spread = dbs.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / dbs.len() as f64;
    // static additive hiss measures ~0.1-0.2db std; slip-correlated noise > 0.5
    assert!(
        spread.sqrt() > 0.5,
        "2-6khz band too static ({:.2}db std): slip noise not textured",
        spread.sqrt()
    );
}

#[test]
fn mono_retrigger_has_no_step_click() {
    // overlapping mono notes with an adsr: the retrigger must continue from
    // the current envelope value (no instant step to 0 = the sub "pipoco")
    let buf = render_patch(
        "synth s { mono glide 0ms voice { let a = adsr(attack: 60ms, decay: 200ms, sustain: 0.8, release: 300ms) out sine(freq: note, gain: 0.9) * a } }",
        "tempo 120\ntrack a s\n0 d2 1.6 0.9\n1.5 d2 1.6 0.8\n3.0 c2 1.2 0.8\n",
    );
    let mono: Vec<f64> = buf.iter().map(|s| 0.5 * (s.0 + s.1)).collect();
    let peak = mono.iter().map(|v| v.abs()).fold(0.0f64, f64::max);
    let max_step = mono.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f64, f64::max);
    // a 73hz sine at full scale moves at most ~1% of peak per sample; a
    // retrigger step is ~full scale. 10% threshold separates them cleanly.
    assert!(
        max_step < peak * 0.10,
        "retrigger click: max inter-sample step {:.3} vs peak {:.3}",
        max_step,
        peak
    );
}

#[test]
fn string_speaks_in_tune_with_two_stage_decay() {
    // string() em a3: f0 dentro de 1% e decay em 2 estagios (polarizacao
    // vertical morre rapido, horizontal sustenta - assinatura de corda real)
    let buf = render_patch(
        "synth s { poly 1 kill after 6s voice { out string(freq: hz(note), decay: 4s, bright: 0.6, position: 0.28, exciter: finger, pol: 0.5) } }",
        "tempo 120\ntrack a s\n0 a3 6 0.9\n",
    );
    let mono: Vec<f64> = buf.iter().map(|s| 0.5 * (s.0 + s.1)).collect();
    let f0 = f0_autocorr(&mono, 8820, 150.0, 320.0);
    assert!((f0 - 220.0).abs() < 220.0 * 0.01, "string f0 {:.2}hz, esperado ~220hz", f0);
    // 2 estagios: taxa de decaimento (db/s) dos primeiros 400ms deve ser
    // bem maior que a taxa entre 1.5s e 2.5s
    let rms = |a: f64, b: f64| -> f64 {
        let (a, b) = ((a * 44100.0) as usize, (b * 44100.0) as usize);
        (mono[a..b].iter().map(|v| v * v).sum::<f64>() / (b - a) as f64).sqrt().max(1e-12)
    };
    let early_rate = 20.0 * (rms(0.35, 0.45) / rms(0.02, 0.12)).log10() / 0.33;
    let late_rate = 20.0 * (rms(2.4, 2.5) / rms(1.5, 1.6)).log10() / 0.9;
    assert!(
        early_rate < late_rate - 2.0,
        "sem decay em 2 estagios: cedo {:.1}db/s, tarde {:.1}db/s",
        early_rate, late_rate
    );
}

#[test]
fn string_mute_chokes_ring() {
    // mute: 1 apos o gate = pizzicato seco; a cauda 300ms depois do
    // release deve estar >25db abaixo da versao sem mute
    let patch = |mute: &str| -> String {
        format!(
            "synth s {{ poly 1 kill after 4s voice {{ let m = env {{ 0 -> 0 in 1ms sustain 0 release -> 1 in 20ms curve lin }} out string(freq: hz(note), decay: 5s, bright: 0.6, exciter: finger, mute: {}) }} }}",
            mute
        )
    };
    let ring = render_patch(&patch("0"), "tempo 120\ntrack a s\n0 a3 1 0.9\n");
    let choked = render_patch(&patch("m"), "tempo 120\ntrack a s\n0 a3 1 0.9\n");
    // nota dura 0.5s (tempo 120); medir 0.9..1.1s
    let tail = |buf: &Vec<(f64, f64)>| -> f64 {
        let m: Vec<f64> = buf.iter().map(|s| 0.5 * (s.0 + s.1)).collect();
        let (a, b) = ((0.9 * 44100.0) as usize, (1.1 * 44100.0) as usize);
        (m[a..b].iter().map(|v| v * v).sum::<f64>() / (b - a) as f64).sqrt().max(1e-12)
    };
    let (tr, tc) = (tail(&ring), tail(&choked));
    let drop_db = 20.0 * (tc / tr).log10();
    assert!(drop_db < -20.0, "mute nao abafou: cauda caiu so {:.1}db", drop_db);
}

#[test]
fn string_pickup_and_stiff_stay_in_tune() {
    // pickup tap + dispersao nao podem desafinar a fundamental (>1%)
    let buf = render_patch(
        "synth s { poly 1 kill after 4s voice { out string(freq: hz(note), decay: 4s, bright: 0.8, exciter: pick, stiff: 0.6, pickup: 0.12) } }",
        "tempo 120\ntrack a s\n0 e2 4 0.9\n",
    );
    let mono: Vec<f64> = buf.iter().map(|s| 0.5 * (s.0 + s.1)).collect();
    let f0 = f0_autocorr(&mono, 8820, 55.0, 120.0);
    assert!((f0 - 82.41).abs() < 82.41 * 0.01, "string stiff/pickup f0 {:.2}hz, esperado ~82.4hz", f0);
}

// ---------- mixer, fx de usuario, EQ parametrico, meter ----------

/// db num bin de frequencia (max sobre +-3 bins pra tolerar leakage)
fn bin_db(buf: &[(f64, f64)], freq: f64) -> f64 {
    let mono: Vec<f64> = buf.iter().skip(4410).map(|s| 0.5 * (s.0 + s.1)).collect();
    let spec = spectrum_db(&mono);
    let bin_hz = 44100.0 / (spec.len() as f64 * 2.0);
    let b = (freq / bin_hz) as usize;
    spec[b.saturating_sub(3)..(b + 4).min(spec.len())].iter().cloned().fold(f64::MIN, f64::max)
}

const TWO_TONE: &str = "synth t { poly 1 voice { out sine(freq: 220hz, gain: 0.2) + sine(freq: 4khz, gain: 0.2) } }";
const TONE_SCORE: &str = "tempo 120\ntrack a t\n0 c4 4 1.0\n";

#[test]
fn peak_eq_cuts_at_center_leaves_rest() {
    // -12db em 220hz nao pode mexer em 4khz: delta(low-high) entre
    // render com e sem EQ = -12db (normalizacao cancela na diferenca)
    let dry = render_patch(TWO_TONE, TONE_SCORE);
    let wet = render_patch(
        &format!("{} mixer {{ channel c {{ in: t peak(freq: 220hz, gain: -12db, q: 1.0) }} }}", TWO_TONE),
        TONE_SCORE,
    );
    let d_dry = bin_db(&dry, 220.0) - bin_db(&dry, 4000.0);
    let d_wet = bin_db(&wet, 220.0) - bin_db(&wet, 4000.0);
    let cut = d_wet - d_dry;
    assert!((cut + 12.0).abs() < 1.5, "peak -12db aplicou {:.1}db", cut);
}

#[test]
fn shelves_boost_their_side_only() {
    let dry = render_patch(TWO_TONE, TONE_SCORE);
    let low = render_patch(
        &format!("{} mixer {{ channel c {{ in: t lowshelf(freq: 600hz, gain: 6db) }} }}", TWO_TONE),
        TONE_SCORE,
    );
    let hi = render_patch(
        &format!("{} mixer {{ channel c {{ in: t highshelf(freq: 1500hz, gain: -9db) }} }}", TWO_TONE),
        TONE_SCORE,
    );
    let d = |b: &Vec<(f64, f64)>| bin_db(b, 220.0) - bin_db(b, 4000.0);
    assert!((d(&low) - d(&dry) - 6.0).abs() < 1.5, "lowshelf +6db: delta {:.1}db", d(&low) - d(&dry));
    assert!((d(&hi) - d(&dry) - 9.0).abs() < 1.5, "highshelf -9db: delta {:.1}db", d(&hi) - d(&dry));
}

#[test]
fn fx_expansion_matches_direct_nodes() {
    // fx de usuario instanciado = nos diretos com os mesmos args (bit-igual:
    // biquad nao tem rng; so os node ids mudam)
    let direct = render_patch(
        &format!("{} mixer {{ channel c {{ in: t peak(freq: 800hz, gain: 5db, q: 1.3) highshelf(freq: 3khz, gain: -4db) }} }}", TWO_TONE),
        TONE_SCORE,
    );
    let viafx = render_patch(
        &format!("fx meueq(g: 0db, hs: 0db) {{ peak(freq: 800hz, gain: g, q: 1.3) highshelf(freq: 3khz, gain: hs) }} {} mixer {{ channel c {{ in: t meueq(g: 5db, hs: -4db) }} }}", TWO_TONE),
        TONE_SCORE,
    );
    assert_eq!(direct.len(), viafx.len());
    let maxd = direct
        .iter()
        .zip(&viafx)
        .map(|(a, b)| (a.0 - b.0).abs().max((a.1 - b.1).abs()))
        .fold(0.0f64, f64::max);
    assert!(maxd < 1e-12, "fx expandido difere dos nos diretos: max {:.3e}", maxd);
}

const TWO_SYNTHS: &str = "synth a { poly 1 voice { out sine(freq: 400hz, gain: 0.3) } } synth b { poly 1 voice { out sine(freq: 2khz, gain: 0.3) } }";
const TWO_SCORE: &str = "tempo 120\ntrack a\n0 c4 4 1.0\ntrack b\n0 c4 4 1.0\n";

#[test]
fn mixer_channel_gain_applies_to_routed_only() {
    // a passa pelo canal com gain -6db, b vai direto: delta(a-b) cai 6db
    let dry = render_patch(TWO_SYNTHS, TWO_SCORE);
    let wet = render_patch(
        &format!("{} mixer {{ channel ca {{ in: a gain -6db }} }}", TWO_SYNTHS),
        TWO_SCORE,
    );
    let d = |b: &Vec<(f64, f64)>| bin_db(b, 400.0) - bin_db(b, 2000.0);
    let delta = d(&wet) - d(&dry);
    assert!((delta + 6.0).abs() < 0.8, "gain -6db no canal aplicou {:.1}db", delta);
}

#[test]
fn mixer_send_adds_parallel_path() {
    // send -6db (0.5x) pra canal neutro somando no master: 1.5x = +3.52db
    let dry = render_patch(
        &format!("{} mixer {{ channel ca {{ in: a }} }}", TWO_SYNTHS),
        TWO_SCORE,
    );
    let wet = render_patch(
        &format!("{} mixer {{ channel ca {{ in: a send eco: -6db }} channel eco {{ gain 0db }} }}", TWO_SYNTHS),
        TWO_SCORE,
    );
    let d = |b: &Vec<(f64, f64)>| bin_db(b, 400.0) - bin_db(b, 2000.0);
    let delta = d(&wet) - d(&dry);
    let expect = 20.0 * 1.5011f64.log10(); // 1 + 10^(-6/20)
    assert!((delta - expect).abs() < 0.8, "send: esperado +{:.2}db, veio {:.2}db", expect, delta);
}

#[test]
fn mixer_pan_hard_right_kills_left() {
    let buf = render_patch(
        "synth a { poly 1 voice { out sine(freq: 500hz, gain: 0.3) } } mixer { channel ca { in: a pan 1.0 } }",
        "tempo 120\ntrack a\n0 c4 2 1.0\n",
    );
    let seg = &buf[8820..44100];
    let el: f64 = seg.iter().map(|s| s.0 * s.0).sum();
    let er: f64 = seg.iter().map(|s| s.1 * s.1).sum();
    assert!(el < er * 0.01, "pan 1.0: energia L {:.3e} vs R {:.3e}", el, er);
}

#[test]
fn mixer_routing_cycle_is_error() {
    let err = match lutier::render::render_song(
        "synth a { poly 1 voice { out sine(freq: note) } } mixer { channel x { in: a out y } channel y { out x } }",
        "tempo 120\ntrack a\n0 c4 1 1.0\n",
        44100.0,
        1,
    ) {
        Err(e) => e,
        Ok(_) => panic!("ciclo de mixer deveria falhar"),
    };
    assert!(err.contains("E044"), "ciclo de mixer nao acusou E044: {}", err);
}

#[test]
fn mixer_gain_automation_ramps() {
    // gain -40db -> 0db em 8 beats: fim tem que estar >20db acima do comeco
    let buf = render_patch(
        "synth a { poly 1 voice { out sine(freq: 500hz, gain: 0.3) } } mixer { channel ca { in: a } }",
        "tempo 120\ntrack a\n0 c4 8 1.0\nmixer ca\nautomate gain 0 -40 -> 8 0\n",
    );
    let rms = |s: &[(f64, f64)]| -> f64 {
        (s.iter().map(|v| (v.0 * v.0 + v.1 * v.1) * 0.5).sum::<f64>() / s.len() as f64).sqrt()
    };
    let start = rms(&buf[4410..22050]); // 0.1..0.5s
    let end = rms(&buf[3 * 44100..(3.9 * 44100.0) as usize]); // 3..3.9s (nota ate 4s)
    let rise_db = 20.0 * (end / start.max(1e-12)).log10();
    assert!(rise_db > 20.0, "automacao de gain subiu so {:.1}db", rise_db);
}

#[test]
fn lufs_of_known_sine_matches_bs1770() {
    // seno 997hz amplitude 0.1 nos dois canais: K-weight em 997hz =
    // +0.691db (por design do offset -0.691), entao LUFS = -20.0 exato
    let sr = 44100.0;
    let buf: Vec<(f64, f64)> = (0..(sr as usize * 5))
        .map(|i| {
            let v = 0.1 * (2.0 * std::f64::consts::PI * 997.0 * i as f64 / sr).sin();
            (v, v)
        })
        .collect();
    let lufs = lutier::meter::lufs_integrated(&buf, sr);
    assert!((lufs + 20.0).abs() < 0.1, "LUFS de seno conhecido: {:.2}, esperado -20.0", lufs);
}

#[test]
fn mixer_transparent_channel_is_noop() {
    // canal vazio (gain 0db, pan 0, sem inserts) nao pode mudar o som
    let dry = render_patch(TWO_SYNTHS, TWO_SCORE);
    let wet = render_patch(
        &format!("{} mixer {{ channel ca {{ in: a }} channel cb {{ in: b }} }}", TWO_SYNTHS),
        TWO_SCORE,
    );
    let maxd = dry
        .iter()
        .zip(&wet)
        .map(|(x, y)| (x.0 - y.0).abs().max((x.1 - y.1).abs()))
        .fold(0.0f64, f64::max);
    assert!(maxd < 1e-9, "canal neutro alterou o audio: max diff {:.3e}", maxd);
}
