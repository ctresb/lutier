// captura + analise: cpal abre o input escolhido, um ring buffer
// guarda as ultimas amostras e uma thread a ~60hz roda fft (hann
// 2048), goniometro e metricas, emitindo o frame binario compacto
// em base64 pro frontend (evento 'af').

use std::f32::consts::PI;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use base64::Engine;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rustfft::{num_complex::Complex, FftPlanner};
use tauri::{AppHandle, Emitter, State};

pub const SPEC_N: usize = 216;
pub const WAVE_N: usize = 480;
pub const GONIO_N: usize = 512;
const FFT_N: usize = 2048;
const RING_N: usize = 16384;

#[derive(Default)]
pub struct Capture {
    stop: Mutex<Option<Arc<AtomicBool>>>,
}

struct Ring {
    l: Vec<f32>,
    r: Vec<f32>,
    pos: usize,
}

impl Ring {
    fn new() -> Self {
        Ring { l: vec![0.0; RING_N], r: vec![0.0; RING_N], pos: 0 }
    }
    fn push(&mut self, l: f32, r: f32) {
        self.l[self.pos] = l;
        self.r[self.pos] = r;
        self.pos = (self.pos + 1) % RING_N;
    }
    /// copia as ultimas n amostras (mais antiga primeiro)
    fn tail(&self, n: usize, out_l: &mut [f32], out_r: &mut [f32]) {
        let start = (self.pos + RING_N - n) % RING_N;
        for i in 0..n {
            let j = (start + i) % RING_N;
            out_l[i] = self.l[j];
            out_r[i] = self.r[j];
        }
    }
}

/// entrada virtual de audio do desktop: no mac via screencapturekit
/// (pede permissao de gravacao de tela), no windows via loopback wasapi
pub const DESKTOP_DEV: &str = "DESKTOP AUDIO (SYSTEM)";

#[tauri::command]
pub fn list_inputs() -> Vec<String> {
    let host = cpal::default_host();
    let mut out: Vec<String> = Vec::new();
    #[cfg(target_os = "macos")]
    out.push(DESKTOP_DEV.to_string());
    #[cfg(target_os = "windows")]
    if let Ok(devs) = host.output_devices() {
        for d in devs {
            if let Ok(n) = d.name() {
                out.push(format!("DESKTOP: {n}"));
            }
        }
    }
    if let Ok(devs) = host.input_devices() {
        out.extend(devs.filter_map(|d| d.name().ok()));
    }
    out
}

#[tauri::command]
pub fn start_capture(
    app: AppHandle,
    state: State<Capture>,
    name: String,
) -> Result<(), String> {
    start_named(&app, &state, Some(name)).map(|_| ())
}

/// abre o input default do sistema e devolve o nome dele
#[tauri::command]
pub fn start_default(app: AppHandle, state: State<Capture>) -> Result<String, String> {
    start_named(&app, &state, None)
}

/// derruba a captura anterior so pelo flag e arma um novo. NUNCA da
/// join aqui: o comando roda na main thread e a thread antiga pode
/// estar dentro de app.emit (que precisa da main) - join = deadlock.
/// ela ve o flag em <16ms, sai do loop e derruba o stream sozinha.
fn swap_stop(state: &State<Capture>) -> Arc<AtomicBool> {
    if let Some(stop) = state.stop.lock().unwrap().take() {
        stop.store(true, Ordering::Relaxed);
    }
    let stop = Arc::new(AtomicBool::new(false));
    *state.stop.lock().unwrap() = Some(stop.clone());
    stop
}

fn start_named(
    app: &AppHandle,
    state: &State<Capture>,
    name: Option<String>,
) -> Result<String, String> {
    // audio do desktop (mac): tap de sistema via screencapturekit
    #[cfg(target_os = "macos")]
    if name.as_deref() == Some(DESKTOP_DEV) {
        let stop = swap_stop(state);
        let app2 = app.clone();
        std::thread::spawn(move || {
            if let Err(e) = desktop::run_desktop_capture(app2.clone(), stop) {
                let _ = app2.emit("af_err", format!("desktop: {e}"));
            }
        });
        return Ok(DESKTOP_DEV.to_string());
    }

    let host = cpal::default_host();

    // audio do desktop (windows): loopback wasapi do device de saida
    #[cfg(target_os = "windows")]
    if let Some(out_name) = name.as_deref().and_then(|n| n.strip_prefix("DESKTOP: ")) {
        let device = host
            .output_devices()
            .map_err(|e| e.to_string())?
            .find(|d| d.name().map(|dn| dn == out_name).unwrap_or(false))
            .ok_or_else(|| format!("saida nao encontrada: {out_name}"))?;
        let stop = swap_stop(state);
        let app2 = app.clone();
        let label = format!("DESKTOP: {out_name}");
        let label2 = label.clone();
        std::thread::spawn(move || {
            if let Err(e) = run_capture(app2.clone(), stop, device, true) {
                let _ = app2.emit("af_err", format!("{label2}: {e}"));
            }
        });
        return Ok(label);
    }

    let device = match &name {
        Some(n) => host
            .input_devices()
            .map_err(|e| e.to_string())?
            .find(|d| d.name().map(|dn| dn == *n).unwrap_or(false))
            .ok_or_else(|| format!("input nao encontrado: {n}"))?,
        None => host
            .default_input_device()
            .ok_or_else(|| "nenhum input default".to_string())?,
    };
    let dev_name = device.name().unwrap_or_else(|_| "UNKNOWN".into());

    let stop = swap_stop(state);
    let app2 = app.clone();
    let dev_name2 = dev_name.clone();
    std::thread::spawn(move || {
        if let Err(e) = run_capture(app2.clone(), stop, device, false) {
            let _ = app2.emit("af_err", format!("{dev_name2}: {e}"));
        }
    });
    Ok(dev_name)
}

fn run_capture(
    app: AppHandle,
    stop: Arc<AtomicBool>,
    device: cpal::Device,
    loopback: bool,
) -> Result<(), String> {
    // no loopback wasapi o stream de captura abre com a config de SAIDA
    let config = if loopback {
        device.default_output_config().map_err(|e| e.to_string())?
    } else {
        device.default_input_config().map_err(|e| e.to_string())?
    };
    let sr = config.sample_rate().0 as f32;
    let channels = config.channels() as usize;
    let ring = Arc::new(Mutex::new(Ring::new()));
    let ring_cb = ring.clone();
    let err_stop = stop.clone();

    let push = move |samples: &[f32]| {
        let mut rg = ring_cb.lock().unwrap();
        if channels >= 2 {
            for fr in samples.chunks_exact(channels) {
                rg.push(fr[0], fr[1]);
            }
        } else {
            for s in samples {
                rg.push(*s, *s);
            }
        }
    };
    let err_fn = move |_e: cpal::StreamError| {
        err_stop.store(true, Ordering::Relaxed);
    };

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config.into(),
            move |data: &[f32], _| push(data),
            err_fn,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_input_stream(
            &config.into(),
            move |data: &[i16], _| {
                let f: Vec<f32> = data.iter().map(|s| *s as f32 / 32768.0).collect();
                push(&f);
            },
            err_fn,
            None,
        ),
        cpal::SampleFormat::U16 => device.build_input_stream(
            &config.into(),
            move |data: &[u16], _| {
                let f: Vec<f32> =
                    data.iter().map(|s| (*s as f32 - 32768.0) / 32768.0).collect();
                push(&f);
            },
            err_fn,
            None,
        ),
        other => return Err(format!("formato de amostra sem suporte: {other:?}")),
    }
    .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    analysis_loop(&app, &stop, &ring, sr);
    Ok(())
}

/// loop de analise a ~60hz na thread dona do stream: fft, metricas e
/// emissao do frame 'af'. compartilhado entre cpal e captura de desktop.
fn analysis_loop(app: &AppHandle, stop: &AtomicBool, ring: &Mutex<Ring>, sr: f32) {
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(FFT_N);
    let hann: Vec<f32> = (0..FFT_N)
        .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / FFT_N as f32).cos())
        .collect();
    let win_sum: f32 = hann.iter().sum();

    // limites dos bins log 28hz..18khz
    let f_hi = (18000.0_f32).min(sr * 0.45);
    let edges: Vec<f32> = (0..=SPEC_N)
        .map(|k| 28.0 * (f_hi / 28.0_f32).powf(k as f32 / SPEC_N as f32))
        .collect();

    let mut l = vec![0.0f32; FFT_N];
    let mut r = vec![0.0f32; FFT_N];
    let mut buf = vec![Complex::new(0.0f32, 0.0); FFT_N];
    let mut mags = vec![0.0f32; FFT_N / 2];
    let mut prev_spec = vec![0.0f32; SPEC_N];
    let mut payload = vec![0u8; SPEC_N + WAVE_N + GONIO_N * 2 + 40];
    let b64 = base64::engine::general_purpose::STANDARD;

    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(std::time::Duration::from_millis(16));
        ring.lock().unwrap().tail(FFT_N, &mut l, &mut r);

        // tempo: rms / peak / crest
        let mut rms = 0.0f32;
        let mut peak = 0.0f32;
        for i in 0..FFT_N {
            let m = 0.5 * (l[i] + r[i]);
            rms += m * m;
            peak = peak.max(m.abs());
        }
        rms = (rms / FFT_N as f32).sqrt();
        let crest = if rms > 1e-6 { peak / rms } else { 0.0 };

        // correlacao estereo -> largura
        let mut sl = 0.0f32;
        let mut srr = 0.0f32;
        let mut slr = 0.0f32;
        for i in 0..FFT_N {
            sl += l[i] * l[i];
            srr += r[i] * r[i];
            slr += l[i] * r[i];
        }
        let corr = slr / (sl.sqrt() * srr.sqrt() + 1e-9);
        let width = ((1.0 - corr) * 0.5).clamp(0.0, 1.0);

        // fft do mono janelado
        for i in 0..FFT_N {
            buf[i] = Complex::new(0.5 * (l[i] + r[i]) * hann[i], 0.0);
        }
        fft.process(&mut buf);
        for i in 0..FFT_N / 2 {
            mags[i] = buf[i].norm() * 2.0 / win_sum;
        }

        // centroide espectral
        let mut num = 0.0f32;
        let mut den = 0.0f32;
        for i in 1..FFT_N / 2 {
            let fq = i as f32 * sr / FFT_N as f32;
            num += fq * mags[i];
            den += mags[i];
        }
        let centroid = if den > 1e-9 { num / den } else { 0.0 };

        // bins log + flux + bandas
        let mut flux = 0.0f32;
        let mut band = [0.0f32; 3];
        let mut bandn = [0.0f32; 3];
        for k in 0..SPEC_N {
            let b0 = ((edges[k] / sr * FFT_N as f32) as usize).max(1);
            let b1 = ((edges[k + 1] / sr * FFT_N as f32) as usize).max(b0 + 1);
            let mut m = 0.0f32;
            for i in b0..b1.min(FFT_N / 2) {
                m = m.max(mags[i]);
            }
            let db = 20.0 * (m + 1e-7).log10();
            let v = ((db + 72.0) / 69.0).clamp(0.0, 1.0);
            flux += (v - prev_spec[k]).max(0.0);
            prev_spec[k] = v;
            payload[k] = (v * 255.0) as u8;
            let fc = edges[k];
            let bi = if fc < 150.0 { 0 } else if fc < 2000.0 { 1 } else { 2 };
            band[bi] += v;
            bandn[bi] += 1.0;
        }
        let flux = (flux / SPEC_N as f32 * 8.0).clamp(0.0, 1.0);
        for i in 0..3 {
            band[i] /= bandn[i].max(1.0);
        }

        // waveform decimada (media por bloco)
        let step = FFT_N / WAVE_N;
        for k in 0..WAVE_N {
            let mut acc = 0.0f32;
            for i in 0..step {
                acc += 0.5 * (l[k * step + i] + r[k * step + i]);
            }
            let v = (acc / step as f32 * 1.6).clamp(-1.0, 1.0);
            payload[SPEC_N + k] = (v * 127.0) as i8 as u8;
        }

        // goniometro: ultimos 512 pares em mid/side
        let gof = SPEC_N + WAVE_N;
        for k in 0..GONIO_N {
            let i = FFT_N - GONIO_N + k;
            let x = ((l[i] - r[i]) * 0.7071 * 2.4).clamp(-1.0, 1.0);
            let y = ((l[i] + r[i]) * 0.7071 * 2.4).clamp(-1.0, 1.0);
            payload[gof + k * 2] = (x * 127.0) as i8 as u8;
            payload[gof + k * 2 + 1] = (y * 127.0) as i8 as u8;
        }

        // metricas f32 le
        let mof = gof + GONIO_N * 2;
        let metrics = [
            rms, peak, centroid, flux, crest, width, band[0], band[1], band[2], sr,
        ];
        for (i, v) in metrics.iter().enumerate() {
            payload[mof + i * 4..mof + i * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }

        if app.emit("af", b64.encode(&payload)).is_err() {
            break;
        }
    }
}

// ---------- captura de audio do desktop (macos) ----------
// tap de sistema via screencapturekit: sem driver de loopback, mas
// pede permissao de gravacao de tela na primeira vez.
#[cfg(target_os = "macos")]
mod desktop {
    use super::*;
    use screencapturekit::prelude::*;

    struct AudioTap {
        ring: Arc<Mutex<Ring>>,
    }

    fn f32_at(d: &[u8], i: usize) -> f32 {
        f32::from_le_bytes([d[i], d[i + 1], d[i + 2], d[i + 3]])
    }

    impl SCStreamOutputTrait for AudioTap {
        fn did_output_sample_buffer(&self, sample: CMSampleBuffer, of_type: SCStreamOutputType) {
            if of_type != SCStreamOutputType::Audio {
                return;
            }
            let Some(list) = sample.audio_buffer_list() else {
                return;
            };
            let mut rg = self.ring.lock().unwrap();
            if list.num_buffers() >= 2 {
                // canais nao intercalados: buffer 0 = L, buffer 1 = R
                let (Some(lb), Some(rb)) = (list.get(0), list.get(1)) else {
                    return;
                };
                let (ld, rd) = (lb.data(), rb.data());
                let n = ld.len().min(rd.len()) / 4;
                for i in 0..n {
                    rg.push(f32_at(ld, i * 4), f32_at(rd, i * 4));
                }
            } else if let Some(b) = list.get(0) {
                let d = b.data();
                if b.number_channels >= 2 {
                    let n = d.len() / 8;
                    for i in 0..n {
                        rg.push(f32_at(d, i * 8), f32_at(d, i * 8 + 4));
                    }
                } else {
                    let n = d.len() / 4;
                    for i in 0..n {
                        let v = f32_at(d, i * 4);
                        rg.push(v, v);
                    }
                }
            }
        }
    }

    pub fn run_desktop_capture(app: AppHandle, stop: Arc<AtomicBool>) -> Result<(), String> {
        let content = SCShareableContent::get()
            .map_err(|e| format!("sem permissao de gravacao de tela? {e}"))?;
        let display = content
            .displays()
            .into_iter()
            .next()
            .ok_or_else(|| "nenhum display".to_string())?;
        let filter = SCContentFilter::create()
            .with_display(&display)
            .with_excluding_windows(&[])
            .build();
        // video minimo (2x2): so o audio interessa, o resto e custo
        let config = SCStreamConfiguration::new()
            .with_captures_audio(true)
            .with_sample_rate(48_000)
            .with_channel_count(2)
            .with_width(2)
            .with_height(2);
        let ring = Arc::new(Mutex::new(Ring::new()));
        let mut stream = SCStream::new(&filter, &config);
        stream.add_output_handler(AudioTap { ring: ring.clone() }, SCStreamOutputType::Audio);
        stream.start_capture().map_err(|e| e.to_string())?;
        analysis_loop(&app, &stop, &ring, 48_000.0);
        let _ = stream.stop_capture();
        Ok(())
    }
}
