// Offline dataflow-graph interpreter for lutier synths.
use crate::parser::{Curve, Expr, Mode, Op, Seg, SynthDef, Unit};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy)]
pub enum Val {
    S(f64),           // scalar / control / mono audio
    St2(f64, f64),    // stereo audio
    Hz(f64),
    Pitch(f64),
    Ms(f64),
    StI(f64),         // semitone interval
    Beat(f64),        // musical beats (needs bpm to become time)
}

impl Val {
    pub fn num(self) -> f64 {
        match self {
            Val::S(v) | Val::Hz(v) | Val::Pitch(v) | Val::Ms(v) | Val::StI(v) | Val::Beat(v) => v,
            Val::St2(l, r) => 0.5 * (l + r),
        }
    }
    /// time value in seconds; bare scalars read as seconds, other units as ms
    fn as_sec(self, bpm: f64) -> f64 {
        match self {
            Val::Ms(v) => v / 1000.0,
            Val::Beat(v) => v * 60.0 / bpm,
            Val::S(v) => v,
            v => v.num() / 1000.0,
        }
    }
    pub fn stereo(self) -> (f64, f64) {
        match self {
            Val::St2(l, r) => (l, r),
            v => {
                let x = v.num();
                (x, x)
            }
        }
    }
    fn as_hz(self) -> f64 {
        match self {
            Val::Hz(v) => v,
            Val::Pitch(p) => 440.0 * 2f64.powf((p - 69.0) / 12.0),
            v => v.num(),
        }
    }
}

fn binop(op: char, a: Val, b: Val) -> Val {
    use Val::*;
    // fast path: plain scalars (the vast majority) - same apply(), bit-identical
    if let (S(x), S(y)) = (a, b) {
        return S(apply(op, x, y));
    }
    // stereo broadcast
    if let (St2(al, ar), _) = (a, b) {
        let (bl, br) = b.stereo();
        return St2(apply(op, al, bl), apply(op, ar, br));
    }
    if let St2(_, _) = b {
        let (al, ar) = a.stereo();
        let (bl, br) = b.stereo();
        return St2(apply(op, al, bl), apply(op, ar, br));
    }
    let r = apply(op, a.num(), b.num());
    // unit propagation (lenient): pitch +- interval stays pitch; else first non-scalar unit wins
    match (a, b, op) {
        (Pitch(_), _, '+') | (Pitch(_), _, '-') => Pitch(r),
        (_, Pitch(_), '+') => Pitch(r),
        (Hz(_), Hz(_), '/') => S(r),
        (Beat(_), _, _) | (_, Beat(_), _) => Beat(r),
        (Hz(_), _, _) | (_, Hz(_), _) => Hz(r),
        (Ms(_), _, _) | (_, Ms(_), _) => Ms(r),
        (StI(_), _, _) | (_, StI(_), _) => StI(r),
        _ => S(r),
    }
}

fn apply(op: char, a: f64, b: f64) -> f64 {
    match op {
        '+' => a + b,
        '-' => a - b,
        '*' => a * b,
        '/' => {
            if b == 0.0 {
                0.0
            } else {
                a / b
            }
        }
        _ => 0.0,
    }
}

// ---------- node state ----------

struct EnvSegR {
    target: f64,
    time_s: f64,
    curve: Curve,
}

struct EnvState {
    segs: Vec<EnvSegR>,
    sustain: Option<f64>,
    release: Vec<EnvSegR>,
    is_hz: bool,
    // runtime
    seg: usize,
    t: f64,
    seg_start_val: f64,
    cur: f64,
    released: bool,
    in_release: bool,
    done: bool,
}

enum NodeState {
    Phase(f64),
    Unison { ph: Vec<f64>, det: Vec<f64>, pan: Vec<f64> },
    Svf { ic1: [f64; 4], ic2: [f64; 4] }, // [ch*2 + stage]
    Env(EnvState),
    Lfo { ph: f64, hold: f64, rng: u64 },
    Rng(u64),
    Pink { rng: u64, b: [f64; 3] },
    Brown { rng: u64, y: f64 },
    Blue { rng: u64, b: [f64; 3], prev: f64 },
    Violet { rng: u64, prev: f64 },
    Velvet { rng: u64 },
    Crackle { rng: u64, env: f64, k: f64, lp: f64 },
    Delay1 { prev: (f64, f64) },
    Delay { buf: Vec<(f64, f64)>, w: usize },
    DelayFx {
        buf: Vec<(f64, f64)>,
        w: usize,
        damp: (f64, f64),
        cur: f64,       // current delay in samples
        from: f64,      // crossfade source delay
        xfade: f64,     // remaining crossfade seconds (0 = idle)
    },
    Chorus { buf: Vec<(f64, f64)>, w: usize, ph: Vec<f64> },
    Reverb {
        pre: Vec<(f64, f64)>,
        prew: usize,
        ap1: Vec<f64>, ap1w: usize,
        ap2: Vec<f64>, ap2w: usize,
        lines: Vec<Vec<f64>>,
        lw: Vec<usize>,
        damp: Vec<f64>,
        g: Vec<f64>,
    },
    Haas { buf: Vec<(f64, f64)>, w: usize },
    Comp { env: f64 },
    Limiter { buf: Vec<(f64, f64)>, w: usize, gain: f64 },
    Rms { buf: Vec<f64>, w: usize, sum: f64 },
    Os { up: [Vec<f64>; 2], dn: [Vec<f64>; 2], w: usize },
    Sample { pos: f64, dir: f64 },
    Pluck { buf: Vec<f64>, w: usize, lp: f64, ap: f64 },
    // corda dedilhada universal (EKS/waveguide SDL, 2 polarizacoes)
    Str(Box<StrS>),
    Modal { s1: Vec<f64>, s2: Vec<f64>, exc: f64, rng: u64 },
    Modal2 {
        s1: Vec<f64>,       // 2 resonators per mode (doublet pair)
        s2: Vec<f64>,
        split: Vec<f64>,    // per-mode doublet detune fraction (seeded)
        imp: f64,           // pending impulse (1.0 at t=0, then 0)
        hammer: f64,        // one-pole lowpass state = hammer contact force
        rng: u64,
    },
    Nwave { t: f64, buf: Vec<f64>, w: usize, lp: f64 },
    Conv(Box<ConvState>),
    // bowed string waveguide: two delay lines (nut side / bridge side) meeting
    // at the bow point in an exact MSW stick-slip junction (Stribeck curve
    // solved per sample with wave feedback). z = last relative velocity
    // (hysteresis state), nlp = slip-noise lowpass state, oz = one-zero do
    // filtro de reflexao, tq = temperatura de contato do breu (friccao
    // termica), cfreq/comp = cache da compensacao de fase do loop
    // pos = posicao do arco sobre a corda em "graos" de textura: a crina
    // com breu e uma SUPERFICIE fractal que a corda le na velocidade do
    // arco (o ruido de friccao real nao e branco - e textura escaneada:
    // arco parado = silencio, lento = rumor grave, rapido = mais denso).
    // O arco e FINITO (talao/ponta): h = posicao na crina 0..1, dir =
    // sentido alvo (+1 arcada pra baixo, -1 pra cima), dsm = sentido
    // suavizado (a mao desacelera, para e volta - vb cruza zero na
    // inversao). Na virada a textura e relida DE VOLTA (mesma crina).
    // nlp2 = lowpass da pestana/dedo (terminacao de carne, nao parede
    // perfeita), ap = allpass de rigidez da corda, iw* = imperfeicao
    // humana: random walks LENTOS de afinacao da mao esquerda (+-1.5ct)
    // e do ponto de contato do arco (+-6%) - o timbre respira, nao treme
    Bow {
        nut: Vec<f64>,
        bridge: Vec<f64>,
        w: usize,
        lp: f64,
        z: f64,
        nlp: f64,
        nlp2: f64,
        flp: f64,
        oz: f64,
        ap: f64,
        tq: f64,
        cfreq: f64,
        comp: f64,
        pos: f64,
        h: f64,
        dir: f64,
        dsm: f64,
        iw_ph: f64,
        ip_s: f64,
        ip_t: f64,
        ic_s: f64,
        ic_t: f64,
        rng: u64,
    },
    // Leslie rotary cabinet: band split + two rotors (horn/drum), each a
    // circularly modulated delay (doppler) + synchronized AM, opposite L/R mics
    Leslie {
        h: Vec<f64>,   // horn band ring
        d: Vec<f64>,   // drum band ring
        w: usize,
        ph_h: f64,     // horn rotor phase 0..1
        ph_d: f64,
        rh: f64,       // current horn rate hz (mechanical inertia)
        rd: f64,
        lp1: f64,      // crossover lowpass states (2x one-pole)
        lp2: f64,
    },
    Hall(Box<HallState>),
    // brass: lip valve (2nd-order resonator) + bore waveguide + steepening
    // nao-linear distribuido + bell reflection/radiation
    Brass(Box<BrassS>),
    Voz { singers: Vec<VozSinger> },
    // flute: jet delay + bore delay, smooth cubic jet nonlinearity
    Flute(Box<FluteS>),
    // clarinet: Bernoulli reed flow + closed bore (odd harmonics)
    Reed(Box<ReedS>),
    // breath source: DC pressure + physical turbulence
    Breath { rng: u64, turb: TurbState },
    Grain { grains: Vec<GrainVoice>, rng: u64, next_spawn: f64 },
}

/// fractional ring-buffer read at delay d behind write index w (Hermite)
#[inline]
fn ring_read(buf: &[f64], w: usize, d: f64) -> f64 {
    let n = buf.len();
    let nf = n as f64;
    let rp = (w as f64 - d + nf * 2.0) % nf;
    let i0 = rp.floor() as usize;
    let fr = rp - rp.floor();
    let im1 = (i0 + n - 1) % n;
    let i1 = (i0 + 1) % n;
    let i2 = (i0 + 2) % n;
    hermite(buf[im1], buf[i0], buf[i1], buf[i2], fr)
}

/// zero-latency hybrid partitioned convolution: partition 0 of the IR runs as
/// a direct FIR (per sample), partitions 1..P as FFT overlap-add blocks that
/// are always ready one block ahead - no lookahead delay, offline-friendly cost
struct ConvState {
    b: usize,             // partition/block size
    // per channel (ir2: decorrelated right-channel IR; both point to the same
    // data when only ir: is given)
    ir_head: [Vec<f64>; 2],    // ir[0..b], direct FIR taps
    parts: [Vec<(Vec<f64>, Vec<f64>)>; 2], // fft of ir[kb..kb+b] zero-padded to 2b, k >= 1
    // per channel (0 = L, 1 = R):
    in_ring: [Vec<f64>; 2],  // last b input samples (direct FIR + block collection)
    fdl: [Vec<(Vec<f64>, Vec<f64>)>; 2], // spectra of past input blocks (ring)
    fdl_w: [usize; 2],
    ytime: [Vec<f64>; 2],    // fft-tail output for the current block
    overlap: [Vec<f64>; 2],
    pos: usize,              // sample index within the current block
    dead: bool,              // ir failed to render: passthrough
}

/// Scattering Delay Network room (survey 2.9.2): one scattering node at each
/// wall's first-reflection point, bidirectional delay lines between nodes,
/// source->node and node->ear delays from real shoebox geometry. Early
/// reflections are geometrically correct; the recirculation makes the tail.
struct HallState {
    /// lines[k][j]: delay line carrying the wave leaving node k toward node j
    lines: Vec<Vec<Vec<f64>>>,
    lw: usize, // shared write cursor (all lines sized >= their delay + 1 margin)
    dline: Vec<f64>, // per directed pair delay in samples, flattened [k*6+j]
    src: Vec<Vec<f64>>,      // source -> node k delay lines
    dsrc: Vec<f64>,          // delays in samples
    gsrc: Vec<f64>,          // 1/dist attenuation source->node
    dear: [Vec<f64>; 2],     // node k -> ear (L,R) delay in samples
    gear: [Vec<f64>; 2],     // attenuation node->ear
    ebuf: [Vec<f64>; 2],     // ear accumulation ring (delayed node pressures)
    g_wall: Vec<f64>,        // per-node wall gain (from decay)
    damp: Vec<f64>,          // per-node lowpass state
    p_in: Vec<[f64; 6]>,     // scratch: incoming waves per node
}

struct BrassS {
    bore: Vec<f64>,
    w: usize,
    lip1: f64, // estados do ressonador do labio
    lip2: f64,
    lp: f64, // lowpass da reflexao da campana
    dc: (f64, f64),
    ap: (f64, f64), // allpass modulado do steepening (x1, y1)
    rng: u64,
    turb: TurbState,
    prev_out: f64,
}

struct ReedS {
    bore: Vec<f64>,
    w: usize,
    lp: f64,       // polo do cutoff de toneholes na reflexao
    oz: f64,       // one-zero da reflexao (x anterior)
    rng: u64,
    turb: TurbState,
    prev_out: f64, // diferenciador de radiacao
}

struct FluteS {
    bore: Vec<f64>,
    jet: Vec<f64>,
    w: usize,
    lp: f64,        // polo do filtro de reflexao
    dc: (f64, f64), // DC blocker pos-nao-linearidade
    rng: u64,
    turb: TurbState,
    prev_out: f64, // saida anterior do bore (diferenciador de radiacao)
}

#[derive(Clone)]
// corda dedilhada universal: single delay-loop (Valimaki/Karjalainen CMJ98)
// x2 polarizacoes. A vertical decai ~2x mais rapido que a horizontal e as
// duas ficam desafinadas por FRACAO DE HZ (absoluto, nao cents) - e o que
// da o decay em 2 estagios + batimento lento de corda real. Loop de cada
// polarizacao: one-pole (perda HF) + one-zero + 2 allpasses de dispersao
// (rigidez: parciais fn = n*f0*sqrt(1+B*n^2)) + ganho g=10^(-3T/(T60*sr)).
// Acoplamento h->v one-way (gc pequeno via lowpass) = troca de energia
// sem risco de instabilidade (CMJ98/STK Guitar). Tension modulation:
// energia do loop encurta o delay (glide de ataque, twang de banjo).
struct StrS {
    v: Vec<f64>,       // polarizacao vertical (decay rapido)
    h: Vec<f64>,       // polarizacao horizontal (decay lento)
    w: usize,
    lp: [f64; 2],      // one-pole do loop, por polarizacao
    z1: [f64; 2],      // estado do one-zero
    ap: [[f64; 2]; 2], // allpasses de dispersao [pol][estagio]
    cpl: f64,          // lowpass do acoplamento h->v
    en: f64,           // energia recente (tension modulation)
    cfreq: f64,        // cache: freq da ultima compensacao de fase
    comp: f64,         // atraso de fase dos filtros do loop, em samples
}

/// atraso de fase em samples de um one-pole H(z)=k/(1-(1-k)z^-1) em om rad
fn pd_onepole(k: f64, om: f64) -> f64 {
    let b = 1.0 - k;
    (b * om.sin()).atan2(1.0 - b * om.cos()) / om
}

/// atraso de fase de um one-zero H(z)=(1-c)+c*z^-1
fn pd_onezero(c: f64, om: f64) -> f64 {
    (c * om.sin()).atan2((1.0 - c) + c * om.cos()) / om
}

/// atraso de fase de um allpass de 1a ordem H(z)=(a+z^-1)/(1+a*z^-1)
fn pd_allpass(a: f64, om: f64) -> f64 {
    let tn = (-om.sin()).atan2(a + om.cos());
    let td = (-a * om.sin()).atan2(1.0 + a * om.cos());
    -(tn - td) / om
}

/// hash deterministico de um ponto inteiro da malha de textura -> -1..1
#[inline]
fn tex_hash(seed: u64, i: i64) -> f64 {
    let mut x = (i as u64).wrapping_mul(0x9E3779B97F4A7C15) ^ seed;
    x ^= x >> 33;
    x = x.wrapping_mul(0xFF51AFD7ED558CCD);
    x ^= x >> 33;
    (x as f64 / u64::MAX as f64) * 2.0 - 1.0
}

/// textura de superficie fractal (3 oitavas de value noise, smoothstep):
/// o perfil de rugosidade da crina com breu, lido na POSICAO do arco.
/// Ler devagar = espectro grave; rapido = agudo. E o "raytracing" da
/// friccao: a superficie existe, o som e a leitura dela.
fn tex_surface(seed: u64, pos: f64) -> f64 {
    let mut acc = 0.0;
    let mut amp = 1.0;
    let mut p = pos;
    for o in 0..3u64 {
        let i = p.floor() as i64;
        let f = p - p.floor();
        let s = f * f * (3.0 - 2.0 * f);
        let os = seed ^ (o << 32);
        let a = tex_hash(os, i);
        let b = tex_hash(os, i + 1);
        acc += (a + (b - a) * s) * amp;
        amp *= 0.55;
        p = p * 2.7 + 13.7;
    }
    acc * 0.55
}

struct VozSinger {
    ph: f64,      // glottal phase 0..1
    jit: f64,     // smoothed jitter state (semitones)
    jt: f64,      // jitter random-walk target
    vph: f64,     // vibrato phase
    vrate: f64,   // personal vibrato rate hz
    vdel: f64,    // personal vibrato onset delay s
    shim: f64,    // smoothed shimmer state
    sht: f64,     // shimmer target
    s1: [f64; 4], // formant resonator states
    s2: [f64; 4],
    gprev: f64,   // previous glottal sample (flow derivative)
    asp: f64,     // lowpass da aspiracao (tilt -6db/oct)
    fsc: f64,     // personal formant scale (vocal tract length)
    pan: f64,
    onset: f64,   // personal attack offset s
    rng: u64,
    t: f64,       // singer-local time
}

#[derive(Clone)]
struct GrainVoice {
    pos: f64,  // playhead in source frames
    rate: f64, // frames per output sample
    age: f64,  // samples since spawn
    len: f64,  // grain length in samples
    pan: f64,  // -1..1
    amp: f64,
}

// ---------- 2x oversampling: 31-tap halfband FIR (windowed sinc, Blackman) ----------

const HB_TAPS: usize = 32; // ring size (31 taps + pad)

fn halfband() -> &'static [f64; 31] {
    use std::sync::OnceLock;
    static H: OnceLock<[f64; 31]> = OnceLock::new();
    H.get_or_init(|| {
        let mut h = [0.0f64; 31];
        let n = 31usize;
        for i in 0..n {
            let x = i as f64 - 15.0;
            let sinc = if x == 0.0 {
                0.5
            } else {
                (std::f64::consts::PI * x / 2.0).sin() / (std::f64::consts::PI * x)
            };
            let w = 0.42 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / (n - 1) as f64).cos()
                + 0.08 * (4.0 * std::f64::consts::PI * i as f64 / (n - 1) as f64).cos();
            h[i] = sinc * w;
        }
        // normalize DC gain to 1
        let s: f64 = h.iter().sum();
        for v in h.iter_mut() {
            *v /= s;
        }
        h
    })
}

fn fir_ring(buf: &[f64], w: usize, h: &[f64; 31]) -> f64 {
    let n = buf.len();
    let mut acc = 0.0;
    for (k, &c) in h.iter().enumerate() {
        acc += c * buf[(w + n - k) % n];
    }
    acc
}

/// one input sample -> upsample 2x -> nonlinear -> decimate back to 1x
fn os_process(x: f64, f: &dyn Fn(f64) -> f64, up: &mut Vec<f64>, dn: &mut Vec<f64>, w: usize) -> f64 {
    let h = halfband();
    let n = up.len();
    // zero-stuff (x, 0), interpolate with halfband (x2 gain), apply NL at 2x, then lowpass
    up[w % n] = x * 2.0;
    let u1 = fir_ring(up, w % n, h);
    up[(w + 1) % n] = 0.0;
    let u2 = fir_ring(up, (w + 1) % n, h);
    dn[w % n] = f(u1);
    let _ = fir_ring(dn, w % n, h); // keep history aligned
    dn[(w + 1) % n] = f(u2);
    fir_ring(dn, (w + 1) % n, h)
}

fn hermite(xm1: f64, x0: f64, x1: f64, x2: f64, t: f64) -> f64 {
    let c = (x1 - xm1) * 0.5;
    let v = x0 - x1;
    let w = c + v;
    let a = w + v + (x2 - x0) * 0.5;
    let b = w + a;
    ((a * t - b) * t + c) * t + x0
}

fn onepole_k(fc_hz: f64, sr: f64) -> f64 {
    1.0 - (-2.0 * std::f64::consts::PI * fc_hz.max(1.0) / sr).exp()
}

fn eq_power_mix(dry: (f64, f64), wet: (f64, f64), mix: f64) -> (f64, f64) {
    let m = mix.clamp(0.0, 1.0);
    let a = (m * std::f64::consts::FRAC_PI_2).cos();
    let b = (m * std::f64::consts::FRAC_PI_2).sin();
    (dry.0 * a + wet.0 * b, dry.1 * a + wet.1 * b)
}

fn flush_denorm(x: f64) -> f64 {
    if x.abs() < 1e-15 { 0.0 } else { x }
}

fn xorshift(s: &mut u64) -> f64 {
    let mut x = *s;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *s = x;
    // -1..1
    (x >> 11) as f64 / (1u64 << 52) as f64 * 2.0 - 1.0
}

fn smoothstep(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Turbulencia de sopro (fonte unica de flute/reed/brass/breath/voz).
/// Fisica: o ruido de sopro real e um dipolo cuja amplitude escala com U^2
/// (Verge/Fabre), so existe acima da transicao de Reynolds (sem fluxo = sem
/// ruido, sem piso), tem espectro em corcova (pico de Strouhal ~1..3khz
/// seguindo o fluxo) com rolloff forte acima - a oitava 5..15khz de ruido
/// branco e exatamente o percepto de "chiado de fita" - e NAO e estacionario:
/// o suporte do sopro deriva em ~centenas de ms e o jato tem intermitencia
/// em ~dezenas de ms (duas camadas Ornstein-Uhlenbeck).
/// O chamador injeta o resultado NO LOOP (junction/fluxo), nunca soma na
/// saida: dentro do loop o ruido ganha o pente harmonico do tubo e a
/// nao-linearidade o pulsa na taxa do ciclo (pitch-sincrono de graca).
#[derive(Clone, Default)]
struct TurbState {
    svf_lo: f64,
    svf_bp: f64,
    tilt: f64,
    ou_s: f64,  // wander lento (suporte do sopro), alvo/estado
    ou_st: f64,
    ou_f: f64,  // wander rapido (intermitencia do jato)
    ou_ft: f64,
}

impl TurbState {
    /// uj: velocidade de jato normalizada (~sqrt(pressao), ~1.0 em mf)
    #[inline]
    fn tick(&mut self, rng: &mut u64, uj: f64, sr: f64) -> f64 {
        let dt = 1.0 / sr;
        // wander OU por retarget aleatorio + smoothing (como o jitter de voz)
        if xorshift(rng) * 0.5 + 0.5 < 3.0 * dt {
            self.ou_st = xorshift(rng) * 0.30;
        }
        self.ou_s += onepole_k(1.2, sr) * (self.ou_st - self.ou_s);
        if xorshift(rng) * 0.5 + 0.5 < 24.0 * dt {
            self.ou_ft = xorshift(rng) * 0.15;
        }
        self.ou_f += onepole_k(9.0, sr) * (self.ou_ft - self.ou_f);
        let wander = (1.0 + self.ou_s + self.ou_f).max(0.0);
        // gate de Reynolds: nasce suave a partir de uj~0.28
        let gate = smoothstep(0.28, 0.55, uj);
        if gate <= 0.0 {
            // fluxo parado: estado decai, saida zero (silencio de verdade)
            self.svf_bp = flush_denorm(self.svf_bp * 0.999);
            self.tilt = flush_denorm(self.tilt * 0.999);
            return 0.0;
        }
        // corcova de Strouhal: fc segue o fluxo (pp escuro, ff mais claro)
        let fc = (800.0 + 1700.0 * uj.min(1.6)).clamp(500.0, 3400.0);
        let f = 2.0 * (std::f64::consts::PI * fc / sr).sin();
        let white = xorshift(rng);
        self.svf_lo = flush_denorm(self.svf_lo + f * self.svf_bp);
        let hi = white - self.svf_lo - 1.25 * self.svf_bp; // Q ~0.8 (largo)
        self.svf_bp = flush_denorm(self.svf_bp + f * hi);
        // tilt: mais -6db/oct acima de ~3.5khz mata a regiao "fita"
        let kt = onepole_k(3500.0, sr);
        self.tilt = flush_denorm(self.tilt + kt * (self.svf_bp - self.tilt));
        self.tilt * uj * uj * gate * wander
    }
}

fn poly_blep(t: f64, dt: f64) -> f64 {
    if t < dt {
        let x = t / dt;
        x + x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

// ---------- node state store ----------

/// Flat per-scope node state: ids within one synth def are contiguous (the
/// parser assigns them sequentially during that def's parse), so a Vec indexed
/// by (id - base) replaces the old HashMap. Ids themselves are never remapped -
/// they seed per-node rngs, so audio stays bit-identical to the hashed path.
pub struct StateStore {
    base: usize,
    slots: Vec<Option<NodeStateBox>>,
}

impl Default for StateStore {
    fn default() -> Self {
        Self::new()
    }
}

impl StateStore {
    pub fn new() -> Self {
        StateStore { base: 0, slots: Vec::new() }
    }

    /// reset all node state, keeping the allocation (voice restart)
    pub fn clear(&mut self) {
        for s in self.slots.iter_mut() {
            *s = None;
        }
    }

    #[inline]
    fn slot(&mut self, id: usize) -> &mut Option<NodeStateBox> {
        if self.slots.is_empty() {
            self.base = id;
        } else if id < self.base {
            // rebase (rare: only while the range is being discovered)
            let shift = self.base - id;
            for _ in 0..shift {
                self.slots.insert(0, None);
            }
            self.base = id;
        }
        let i = id - self.base;
        if i >= self.slots.len() {
            self.slots.resize_with(i + 1, || None);
        }
        &mut self.slots[i]
    }

    #[inline]
    pub fn get_or(&mut self, id: usize, f: impl FnOnce() -> NodeStateBox) -> &mut NodeStateBox {
        self.slot(id).get_or_insert_with(f)
    }

    #[inline]
    pub fn contains_key(&mut self, id: &usize) -> bool {
        self.slot(*id).is_some()
    }

    #[inline]
    pub fn insert(&mut self, id: usize, b: NodeStateBox) {
        *self.slot(id) = Some(b);
    }

    #[inline]
    pub fn get_mut(&mut self, id: &usize) -> Option<&mut NodeStateBox> {
        self.slot(*id).as_mut()
    }
}

// ---------- eval context ----------

pub struct Ctx<'a> {
    pub sr: f64,
    pub bpm: f64,
    pub note: f64,
    pub vel: f64,
    pub gate: f64,
    pub time: f64,
    pub rand: f64,
    pub vidx: f64,
    /// scheduled note duration in seconds (0 when unknown: live/one-shot contexts)
    pub dur: f64,
    pub state: &'a mut StateStore,
    /// this-sample let values, slot-indexed by resolve (voice or global scope)
    pub cur: &'a [Val],
    /// previous-sample let values, slot-indexed
    pub prev: &'a [Val],
    pub globals: &'a [Val],
    pub params: &'a [Val],
    /// bus/master chain input signal
    pub bus_in: Val,
    /// sidechain routing: other synths' bus outputs by name (bus scope only)
    pub synth_outs: Option<&'a HashMap<String, (f64, f64)>>,
    /// name fallback for unresolved idents in bus scope (params by name)
    pub params_by_name: Option<&'a HashMap<String, Val>>,
    pub seed: u64,
}

pub struct NodeStateBox(NodeState);

fn arg<'e>(args: &'e [(String, Expr)], name: &str) -> Option<&'e Expr> {
    args.iter().find(|(k, _)| k == name).map(|(_, e)| e)
}

/// table literal arg (rows of (value, unit) columns), if present
fn table_arg<'e>(args: &'e [(String, Expr)], name: &str) -> Option<&'e Vec<Vec<(f64, Unit)>>> {
    match arg(args, name) {
        Some(Expr::Table(rows)) => Some(rows),
        _ => None,
    }
}

/// table column as seconds: ms-unit entries convert, bare scalars read as seconds
fn col_sec(v: (f64, Unit)) -> f64 {
    match v.1 {
        Unit::Ms => v.0 / 1000.0,
        _ => v.0,
    }
}

fn eval_arg(args: &[(String, Expr)], name: &str, default: Val, ctx: &mut Ctx) -> Val {
    match arg(args, name) {
        Some(e) => eval(e, ctx),
        None => default,
    }
}

fn input_sig(args: &[(String, Expr)], ctx: &mut Ctx) -> Val {
    match arg(args, "_0") {
        Some(e) => eval(e, ctx),
        None => Val::S(0.0),
    }
}

fn curve_val(start: f64, target: f64, t: f64, dur: f64, curve: Curve) -> f64 {
    if dur <= 0.0 || t >= dur {
        return target;
    }
    let p = (t / dur).clamp(0.0, 1.0);
    match curve {
        Curve::Lin => start + (target - start) * p,
        Curve::Exp => target + (start - target) * (-6.9 * p).exp(),
        Curve::Log => start + (target - start) * ((1.0 + 9.0 * p).ln() / 10f64.ln()),
        Curve::Pow(n) => start + (target - start) * p.powf(n),
    }
}

fn env_step(st: &mut EnvState, gate: f64, dt: f64) -> f64 {
    if st.done {
        return st.cur;
    }
    let has_sustain = st.sustain.is_some();
    if has_sustain && !st.released && gate < 0.5 {
        st.released = true;
        st.in_release = true;
        st.seg = 0;
        st.t = 0.0;
        st.seg_start_val = st.cur;
        if st.release.is_empty() {
            st.done = true;
            return st.cur;
        }
    }
    let segs = if st.in_release { &st.release } else { &st.segs };
    if st.seg >= segs.len() {
        if st.in_release {
            st.done = true;
        } else if has_sustain {
            st.cur = st.sustain.unwrap();
        } else {
            st.done = true;
        }
        return st.cur;
    }
    let seg = &segs[st.seg];
    st.cur = curve_val(st.seg_start_val, seg.target, st.t, seg.time_s, seg.curve);
    st.t += dt;
    if st.t >= seg.time_s {
        st.cur = seg.target;
        st.seg += 1;
        st.t = 0.0;
        st.seg_start_val = st.cur;
    }
    st.cur
}

// Safe by construction: interprets the parsed .synth AST (a closed set of DSP
// node types) - no dynamic code execution, no host access.
pub fn eval(e: &Expr, ctx: &mut Ctx) -> Val {
    match e {
        Expr::Num { v, unit } => match unit {
            Unit::Scalar => Val::S(*v),
            Unit::Hz => Val::Hz(*v),
            Unit::Ms => Val::Ms(*v),
            Unit::St => Val::StI(*v),
            Unit::Beat => Val::Beat(*v),
        },
        Expr::Str(_) => Val::S(0.0), // strings are only meaningful as node args (table/sample paths)
        Expr::Table(_) => Val::S(0.0), // tables are only meaningful as node args (modes: etc)
        Expr::Neg(x) => binop('-', Val::S(0.0), eval(x, ctx)),
        Expr::Bin { op, l, r } => {
            let a = eval(l, ctx);
            let b = eval(r, ctx);
            binop(*op, a, b)
        }
        // slot-indexed reads (resolve::resolve_synth), zero hashing per sample
        Expr::VarCur(i) => ctx.cur.get(*i).copied().unwrap_or(Val::S(0.0)),
        Expr::VarPrev(i) => ctx.prev.get(*i).copied().unwrap_or(Val::S(0.0)),
        Expr::VarGlobal(i) => ctx.globals.get(*i).copied().unwrap_or(Val::S(0.0)),
        Expr::VarParam(i) => ctx.params.get(*i).copied().unwrap_or(Val::S(0.0)),
        Expr::BusIn => ctx.bus_in,
        Expr::Builtin(b) => match b {
            crate::parser::BuiltinVar::Note => Val::Pitch(ctx.note),
            crate::parser::BuiltinVar::Velocity => Val::S(ctx.vel),
            crate::parser::BuiltinVar::Gate => Val::S(ctx.gate),
            crate::parser::BuiltinVar::Time => Val::S(ctx.time),
            crate::parser::BuiltinVar::Dur => Val::S(ctx.dur),
            crate::parser::BuiltinVar::Rand => Val::S(ctx.rand),
            crate::parser::BuiltinVar::VoiceIdx => Val::S(ctx.vidx),
        },
        Expr::Ident(name) => {
            // only reachable in bus/master scope (synth names for sidechain key:)
            // or for unknown names; fallback order matches the old interpreter:
            // synth outs > params by name > builtins > 0
            if let Some(outs) = ctx.synth_outs {
                if let Some((l, r)) = outs.get(name) {
                    return Val::St2(*l, *r);
                }
            }
            if let Some(pm) = ctx.params_by_name {
                if let Some(v) = pm.get(name) {
                    return *v;
                }
            }
            match name.as_str() {
                "note" => Val::Pitch(ctx.note),
                "velocity" => Val::S(ctx.vel),
                "gate" => Val::S(ctx.gate),
                "time" => Val::S(ctx.time),
                "dur" => Val::S(ctx.dur),
                "rand" => Val::S(ctx.rand),
                "voice_idx" => Val::S(ctx.vidx),
                _ => Val::S(0.0),
            }
        }
        Expr::Env { start, segs, sustain, release, id } => {
            let dt = 1.0 / ctx.sr;
            if !ctx.state.contains_key(id) {
                // freeze targets at note_on
                let mut is_hz = false;
                let mk = |s: &Seg, ctx: &mut Ctx, is_hz: &mut bool| -> EnvSegR {
                    let tv = eval(&s.target, ctx);
                    if matches!(tv, Val::Hz(_) | Val::Pitch(_)) {
                        *is_hz = true;
                    }
                    EnvSegR {
                        target: if matches!(tv, Val::Pitch(_)) { tv.as_hz() } else { tv.num() },
                        time_s: eval(&s.time, ctx).as_sec(ctx.bpm),
                        curve: s.curve,
                    }
                };
                let rsegs: Vec<EnvSegR> = segs.iter().map(|s| mk(s, ctx, &mut is_hz)).collect();
                let rrel: Vec<EnvSegR> = release.iter().map(|s| mk(s, ctx, &mut is_hz)).collect();
                let sus = sustain.as_ref().map(|s| eval(s, ctx).num());
                let start_v = match start {
                    Some(s) => {
                        let v = eval(s, ctx);
                        if matches!(v, Val::Hz(_) | Val::Pitch(_)) {
                            is_hz = true;
                        }
                        v.as_hz_or_num()
                    }
                    None => 0.0,
                };
                let st = EnvState {
                    seg_start_val: start_v,
                    segs: rsegs,
                    sustain: sus,
                    release: rrel,
                    is_hz,
                    seg: 0,
                    t: 0.0,
                    cur: start_v,
                    released: false,
                    in_release: false,
                    done: false,
                };
                ctx.state.insert(*id, NodeStateBox(NodeState::Env(st)));
            }
            let gate = ctx.gate;
            if let Some(NodeStateBox(NodeState::Env(st))) = ctx.state.get_mut(id) {
                let v = env_step(st, gate, dt);
                if st.is_hz {
                    Val::Hz(v)
                } else {
                    Val::S(v)
                }
            } else {
                Val::S(0.0)
            }
        }
        Expr::Call { op, args, id, .. } => eval_call(*op, args, *id, ctx),
    }
}

impl Val {
    fn as_hz_or_num(self) -> f64 {
        match self {
            Val::Pitch(_) | Val::Hz(_) => self.as_hz(),
            v => v.num(),
        }
    }
}

fn eval_call(op: Op, args: &[(String, Expr)], id: usize, ctx: &mut Ctx) -> Val {
    let dt = 1.0 / ctx.sr;
    match op {
        Op::Hz => Val::Hz(input_sig(args, ctx).as_hz()),
        Op::PitchOp => {
            let f = input_sig(args, ctx).as_hz();
            Val::Pitch(69.0 + 12.0 * (f / 440.0).log2())
        }
        Op::Unipolar => {
            let v = input_sig(args, ctx);
            binop('*', binop('+', v, Val::S(1.0)), Val::S(0.5))
        }
        Op::Min | Op::Max | Op::Clamp | Op::Abs => {
            let a = input_sig(args, ctx);
            match op {
                Op::Abs => Val::S(a.num().abs()),
                Op::Min => {
                    let b = eval_arg(args, "_1", Val::S(0.0), ctx);
                    Val::S(a.num().min(b.num()))
                }
                Op::Max => {
                    let b = eval_arg(args, "_1", Val::S(0.0), ctx);
                    Val::S(a.num().max(b.num()))
                }
                _ => {
                    let lo = eval_arg(args, "_1", Val::S(0.0), ctx);
                    let hi = eval_arg(args, "_2", Val::S(1.0), ctx);
                    Val::S(a.num().clamp(lo.num(), hi.num()))
                }
            }
        }
        Op::Gain => {
            let sig = input_sig(args, ctx);
            let amt = eval_arg(args, "amount", eval_arg(args, "_1", Val::S(1.0), ctx), ctx);
            binop('*', sig, Val::S(amt.num()))
        }
        Op::Pan => {
            let sig = input_sig(args, ctx);
            let pos = eval_arg(args, "pos", eval_arg(args, "_1", Val::S(0.0), ctx), ctx)
                .num()
                .clamp(-1.0, 1.0);
            let a = (pos + 1.0) * std::f64::consts::FRAC_PI_4;
            let x = sig.num();
            Val::St2(x * a.cos() * std::f64::consts::SQRT_2 * 0.70710678,
                     x * a.sin() * std::f64::consts::SQRT_2 * 0.70710678)
        }
        Op::Sine | Op::Triangle | Op::Saw | Op::Square | Op::Pulse => osc(op, args, id, ctx),
        Op::Wavetable => {
            let freq = eval_arg(args, "freq", Val::Hz(440.0), ctx).as_hz().clamp(0.01, 20000.0);
            let g = eval_arg(args, "gain", Val::S(1.0), ctx).num();
            let pos = eval_arg(args, "pos", Val::S(0.0), ctx).num();
            let tname: &str = match arg(args, "table") {
                Some(Expr::Str(s)) => s,
                Some(Expr::Ident(s)) => s,
                _ => "basic_shapes",
            };
            let tab = match get_table(tname) {
                Some(t) => t,
                None => return Val::S(0.0), // E022 candidate
            };
            let st = ctx.state.get_or(id, || NodeStateBox(NodeState::Phase(0.0)));
            if let NodeStateBox(NodeState::Phase(ph)) = st {
                // mip by fundamental: level k keeps harmonics <= 1024/2^k;
                // fractional part crossfades adjacent levels (kills brightness steps in glides)
                let nyq = ctx.sr * 0.5;
                let allowed_h = (nyq / freq).max(1.0);
                let mip_f = ((WT_LEN as f64 / 2.0) / allowed_h).log2().max(0.0);
                let v = wt_read(&tab, pos, mip_f, *ph);
                *ph = (*ph + freq * dt).fract();
                Val::S(v * g)
            } else {
                Val::S(0.0)
            }
        }
        Op::Noise => {
            let g = eval_arg(args, "gain", Val::S(1.0), ctx).num();
            let color: &str = match arg(args, "color") {
                Some(Expr::Ident(c)) => c,
                _ => "white",
            };
            let nseed = ctx.seed ^ (id as u64).wrapping_mul(0x9E3779B97F4A7C15) | 1;
            if color == "pink" {
                let st = ctx.state.get_or(id, || {
                    NodeStateBox(NodeState::Pink { rng: nseed, b: [0.0; 3] })
                });
                if let NodeStateBox(NodeState::Pink { rng, b }) = st {
                    let w = xorshift(rng);
                    b[0] = 0.99765 * b[0] + w * 0.0990460;
                    b[1] = 0.96300 * b[1] + w * 0.2965164;
                    b[2] = 0.57000 * b[2] + w * 1.0526913;
                    let p = (b[0] + b[1] + b[2] + w * 0.1848) * 0.2;
                    return Val::S(p * g);
                }
                Val::S(0.0)
            } else if color == "brown" || color == "red" {
                let st = ctx.state.get_or(id, || {
                    NodeStateBox(NodeState::Brown { rng: nseed, y: 0.0 })
                });
                if let NodeStateBox(NodeState::Brown { rng, y }) = st {
                    // leaky integration of white; ~x3 brings RMS near white (~0.58)
                    *y = (*y + xorshift(rng) * 0.02) * 0.998;
                    return Val::S(*y * 3.0 * g);
                }
                Val::S(0.0)
            } else if color == "blue" {
                let st = ctx.state.get_or(id, || {
                    NodeStateBox(NodeState::Blue { rng: nseed, b: [0.0; 3], prev: 0.0 })
                });
                if let NodeStateBox(NodeState::Blue { rng, b, prev }) = st {
                    let w = xorshift(rng);
                    b[0] = 0.99765 * b[0] + w * 0.0990460;
                    b[1] = 0.96300 * b[1] + w * 0.2965164;
                    b[2] = 0.57000 * b[2] + w * 1.0526913;
                    let p = (b[0] + b[1] + b[2] + w * 0.1848) * 0.2;
                    let v = (p - *prev) * 4.0; // differentiated pink, gain-compensated
                    *prev = p;
                    return Val::S(v * g);
                }
                Val::S(0.0)
            } else if color == "violet" {
                let st = ctx.state.get_or(id, || {
                    NodeStateBox(NodeState::Violet { rng: nseed, prev: 0.0 })
                });
                if let NodeStateBox(NodeState::Violet { rng, prev }) = st {
                    let w = xorshift(rng);
                    let v = (w - *prev) * 0.5;
                    *prev = w;
                    return Val::S(v * g);
                }
                Val::S(0.0)
            } else if color == "velvet" {
                let density = eval_arg(args, "density", Val::S(2000.0), ctx).num().max(1.0);
                let p_hit = density / ctx.sr;
                let st = ctx.state.get_or(id, || {
                    NodeStateBox(NodeState::Velvet { rng: nseed })
                });
                if let NodeStateBox(NodeState::Velvet { rng }) = st {
                    let u = xorshift(rng) * 0.5 + 0.5;
                    let v = if u < p_hit {
                        if xorshift(rng) >= 0.0 { 1.0 } else { -1.0 }
                    } else {
                        0.0
                    };
                    return Val::S(v * g);
                }
                Val::S(0.0)
            } else if color == "crackle" {
                let density = eval_arg(args, "density", Val::S(30.0), ctx).num().max(0.1);
                let p_hit = density / ctx.sr;
                let st = ctx.state.get_or(id, || {
                    NodeStateBox(NodeState::Crackle { rng: nseed, env: 0.0, k: 0.0, lp: 0.0 })
                });
                if let NodeStateBox(NodeState::Crackle { rng, env, k, lp }) = st {
                    let u = xorshift(rng) * 0.5 + 0.5;
                    if u < p_hit {
                        let amp = xorshift(rng); // random amplitude/sign
                        *env = amp;
                        // decay tau 0.5..4ms
                        let tau = 0.0005 + (xorshift(rng) * 0.5 + 0.5) * 0.0035;
                        *k = (-1.0 / (tau * ctx.sr)).exp();
                    }
                    *env *= *k;
                    // fixed 6khz lowpass
                    let klp = onepole_k(6000.0, ctx.sr);
                    *lp += klp * (*env - *lp);
                    return Val::S(flush_denorm(*lp) * g);
                }
                Val::S(0.0)
            } else if color == "grey" {
                // approx inverse-loudness: pink lows + violet highs over white bed
                let st = ctx.state.get_or(id, || {
                    NodeStateBox(NodeState::Blue { rng: nseed, b: [0.0; 3], prev: 0.0 })
                });
                if let NodeStateBox(NodeState::Blue { rng, b, prev }) = st {
                    let w = xorshift(rng);
                    b[0] = 0.99765 * b[0] + w * 0.0990460;
                    b[1] = 0.96300 * b[1] + w * 0.2965164;
                    b[2] = 0.57000 * b[2] + w * 1.0526913;
                    let pink = (b[0] + b[1] + b[2] + w * 0.1848) * 0.2;
                    let violet = (w - *prev) * 0.5;
                    *prev = w;
                    return Val::S((pink * 0.7 + violet * 0.5 + w * 0.15) * g);
                }
                Val::S(0.0)
            } else {
                let st = ctx.state.get_or(id, || {
                    NodeStateBox(NodeState::Rng(ctx.seed ^ (id as u64).wrapping_mul(0x9E3779B97F4A7C15) | 1))
                });
                if let NodeStateBox(NodeState::Rng(rng)) = st {
                    Val::S(xorshift(rng) * g)
                } else {
                    Val::S(0.0)
                }
            }
        }
        Op::Lowpass | Op::Highpass | Op::Bandpass | Op::Notch => {
            let sig = input_sig(args, ctx);
            let fc = eval_arg(args, "cutoff", Val::Hz(1000.0), ctx).as_hz();
            // q extended to 0..1.2: >1 self-oscillates (opt-in per tier1 §6.2)
            let q = eval_arg(args, "q", Val::S(0.5), ctx).num().clamp(0.0, 1.2);
            // slope: 24db = two cascaded SVF stages, q distributed as sqrt
            let stages = match arg(args, "slope") {
                Some(e) => {
                    let v = eval(e, ctx).num();
                    if v > 6.0 { 2 } else { 1 } // 24db (linear 15.85 or raw 24) vs 12db
                }
                None => 1,
            };
            let q_stage = if stages == 2 { q.sqrt() } else { q };
            let fc = fc.clamp(5.0, (ctx.sr * 0.49).min(20000.0));
            let g = (std::f64::consts::PI * fc / ctx.sr).tan();
            let k = 2.0 - 1.9 * q_stage.min(1.0) - 3.0 * (q_stage - 1.0).max(0.0);
            let clip_state = q > 1.0;
            let a1 = 1.0 / (1.0 + g * (g + k));
            let a2 = g * a1;
            let a3 = g * a2;
            let st = ctx.state.get_or(id, || NodeStateBox(NodeState::Svf { ic1: [0.0; 4], ic2: [0.0; 4] }));
            if let NodeStateBox(NodeState::Svf { ic1, ic2 }) = st {
                let (l, r) = sig.stereo();
                let mono = !matches!(sig, Val::St2(_, _));
                let mut process = |v0: f64, ch: usize, stage: usize| -> f64 {
                    let i = ch * 2 + stage;
                    let v3 = v0 - ic2[i];
                    let v1 = a1 * ic1[i] + a2 * v3;
                    let v2 = ic2[i] + a2 * ic1[i] + a3 * v3;
                    ic1[i] = 2.0 * v1 - ic1[i];
                    ic2[i] = 2.0 * v2 - ic2[i];
                    if clip_state {
                        ic1[i] = ic1[i].clamp(-4.0, 4.0);
                        ic2[i] = ic2[i].clamp(-4.0, 4.0);
                    }
                    match op {
                        Op::Lowpass => v2,
                        Op::Bandpass => v1,
                        Op::Highpass => v0 - k * v1 - v2,
                        _ => v0 - k * v1, // notch
                    }
                };
                let mut run = |x: f64, ch: usize| -> f64 {
                    let mut y = process(x, ch, 0);
                    if stages == 2 {
                        y = process(y, ch, 1);
                    }
                    y
                };
                let ol = run(l, 0);
                if mono {
                    // keep right channel state in sync for later stereo use
                    let _ = run(r, 1);
                    Val::S(ol)
                } else {
                    let or = run(r, 1);
                    Val::St2(ol, or)
                }
            } else {
                Val::S(0.0)
            }
        }
        Op::Lfo => {
            let rate_v = eval_arg(args, "rate", Val::Hz(1.0), ctx);
            // beat rate = PERIOD in beats (musical reading): freq = bpm/60/beats
            let rate = match rate_v {
                Val::Beat(b) => ctx.bpm / 60.0 / b.max(1e-6),
                v => v.as_hz(),
            };
            let amount = eval_arg(args, "amount", Val::S(1.0), ctx).num();
            let phase0 = eval_arg(args, "phase", Val::S(0.0), ctx).num();
            let shape: &str = match arg(args, "shape") {
                Some(Expr::Ident(s)) => s,
                _ => "sine",
            };
            let st = ctx.state.get_or(id, || {
                NodeStateBox(NodeState::Lfo { ph: phase0.fract(), hold: 0.0, rng: ctx.seed ^ (id as u64) | 1 })
            });
            if let NodeStateBox(NodeState::Lfo { ph, hold, rng }) = st {
                let p = *ph;
                let v = match shape {
                    "triangle" => 1.0 - 4.0 * (p - 0.5).abs(),
                    "square" => if p < 0.5 { 1.0 } else { -1.0 },
                    "saw" => 2.0 * p - 1.0,
                    "saw_down" => 1.0 - 2.0 * p,
                    "sample_hold" => *hold,
                    _ => (2.0 * std::f64::consts::PI * p).sin(),
                };
                *ph += rate * dt;
                if *ph >= 1.0 {
                    *ph -= 1.0;
                    *hold = xorshift(rng);
                }
                Val::S(v * amount)
            } else {
                Val::S(0.0)
            }
        }
        Op::Saturate | Op::Clip | Op::Drive => {
            let sig = input_sig(args, ctx);
            let (l, r) = sig.stereo();
            let stereo = matches!(sig, Val::St2(_, _));
            let amount = eval_arg(args, "amount", Val::S(0.5), ctx).num();
            enum Shaper {
                Clip(f64),
                Tanh(f64, f64),
            }
            impl Shaper {
                #[inline]
                fn go(&self, x: f64) -> f64 {
                    match self {
                        Shaper::Clip(lev) => x.clamp(-lev, *lev),
                        Shaper::Tanh(g, norm) => (x * g).tanh() / norm,
                    }
                }
            }
            let sh = match op {
                Op::Clip => Shaper::Clip(eval_arg(args, "level", Val::S(1.0), ctx).num()),
                Op::Drive => Shaper::Tanh(1.0 + 3.0 * amount, (1.0 + 4.0 * amount).tanh()),
                _ => {
                    let g = 1.0 + 4.0 * amount;
                    Shaper::Tanh(g, g.tanh().max(1e-9))
                }
            };
            let f = |x: f64| sh.go(x);
            // selective 2x oversampling (tier3 §4): amount > 0.5, unless oversample: off
            let os_off = matches!(arg(args, "oversample"), Some(Expr::Ident(s)) if s == "off");
            let oversample = op != Op::Clip && amount > 0.5 && !os_off;
            if oversample {
                let st = ctx.state.get_or(id, || {
                    NodeStateBox(NodeState::Os {
                        up: [vec![0.0; HB_TAPS], vec![0.0; HB_TAPS]],
                        dn: [vec![0.0; HB_TAPS], vec![0.0; HB_TAPS]],
                        w: 0,
                    })
                });
                if let NodeStateBox(NodeState::Os { up, dn, w }) = st {
                    let ol = os_process(l, &f, &mut up[0], &mut dn[0], *w);
                    let or = if stereo { os_process(r, &f, &mut up[1], &mut dn[1], *w) } else { ol };
                    *w = (*w + 2) % HB_TAPS;
                    if stereo {
                        return Val::St2(ol, or);
                    }
                    return Val::S(ol);
                }
            }
            if stereo {
                Val::St2(f(l), f(r))
            } else {
                Val::S(f(l))
            }
        }
        Op::Delay1 => {
            let sig = input_sig(args, ctx);
            let st = ctx.state.get_or(id, || NodeStateBox(NodeState::Delay1 { prev: (0.0, 0.0) }));
            if let NodeStateBox(NodeState::Delay1 { prev }) = st {
                let out = *prev;
                *prev = sig.stereo();
                if matches!(sig, Val::St2(_, _)) {
                    Val::St2(out.0, out.1)
                } else {
                    Val::S(out.0)
                }
            } else {
                Val::S(0.0)
            }
        }
        Op::Delay => {
            let sig = input_sig(args, ctx);
            let bpm = ctx.bpm;
            let time_ms = (eval_arg(args, "time", Val::Ms(10.0), ctx).as_sec(bpm) * 1000.0).clamp(0.02, 200.0);
            let fb = eval_arg(args, "feedback", Val::S(0.0), ctx).num();
            let cap = (ctx.sr * 0.25) as usize;
            let st = ctx.state.get_or(id, || NodeStateBox(NodeState::Delay { buf: vec![(0.0, 0.0); cap], w: 0 }));
            if let NodeStateBox(NodeState::Delay { buf, w }) = st {
                let n = buf.len() as f64;
                let d = (time_ms / 1000.0 * ctx.sr).clamp(1.0, n - 2.0);
                let rp = (*w as f64 - d + n) % n;
                let i0 = rp.floor() as usize;
                let i1 = (i0 + 1) % buf.len();
                let fr = rp - rp.floor();
                let (a, b) = (buf[i0], buf[i1]);
                let out = (a.0 + (b.0 - a.0) * fr, a.1 + (b.1 - a.1) * fr);
                let (il, ir) = sig.stereo();
                buf[*w] = (il + out.0 * fb, ir + out.1 * fb);
                *w = (*w + 1) % buf.len();
                if matches!(sig, Val::St2(_, _)) {
                    Val::St2(out.0, out.1)
                } else {
                    Val::S(out.0)
                }
            } else {
                Val::S(0.0)
            }
        }
        Op::Sample => {
            let path: &str = match arg(args, "_0").or(arg(args, "path")) {
                Some(Expr::Str(s)) => s,
                _ => return Val::S(0.0),
            };
            let smp = match get_sample(path) {
                Some(s) => s,
                None => return Val::S(0.0),
            };
            let g = eval_arg(args, "gain", Val::S(1.0), ctx).num();
            let root = match arg(args, "root") {
                Some(Expr::Ident(n)) => crate::score::note_name_to_midi(n).unwrap_or(48.0),
                Some(e) => {
                    let v = eval(e, ctx);
                    match v {
                        Val::Pitch(p) => p,
                        _ => 48.0,
                    }
                }
                None => 48.0, // c3
            };
            let pitch = match arg(args, "pitch") {
                Some(e) => match eval(e, ctx) {
                    Val::Pitch(p) => p,
                    v => 69.0 + 12.0 * (v.as_hz() / 440.0).log2(),
                },
                None => root,
            };
            let mut ratio = 2f64.powf((pitch - root) / 12.0) * (smp.sr / ctx.sr);
            // W007 range: clamp repitch ratio 0.25..4
            ratio = ratio.clamp(0.25, 4.0);
            let start = eval_arg(args, "start", Val::S(0.0), ctx).num().clamp(0.0, 1.0);
            let loop_mode: &str = match arg(args, "loop") {
                Some(Expr::Ident(s)) => s.as_str(),
                _ => "off",
            };
            let nf = (smp.data.len() / smp.ch) as f64;
            let st = ctx.state.get_or(id, || {
                NodeStateBox(NodeState::Sample { pos: start * nf, dir: 1.0 })
            });
            if let NodeStateBox(NodeState::Sample { pos, dir }) = st {
                if *pos >= nf && loop_mode == "off" {
                    return Val::St2(0.0, 0.0);
                }
                let xf = (0.005 * ctx.sr).min(nf * 0.1); // 5ms loop crossfade (NORMATIVE)
                let (mut l, mut r) = sample_frame(&smp, *pos);
                match loop_mode {
                    "forward" => {
                        // crossfade tail into head
                        if *pos > nf - xf {
                            let t = (*pos - (nf - xf)) / xf;
                            let (l2, r2) = sample_frame(&smp, *pos - (nf - xf));
                            l = l * (1.0 - t) + l2 * t;
                            r = r * (1.0 - t) + r2 * t;
                        }
                        *pos += ratio;
                        if *pos >= nf {
                            *pos -= nf - xf;
                        }
                    }
                    "pingpong" => {
                        *pos += ratio * *dir;
                        if *pos >= nf - 1.0 {
                            *pos = nf - 1.0;
                            *dir = -1.0;
                        } else if *pos <= 0.0 {
                            *pos = 0.0;
                            *dir = 1.0;
                        }
                    }
                    _ => {
                        *pos += ratio;
                    }
                }
                Val::St2(l * g, r * g)
            } else {
                Val::S(0.0)
            }
        }
        Op::Pluck => {
            // Karplus-Strong with fractional allpass tuning (stays in tune up high)
            let freq = eval_arg(args, "freq", Val::Hz(220.0), ctx).as_hz().clamp(20.0, 8000.0);
            let damp = eval_arg(args, "damp", Val::S(0.5), ctx).num().clamp(0.0, 1.0);
            let bpm = ctx.bpm;
            let decay_s = eval_arg(args, "decay", Val::Ms(2000.0), ctx).as_sec(bpm).max(0.05);
            let position = eval_arg(args, "position", Val::S(0.3), ctx).num().clamp(0.02, 0.5);
            let exciter: &str = match arg(args, "exciter") {
                Some(Expr::Ident(s)) => s.as_str(),
                _ => "white",
            };
            let period = ctx.sr / freq;
            let n = ((period - 0.5).floor() as usize).max(2);
            let frac = period - n as f64 - 0.5; // ~0..1, absorbed by the allpass
            let c = ((1.0 - frac.clamp(0.0, 0.95)) / (1.0 + frac.clamp(0.0, 0.95))).clamp(0.0, 1.0);
            let seed = ctx.seed ^ (id as u64).wrapping_mul(0x9E3779B97F4A7C15) | 1;
            let st = ctx.state.get_or(id, || {
                let mut rng = seed;
                let mut exc: Vec<f64> = (0..n)
                    .map(|_| match exciter {
                        "velvet" => {
                            let u = xorshift(&mut rng) * 0.5 + 0.5;
                            if u < 0.15 {
                                if xorshift(&mut rng) >= 0.0 { 1.0 } else { -1.0 }
                            } else {
                                0.0
                            }
                        }
                        _ => xorshift(&mut rng),
                    })
                    .collect();
                // pluck-position comb: exc' = exc - delay(exc, period*position)
                let off = ((n as f64) * position) as usize;
                let orig = exc.clone();
                for i in 0..n {
                    exc[i] = orig[i] - orig[(i + n - off.max(1)) % n];
                }
                NodeStateBox(NodeState::Pluck { buf: exc, w: 0, lp: 0.0, ap: 0.0 })
            });
            if let NodeStateBox(NodeState::Pluck { buf, w, lp, ap }) = st {
                let x = buf[*w];
                // fractional-delay allpass (1st order, transposed DF2: one state)
                let y = c * x + *ap;
                *ap = x - c * y;
                // in-loop damping lowpass
                let cutoff = 1000.0 + (1.0 - damp) * 9000.0;
                let k = onepole_k(cutoff, ctx.sr);
                *lp = flush_denorm(*lp + k * (y - *lp));
                // RT60 feedback gain
                let g = 10f64.powf(-3.0 * period / (decay_s * ctx.sr));
                buf[*w] = *lp * g;
                *w = (*w + 1) % buf.len();
                Val::S(x)
            } else {
                Val::S(0.0)
            }
        }
        Op::Strings => {
            // string(): corda dedilhada com fisica real (ver StrS).
            // exciter: pick | finger | thumb | slap | snap (bartok).
            // stiff: rigidez (dispersao), pol: split de polarizacao,
            // tension: glide de ataque/twang, pickup: tap de captador
            // (comb sin(n*pi*p)), mute: abafamento do dedo (pizz seco).
            let freq = eval_arg(args, "freq", Val::Hz(220.0), ctx).as_hz().clamp(24.0, 6000.0);
            let bpm = ctx.bpm;
            let decay_s = eval_arg(args, "decay", Val::Ms(4000.0), ctx).as_sec(bpm).max(0.05);
            let bright = eval_arg(args, "bright", Val::S(0.6), ctx).num().clamp(0.0, 1.0);
            let position = eval_arg(args, "position", Val::S(0.28), ctx).num().clamp(0.02, 0.5);
            let hard = eval_arg(args, "hard", Val::S(0.5), ctx).num().clamp(0.0, 1.0);
            let stiff = eval_arg(args, "stiff", Val::S(0.0), ctx).num().clamp(0.0, 1.0);
            let pol = eval_arg(args, "pol", Val::S(0.4), ctx).num().clamp(0.0, 1.0);
            let tension = eval_arg(args, "tension", Val::S(0.0), ctx).num().clamp(0.0, 1.0);
            let pickup = eval_arg(args, "pickup", Val::S(0.0), ctx).num().clamp(0.0, 0.45);
            let mute = eval_arg(args, "mute", Val::S(0.0), ctx).num().clamp(0.0, 1.0);
            let g = eval_arg(args, "gain", Val::S(1.0), ctx).num();
            let exciter: &str = match arg(args, "exciter") {
                Some(Expr::Ident(s)) => s.as_str(),
                _ => "finger",
            };
            let collide = matches!(exciter, "slap" | "snap");
            let seed = ctx.seed ^ (id as u64).wrapping_mul(0x9E3779B97F4A7C15) | 1;
            let st = ctx.state.get_or(id, || {
                let cap = (ctx.sr / 24.0) as usize + 8;
                let n = ((ctx.sr / freq) as usize).clamp(4, cap - 2);
                let mut rng = seed;
                let mut exc = vec![0.0; n];
                let peak = (((n as f64) * position) as usize).clamp(1, n - 2);
                if exciter == "slap" {
                    // slap (Rank-Kubin): impulso de VELOCIDADE, nao de
                    // deslocamento - doublet curto no ponto do polegar
                    let wid = ((0.0015 * ctx.sr) as usize + 2).min(n);
                    for i in 0..wid {
                        let ph = i as f64 / wid as f64;
                        exc[(peak + i) % n] =
                            (2.0 * std::f64::consts::PI * ph).sin() * (1.0 - ph);
                    }
                } else {
                    // deslocamento triangular: o comb de posicao nasce da
                    // geometria do triangulo, nao de um filtro
                    for (i, v) in exc.iter_mut().enumerate() {
                        *v = if i <= peak {
                            i as f64 / peak as f64
                        } else {
                            (n - i) as f64 / (n - peak) as f64
                        };
                    }
                }
                // contato dedo/palheta: lowpass no formato (largura do
                // contato mata os harmonicos altos; hard = mais brilho)
                let cut = match exciter {
                    "pick" => 3000.0 + 9000.0 * hard,
                    "thumb" => 500.0 + 1200.0 * hard,
                    "snap" => 2500.0 + 7000.0 * hard,
                    "slap" => 2000.0 + 6000.0 * hard,
                    _ => 900.0 + 2600.0 * hard,
                };
                let kx = onepole_k(cut, ctx.sr);
                let mut s1 = 0.0;
                for v in exc.iter_mut() {
                    s1 += kx * (*v - s1);
                    *v = s1;
                }
                // textura de ruido leve (atrito da liberacao)
                let namt = 0.04 + 0.10 * hard;
                let mut s2 = 0.0;
                for v in exc.iter_mut() {
                    s2 += kx * (xorshift(&mut rng) - s2);
                    *v += s2 * namt;
                }
                // DC fora (loop com g ~1 carregaria offset) + normaliza;
                // snap sobra amplitude p/ colidir com o traste
                let amp = if exciter == "snap" { 1.5 } else { 1.0 };
                let mean = exc.iter().sum::<f64>() / n as f64;
                let pk = exc.iter().map(|v| (v - mean).abs()).fold(1e-9, f64::max);
                for v in exc.iter_mut() {
                    *v = (*v - mean) / pk * amp;
                }
                let mut vb = vec![0.0; cap];
                let mut hb = vec![0.0; cap];
                vb[..n].copy_from_slice(&exc);
                hb[..n].copy_from_slice(&exc);
                NodeStateBox(NodeState::Str(Box::new(StrS {
                    v: vb,
                    h: hb,
                    w: n, // leitura comeca dentro da regiao pre-carregada
                    lp: [0.0; 2],
                    z1: [0.0; 2],
                    ap: [[0.0; 2]; 2],
                    cpl: 0.0,
                    en: 0.0,
                    cfreq: 0.0,
                    comp: 0.0,
                })))
            });
            if let NodeStateBox(NodeState::Str(s)) = st {
                let om = 2.0 * std::f64::consts::PI * freq / ctx.sr;
                let cutoff = 800.0 * 15f64.powf(bright);
                let k = onepole_k(cutoff, ctx.sr);
                let cz = 0.22 * (1.0 - bright);
                let ad = -0.42 * stiff;
                if (freq - s.cfreq).abs() > freq * 1e-4 {
                    s.comp = pd_onepole(k, om)
                        + pd_onezero(cz, om)
                        + if ad != 0.0 { 2.0 * pd_allpass(ad, om) } else { 0.0 };
                    s.cfreq = freq;
                }
                // tension modulation: energia recente encurta o loop
                // (ataque forte nasce ~30ct agudo e assenta - o twang)
                let sharp = 1.0 - 0.0175 * tension * (s.en * 6.0).min(1.0);
                let dhz = 0.05 + 0.45 * pol; // detune entre polarizacoes, hz
                let StrS { v, h, w, lp, z1, ap, cpl, en, comp, .. } = &mut **s;
                let bufs: [&mut Vec<f64>; 2] = [v, h];
                let cap = bufs[0].len();
                let mut outs = [0.0; 2];
                for p in 0..2 {
                    let fp = if p == 0 { freq + 0.5 * dhz } else { freq - 0.5 * dhz };
                    let period = ctx.sr / fp;
                    let d = (period * sharp - *comp).clamp(2.0, (cap - 4) as f64);
                    let mut y = ring_read(bufs[p], *w, d);
                    // colisao com o traste/espelho: reflexao unilateral
                    // (slap e bartok snap - o "estalo" com buzz curto)
                    if collide && y < -0.55 {
                        y = -0.55 + (y + 0.55) * -0.6;
                    }
                    // saida no captador: comb 2*sin(pi*n*p) fisico do tap
                    let tap = if pickup > 0.0 {
                        y - ring_read(bufs[p], *w, (period * pickup).max(1.0))
                    } else {
                        y
                    };
                    lp[p] = flush_denorm(lp[p] + k * (y - lp[p]));
                    let oz = (1.0 - cz) * lp[p] + cz * z1[p];
                    z1[p] = lp[p];
                    let mut x = oz;
                    if ad != 0.0 {
                        for stg in 0..2 {
                            let yn = ad * x + ap[p][stg];
                            ap[p][stg] = x - ad * yn;
                            x = flush_denorm(yn);
                        }
                    }
                    // T60 por polarizacao; mute = dedo abafando a corda
                    let t60 = decay_s * if p == 0 { 0.5 } else { 1.0 };
                    let t60 = t60 / (1.0 + 30.0 * mute);
                    let gl = 10f64.powf(-3.0 * period / (t60.max(0.03) * ctx.sr));
                    let mut fb = x * gl;
                    if p == 0 {
                        // acoplamento h->v one-way; ganho pequeno porque o
                        // loop ressonante amplifica ~1/(1-g) (~60x): 0.002
                        // vira ~-18db de halo sem apagar o decay proprio
                        fb += 0.002 * *cpl;
                    }
                    bufs[p][*w] = flush_denorm(fb);
                    outs[p] = tap;
                }
                *cpl = flush_denorm(*cpl + onepole_k(800.0, ctx.sr) * (outs[1] - *cpl));
                *w = (*w + 1) % cap;
                let out = 0.58 * outs[0] + 0.42 * outs[1];
                *en = flush_denorm(*en + onepole_k(25.0, ctx.sr) * (out * out - *en));
                Val::S(out * 0.9 * g)
            } else {
                Val::S(0.0)
            }
        }
        Op::Modal => {
            // bank of 2-pole resonators; presets tuned per body type
            let freq = eval_arg(args, "freq", Val::Hz(220.0), ctx).as_hz().clamp(20.0, 8000.0);
            let bpm = ctx.bpm;
            let decay_s = eval_arg(args, "decay", Val::Ms(3000.0), ctx).as_sec(bpm).max(0.05);
            let strike = eval_arg(args, "strike", Val::S(0.5), ctx).num().clamp(0.0, 1.0);
            let preset: &str = match arg(args, "modes") {
                Some(Expr::Ident(s)) => s.as_str(),
                _ => "bell",
            };
            let ratios: &[f64] = match preset {
                "bar" => &[1.0, 2.756, 5.404, 8.933, 13.34, 18.64],
                "membrane" => &[1.0, 1.594, 2.136, 2.296, 2.653, 2.918, 3.156, 3.501],
                "pipe" => &[1.0, 3.0, 5.0, 7.0, 9.0, 11.0],
                _ => &[1.0, 2.0, 2.74, 3.0, 3.76, 4.07, 5.4, 6.8], // bell
            };
            let seed = ctx.seed ^ (id as u64).wrapping_mul(0x2545F4914F6CDD1D) | 1;
            let nm = ratios.len();
            let st = ctx.state.get_or(id, || {
                NodeStateBox(NodeState::Modal { s1: vec![0.0; nm], s2: vec![0.0; nm], exc: 1.0, rng: seed })
            });
            if let NodeStateBox(NodeState::Modal { s1, s2, exc, rng }) = st {
                // 2ms noise burst excitation
                let inp = if *exc > 0.0 {
                    let k = (-1.0 / (0.002 * ctx.sr)).exp();
                    *exc *= k;
                    xorshift(rng) * *exc
                } else {
                    0.0
                };
                let mut out = 0.0;
                for i in 0..nm {
                    let f = freq * ratios[i];
                    if f > ctx.sr * 0.45 {
                        continue;
                    }
                    // higher modes die faster (physical); RT60 per mode
                    let d = decay_s / (1.0 + 0.7 * i as f64);
                    let r = (-3.0 * std::f64::consts::LN_10 / (d * ctx.sr)).exp();
                    let th = 2.0 * std::f64::consts::PI * f / ctx.sr;
                    let a1 = 2.0 * r * th.cos();
                    let a2 = -r * r;
                    let amp = (std::f64::consts::PI * (i as f64 + 1.0) * (0.1 + 0.85 * strike)).sin().abs()
                        / (1.0 + 0.5 * i as f64);
                    // compensate resonator gain (~1/(1-r) at resonance) so output is ~unit level
                    let amp = amp * (1.0 - r) * 40.0;
                    let y = a1 * s1[i] + a2 * s2[i] + inp * amp;
                    s2[i] = s1[i];
                    s1[i] = flush_denorm(y);
                    out += y;
                }
                Val::S(out * 0.5)
            } else {
                Val::S(0.0)
            }
        }
        Op::Modal2 => {
            // modal synthesis v2: user mode table + doublets (beating) + hammer exciter.
            // modes: [(ratio, decay, amp), ...]; decay in s/ms, per mode.
            // doublet: fractional detune between each mode pair (0.15% = 0.0015).
            // strike: position 0..1 weights mode i by sin(pi*(i+1)*strike).
            // hard: hammer hardness 0..1 (contact lowpass cutoff; soft = dark/long contact).
            let freq = eval_arg(args, "freq", Val::Hz(220.0), ctx).as_hz().clamp(10.0, 12000.0);
            let doublet = eval_arg(args, "doublet", Val::S(0.0015), ctx).num().clamp(0.0, 0.02);
            let strike = eval_arg(args, "strike", Val::S(0.3), ctx).num().clamp(0.0, 1.0);
            let hard = eval_arg(args, "hard", Val::S(0.6), ctx).num().clamp(0.0, 1.0);
            let dmul = eval_arg(args, "decay", Val::S(1.0), ctx).num().max(0.01);
            let noise_amt = eval_arg(args, "noise", Val::S(0.1), ctx).num().clamp(0.0, 1.0);
            // default table: real bell partials (hum, prime, tierce, quint, nominal, deciem, undeciem)
            let default_modes: Vec<Vec<(f64, Unit)>> = vec![
                vec![(0.5, Unit::Scalar), (9.0, Unit::Scalar), (0.9, Unit::Scalar)],
                vec![(1.0, Unit::Scalar), (7.0, Unit::Scalar), (1.0, Unit::Scalar)],
                vec![(1.183, Unit::Scalar), (5.0, Unit::Scalar), (0.85, Unit::Scalar)],
                vec![(1.506, Unit::Scalar), (4.0, Unit::Scalar), (0.7, Unit::Scalar)],
                vec![(2.0, Unit::Scalar), (3.0, Unit::Scalar), (0.8, Unit::Scalar)],
                vec![(2.662, Unit::Scalar), (1.8, Unit::Scalar), (0.5, Unit::Scalar)],
                vec![(3.011, Unit::Scalar), (1.2, Unit::Scalar), (0.4, Unit::Scalar)],
            ];
            let rows = table_arg(args, "modes").cloned().unwrap_or(default_modes);
            if rows.is_empty() {
                return Val::S(0.0);
            }
            let nm = rows.len();
            let seed = ctx.seed ^ (id as u64).wrapping_mul(0x2545F4914F6CDD1D) | 1;
            let st = ctx.state.get_or(id, || {
                let mut rng = seed;
                // per-mode doublet spread jitter: pairs beat at different rates (the "mmm")
                let split: Vec<f64> = (0..nm)
                    .map(|_| 0.5 * (0.6 + 0.4 * (xorshift(&mut rng) * 0.5 + 0.5)))
                    .collect();
                NodeStateBox(NodeState::Modal2 {
                    s1: vec![0.0; nm * 2],
                    s2: vec![0.0; nm * 2],
                    split,
                    imp: 1.0,
                    hammer: 0.0,
                    rng,
                })
            });
            if let NodeStateBox(NodeState::Modal2 { s1, s2, split, imp, hammer, rng }) = st {
                // hammer: unit impulse through a one-pole lowpass = exponential contact
                // force pulse; hardness sets the cutoff (hard mallet = short bright contact)
                let fc = 250.0 * 2f64.powf(hard * 5.0); // 250hz .. 8khz
                let k = onepole_k(fc, ctx.sr);
                *hammer += k * (*imp / k.max(1e-9) - *hammer); // area-normalized impulse
                *imp = 0.0;
                *hammer = flush_denorm(*hammer);
                let inp = if hammer.abs() > 1e-7 {
                    *hammer * (1.0 + noise_amt * xorshift(rng))
                } else {
                    0.0
                };
                let mut out = 0.0;
                // pitch scaling: striking higher shortens ring (physical)
                let pitch_scale = (261.63 / freq).powf(0.35).clamp(0.1, 3.0);
                for m in 0..nm {
                    let ratio = rows[m][0].0;
                    let d_tab = if rows[m].len() > 1 { col_sec(rows[m][1]) } else { 3.0 };
                    let a_tab = if rows[m].len() > 2 { rows[m][2].0 } else { 1.0 };
                    let f_mode = freq * ratio;
                    if f_mode > ctx.sr * 0.45 || f_mode < 5.0 {
                        continue;
                    }
                    let d = (d_tab * dmul * pitch_scale).max(0.01);
                    // strike position: node of mode i+1 at multiples of 1/(i+1)
                    let pos_w = (std::f64::consts::PI * (m as f64 + 1.0) * (0.02 + 0.96 * strike))
                        .sin()
                        .abs();
                    let amp = a_tab * (0.15 + 0.85 * pos_w);
                    for pair in 0..2 {
                        let sgn = if pair == 0 { 1.0 } else { -1.0 };
                        let f = f_mode * (1.0 + sgn * doublet * split[m]);
                        let r = (-3.0 * std::f64::consts::LN_10 / (d * ctx.sr)).exp();
                        let th = 2.0 * std::f64::consts::PI * f / ctx.sr;
                        let a1 = 2.0 * r * th.cos();
                        let a2 = -r * r;
                        // (1-r) input compensation: resonator gain at fc is ~1/(1-r)
                        let g_in = amp * (1.0 - r) * 20.0;
                        let i = m * 2 + pair;
                        let y = a1 * s1[i] + a2 * s2[i] + inp * g_in;
                        s2[i] = s1[i];
                        s1[i] = flush_denorm(y);
                        out += y;
                    }
                }
                Val::S(out * 0.5)
            } else {
                Val::S(0.0)
            }
        }
        Op::Nwave => {
            // coherent N-wave (shock front): instant rise, linear pressure ramp to -1,
            // recovery back to 0. Adds ground reflection (short delay) + air absorption
            // (lowpass). The "crack" that filtered noise can't produce.
            let bpm = ctx.bpm;
            let dur = eval_arg(args, "dur", Val::Ms(2.5), ctx).as_sec(bpm).clamp(0.0002, 0.05);
            let sharp = eval_arg(args, "sharp", Val::S(0.85), ctx).num().clamp(0.0, 1.0);
            let refl_s = eval_arg(args, "reflect", Val::Ms(4.0), ctx).as_sec(bpm).clamp(0.0, 0.02);
            let refl_g = eval_arg(args, "reflect_gain", Val::S(0.4), ctx).num().clamp(0.0, 1.0);
            let air = eval_arg(args, "air", Val::Hz(9000.0), ctx).as_hz().clamp(500.0, 20000.0);
            let g = eval_arg(args, "gain", Val::S(1.0), ctx).num();
            let cap = (ctx.sr * 0.021) as usize + 4;
            let st = ctx.state.get_or(id, || {
                NodeStateBox(NodeState::Nwave { t: 0.0, buf: vec![0.0; cap], w: 0, lp: 0.0 })
            });
            if let NodeStateBox(NodeState::Nwave { t, buf, w, lp }) = st {
                // rise time: sharp 1 = instantaneous front (1 sample), sharp 0 = 20% of dur
                let tr = (dur * 0.2 * (1.0 - sharp)).max(1.0 / ctx.sr);
                let s = if *t < tr {
                    *t / tr // shock front 0 -> +1
                } else if *t < tr + dur {
                    1.0 - 2.0 * (*t - tr) / dur // linear ramp +1 -> -1
                } else if *t < tr + dur + tr * 2.0 {
                    -1.0 + (*t - tr - dur) / (tr * 2.0) // recovery -1 -> 0
                } else {
                    0.0
                };
                *t += dt;
                // ground reflection: same-sign copy a few ms later, quieter
                let n = buf.len();
                buf[*w] = s;
                let d = ((refl_s * ctx.sr) as usize).min(n - 1);
                let refl = if d > 0 { buf[(*w + n - d) % n] * refl_g } else { 0.0 };
                *w = (*w + 1) % n;
                // air absorption
                let k = onepole_k(air, ctx.sr);
                *lp = flush_denorm(*lp + k * ((s + refl) - *lp));
                Val::S(*lp * g)
            } else {
                Val::S(0.0)
            }
        }
        Op::Bow => {
            // bowed string: nut-side + bridge-side delay lines, exact
            // stick-slip junction (McIntyre-Schumacher-Woodhouse): the
            // Stribeck friction curve is solved against the wave feedback
            // every sample, with the falling-friction hysteresis rule.
            // Feed the output through convolve(ir: <body>) for an instrument.
            let freq = eval_arg(args, "freq", Val::Hz(220.0), ctx).as_hz().clamp(20.0, 4000.0);
            let pressure = eval_arg(args, "pressure", Val::S(0.5), ctx).num().clamp(0.0, 1.0);
            let velocity = eval_arg(args, "velocity", Val::S(0.5), ctx).num().clamp(0.0, 1.0);
            let position = eval_arg(args, "position", Val::S(0.13), ctx).num().clamp(0.02, 0.5);
            let damp = eval_arg(args, "damp", Val::S(0.3), ctx).num().clamp(0.0, 1.0);
            let noise_amt = eval_arg(args, "noise", Val::S(0.15), ctx).num().clamp(0.0, 1.0);
            // stroke: duracao de uma arcada completa (talao->ponta) na
            // velocidade nominal; chegando ao fim da crina o arco VOLTA
            let bpm = ctx.bpm;
            let stroke_s =
                eval_arg(args, "stroke", Val::Ms(2200.0), ctx).as_sec(bpm).clamp(0.4, 30.0);
            let g = eval_arg(args, "gain", Val::S(1.0), ctx).num();
            let cap = (ctx.sr / 20.0) as usize + 8;
            let nseed = ctx.seed ^ (id as u64).wrapping_mul(0x9E3779B97F4A7C15) | 1;
            let st = ctx.state.get_or(id, || {
                // arcada inicial sorteada por voz: metade das notas comeca
                // pra baixo perto do talao, metade pra cima perto da ponta
                // (detache real alterna; estantes de secao dessincronizam)
                let mut r = nseed;
                let down = xorshift(&mut r) >= 0.0;
                let frac = xorshift(&mut r) * 0.5 + 0.5; // 0..1
                let (h0, d0) = if down {
                    (0.08 + 0.30 * frac, 1.0)
                } else {
                    (0.92 - 0.30 * frac, -1.0)
                };
                // walks de imperfeicao ja nascem fora do zero (a mao nunca
                // esta perfeitamente no lugar)
                let ip0 = xorshift(&mut r) * 0.6;
                let ic0 = xorshift(&mut r) * 0.6;
                let ph0 = xorshift(&mut r) * 0.5 + 0.5;
                NodeStateBox(NodeState::Bow {
                    nut: vec![0.0; cap],
                    bridge: vec![0.0; cap],
                    w: 0,
                    lp: 0.0,
                    z: 0.0,
                    nlp: 0.0,
                    nlp2: 0.0,
                    flp: 0.0,
                    oz: 0.0,
                    ap: 0.0,
                    tq: 0.0,
                    cfreq: 0.0,
                    comp: 0.0,
                    pos: 0.0,
                    h: h0,
                    dir: d0,
                    dsm: d0,
                    iw_ph: ph0,
                    ip_s: ip0,
                    ip_t: ip0,
                    ic_s: ic0,
                    ic_t: ic0,
                    rng: r,
                })
            });
            if let NodeStateBox(NodeState::Bow {
                nut, bridge, w, lp, z, nlp, nlp2, flp, oz, ap, tq, cfreq, comp, pos, h, dir,
                dsm, iw_ph, ip_s, ip_t, ic_s, ic_t, rng,
            }) = st
            {
                // imperfeicao humana: nada num instrumento real e uma onda
                // perfeita. Dois random walks LENTOS (sorteados por voz,
                // suavizados ~0.8hz): a mao esquerda deriva a afinacao em
                // +-1.5ct e o ponto de contato do arco anda +-6% - cada
                // ciclo nasce de condicoes levemente diferentes e o timbre
                // respira, sem tremer nem soar "efeito".
                *iw_ph += 1.4 / ctx.sr;
                if *iw_ph >= 1.0 {
                    *iw_ph -= 1.0;
                    *ip_t = xorshift(rng);
                    *ic_t = xorshift(rng);
                }
                let kw = onepole_k(0.8, ctx.sr);
                *ip_s = flush_denorm(*ip_s + kw * (*ip_t - *ip_s));
                *ic_s = flush_denorm(*ic_s + kw * (*ic_t - *ic_s));
                let freq_e = freq * (1.0 + *ip_s * 0.0009); // ~ +-1.5ct
                let position_e = (position * (1.0 + *ic_s * 0.06)).clamp(0.02, 0.5);
                let period = ctx.sr / freq_e;
                // reflexao do cavalete: g0 (perda escalar) + one-pole (perda
                // HF) + one-zero + allpass de RIGIDEZ (corda real nao e
                // ideal: parciais levemente esticados, o canto de Helmholtz
                // arredonda assimetrico). A cadeia aproxima o Q(f) medido
                // (Cuesta-Valette): one-pole sozinho matava 1-4khz rapido
                // demais (parte do zumbido).
                let cutoff = 1800.0 + (1.0 - damp) * 8000.0;
                let k = onepole_k(cutoff, ctx.sr);
                let czb = 0.16; // one-zero da reflexao
                let a_st = -0.055; // rigidez (allpass no lado do cavalete)
                // pestana/dedo: terminacao de CARNE, nao parede rigida -
                // reflexao levemente com perda e escura
                let k_nut = onepole_k(6500.0, ctx.sr);
                let g0 = 0.975 - 0.025 * damp;
                // compensacao de fase do loop calculada EXATA (nao fitted)
                if (freq_e - *cfreq).abs() > freq_e * 1e-4 {
                    let om = 2.0 * std::f64::consts::PI * freq_e / ctx.sr;
                    *comp = pd_onepole(k, om)
                        + pd_onezero(czb, om)
                        + pd_allpass(a_st, om)
                        + pd_onepole(k_nut, om);
                    *cfreq = freq_e;
                }
                let d_total = (period - *comp).max(4.0);
                let d_bridge = (d_total * position_e).max(2.0);
                let d_nut = (d_total - d_bridge).max(2.0);
                let bridge_out = ring_read(bridge, *w, d_bridge);
                let nut_out = ring_read(nut, *w, d_nut);
                *lp = flush_denorm(*lp + k * (bridge_out - *lp));
                let ozv = (1.0 - czb) * *lp + czb * *oz;
                *oz = *lp;
                // allpass de rigidez
                let apy = a_st * ozv + *ap;
                *ap = flush_denorm(ozv - a_st * apy);
                let bridge_refl = -g0 * apy;
                *nlp2 = flush_denorm(*nlp2 + k_nut * (nut_out - *nlp2));
                let nut_refl = -0.997 * *nlp2;
                // arco FINITO: h avanca pela crina na velocidade do arco;
                // no talao (0) ou na ponta (1) o sentido alvo inverte e o
                // sentido efetivo (dsm) cruza zero suavemente - a mao
                // desacelera, para e volta (~35ms). vb assinado: toda a
                // fisica abaixo (friccao, Schelleng, textura) reage a
                // inversao sozinha - dip de amplitude, re-articulacao e a
                // textura relida de volta.
                let vmag = 0.05 + 0.25 * velocity;
                let kd = onepole_k(9.0, ctx.sr);
                *dsm = flush_denorm(*dsm + kd * (*dir - *dsm));
                let vb = vmag * *dsm;
                *h += vb / (0.30 * stroke_s) / ctx.sr;
                if *h >= 1.0 {
                    *dir = -1.0;
                } else if *h <= 0.0 {
                    *dir = 1.0;
                }
                *h = h.clamp(-0.06, 1.06);
                let dv0 = vb - (bridge_refl + nut_refl);
                // friction: Stribeck curve resolved EXACTLY per sample in the
                // McIntyre-Schumacher-Woodhouse manner, WITH the wave feedback
                // in the solve, MAIS dois refinamentos fisicos:
                //  - canal torsional (kappa): a forca do arco tambem torce a
                //    corda; ondas torsionais sao surdas e MUITO amortecidas,
                //    entao agem como perda resistiva no ponto de contato:
                //    dv = dv0 - (1+kappa)*F. Isso suprime a instabilidade de
                //    Friedlander - o jitter/zumbido agudo do modelo puro.
                //  - friccao termica (Woodhouse 2003): o breu opera perto da
                //    transicao vitrea; slip aquece o contato (tq) e derruba a
                //    friccao, stick esfria e recupera. Da o loop de histerese
                //    medido no plano f-v (a curva estatica nao passa perto).
                //  stick: F = dv0/(1+kappa) segura v = vb exato - sem isso a
                //         corda trava em period +- d_bridge (o bug da "secao
                //         azeda caotica").
                //  slip:  bissecao no ramo cinetico; a ambiguidade da curva
                //         caindo resolve para o estado anterior (*z), regra
                //         de histerese MSW classica.
                let kappa = 0.28; // admitancia torsional / transversal
                let hot = 1.0 / (1.0 + 8.0 * *tq);
                // Schelleng: a forca maxima MUSICAL cresce com a velocidade
                // do arco (f_max ~ 2*Z0*vb/(beta*dmu)). Arco parado nao
                // aguenta pressao nenhuma sem raspar; o cap acopla a forca
                // efetiva a vb - e a rampa de Guettler embutida: qualquer
                // envelope de pressure comeca suave enquanto o arco parte.
                // (O chiado irritante de ataque era pressao de overshoot
                // com vb ~ 0 = regiao raucous do diagrama de Schelleng.)
                let vbn = (vb.abs() / 0.30).min(1.0);
                // alavanca talao/ponta: no talao a mao pesa direto na
                // corda (mais forca disponivel), na ponta o braco de
                // alavanca come a forca - cada arcada respira sozinha
                let lever = 1.05 - 0.25 * h.clamp(0.0, 1.0);
                // TEXTURA COMO FISICA, nao como ruido: o grao da crina com
                // breu modula o COEFICIENTE DE FRICCAO local (escala grossa
                // ~ crina: ~1.1khz na velocidade cheia, sobe e desce com o
                // arco) e a corda responde - o grit vive DENTRO do tom.
                // Somar ruido ao sinal era o "chiado merda".
                *pos += vb * 16000.0 / ctx.sr;
                let grain = tex_surface(nseed ^ 0xA5A5, *pos * 0.07);
                let gdepth = 0.10 + 0.30 * noise_amt;
                let fn_ = (0.10 + 0.55 * pressure)
                    * (0.22 + 0.78 * vbn)
                    * lever
                    * (1.0 + grain * gdepth);
                let fs = fn_ * (0.60 + 0.40 * hot); // static level (esfria = sobe)
                let fc = 0.35 * fn_ * (0.75 + 0.25 * hot); // coulomb level
                let vs = 0.06; // Stribeck velocity
                let (dv, force);
                if dv0.abs() <= fs * (1.0 + kappa) {
                    force = dv0 / (1.0 + kappa);
                    dv = 0.0;
                } else {
                    let sgn = dv0.signum();
                    let mut a = dv0 - (1.0 + kappa) * fs * sgn;
                    let mut b = dv0 - (1.0 + kappa) * fc * sgn;
                    if a > b {
                        std::mem::swap(&mut a, &mut b);
                    }
                    let h = |x: f64| -> f64 {
                        let g = fc + (fs - fc) * (-(x / vs) * (x / vs)).exp();
                        dv0 - (1.0 + kappa) * g * sgn - x
                    };
                    for _ in 0..8 {
                        let mid = 0.5 * (a + b);
                        if h(a) * h(mid) <= 0.0 {
                            b = mid;
                        } else {
                            a = mid;
                        }
                    }
                    let mut x = 0.5 * (a + b);
                    // hysteresis: bias abrupt branch flips toward the last dv
                    if (x - *z) * (dv0 - *z) < 0.0 {
                        x = 0.5 * (x + *z);
                    }
                    dv = x;
                    force = (fc + (fs - fc) * (-(dv / vs) * (dv / vs)).exp()) * sgn;
                }
                *z = dv; // last relative velocity (hysteresis state)
                // calor de contato: slip gera q = |F*dv|, conducao esfria
                // (~4ms). tq do sample anterior alimenta fs/fc acima.
                let kq = onepole_k(40.0, ctx.sr);
                *tq = flush_denorm(*tq + kq * ((force * dv).abs() * 6.0 - *tq));
                // Cremer corner rounding: bow-hair compliance lowpasses the
                // friction force; without it the hard stick-slip corner
                // regenerates a 5-7khz plateau every period (sectional "hiss").
                // O canto acompanha a forca efetiva: toque leve = canto
                // redondo/escuro, digging = brilha (fisico: a largura do
                // corner de Helmholtz encolhe com a forca do arco).
                let kf = onepole_k(1500.0 + 3200.0 * (fn_ / 0.65).min(1.0), ctx.sr);
                *flp = flush_denorm(*flp + kf * (force - *flp));
                let mut excite = *flp;
                // sopro residual de ar do contato: a textura fina do breu
                // lida na velocidade do arco, BEM baixa (o grosso do grit
                // agora vive na modulacao de friccao la em cima; isto e
                // so o ar do contato)
                if noise_amt > 0.0 {
                    let slip = (dv.abs() / vs).min(2.0);
                    let tex = tex_surface(nseed | 1, *pos);
                    let kn = onepole_k(4200.0, ctx.sr);
                    *nlp = flush_denorm(*nlp + kn * (tex - *nlp));
                    excite += *nlp * slip * (0.4 + 1.2 * fn_) * noise_amt * 0.16;
                }
                nut[*w] = bridge_refl + excite;
                bridge[*w] = nut_refl + excite;
                *w = (*w + 1) % nut.len();
                // bridge force wave, scaled to useful level
                Val::S((nut_refl + excite) * 3.0 * g)
            } else {
                Val::S(0.0)
            }
        }
        Op::Breath => {
            // breath(pressure, turbulence): pressao DC + turbulencia fisica
            // (ver TurbState: espectro de Strouhal, escala U^2, gate de
            // Reynolds, wander nao-estacionario), fonte de excitacao para
            // designs de sopro custom. pressure 0 = silencio de verdade.
            let pressure = eval_arg(args, "pressure", Val::S(0.8), ctx).num().clamp(0.0, 2.0);
            let tamt = eval_arg(args, "turbulence", Val::S(0.1), ctx).num().clamp(0.0, 1.0);
            let nseed = ctx.seed ^ (id as u64).wrapping_mul(0x9E3779B97F4A7C15) | 1;
            let st = ctx.state.get_or(id, || {
                NodeStateBox(NodeState::Breath { rng: nseed, turb: TurbState::default() })
            });
            if let NodeStateBox(NodeState::Breath { rng, turb }) = st {
                let uj = pressure.max(0.0).sqrt();
                let n = turb.tick(rng, uj, ctx.sr);
                Val::S(pressure + tamt * 0.9 * n)
            } else {
                Val::S(0.0)
            }
        }
        Op::Flute => {
            // flute jet-drive waveguide (Cook/STK + Karjalainen/Valimaki,
            // fisica em Verge 1995 / de la Cuadra 2005): bore de 1.5 periodo
            // tocado em OVERBLOW no 2o modo (o timbre real de flauta), jato com
            // delay proprio (jet: ratio do loop, ~0.32) + sigmoide cubica
            // SUAVIZADA (tanh da cubica: sem o corner do clamp, que soprava
            // ruido de banda larga proprio), e turbulencia fisica injetada NO
            // DESLOCAMENTO DO JATO antes da nao-linearidade: o tubo penteia o
            // ruido em skirts harmonicas e a sigmoide o pulsa a 2xf0 - o sopro
            // vive dentro da nota em vez de chiar por cima.
            // pressure ~0.5..1.3 soa (remap interno); brilho sobe com pressure
            // via fc da reflexao. jet: balanco harmonico (0.2 brilhante/oco ..
            // 0.45 escuro/cheio).
            let freq = eval_arg(args, "freq", Val::Hz(440.0), ctx).as_hz().clamp(80.0, 4000.0);
            let pressure = eval_arg(args, "pressure", Val::S(0.9), ctx).num().clamp(0.0, 1.6);
            let breath_noise =
                eval_arg(args, "breath", Val::S(0.05), ctx).num().clamp(0.0, 1.0);
            let jet_ratio = eval_arg(args, "jet", Val::S(0.32), ctx).num().clamp(0.08, 0.56);
            let g = eval_arg(args, "gain", Val::S(1.0), ctx).num();
            let cap = (ctx.sr / 24.0) as usize + 8;
            let nseed = ctx.seed ^ (id as u64).wrapping_mul(0x2545F4914F6CDD1D) | 1;
            let st = ctx.state.get_or(id, || {
                NodeStateBox(NodeState::Flute(Box::new(FluteS {
                    bore: vec![0.0; cap],
                    jet: vec![0.0; cap],
                    w: 0,
                    lp: 0.0,
                    dc: (0.0, 0.0),
                    rng: nseed,
                    turb: TurbState::default(),
                    prev_out: 0.0,
                })))
            });
            if let NodeStateBox(NodeState::Flute(s)) = st {
                let period = ctx.sr / freq;
                // remap interno de pressao: a janela de oscilacao do jato
                // cubico e estreita (~0.85..1.15 de pressao de boca); o user
                // controla dinamica em 0.3..1.3 e o remap poe tudo dentro da
                // janela - a DINAMICA sai do brilho da reflexao + turbulencia,
                // como numa flauta real (nao do jato morrer)
                // smoothstep: sobe/desce continuo em 0..0.12 (sem degrau de
                // pressao no ataque/release = sem clique nem corte seco)
                let p_eff = (0.72 + 0.36 * pressure) * smoothstep(0.0, 0.12, pressure);
                // brilho vs dinamica: reflexao mais aberta soprando forte
                let fc_refl = (1500.0 + 2200.0 * (pressure - 0.85)).clamp(900.0, 3400.0);
                // fase do filtro de reflexao compensada analiticamente para a
                // afinacao nao depender de pressure (fc_refl varia com ela)
                // 0.45: a shelving (72% lp + 28% direto) tem ~metade da fase
                // de um lowpass puro (medido no sweep de pressao em g4)
                let tau = 2.0 * std::f64::consts::PI;
                let refl_delay = 0.45 * (freq / fc_refl).atan() / (tau * freq) * ctx.sr;
                // bore de 1.5T (afinado uma quinta abaixo) + reflexao invertida:
                // o jato trava no 2o modo = f. E o overblow do STK: da a
                // estrutura harmonica de flauta real (tubo aberto), nao a de
                // tubo fechado do loop de meio periodo.
                // 1.5187/+1.19: afinacao recalibrada por sweep g3..g5 depois
                // da reflexao shelving (fit linear em period).
                // O delay do jato tambem entra na fase do loop (~52%, medido
                // com jet 0.42): compensa para a afinacao nao depender de jet.
                let d_bore = (((1.5162 * period - 0.19) - refl_delay)
                    * (1.0 - 0.52 * (jet_ratio - 0.32)))
                    .max(4.0);
                let d_jet = (d_bore * jet_ratio).max(2.0);
                let bore_out = ring_read(&s.bore, s.w, d_bore);
                // reflexao shelving invertida: graves refletem ~total, agudos
                // refletem 28% (o resto radia) - o papel do lowpass cheio era
                // matar TUDO em cima, deixando o tom opaco e o chiado exposto
                let k = onepole_k(fc_refl, ctx.sr);
                s.lp = flush_denorm(s.lp + k * (bore_out - s.lp));
                let refl = -0.98 * (s.lp + 0.28 * (bore_out - s.lp));
                // turbulencia fisica (ver TurbState): escala U^2, gate de
                // Reynolds, espectro de Strouhal, wander nao-estacionario
                let uj = pressure.max(0.0).sqrt();
                let mut rng = s.rng;
                let turb = s.turb.tick(&mut rng, uj, ctx.sr);
                s.rng = rng;
                let breath_p = p_eff * 0.9;
                // jato: diferenca de pressao viaja o jet delay; o ruido entra
                // como perturbacao do DESLOCAMENTO do jato (in-loop, pre-NL)
                let jet_in = breath_p - 0.5 * refl;
                s.jet[s.w] = jet_in;
                let x = ring_read(&s.jet, s.w, d_jet) + breath_noise * 0.9 * turb;
                // tanh(cubica): identica a x^3-x na regiao util, saturacao
                // suave fora (sem derivada descontinua = sem spray espectral)
                let jet_out = (x * (x * x - 1.0)).tanh();
                let dc_out = jet_out - s.dc.0 + 0.995 * s.dc.1;
                s.dc.0 = jet_out;
                s.dc.1 = dc_out;
                s.bore[s.w] = flush_denorm(dc_out) + 0.5 * refl;
                s.w = (s.w + 1) % s.bore.len();
                // radiacao: o que sai da embocadura e a DERIVADA da pressao
                // (dipolo, +6db/oct ate ~1khz) - o sopro herda a mesma cor
                let out = (bore_out - 0.86 * s.prev_out) * 3.0;
                s.prev_out = bore_out;
                Val::S(out * g)
            } else {
                Val::S(0.0)
            }
        }
        Op::Reed => {
            // clarinete: lei de fluxo de Bernoulli adimensional (Kergomard/
            // Guillemain 2005) + tubo fechado-aberto (meio periodo -> impares).
            // u = zeta*h*sign(gamma-p)*sqrt(|gamma-p|), h = abertura da palheta
            // com joelho SUAVE no beating (fortissimo comprime e brilha em vez
            // de morrer, que era o defeito da tabela linear antiga).
            // Reflexao = -0.95 * one-zero * cutoff de toneholes (~1.5khz):
            // abaixo do cutoff reflete (tom escuro chalumeau), acima radia.
            // Turbulencia entra NO FLUXO u (variancia ~u^2): pulsa em f0
            // porque o proprio fluxo da palheta pulsa - sopro dentro da nota.
            let freq = eval_arg(args, "freq", Val::Hz(220.0), ctx).as_hz().clamp(60.0, 2500.0);
            let pressure = eval_arg(args, "pressure", Val::S(0.8), ctx).num().clamp(0.0, 1.5);
            let stiffness = eval_arg(args, "stiffness", Val::S(0.5), ctx).num().clamp(0.0, 1.0);
            let breath_noise =
                eval_arg(args, "breath", Val::S(0.03), ctx).num().clamp(0.0, 1.0);
            let g = eval_arg(args, "gain", Val::S(1.0), ctx).num();
            let cap = (ctx.sr / 40.0) as usize + 8;
            let nseed = ctx.seed ^ (id as u64).wrapping_mul(0x9E3779B97F4A7C15) | 1;
            let st = ctx.state.get_or(id, || {
                NodeStateBox(NodeState::Reed(Box::new(ReedS {
                    bore: vec![0.0; cap],
                    w: 0,
                    lp: 0.0,
                    oz: 0.0,
                    rng: nseed,
                    turb: TurbState::default(),
                    prev_out: 0.0,
                })))
            });
            if let NodeStateBox(NodeState::Reed(s)) = st {
                let period = ctx.sr / freq;
                let fc_th = 1500.0;
                // fase da reflexao: one-zero (0.5 sample) + polo do tonehole
                let tau = 2.0 * std::f64::consts::PI;
                let refl_delay = 0.5 + (freq / fc_th).atan() / (tau * freq) * ctx.sr;
                // tubo fechado: meio periodo por volta; constantes calibradas
                // por sweep d3..a4 (a tabela antiga tocava +6..+20ct sharp)
                let d_bore = (period * 0.50080 + 0.45 - refl_delay).max(2.0);
                let bore_out = ring_read(&s.bore, s.w, d_bore);
                // reflexao no extremo aberto: one-zero (media de 2 samples,
                // como STK) + cutoff de toneholes; inversao de pressao
                let oz = 0.5 * (bore_out + s.oz);
                s.oz = bore_out;
                let k = onepole_k(fc_th, ctx.sr);
                s.lp = flush_denorm(s.lp + k * (oz - s.lp));
                let pm = -0.95 * s.lp;
                // gamma: pressao de boca normalizada (limiar 1/3, beating >0.5)
                let gamma = (0.10 + 0.40 * pressure) * smoothstep(0.0, 0.10, pressure);
                // zeta: palheta dura = abertura menor = menos fluxo
                let zeta = 0.45 - 0.25 * stiffness;
                // Newton (4 iteracoes) em F(u) = u - u_flow(gamma - 2pm - u):
                // joelho suave do beating via h = softmax(0, 1 - x)
                let gg = gamma - 2.0 * pm;
                let uflow = |x: f64| -> f64 {
                    let z = 1.0 - x;
                    let h = 0.5 * (z + (z * z + 0.004).sqrt());
                    zeta * h * x.signum() * x.abs().sqrt()
                };
                let mut u = 0.0;
                for _ in 0..4 {
                    let f0v = u - uflow(gg - u);
                    let df = 1e-4;
                    let f1v = (u + df) - uflow(gg - u - df);
                    let d = (f1v - f0v) / df;
                    if d.abs() > 1e-9 {
                        u -= f0v / d;
                    }
                }
                // turbulencia no fluxo: escala |u|*u (variancia ~u^4 fisica),
                // pulsa em f0 porque u pulsa (fecha a cada ciclo da palheta)
                let uj = (gamma.max(0.0) / 0.45).sqrt();
                let mut rng = s.rng;
                let turb = s.turb.tick(&mut rng, uj, ctx.sr);
                s.rng = rng;
                let u_total = u + breath_noise * 0.7 * u.abs().min(0.5) * turb;
                // onda que desce o tubo: p+ = pm + u
                s.bore[s.w] = flush_denorm(pm + u_total);
                s.w = (s.w + 1) % s.bore.len();
                // radiacao: incidente menos refletida = passa-altas fisico
                // (graves voltam, agudos saem), depois derivada (dipolo)
                // 85: ondas adimensionais (u~0.1) -> paridade de nivel com o
                // flute a gain 1 (medido A/B a v0.7)
                let trans = bore_out - 0.95 * s.lp;
                let out = (trans - 0.82 * s.prev_out) * 85.0;
                s.prev_out = trans;
                Val::S(out * g)
            } else {
                Val::S(0.0)
            }
        }
        Op::Convolve => {
            // convolve(sig, ir: synth_name, dur: 150ms, mix: 100%, gain: 1)
            // The IR is RENDERED from another synth def in the file at first use
            // (one c4 hit, fixed seed) - resonant bodies and rooms from code,
            // never from audio files.
            let sig = input_sig(args, ctx);
            let ir_name: &str = match arg(args, "ir") {
                Some(Expr::Ident(s)) => s.as_str(),
                Some(Expr::Str(s)) => s.as_str(),
                _ => return sig,
            };
            let ir2_name: Option<&str> = match arg(args, "ir2") {
                Some(Expr::Ident(s)) | Some(Expr::Str(s)) => Some(s.as_str()),
                _ => None,
            };
            let bpm = ctx.bpm;
            let dur = eval_arg(args, "dur", Val::Ms(150.0), ctx).as_sec(bpm).clamp(0.005, 2.0);
            let mix = eval_arg(args, "mix", Val::S(1.0), ctx).num().clamp(0.0, 1.0);
            let g = eval_arg(args, "gain", Val::S(1.0), ctx).num();
            let sr = ctx.sr;
            let st = ctx.state.get_or(id, || {
                NodeStateBox(NodeState::Conv(Box::new(make_conv(ir_name, ir2_name, dur, sr))))
            });
            if let NodeStateBox(NodeState::Conv(cs)) = st {
                if cs.dead {
                    return sig; // E033 already reported; pass dry
                }
                let (il, irr) = sig.stereo();
                let ol = cs.tick(0, il);
                let orr = cs.tick(1, irr);
                cs.advance();
                let out = eq_power_mix((il, irr), (ol * g, orr * g), mix);
                Val::St2(out.0, out.1)
            } else {
                sig
            }
        }
        Op::Grain => {
            let path: &str = match arg(args, "source").or(arg(args, "_0")) {
                Some(Expr::Str(s)) => s,
                _ => return Val::S(0.0),
            };
            let smp = match get_sample(path) {
                Some(s) => s,
                None => return Val::S(0.0),
            };
            let bpm = ctx.bpm;
            let position = eval_arg(args, "position", Val::S(0.0), ctx).num().clamp(0.0, 1.0);
            let size_s = eval_arg(args, "size", Val::Ms(90.0), ctx).as_sec(bpm).clamp(0.01, 0.5);
            let density = eval_arg(args, "density", Val::Hz(25.0), ctx).as_hz().clamp(0.5, 500.0);
            let jitter = eval_arg(args, "jitter", Val::S(0.2), ctx).num().clamp(0.0, 1.0);
            let spread = eval_arg(args, "spread", Val::S(0.6), ctx).num().clamp(0.0, 1.0);
            let root = match arg(args, "root") {
                Some(Expr::Ident(n)) => crate::score::note_name_to_midi(n).unwrap_or(48.0),
                _ => 48.0,
            };
            let pitch = match arg(args, "pitch") {
                Some(e) => match eval(e, ctx) {
                    Val::Pitch(p) => p,
                    v => 69.0 + 12.0 * (v.as_hz() / 440.0).log2(),
                },
                None => root,
            };
            let rate = (2f64.powf((pitch - root) / 12.0) * (smp.sr / ctx.sr)).clamp(0.25, 4.0);
            let seed = ctx.seed ^ (id as u64).wrapping_mul(0x9E3779B97F4A7C15) | 1;
            let st = ctx.state.get_or(id, || {
                NodeStateBox(NodeState::Grain { grains: Vec::new(), rng: seed, next_spawn: 0.0 })
            });
            if let NodeStateBox(NodeState::Grain { grains, rng, next_spawn }) = st {
                let nf = (smp.data.len() / smp.ch) as f64;
                *next_spawn -= 1.0;
                if *next_spawn <= 0.0 {
                    let len = size_s * ctx.sr;
                    let pos_j = xorshift(rng) * jitter * len;
                    let g = GrainVoice {
                        pos: (position * nf + pos_j).clamp(0.0, (nf - 2.0).max(0.0)),
                        rate,
                        age: 0.0,
                        len,
                        pan: xorshift(rng) * spread,
                        amp: 1.0,
                    };
                    if grains.len() >= 64 {
                        // steal oldest (max age)
                        let oldest = grains
                            .iter()
                            .enumerate()
                            .max_by(|a, b| a.1.age.partial_cmp(&b.1.age).unwrap())
                            .map(|(i, _)| i)
                            .unwrap_or(0);
                        grains[oldest] = g;
                    } else {
                        grains.push(g);
                    }
                    // spawn-time jitter desynchronizes the grain clock
                    let base = ctx.sr / density;
                    *next_spawn = base * (1.0 + xorshift(rng) * jitter);
                }
                let mut l = 0.0;
                let mut r = 0.0;
                grains.retain_mut(|gr| {
                    let win = (std::f64::consts::PI * (gr.age / gr.len).clamp(0.0, 1.0)).sin();
                    let w2 = win * win; // hann
                    let (sl, sr_) = sample_frame(&smp, gr.pos);
                    let v = 0.5 * (sl + sr_) * w2 * gr.amp;
                    let a = (gr.pan.clamp(-1.0, 1.0) + 1.0) * std::f64::consts::FRAC_PI_4;
                    l += v * a.cos();
                    r += v * a.sin();
                    gr.pos += gr.rate;
                    gr.age += 1.0;
                    gr.age < gr.len && gr.pos < nf - 2.0
                });
                let norm = 1.0 / (1.0 + (density * size_s).sqrt());
                Val::St2(l * norm, r * norm)
            } else {
                Val::S(0.0)
            }
        }
        Op::Follower => {
            // peak follower: the legal audio -> control node (tier4 §2.1)
            let sig = input_sig(args, ctx);
            let bpm = ctx.bpm;
            let att = eval_arg(args, "attack", Val::Ms(5.0), ctx).as_sec(bpm).max(0.0001);
            let rel = eval_arg(args, "release", Val::Ms(80.0), ctx).as_sec(bpm).max(0.001);
            let st = ctx.state.get_or(id, || NodeStateBox(NodeState::Comp { env: 0.0 }));
            if let NodeStateBox(NodeState::Comp { env }) = st {
                let (l, r) = sig.stereo();
                let x = l.abs().max(r.abs());
                let k = if x > *env {
                    1.0 - (-dt / att).exp()
                } else {
                    1.0 - (-dt / rel).exp()
                };
                *env += k * (x - *env);
                Val::S(*env)
            } else {
                Val::S(0.0)
            }
        }
        Op::Rms => {
            let sig = input_sig(args, ctx);
            let bpm = ctx.bpm;
            let win_s = eval_arg(args, "window", Val::Ms(30.0), ctx).as_sec(bpm).clamp(0.001, 1.0);
            let n = ((win_s * ctx.sr) as usize).max(1);
            let st = ctx.state.get_or(id, || {
                NodeStateBox(NodeState::Rms { buf: vec![0.0; n], w: 0, sum: 0.0 })
            });
            if let NodeStateBox(NodeState::Rms { buf, w, sum }) = st {
                let x = sig.num();
                let x2 = x * x;
                *sum += x2 - buf[*w];
                buf[*w] = x2;
                *w = (*w + 1) % buf.len();
                Val::S((sum.max(0.0) / buf.len() as f64).sqrt())
            } else {
                Val::S(0.0)
            }
        }
        Op::Ringmod => {
            let a = input_sig(args, ctx);
            let b = eval_arg(args, "_1", Val::S(0.0), ctx);
            binop('*', a, b)
        }
        Op::Widen => {
            // M/S width; never touches M (mono-compatible by construction)
            let sig = input_sig(args, ctx);
            let amount = eval_arg(args, "amount", eval_arg(args, "_1", Val::S(0.5), ctx), ctx)
                .num()
                .clamp(0.0, 1.0);
            let (l, r) = sig.stereo();
            let m = 0.5 * (l + r);
            let s = 0.5 * (l - r) * (1.0 + amount);
            Val::St2(m + s, m - s)
        }
        Op::Haas => {
            let sig = input_sig(args, ctx);
            let bpm = ctx.bpm;
            let d_s = eval_arg(args, "delay", Val::Ms(12.0), ctx).as_sec(bpm).clamp(0.001, 0.05);
            let right = match arg(args, "side") {
                Some(Expr::Ident(s)) => s == "right",
                _ => true,
            };
            let cap = (ctx.sr * 0.05) as usize + 4;
            let st = ctx.state.get_or(id, || {
                NodeStateBox(NodeState::Haas { buf: vec![(0.0, 0.0); cap], w: 0 })
            });
            if let NodeStateBox(NodeState::Haas { buf, w }) = st {
                let n = buf.len();
                let d = ((d_s * ctx.sr) as usize).min(n - 1);
                let (l, r) = sig.stereo();
                buf[*w] = (l, r);
                let rd = (*w + n - d) % n;
                let out = if right { (l, buf[rd].1) } else { (buf[rd].0, r) };
                *w = (*w + 1) % n;
                Val::St2(out.0, out.1)
            } else {
                Val::S(0.0)
            }
        }
        Op::DelayFx => {
            let sig = input_sig(args, ctx);
            let bpm = ctx.bpm;
            let t_s = eval_arg(args, "time", Val::Ms(250.0), ctx).as_sec(bpm).clamp(0.001, 4.0);
            // NORMATIVE clamp 0..0.95: prevents feedback runaway (E014 at check time)
            let fb = eval_arg(args, "feedback", Val::S(0.45), ctx).num().clamp(0.0, 0.95);
            let mix = eval_arg(args, "mix", Val::S(0.3), ctx).num();
            let damp_hz = eval_arg(args, "damp", Val::Hz(6000.0), ctx).as_hz();
            let pingpong = matches!(arg(args, "pingpong"), Some(Expr::Ident(s)) if s == "true");
            let cap = (ctx.sr * 4.0).ceil() as usize + 4;
            let sr = ctx.sr;
            let st = ctx.state.get_or(id, || {
                NodeStateBox(NodeState::DelayFx {
                    buf: vec![(0.0, 0.0); cap],
                    w: 0,
                    damp: (0.0, 0.0),
                    cur: t_s * sr,
                    from: 0.0,
                    xfade: 0.0,
                })
            });
            if let NodeStateBox(NodeState::DelayFx { buf, w, damp, cur, from, xfade }) = st {
                let n = buf.len() as f64;
                let want = (t_s * ctx.sr).clamp(1.0, n - 4.0);
                if *xfade <= 0.0 && (want - *cur).abs() > 0.5 {
                    if *cur <= 0.0 || cur.is_nan() {
                        *cur = want;
                    } else {
                        // NORMATIVE: 20ms crossfade between read heads (avoids chirp/click)
                        *from = *cur;
                        *cur = want;
                        *xfade = 0.02;
                    }
                }
                let read = |buf: &Vec<(f64, f64)>, w: usize, d: f64| -> (f64, f64) {
                    let n = buf.len() as f64;
                    let rp = (w as f64 - d + n) % n;
                    let i0 = rp.floor() as usize;
                    let i1 = (i0 + 1) % buf.len();
                    let fr = rp - rp.floor();
                    let (a, b) = (buf[i0], buf[i1]);
                    (a.0 + (b.0 - a.0) * fr, a.1 + (b.1 - a.1) * fr)
                };
                let mut wet = read(buf, *w, *cur);
                if *xfade > 0.0 {
                    let old = read(buf, *w, *from);
                    let t = (*xfade / 0.02).clamp(0.0, 1.0);
                    wet = (wet.0 * (1.0 - t) + old.0 * t, wet.1 * (1.0 - t) + old.1 * t);
                    *xfade -= dt;
                }
                // damp: one-pole lowpass INSIDE the feedback loop (tape-style darkening)
                let k = onepole_k(damp_hz, ctx.sr);
                damp.0 = flush_denorm(damp.0 + k * (wet.0 - damp.0));
                damp.1 = flush_denorm(damp.1 + k * (wet.1 - damp.1));
                let (il, ir) = sig.stereo();
                if pingpong {
                    // input feeds L; L output crosses into R; R feeds back into L
                    buf[*w] = (0.5 * (il + ir) + damp.1 * fb, damp.0);
                } else {
                    buf[*w] = (il + damp.0 * fb, ir + damp.1 * fb);
                }
                *w = (*w + 1) % buf.len();
                let out = eq_power_mix((il, ir), wet, mix);
                Val::St2(out.0, out.1)
            } else {
                Val::S(0.0)
            }
        }
        Op::Chorus => {
            let sig = input_sig(args, ctx);
            let nv = eval_arg(args, "voices", Val::S(3.0), ctx).num().clamp(2.0, 4.0) as usize;
            let depth = eval_arg(args, "depth", Val::S(0.4), ctx).num().clamp(0.0, 1.0);
            let rate = eval_arg(args, "rate", Val::Hz(0.4), ctx).as_hz();
            let mix = eval_arg(args, "mix", Val::S(0.35), ctx).num();
            let spread = eval_arg(args, "spread", Val::S(1.0), ctx).num().clamp(0.0, 1.0);
            let cap = (ctx.sr * 0.04) as usize + 8;
            let st = ctx.state.get_or(id, || {
                let ph: Vec<f64> = (0..nv).map(|i| i as f64 / nv as f64).collect();
                NodeStateBox(NodeState::Chorus { buf: vec![(0.0, 0.0); cap], w: 0, ph })
            });
            if let NodeStateBox(NodeState::Chorus { buf, w, ph }) = st {
                let (il, ir) = sig.stereo();
                buf[*w] = (il, ir);
                let n = buf.len();
                let base_ms = 15.0;
                let mod_ms = 1.0 + 7.0 * depth;
                let mut wl = 0.0;
                let mut wr = 0.0;
                for i in 0..ph.len() {
                    // NORMATIVE: per-voice detuned rate + phase offset (desyncs; else it's vibrato)
                    let r_i = rate * (1.0 + 0.13 * i as f64);
                    let lfo = (2.0 * std::f64::consts::PI * ph[i]).sin();
                    ph[i] = (ph[i] + r_i * dt).fract();
                    let d = ((base_ms + mod_ms * (0.5 + 0.5 * lfo)) / 1000.0 * ctx.sr)
                        .clamp(2.0, n as f64 - 4.0);
                    let rp = (*w as f64 - d + n as f64) % n as f64;
                    let i0 = rp.floor() as usize;
                    let fr = rp - rp.floor();
                    // NORMATIVE: cubic Hermite (linear interp on modulated delay = HF zipper)
                    let im1 = (i0 + n - 1) % n;
                    let i1 = (i0 + 1) % n;
                    let i2 = (i0 + 2) % n;
                    let v = 0.5
                        * (hermite(buf[im1].0, buf[i0].0, buf[i1].0, buf[i2].0, fr)
                            + hermite(buf[im1].1, buf[i0].1, buf[i1].1, buf[i2].1, fr));
                    // alternate L/R panning per voice
                    let pan = if ph.len() > 1 {
                        spread * (if i % 2 == 0 { -1.0 } else { 1.0 })
                            * (1.0 - i as f64 / (2.0 * ph.len() as f64))
                    } else {
                        0.0
                    };
                    let a = (pan.clamp(-1.0, 1.0) + 1.0) * std::f64::consts::FRAC_PI_4;
                    wl += v * a.cos();
                    wr += v * a.sin();
                }
                let norm = std::f64::consts::SQRT_2 / (ph.len() as f64).sqrt();
                *w = (*w + 1) % n;
                let out = eq_power_mix((il, ir), (wl * norm, wr * norm), mix);
                Val::St2(out.0, out.1)
            } else {
                Val::S(0.0)
            }
        }
        Op::Reverb => {
            let sig = input_sig(args, ctx);
            let size = eval_arg(args, "size", Val::S(0.8), ctx).num().clamp(0.0, 1.0);
            let bpm = ctx.bpm;
            let decay_s = eval_arg(args, "decay", Val::Ms(2500.0), ctx).as_sec(bpm).max(0.05);
            let damp_hz = eval_arg(args, "damp", Val::Hz(5000.0), ctx).as_hz();
            let pre_s = eval_arg(args, "predelay", Val::Ms(20.0), ctx).as_sec(bpm).clamp(0.0, 0.25);
            let mix = eval_arg(args, "mix", Val::S(0.25), ctx).num();
            let width = eval_arg(args, "width", Val::S(1.0), ctx).num().clamp(0.0, 1.0);
            let sr = ctx.sr;
            let st = ctx.state.get_or(id, || {
                // mutually-prime lengths @44.1k scaled by size and sr (avoids metallic ringing)
                let base = [1123usize, 1237, 1381, 1489, 1601, 1733, 1867, 1993];
                let scale = (0.3 + 0.7 * size) * sr / 44100.0;
                let lens: Vec<usize> = base.iter().map(|&b| ((b as f64 * scale) as usize).max(32)).collect();
                let g: Vec<f64> = lens
                    .iter()
                    .map(|&l| 10f64.powf(-3.0 * l as f64 / (decay_s * sr)))
                    .collect();
                NodeStateBox(NodeState::Reverb {
                    pre: vec![(0.0, 0.0); ((sr * 0.25) as usize).max(1)],
                    prew: 0,
                    ap1: vec![0.0; (sr * 0.005) as usize + 1],
                    ap1w: 0,
                    ap2: vec![0.0; (sr * 0.012) as usize + 1],
                    ap2w: 0,
                    lines: lens.iter().map(|&l| vec![0.0; l]).collect(),
                    lw: vec![0; 8],
                    damp: vec![0.0; 8],
                    g,
                })
            });
            if let NodeStateBox(NodeState::Reverb {
                pre, prew, ap1, ap1w, ap2, ap2w, lines, lw, damp, g,
            }) = st
            {
                let (il, ir) = sig.stereo();
                // predelay
                pre[*prew] = (il, ir);
                let pn = pre.len();
                let pd = ((pre_s * sr) as usize).min(pn - 1);
                let (dl, dr) = pre[(*prew + pn - pd) % pn];
                *prew = (*prew + 1) % pn;
                let mut x = 0.5 * (dl + dr);
                // NORMATIVE: 2 series allpasses (g=0.5) diffuse the input
                for (buf, wp) in [(&mut *ap1, &mut *ap1w), (&mut *ap2, &mut *ap2w)] {
                    let d = buf[*wp];
                    let y = -0.5 * x + d;
                    buf[*wp] = x + 0.5 * y;
                    *wp = (*wp + 1) % buf.len();
                    x = y;
                }
                // read + damp each line
                let kd = onepole_k(damp_hz, sr);
                let mut outs = [0.0f64; 8];
                for i in 0..8 {
                    let v = lines[i][lw[i]];
                    damp[i] = flush_denorm(damp[i] + kd * (v - damp[i]));
                    outs[i] = damp[i] * g[i];
                }
                // Householder 8x8: y_i = x_i - (2/8) * sum(x)
                let sum: f64 = outs.iter().sum();
                let h = 0.25 * sum;
                for i in 0..8 {
                    let fbv = flush_denorm(outs[i] - h + x);
                    let li = &mut lines[i];
                    li[lw[i]] = fbv;
                    lw[i] = (lw[i] + 1) % li.len();
                }
                // output taps: odd lines -> L, even -> R; width via M/S
                let wl0 = outs[1] + outs[3] + outs[5] + outs[7];
                let wr0 = outs[0] + outs[2] + outs[4] + outs[6];
                let m = 0.5 * (wl0 + wr0);
                let s = 0.5 * (wl0 - wr0) * width;
                let (wl, wr) = ((m + s) * 0.6, (m - s) * 0.6);
                let out = eq_power_mix((il, ir), (wl, wr), mix);
                Val::St2(out.0, out.1)
            } else {
                Val::S(0.0)
            }
        }
        Op::Compressor | Op::Duck => {
            let sig = input_sig(args, ctx);
            // key: sidechain detector input (tier4 §2.2); defaults to the signal itself
            let key = match arg(args, "key") {
                Some(e) => eval(e, ctx),
                None => sig,
            };
            let thr_db = 20.0 * eval_arg(args, "threshold", Val::S(10f64.powf(-18.0 / 20.0)), ctx)
                .num()
                .max(1e-6)
                .log10();
            let ratio = eval_arg(args, "ratio", Val::S(3.0), ctx).num().max(1.0);
            let bpm = ctx.bpm;
            let att = eval_arg(args, "attack", Val::Ms(10.0), ctx).as_sec(bpm).max(0.0001);
            let rel = eval_arg(args, "release", Val::Ms(120.0), ctx).as_sec(bpm).max(0.001);
            let amount = eval_arg(args, "amount", Val::S(1.0), ctx).num().clamp(0.0, 1.0);
            let slope = (1.0 - 1.0 / ratio) * amount;
            let makeup_db = match arg(args, "makeup") {
                Some(Expr::Ident(s)) if s == "auto" => -thr_db * slope * 0.5,
                Some(e) => {
                    let v = eval(e, ctx).num();
                    20.0 * v.max(1e-9).log10()
                }
                None => if op == Op::Duck { 0.0 } else { -thr_db * slope * 0.5 },
            };
            let st = ctx.state.get_or(id, || NodeStateBox(NodeState::Comp { env: 0.0 }));
            if let NodeStateBox(NodeState::Comp { env }) = st {
                let (l, r) = sig.stereo();
                let (kl, kr) = key.stereo();
                // NORMATIVE: stereo-linked detector (unlinked wobbles the image)
                let x = kl.abs().max(kr.abs());
                let k_att = 1.0 - (-dt / att).exp();
                let k_rel = 1.0 - (-dt / rel).exp();
                if x > *env {
                    *env += k_att * (x - *env);
                } else {
                    *env += k_rel * (x - *env);
                }
                let lvl_db = 20.0 * env.max(1e-9).log10();
                // NORMATIVE: 6db soft knee (hard knee sounds grainy on pads)
                let knee = 6.0;
                let over = lvl_db - thr_db;
                let gr_db = if over <= -knee * 0.5 {
                    0.0
                } else if over >= knee * 0.5 {
                    slope * over
                } else {
                    slope * (over + knee * 0.5).powi(2) / (2.0 * knee)
                };
                let gain = 10f64.powf((-gr_db + makeup_db) / 20.0);
                Val::St2(l * gain, r * gain)
            } else {
                Val::S(0.0)
            }
        }
        Op::Limiter => {
            let sig = input_sig(args, ctx);
            let ceil = eval_arg(args, "ceiling", Val::S(10f64.powf(-1.0 / 20.0)), ctx)
                .num()
                .clamp(0.01, 1.0);
            let bpm = ctx.bpm;
            let la_s = eval_arg(args, "lookahead", Val::Ms(5.0), ctx).as_sec(bpm).clamp(0.0005, 0.02);
            let rel = eval_arg(args, "release", Val::Ms(60.0), ctx).as_sec(bpm).max(0.001);
            let la = ((la_s * ctx.sr) as usize).max(1);
            let st = ctx.state.get_or(id, || {
                NodeStateBox(NodeState::Limiter { buf: vec![(0.0, 0.0); la + 1], w: 0, gain: 1.0 })
            });
            if let NodeStateBox(NodeState::Limiter { buf, w, gain }) = st {
                let (il, ir) = sig.stereo();
                buf[*w] = (il, ir);
                *w = (*w + 1) % buf.len();
                // peak over the lookahead window
                let mut peak = 1e-9f64;
                for &(l, r) in buf.iter() {
                    peak = peak.max(l.abs()).max(r.abs());
                }
                let target = (ceil / peak).min(1.0);
                if target < *gain {
                    *gain = target; // instant attack - it's a limiter
                } else {
                    let k_rel = 1.0 - (-dt / rel).exp();
                    *gain += k_rel * (target - *gain);
                }
                let (ol, or) = buf[*w]; // oldest = delayed output
                // NORMATIVE: output never exceeds ceiling
                let l = (ol * *gain).clamp(-ceil, ceil);
                let r = (or * *gain).clamp(-ceil, ceil);
                Val::St2(l, r)
            } else {
                Val::S(0.0)
            }
        }
        Op::Leslie => {
            // Leslie rotary cabinet (Pekonen/Valimaki): NOT a chorus. Band split
            // at 800hz; horn (high band) and drum (low band) each get circular
            // doppler (sinusoidally modulated delay) + synchronized AM, read by
            // two virtual mics in opposite phase (L/R). Rotors accelerate with
            // mechanical inertia between chorale (slow) and tremolo (fast).
            let sig = input_sig(args, ctx);
            let speed = eval_arg(args, "speed", Val::S(1.0), ctx).num().clamp(0.0, 1.0);
            let depth = eval_arg(args, "depth", Val::S(1.0), ctx).num().clamp(0.0, 1.5);
            let mix = eval_arg(args, "mix", Val::S(1.0), ctx).num().clamp(0.0, 1.0);
            let cap = (ctx.sr * 0.012) as usize + 8;
            let st = ctx.state.get_or(id, || {
                NodeStateBox(NodeState::Leslie {
                    h: vec![0.0; cap],
                    d: vec![0.0; cap],
                    w: 0,
                    ph_h: 0.15,
                    ph_d: 0.62, // rotors start out of phase (real cabinets do)
                    rh: 0.8 + speed * 6.0,
                    rd: 0.7 + speed * 5.0,
                    lp1: 0.0,
                    lp2: 0.0,
                })
            });
            if let NodeStateBox(NodeState::Leslie { h, d, w, ph_h, ph_d, rh, rd, lp1, lp2 }) = st {
                let (il, ir) = sig.stereo();
                let x = 0.5 * (il + ir); // real cabinet: mono amp
                // crossover ~800hz: 2x one-pole lowpass, complementary high band
                let kxo = onepole_k(800.0, ctx.sr);
                *lp1 = flush_denorm(*lp1 + kxo * (x - *lp1));
                *lp2 = flush_denorm(*lp2 + kxo * (*lp1 - *lp2));
                let low = *lp2;
                let high = x - *lp2;
                h[*w] = high;
                d[*w] = low;
                // rotor speeds: horn 0.8..6.8hz, drum 0.7..5.7hz; inertia: the
                // horn is light (~1s), the drum heavy (~3.5s)
                let rh_t = 0.8 + speed * 6.0;
                let rd_t = 0.7 + speed * 5.0;
                *rh += (1.0 - (-dt / 1.0f64).exp()) * (rh_t - *rh);
                *rd += (1.0 - (-dt / 3.5f64).exp()) * (rd_t - *rd);
                *ph_h = (*ph_h + *rh * dt).fract();
                *ph_d = (*ph_d + *rd * dt).fract();
                let tau = 2.0 * std::f64::consts::PI;
                // horn: base 1.3ms, doppler swing +-0.35ms; drum: smaller swing
                // (bigger radius but slower and heavily baffled)
                let base_h = 0.0013 * ctx.sr;
                let sw_h = 0.00035 * ctx.sr * depth;
                let base_d = 0.0020 * ctx.sr;
                let sw_d = 0.00012 * ctx.sr * depth;
                let n = h.len() as f64;
                let dhl = (base_h + sw_h * (tau * *ph_h).sin()).clamp(2.0, n - 4.0);
                let dhr = (base_h + sw_h * (tau * *ph_h + std::f64::consts::PI).sin()).clamp(2.0, n - 4.0);
                // drum spins the other way
                let ddl = (base_d - sw_d * (tau * *ph_d).sin()).clamp(2.0, n - 4.0);
                let ddr = (base_d - sw_d * (tau * *ph_d + std::f64::consts::PI).sin()).clamp(2.0, n - 4.0);
                let hl = ring_read(h, *w, dhl);
                let hr = ring_read(h, *w, dhr);
                let dl = ring_read(d, *w, ddl);
                let dr = ring_read(d, *w, ddr);
                // AM synchronized with rotation (directivity), opposite phase L/R
                let am_hl = 1.0 + 0.30 * depth * (tau * *ph_h).cos();
                let am_hr = 1.0 + 0.30 * depth * (tau * *ph_h + std::f64::consts::PI).cos();
                let am_dl = 1.0 + 0.18 * depth * (tau * *ph_d).cos();
                let am_dr = 1.0 + 0.18 * depth * (tau * *ph_d + std::f64::consts::PI).cos();
                *w = (*w + 1) % h.len();
                let wl = hl * am_hl + dl * am_dl;
                let wr = hr * am_hr + dr * am_dr;
                let out = eq_power_mix((il, ir), (wl, wr), mix);
                Val::St2(out.0, out.1)
            } else {
                Val::S(0.0)
            }
        }
        Op::Hall => {
            // Scattering Delay Network room (survey 2.9.2): 6 scattering nodes
            // at the walls' first-reflection points of a real shoebox, fully
            // connected by bidirectional delay lines. Early reflections are
            // geometrically exact; recirculation builds the late tail. The
            // shared hall for the whole mix (use on master with sends low).
            let sig = input_sig(args, ctx);
            let size = eval_arg(args, "size", Val::S(0.5), ctx).num().clamp(0.0, 1.0);
            let bpm = ctx.bpm;
            let decay_s = eval_arg(args, "decay", Val::Ms(1800.0), ctx).as_sec(bpm).clamp(0.1, 12.0);
            let damp_hz = eval_arg(args, "damp", Val::Hz(5000.0), ctx).as_hz();
            let mix = eval_arg(args, "mix", Val::S(0.25), ctx).num().clamp(0.0, 1.0);
            let sr = ctx.sr;
            let st = ctx.state.get_or(id, || {
                NodeStateBox(NodeState::Hall(Box::new(make_hall(size, decay_s, sr))))
            });
            if let NodeStateBox(NodeState::Hall(hs)) = st {
                let (il, ir) = sig.stereo();
                let x = 0.5 * (il + ir);
                let kd = onepole_k(damp_hz, sr);
                let (wl, wr) = hs.tick(x, kd);
                let out = eq_power_mix((il, ir), (wl, wr), mix);
                Val::St2(out.0, out.1)
            } else {
                Val::S(0.0)
            }
        }
        Op::Brass => {
            // brass: lip valve (2nd-order resonator squared = time-varying
            // transmission gate, Cook/STK lineage) + bore waveguide + bell
            // reflection lowpass + IN-LOOP level-dependent waveshaper for the
            // nonlinear "brassiness" (dark -> torn metal, continuous with
            // dynamics). Radiated output = what the bell does not reflect.
            let freq = eval_arg(args, "freq", Val::Hz(220.0), ctx).as_hz().clamp(40.0, 2000.0);
            let pressure = eval_arg(args, "pressure", Val::S(0.8), ctx).num().clamp(0.0, 1.6);
            let lip = eval_arg(args, "lip", Val::S(1.0), ctx).num().clamp(0.4, 2.5);
            let bell = eval_arg(args, "bell", Val::Hz(1500.0), ctx).as_hz().clamp(300.0, 8000.0);
            let rasp = eval_arg(args, "rasp", Val::S(0.4), ctx).num().clamp(0.0, 1.0);
            let breath_noise = eval_arg(args, "breath", Val::S(0.02), ctx).num().clamp(0.0, 1.0);
            let g = eval_arg(args, "gain", Val::S(1.0), ctx).num();
            let cap = (ctx.sr / 30.0) as usize + 8;
            let nseed = ctx.seed ^ (id as u64).wrapping_mul(0x2545F4914F6CDD1D) | 1;
            let st = ctx.state.get_or(id, || {
                NodeStateBox(NodeState::Brass(Box::new(BrassS {
                    bore: vec![0.0; cap],
                    w: 0,
                    lip1: 0.0,
                    lip2: 0.0,
                    lp: 0.0,
                    dc: (0.0, 0.0),
                    ap: (0.0, 0.0),
                    rng: nseed,
                    turb: TurbState::default(),
                    prev_out: 0.0,
                })))
            });
            if let NodeStateBox(NodeState::Brass(s)) = st {
                let period = ctx.sr / freq;
                // intonation fit (2-point): lip-filter phase lead pulls the
                // regime sharp; longer bores need proportionally more length.
                // -1.0: sample medio do allpass do steepening no loop
                let comp = 1.1333 + 0.000127 * (period - 168.6).max(0.0);
                let d_bore = ((period - 1.4) * comp - 1.0).max(4.0);
                let bore_out = ring_read(&s.bore, s.w, d_bore);
                // bell: lowpass reflects (dark), the rest radiates (bright)
                let kb = onepole_k(bell, ctx.sr);
                s.lp = flush_denorm(s.lp + kb * (bore_out - s.lp));
                // brassiness = steepening cumulativo de Burgers aproximado no
                // loop (Vergez/Rodet, Hirschberg 1996): allpass de 1a ordem
                // com coeficiente modulado pelo PROPRIO sinal (warp de fase
                // dependente de pressao = frente de onda empina a cada volta,
                // choque gradual pp->ff) + tanh passivo INSTANTANEO (so a
                // parte alta da onda satura; sem envelope follower, que
                // distorcia o ciclo inteiro e mascarava a dinamica real)
                let refl_raw = 0.92 * s.lp;
                let a_ap = (rasp * 1.1 * refl_raw).clamp(-0.28, 0.28);
                let ap_y = a_ap * refl_raw + s.ap.0 - a_ap * s.ap.1;
                s.ap.0 = flush_denorm(refl_raw);
                s.ap.1 = flush_denorm(ap_y);
                // dc-block the reflection (waveguide hygiene)
                let dcv = ap_y - s.dc.0 + 0.995 * s.dc.1;
                s.dc.0 = ap_y;
                s.dc.1 = flush_denorm(dcv);
                let refl = dcv;
                // pressao de boca limpa: metais reais tem o sustain MAIS
                // limpo dos sopros (NHR -35db); o ar entra so no ataque e
                // gated pela abertura do labio (pitch-sincrono), nunca como
                // tapete por cima
                // remap: a janela de oscilacao do labio e ~pm 0.24..0.42; o
                // user controla dinamica em 0.3..1.2 e o brilho vem do
                // steepening (nao do regime morrer em piano)
                let p_eff = (0.38 + 0.68 * pressure) * smoothstep(0.0, 0.10, pressure);
                let pm = 0.35 * p_eff;
                // lip valve: resonator near lip*freq driven by the pressure
                // difference; its output squared (0..1) gates mouth vs bore
                // pull-down leve com pressao: cancela o sharp do labio batido
                // forte (spread medido +4..+15ct de pp a ff sem isso)
                let f_lip = (freq * lip * (0.99 - 0.010 * (p_eff - 0.95)))
                    .clamp(30.0, ctx.sr * 0.4);
                let r_l = (-std::f64::consts::PI * f_lip / (12.0 * ctx.sr)).exp(); // Q ~ 12
                let th = 2.0 * std::f64::consts::PI * f_lip / ctx.sr;
                let a1 = 2.0 * r_l * th.cos();
                let a2 = -r_l * r_l;
                // tanh doma o drive em fortissimo: labio batido com forca
                // demais destrava da resonancia e puxa a afinacao +15ct
                let dp = (pm - refl).tanh();
                // dc-normalized drive (dc gain ~0.8): the resonance peak (~Q x)
                // does the mode selection; unnormalized drive slams the lip
                let x_l = a1 * s.lip1 + a2 * s.lip2 + dp * 0.8 * (1.0 - a1 - a2).abs();
                s.lip2 = s.lip1;
                s.lip1 = flush_denorm(x_l.clamp(-3.0, 3.0));
                // 0.85 cap keeps the gate in its dynamic region at fortissimo
                // (a lip pegged open makes a sine - the OPPOSITE of brass)
                let a_open = (x_l * x_l).min(0.85); // 0 closed .. 0.85 open
                // turbulencia (ver TurbState) gated pela abertura instantanea
                // do labio: pulsa em f0, e quase nada no sustain
                let uj = pressure.max(0.0).sqrt();
                let mut rng = s.rng;
                let turb = s.turb.tick(&mut rng, uj, ctx.sr);
                s.rng = rng;
                let turb_p = breath_noise * 0.07 * a_open * turb;
                // convex gate: open lip lets mouth pressure in, closed reflects
                s.bore[s.w] = a_open * (pm + turb_p) + (1.0 - a_open) * refl;
                s.w = (s.w + 1) % s.bore.len();
                // radiacao: transmitida (incidente - refletida) derivada
                // 16: paridade de nivel com o flute a gain 1 (A/B a v0.7)
                let trans = bore_out - 0.92 * s.lp;
                let out = (trans - 0.72 * s.prev_out) * 50.0;
                s.prev_out = trans;
                Val::S(out * g)
            } else {
                Val::S(0.0)
            }
        }
        Op::Voz => {
            // choir/voice source-filter: glottal pulse (Rosenberg, spectral
            // tilt via closing speed) -> 4 parallel formant resonators (per
            // vowel and voice type, published tables) with flow-derivative
            // source (folds in +6db/oct lip radiation). ens: N independent
            // singers per note: personal jitter, shimmer, vibrato rate/phase/
            // onset, vocal-tract length and pan. This is the statistical layer
            // that makes a section sound like people, not an oscillator.
            let freq = eval_arg(args, "freq", Val::Hz(220.0), ctx).as_hz().clamp(60.0, 1500.0);
            let vowel = eval_arg(args, "vowel", Val::S(0.0), ctx).num().clamp(0.0, 4.0);
            let tipo: &str = match arg(args, "tipo") {
                Some(Expr::Ident(s)) => s.as_str(),
                _ => "tenor",
            };
            let ens = eval_arg(args, "ens", Val::S(1.0), ctx).num().clamp(1.0, 16.0) as usize;
            let vib_st = eval_arg(args, "vib", Val::S(0.18), ctx).num().clamp(0.0, 1.0);
            let vib_rate = eval_arg(args, "vib_rate", Val::Hz(5.5), ctx).as_hz().clamp(0.5, 9.0);
            let jitter_st = eval_arg(args, "jitter", Val::S(0.10), ctx).num().clamp(0.0, 0.5);
            let shimmer = eval_arg(args, "shimmer", Val::S(0.12), ctx).num().clamp(0.0, 0.5);
            let breath = eval_arg(args, "breath", Val::S(0.05), ctx).num().clamp(0.0, 1.0);
            let tension = eval_arg(args, "tension", Val::S(0.6), ctx).num().clamp(0.0, 1.0);
            let spread = eval_arg(args, "spread", Val::S(0.7), ctx).num().clamp(0.0, 1.0);
            let g = eval_arg(args, "gain", Val::S(1.0), ctx).num();
            let seed = ctx.seed ^ (id as u64).wrapping_mul(0x9E3779B97F4A7C15) | 1;
            let st = ctx.state.get_or(id, || {
                let mut rng = seed;
                let singers: Vec<VozSinger> = (0..ens)
                    .map(|i| {
                        let u = |r: &mut u64| xorshift(r) * 0.5 + 0.5; // 0..1
                        let pan = if ens > 1 {
                            spread * (2.0 * i as f64 / (ens as f64 - 1.0) - 1.0)
                                + xorshift(&mut rng) * 0.1
                        } else {
                            0.0
                        };
                        VozSinger {
                            ph: u(&mut rng),
                            jit: 0.0,
                            jt: 0.0,
                            vph: u(&mut rng),
                            vrate: vib_rate + xorshift(&mut rng) * 0.45,
                            vdel: 0.20 + 0.25 * u(&mut rng),
                            shim: 0.0,
                            sht: 0.0,
                            s1: [0.0; 4],
                            s2: [0.0; 4],
                            gprev: 0.0,
                            asp: 0.0,
                            fsc: 1.0 + 0.045 * xorshift(&mut rng),
                            pan: pan.clamp(-1.0, 1.0),
                            onset: 0.04 * u(&mut rng),
                            rng: (rng | 1).wrapping_mul(0x9E3779B97F4A7C15) | 1,
                            t: 0.0,
                        }
                    })
                    .collect();
                NodeStateBox(NodeState::Voz { singers })
            });
            if let NodeStateBox(NodeState::Voz { singers }) = st {
                let (frm, bws) = voz_formants(tipo, vowel);
                let mut l = 0.0;
                let mut r = 0.0;
                let tau = 2.0 * std::f64::consts::PI;
                // closing-phase fraction: tense voice = abrupt closure = bright
                let oq = 0.62;
                let cl = (0.30 * (1.0 - tension) + 0.07) * oq;
                for s in singers.iter_mut() {
                    s.t += dt;
                    if s.t < s.onset {
                        continue;
                    }
                    // jitter: Ornstein-Uhlenbeck walk, retargeted ~8hz
                    if xorshift(&mut s.rng) * 0.5 + 0.5 < 8.0 * dt {
                        s.jt = xorshift(&mut s.rng) * jitter_st;
                    }
                    let kj = onepole_k(4.0, ctx.sr);
                    s.jit += kj * (s.jt - s.jit);
                    // shimmer: slow amplitude walk, retargeted ~6hz
                    if xorshift(&mut s.rng) * 0.5 + 0.5 < 6.0 * dt {
                        s.sht = xorshift(&mut s.rng) * shimmer;
                    }
                    let ks = onepole_k(3.0, ctx.sr);
                    s.shim += ks * (s.sht - s.shim);
                    // vibrato: delayed onset, depth grows over 600ms
                    let venv = ((s.t - s.vdel) / 0.6).clamp(0.0, 1.0);
                    let vib = (tau * s.vph).sin() * vib_st * venv;
                    s.vph = (s.vph + s.vrate * dt).fract();
                    let f0 = freq * 2f64.powf((vib + s.jit) / 12.0);
                    // Rosenberg glottal pulse: raised-cos rise, cos closing
                    let p = s.ph;
                    s.ph = (s.ph + f0 * dt).fract();
                    let rise = oq - cl;
                    let gl = if p < rise {
                        0.5 * (1.0 - (std::f64::consts::PI * p / rise).cos())
                    } else if p < oq {
                        (0.5 * std::f64::consts::PI * (p - rise) / cl).cos()
                    } else {
                        0.0
                    };
                    // flow derivative = glottal source * lip radiation (+6db/oct)
                    let mut src = (gl - s.gprev) * 6.0;
                    s.gprev = gl;
                    // aspiracao: ruido colorido (-6db/oct acima de 3khz, a
                    // banda 5..15khz crua e o percepto de chiado) gated pelo
                    // QUADRADO da abertura glotal - pulsa em f0 junto com a
                    // fonte (Klatt AH), quase zero na fase fechada (sem piso)
                    if breath > 0.0 {
                        let ka = onepole_k(3000.0, ctx.sr);
                        s.asp = flush_denorm(s.asp + ka * (xorshift(&mut s.rng) - s.asp));
                        src += s.asp * breath * (0.02 + 0.98 * gl * gl) * 1.5;
                    }
                    // 4 parallel formant resonators (per-singer tract length fsc)
                    let mut y = 0.0;
                    for i in 0..4 {
                        let f = (frm[i].0 * s.fsc).clamp(60.0, ctx.sr * 0.45);
                        let rr = (-std::f64::consts::PI * bws[i] / ctx.sr).exp();
                        let thf = tau * f / ctx.sr;
                        let a1 = 2.0 * rr * thf.cos();
                        let a2 = -rr * rr;
                        // (1-r)*sin(th): peak gain ~1 regardless of formant freq.
                        // (f/600): +6db/oct source-tilt compensation - published
                        // formant amplitudes are OUTPUT levels; the glottal
                        // derivative source still falls -6db/oct, so drive each
                        // formant with a whitened level (FOF-style)
                        let gin = (1.0 - rr) * thf.sin().max(0.05) * 2.2 * (f / 600.0).max(0.5);
                        let yi = a1 * s.s1[i] + a2 * s.s2[i] + src * gin * frm[i].1;
                        s.s2[i] = s.s1[i];
                        s.s1[i] = flush_denorm(yi);
                        y += yi;
                    }
                    let att = ((s.t - s.onset) / 0.015).clamp(0.0, 1.0);
                    let v = y * (1.0 + s.shim) * att;
                    let a = (s.pan + 1.0) * std::f64::consts::FRAC_PI_4;
                    l += v * a.cos();
                    r += v * a.sin();
                }
                let norm = 1.6 / (singers.len() as f64).sqrt();
                Val::St2(l * norm * g, r * norm * g)
            } else {
                Val::S(0.0)
            }
        }
        _ => Val::S(0.0),
    }
}

/// formant tables (freq hz, linear amp) x4 + bandwidths, per voice type and
/// vowel (a e i o u), interpolated for fractional vowel positions. Values from
/// the published Csound/CNMAT singing-voice formant tables.
fn voz_formants(tipo: &str, vowel: f64) -> ([(f64, f64); 4], [f64; 4]) {
    // [vowel][formant] = (hz, db)
    const SOP: [[(f64, f64); 4]; 5] = [
        [(800.0, 0.0), (1150.0, -6.0), (2900.0, -32.0), (3900.0, -20.0)],
        [(350.0, 0.0), (2000.0, -20.0), (2800.0, -15.0), (3600.0, -40.0)],
        [(270.0, 0.0), (2140.0, -12.0), (2950.0, -26.0), (3900.0, -26.0)],
        [(450.0, 0.0), (800.0, -11.0), (2830.0, -22.0), (3800.0, -22.0)],
        [(325.0, 0.0), (700.0, -16.0), (2700.0, -35.0), (3800.0, -40.0)],
    ];
    const ALT: [[(f64, f64); 4]; 5] = [
        [(800.0, 0.0), (1150.0, -4.0), (2800.0, -20.0), (3500.0, -36.0)],
        [(400.0, 0.0), (1600.0, -24.0), (2700.0, -30.0), (3300.0, -35.0)],
        [(350.0, 0.0), (1700.0, -20.0), (2700.0, -30.0), (3700.0, -36.0)],
        [(450.0, 0.0), (800.0, -9.0), (2830.0, -16.0), (3500.0, -28.0)],
        [(325.0, 0.0), (700.0, -12.0), (2530.0, -30.0), (3500.0, -40.0)],
    ];
    const TEN: [[(f64, f64); 4]; 5] = [
        [(650.0, 0.0), (1080.0, -6.0), (2650.0, -7.0), (2900.0, -8.0)],
        [(400.0, 0.0), (1700.0, -14.0), (2600.0, -12.0), (3200.0, -14.0)],
        [(290.0, 0.0), (1870.0, -15.0), (2800.0, -18.0), (3250.0, -20.0)],
        [(400.0, 0.0), (800.0, -10.0), (2600.0, -12.0), (2800.0, -12.0)],
        [(350.0, 0.0), (600.0, -20.0), (2700.0, -17.0), (2900.0, -14.0)],
    ];
    const BAS: [[(f64, f64); 4]; 5] = [
        [(600.0, 0.0), (1040.0, -7.0), (2250.0, -9.0), (2450.0, -9.0)],
        [(400.0, 0.0), (1620.0, -12.0), (2400.0, -9.0), (2800.0, -12.0)],
        [(250.0, 0.0), (1750.0, -30.0), (2600.0, -16.0), (3050.0, -22.0)],
        [(400.0, 0.0), (750.0, -11.0), (2400.0, -21.0), (2600.0, -20.0)],
        [(350.0, 0.0), (600.0, -20.0), (2400.0, -32.0), (2675.0, -28.0)],
    ];
    let tab: &[[(f64, f64); 4]; 5] = match tipo {
        "soprano" => &SOP,
        "alto" | "contralto" => &ALT,
        "baixo" | "bass" => &BAS,
        _ => &TEN,
    };
    let v0 = vowel.floor() as usize;
    let v1 = (v0 + 1).min(4);
    let fr = vowel - v0 as f64;
    let mut out = [(0.0, 0.0); 4];
    for i in 0..4 {
        let f = tab[v0][i].0 + (tab[v1][i].0 - tab[v0][i].0) * fr;
        let db = tab[v0][i].1 + (tab[v1][i].1 - tab[v0][i].1) * fr;
        out[i] = (f, 10f64.powf(db / 20.0));
    }
    (out, [80.0, 90.0, 120.0, 140.0])
}

fn osc(op: Op, args: &[(String, Expr)], id: usize, ctx: &mut Ctx) -> Val {
    let dt_s = 1.0 / ctx.sr;
    let freq = eval_arg(args, "freq", Val::Hz(440.0), ctx).as_hz().clamp(0.0, 20000.0);
    let g = eval_arg(args, "gain", Val::S(1.0), ctx).num();
    let dt = freq * dt_s;
    match op {
        Op::Sine => {
            let phase0 = eval_arg(args, "phase", Val::S(0.0), ctx).num();
            // fm: phase modulation in radians (DX7-style PM; stable, stays in tune)
            let fm = eval_arg(args, "fm", Val::S(0.0), ctx).num();
            let st = ctx.state.get_or(id, || NodeStateBox(NodeState::Phase(phase0.fract())));
            if let NodeStateBox(NodeState::Phase(ph)) = st {
                let v = (2.0 * std::f64::consts::PI * *ph + fm).sin();
                *ph = (*ph + dt).fract();
                Val::S(v * g)
            } else {
                Val::S(0.0)
            }
        }
        Op::Triangle => {
            let fm = eval_arg(args, "fm", Val::S(0.0), ctx).num();
            let st = ctx.state.get_or(id, || NodeStateBox(NodeState::Phase(0.0)));
            if let NodeStateBox(NodeState::Phase(ph)) = st {
                let p = (*ph + fm / (2.0 * std::f64::consts::PI)).rem_euclid(1.0);
                let v = 1.0 - 4.0 * (p - 0.5).abs();
                *ph = (*ph + dt).fract();
                Val::S(v * g)
            } else {
                Val::S(0.0)
            }
        }
        Op::Square | Op::Pulse => {
            let width = if op == Op::Pulse {
                eval_arg(args, "width", Val::S(0.5), ctx).num().clamp(0.05, 0.95)
            } else {
                0.5
            };
            let st = ctx.state.get_or(id, || NodeStateBox(NodeState::Phase(0.0)));
            if let NodeStateBox(NodeState::Phase(ph)) = st {
                let t = *ph;
                let mut v = if t < width { 1.0 } else { -1.0 };
                v += poly_blep(t, dt);
                v -= poly_blep((t - width).rem_euclid(1.0), dt);
                // DC correction for width != 50% (known v0 bug)
                v -= 2.0 * width - 1.0;
                *ph = (*ph + dt).fract();
                Val::S(v * g)
            } else {
                Val::S(0.0)
            }
        }
        Op::Saw => {
            let n = eval_arg(args, "unison", Val::S(1.0), ctx).num().max(1.0) as usize;
            let spread = eval_arg(args, "spread", Val::StI(0.0), ctx).num(); // semitones
            let width = eval_arg(args, "width", Val::S(0.0), ctx).num().clamp(0.0, 1.0);
            let seed = ctx.seed;
            let st = ctx.state.get_or(id, || {
                let mut rng = seed ^ (id as u64).wrapping_mul(0x2545F4914F6CDD1D) | 1;
                let mut ph = Vec::with_capacity(n);
                let mut det = Vec::with_capacity(n);
                let mut pan = Vec::with_capacity(n);
                for i in 0..n {
                    ph.push((xorshift(&mut rng) * 0.5 + 0.5).fract());
                    let o = if n > 1 { 2.0 * i as f64 / (n as f64 - 1.0) - 1.0 } else { 0.0 };
                    det.push(o);
                    let p = if n > 1 { o } else { 0.0 };
                    pan.push(p);
                }
                NodeStateBox(NodeState::Unison { ph, det, pan })
            });
            if let NodeStateBox(NodeState::Unison { ph, det, pan }) = st {
                let mut l = 0.0;
                let mut r = 0.0;
                let norm = 1.0 / (ph.len() as f64).sqrt();
                for i in 0..ph.len() {
                    let f = freq * 2f64.powf(spread * det[i] / 12.0);
                    let d = f * dt_s;
                    let t = ph[i];
                    let v = 2.0 * t - 1.0 - poly_blep(t, d.max(1e-9));
                    ph[i] = (t + d).fract();
                    let pos = pan[i] * width; // -1..1
                    let a = (pos + 1.0) * std::f64::consts::FRAC_PI_4;
                    l += v * a.cos();
                    r += v * a.sin();
                }
                if ph.len() == 1 && width == 0.0 {
                    Val::S((l + r) * std::f64::consts::SQRT_2 * 0.5 * norm * g * 1.4142)
                } else {
                    Val::St2(l * norm * g * 1.4142 * 0.70710678, r * norm * g * 1.4142 * 0.70710678)
                }
            } else {
                Val::S(0.0)
            }
        }
        _ => Val::S(0.0),
    }
}

// ---------- voices & synth instance ----------

pub struct Voice {
    active: bool,
    note: f64,        // current (glided) pitch
    target_note: f64, // glide target
    vel: f64,
    gate: f64,
    t: f64,   // seconds since note_on
    dur: f64, // scheduled note length in seconds (0 = unknown)
    rand: f64,
    idx: usize,
    start_order: u64,
    state: StateStore,
    /// previous-sample let values (slot-indexed, resolve order)
    prev: Vec<Val>,
    /// scratch for this-sample let values (kept to reuse the allocation)
    cur: Vec<Val>,
    /// |out| of the last sample (steal quietest)
    last_out: f64,
    /// per-chunk output buffer (voice-parallel block processing)
    blk: Vec<(f64, f64)>,
    has_sounded: bool,
    silent_s: f64,
    fade_in: f64, // remaining fade-in seconds after steal
    seed: u64,
}

impl Voice {
    fn new(idx: usize, nlets: usize) -> Self {
        Voice {
            active: false,
            note: 60.0,
            target_note: 60.0,
            vel: 0.0,
            gate: 0.0,
            t: 0.0,
            dur: 0.0,
            rand: 0.0,
            idx,
            start_order: 0,
            state: StateStore::new(),
            prev: vec![Val::S(0.0); nlets],
            cur: Vec::with_capacity(nlets),
            last_out: 0.0,
            blk: Vec::new(),
            has_sounded: false,
            silent_s: 0.0,
            fade_in: 0.0,
            seed: 1,
        }
    }

    fn start(&mut self, note: f64, vel: f64, dur: f64, order: u64, rng: &mut u64, stolen: bool) {
        self.active = true;
        self.note = note;
        self.target_note = note;
        self.vel = vel;
        self.gate = 1.0;
        self.t = 0.0;
        self.dur = dur;
        self.rand = xorshift(rng) * 0.5 + 0.5;
        self.start_order = order;
        self.state.clear();
        for v in self.prev.iter_mut() {
            *v = Val::S(0.0);
        }
        self.last_out = 0.0;
        self.has_sounded = false;
        self.silent_s = 0.0;
        self.fade_in = if stolen { 0.005 } else { 0.0 };
        self.seed = (*rng).wrapping_add(order.wrapping_mul(0x9E3779B97F4A7C15)) | 1;
    }
}

pub struct SynthInstance {
    pub def: SynthDef,
    pub sr: f64,
    pub bpm: f64,
    spread: f64,
    voices: Vec<Voice>,
    global_state: StateStore,
    bus_state: StateStore,
    global_cur: Vec<Val>,
    global_prev: Vec<Val>,
    /// per-chunk global values, nglobals per sample (flat)
    gblock: Vec<Val>,
    /// scratch frame buffer for process_sample_with
    sample_scratch: Vec<(f64, f64)>,
    /// param values slot-indexed in def.params order
    params: Vec<Val>,
    /// same params by name (bus-scope ident fallback, event-rate updates only)
    params_named: HashMap<String, Val>,
    order: u64,
    rng: u64,
    mono: bool,
    glide_s: f64,
    legato: bool,
    steal: String,
    mono_stack: Vec<(f64, f64)>, // (note, vel)
}

impl SynthInstance {
    pub fn new(mut def: SynthDef, sr: f64, bpm: f64) -> Self {
        crate::resolve::resolve_synth(&mut def);
        let (nvoices, mono, glide_s, legato, steal, spread) = match &def.mode {
            Mode::Poly { n, steal, spread } => (*n, false, 0.0, false, steal.clone(), *spread),
            Mode::Mono { glide_ms, legato } => (1, true, glide_ms / 1000.0, *legato, "oldest".to_string(), 0.0),
        };
        let mut params = Vec::new();
        let mut params_named = HashMap::new();
        for p in &def.params {
            let mut dummy_state = StateStore::new();
            let mut ctx = Ctx {
                sr,
                bpm: 120.0,
                note: 60.0,
                vel: 0.0,
                gate: 0.0,
                time: 0.0,
                rand: 0.0,
                vidx: 0.0,
                dur: 0.0,
                state: &mut dummy_state,
                cur: &[],
                prev: &[],
                globals: &[],
                params: &[],
                bus_in: Val::S(0.0),
                synth_outs: None,
                params_by_name: None,
                seed: 1,
            };
            let v = eval(&p.default, &mut ctx);
            params.push(v);
            params_named.insert(p.name.clone(), v);
        }
        let nlets = def.voice.len();
        let nglobals = def.globals.len();
        SynthInstance {
            def,
            sr,
            bpm,
            spread,
            voices: (0..nvoices).map(|i| Voice::new(i, nlets)).collect(),
            global_state: StateStore::new(),
            bus_state: StateStore::new(),
            global_cur: Vec::with_capacity(nglobals),
            global_prev: vec![Val::S(0.0); nglobals],
            gblock: Vec::new(),
            sample_scratch: Vec::with_capacity(1),
            params,
            params_named,
            order: 0,
            rng: 0xDEADBEEFCAFE1234,
            mono,
            glide_s,
            legato,
            steal,
            mono_stack: Vec::new(),
        }
    }

    pub fn set_param(&mut self, name: &str, v: f64) {
        let idx = self.def.params.iter().position(|p| p.name == name);
        let nv = match idx.and_then(|i| self.params.get(i)) {
            Some(Val::Hz(_)) => Val::Hz(v),
            Some(Val::Ms(_)) => Val::Ms(v),
            Some(Val::Pitch(_)) => Val::Pitch(v),
            _ => Val::S(v),
        };
        if let Some(i) = idx {
            self.params[i] = nv;
        }
        self.params_named.insert(name.to_string(), nv);
    }

    pub fn note_on(&mut self, note: f64, vel: f64) {
        self.note_on_dur(note, vel, 0.0);
    }

    /// note_on with the scheduled note length (seconds); exposed to voices as `dur`
    pub fn note_on_dur(&mut self, note: f64, vel: f64, dur: f64) {
        self.order += 1;
        if self.mono {
            let was_empty = self.mono_stack.is_empty();
            self.mono_stack.push((note, vel));
            let v = &mut self.voices[0];
            if !v.active || was_empty || !self.legato {
                // retrigger: reset envelope nodes only (keep osc phase continuity when legato)
                if v.active && self.legato && !was_empty {
                    // legato with held note: no retrigger, just glide
                } else if v.active {
                    // retrigger envelopes IN PLACE, continuing from the current
                    // value (dropping the state restarted them at 0 mid-signal:
                    // the mono-retrigger click / sub "pipoco")
                    for s in v.state.slots.iter_mut() {
                        if let Some(NodeStateBox(NodeState::Env(st))) = s {
                            st.seg = 0;
                            st.t = 0.0;
                            st.seg_start_val = st.cur;
                            st.released = false;
                            st.in_release = false;
                            st.done = false;
                        }
                    }
                    v.t = 0.0;
                } else {
                    v.start(note, vel, dur, self.order, &mut self.rng, false);
                    v.note = note;
                }
            }
            v.active = true;
            v.gate = 1.0;
            v.vel = vel;
            v.dur = dur;
            v.target_note = note;
            if !v.active || was_empty && self.glide_s == 0.0 {
                v.note = note;
            }
            if self.glide_s <= 0.0 {
                v.note = note;
            }
        } else {
            // find free voice
            if let Some(v) = self.voices.iter_mut().find(|v| !v.active) {
                let order = self.order;
                v.start(note, vel, dur, order, &mut self.rng, false);
                return;
            }
            // steal
            let idx = match self.steal.as_str() {
                "newest" => self
                    .voices
                    .iter()
                    .enumerate()
                    .max_by_key(|(_, v)| v.start_order)
                    .map(|(i, _)| i)
                    .unwrap_or(0),
                "quietest" => self
                    .voices
                    .iter()
                    .enumerate()
                    .min_by(|a, b| a.1.last_out.partial_cmp(&b.1.last_out).unwrap())
                    .map(|(i, _)| i)
                    .unwrap_or(0),
                _ => self
                    .voices
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, v)| v.start_order)
                    .map(|(i, _)| i)
                    .unwrap_or(0),
            };
            let order = self.order;
            self.voices[idx].start(note, vel, dur, order, &mut self.rng, true);
        }
    }

    pub fn note_off(&mut self, note: f64) {
        if self.mono {
            self.mono_stack.retain(|(n, _)| (*n - note).abs() > 0.01);
            let v = &mut self.voices[0];
            if let Some(&(n, vel)) = self.mono_stack.last() {
                v.target_note = n;
                v.vel = vel;
                if self.glide_s <= 0.0 {
                    v.note = n;
                }
            } else {
                v.gate = 0.0;
            }
        } else {
            for v in self.voices.iter_mut() {
                if v.active && v.gate > 0.5 && (v.note - note).abs() < 0.01 {
                    v.gate = 0.0;
                }
            }
        }
    }

    pub fn set_seed(&mut self, seed: u64) {
        self.rng = seed.wrapping_mul(0x9E3779B97F4A7C15) ^ 0xDEADBEEFCAFE1234 | 1;
    }

    /// synth names this synth's bus reads (sidechain key: and any other synth
    /// reference). After resolve, the Idents left in bus args are exactly these
    /// cross-synth reads (enum keywords like shape:/color: are skipped).
    pub fn bus_reads(&self) -> Vec<String> {
        fn go(e: &Expr, out: &mut Vec<String>) {
            match e {
                Expr::Ident(n) => out.push(n.clone()),
                Expr::Bin { l, r, .. } => {
                    go(l, out);
                    go(r, out);
                }
                Expr::Neg(x) => go(x, out),
                Expr::Call { args, .. } => {
                    for (k, a) in args {
                        if crate::check::is_enum_arg(k) && matches!(a, Expr::Ident(_)) {
                            continue;
                        }
                        go(a, out);
                    }
                }
                _ => {}
            }
        }
        let mut out = Vec::new();
        for e in &self.def.bus {
            go(e, &mut out);
        }
        out
    }

    pub fn process_sample(&mut self) -> (f64, f64) {
        let empty = HashMap::new();
        self.process_sample_with(&empty)
    }

    pub fn process_sample_with(
        &mut self,
        synth_outs: &HashMap<String, (f64, f64)>,
    ) -> (f64, f64) {
        let mut out = std::mem::take(&mut self.sample_scratch);
        out.clear();
        self.process_chunk(1, synth_outs, &mut out);
        let r = out[0];
        self.sample_scratch = out;
        r
    }

    /// Process n samples with no note events inside (the renderer breaks chunks
    /// at event boundaries) and append n frames to out. Voices run on parallel
    /// threads for large chunks: each voice's sample sequence and the final
    /// per-sample summation order are identical to the sequential path, so the
    /// output is bit-exact either way.
    pub fn process_chunk(
        &mut self,
        n: usize,
        synth_outs: &HashMap<String, (f64, f64)>,
        out: &mut Vec<(f64, f64)>,
    ) {
        let nglobals = self.def.globals.len();
        // globals block (sequential; feeds every voice sample-by-sample)
        self.gblock.clear();
        if nglobals > 0 {
            for _ in 0..n {
                self.global_cur.clear();
                for (_, expr) in &self.def.globals {
                    let mut ctx = Ctx {
                        sr: self.sr,
                        bpm: self.bpm,
                        note: 60.0,
                        vel: 0.0,
                        gate: 0.0,
                        time: 0.0,
                        rand: 0.0,
                        vidx: 0.0,
                        dur: 0.0,
                        state: &mut self.global_state,
                        cur: &self.global_cur,
                        prev: &self.global_prev,
                        globals: &[],
                        params: &self.params,
                        bus_in: Val::S(0.0),
                        synth_outs: None,
                        params_by_name: None,
                        seed: 12345,
                    };
                    let v = eval(expr, &mut ctx);
                    self.global_cur.push(v);
                }
                self.global_prev.clear();
                self.global_prev.extend_from_slice(&self.global_cur);
                self.gblock.extend_from_slice(&self.global_cur);
            }
        }

        // voices (parallel when the chunk is long enough to amortize a spawn)
        {
            let sr = self.sr;
            let bpm = self.bpm;
            let mono = self.mono;
            let glide_s = self.glide_s;
            let spread = self.spread;
            let nvoices = self.voices.len();
            let def = &self.def;
            let params = &self.params[..];
            let gblock = &self.gblock[..];
            let n_active = self.voices.iter().filter(|v| v.active).count();
            if n >= 256 && n_active >= 2 {
                std::thread::scope(|sc| {
                    for v in self.voices.iter_mut() {
                        if !v.active {
                            v.blk.clear();
                            continue;
                        }
                        sc.spawn(move || {
                            voice_chunk(
                                v, def, sr, bpm, mono, glide_s, spread, nvoices, params,
                                gblock, nglobals, n,
                            );
                        });
                    }
                });
            } else {
                for v in self.voices.iter_mut() {
                    if !v.active {
                        v.blk.clear();
                        continue;
                    }
                    voice_chunk(
                        v, def, sr, bpm, mono, glide_s, spread, nvoices, params, gblock,
                        nglobals, n,
                    );
                }
            }
        }

        // per sample: sum voices in voice order, then run the bus chain
        let sr = self.sr;
        let bpm = self.bpm;
        let SynthInstance { def, voices, bus_state, params, params_named, .. } = self;
        let params = &params[..];
        let params_named = &*params_named;
        for i in 0..n {
            let mut l = 0.0;
            let mut r = 0.0;
            for v in voices.iter() {
                if let Some(&(vl, vr)) = v.blk.get(i) {
                    l += vl;
                    r += vr;
                }
            }
            // bus chain (input injected as _0: BusIn at load; sidechain key:
            // names resolve per sample against synth_outs, tier4 §2.2)
            let mut sig = Val::St2(l, r);
            for call in &def.bus {
                if let Expr::Call { op, args, id, .. } = call {
                    let mut ctx = Ctx {
                        sr,
                        bpm,
                        note: 60.0,
                        vel: 0.0,
                        gate: 0.0,
                        time: 0.0,
                        rand: 0.0,
                        vidx: 0.0,
                        dur: 0.0,
                        state: &mut *bus_state,
                        cur: &[],
                        prev: &[],
                        globals: &[],
                        params,
                        bus_in: sig,
                        synth_outs: Some(synth_outs),
                        params_by_name: Some(params_named),
                        seed: 777,
                    };
                    // +1_000_000 keeps the ids that seed bus-node rngs identical
                    // to the previous engine (bus state lives in its own store)
                    sig = eval_call(*op, args, *id + 1_000_000, &mut ctx);
                }
            }
            let (l, r) = sig.stereo();
            out.push((l * def.gain, r * def.gain));
        }
    }

    pub fn any_active(&self) -> bool {
        self.voices.iter().any(|v| v.active)
    }
}

/// one voice over one event-free chunk; writes chunk frames into v.blk.
/// Same per-sample sequence as the old interleaved loop - bit-exact.
#[allow(clippy::too_many_arguments)]
fn voice_chunk(
    v: &mut Voice,
    def: &SynthDef,
    sr: f64,
    bpm: f64,
    mono: bool,
    glide_s: f64,
    spread: f64,
    nvoices: usize,
    params: &[Val],
    gblock: &[Val],
    nglobals: usize,
    n: usize,
) {
    let dt = 1.0 / sr;
    let kill_after = def.kill_after;
    v.blk.clear();
    for i in 0..n {
        if !v.active {
            v.blk.push((0.0, 0.0));
            continue;
        }
        let gcur: &[Val] =
            if nglobals > 0 { &gblock[i * nglobals..(i + 1) * nglobals] } else { &[] };
        // mono glide (log-space: note is already log space)
        if mono && glide_s > 0.0 {
            let k = 1.0 - (-dt / (glide_s / 3.0)).exp();
            v.note += k * (v.target_note - v.note);
        }
        // mod matrix voice.pitch: summed into note (semitones);
        // runs before the lets, so let references read previous-sample values
        let note_eff = match &def.pitch_mod {
            Some(pm) => {
                let mut ctx = Ctx {
                    sr,
                    bpm,
                    note: v.note,
                    vel: v.vel,
                    gate: v.gate,
                    time: v.t,
                    dur: v.dur,
                    rand: v.rand,
                    vidx: v.idx as f64,
                    state: &mut v.state,
                    cur: &[],
                    prev: &v.prev,
                    globals: gcur,
                    params,
                    bus_in: Val::S(0.0),
                    synth_outs: None,
                    params_by_name: None,
                    seed: v.seed,
                };
                v.note + eval(pm, &mut ctx).num()
            }
            None => v.note,
        };
        // sequential lets into the reused scratch vec; each let sees slots 0..k
        let mut cur = std::mem::take(&mut v.cur);
        cur.clear();
        for (_, expr) in &def.voice {
            let mut ctx = Ctx {
                sr,
                bpm,
                note: note_eff,
                vel: v.vel,
                gate: v.gate,
                time: v.t,
                dur: v.dur,
                rand: v.rand,
                vidx: v.idx as f64,
                state: &mut v.state,
                cur: &cur,
                prev: &v.prev,
                globals: gcur,
                params,
                bus_in: Val::S(0.0),
                synth_outs: None,
                params_by_name: None,
                seed: v.seed,
            };
            let val = eval(expr, &mut ctx);
            cur.push(val);
        }
        let (ol, or) = {
            let mut ctx = Ctx {
                sr,
                bpm,
                note: note_eff,
                vel: v.vel,
                gate: v.gate,
                time: v.t,
                dur: v.dur,
                rand: v.rand,
                vidx: v.idx as f64,
                state: &mut v.state,
                cur: &cur,
                prev: &v.prev,
                globals: gcur,
                params,
                bus_in: Val::S(0.0),
                synth_outs: None,
                params_by_name: None,
                seed: v.seed,
            };
            eval(&def.out, &mut ctx).stereo()
        };
        let mut ol = ol;
        let mut or = or;
        // poly auto-spread: implicit equal-power pan per voice index
        if spread > 0.0 && !mono && nvoices > 1 {
            let nv = nvoices as f64;
            let pos = spread * (v.idx as f64 / (nv - 1.0) * 2.0 - 1.0);
            let a = (pos.clamp(-1.0, 1.0) + 1.0) * std::f64::consts::FRAC_PI_4;
            ol *= a.cos() * std::f64::consts::SQRT_2;
            or *= a.sin() * std::f64::consts::SQRT_2;
        }
        if v.fade_in > 0.0 {
            let f = 1.0 - (v.fade_in / 0.005);
            ol *= f;
            or *= f;
            v.fade_in -= dt;
        }
        // swap: cur becomes prev, old prev buffer becomes next scratch
        std::mem::swap(&mut v.prev, &mut cur);
        v.cur = cur;
        v.last_out = ol.abs().max(or.abs());
        v.t += dt;
        // voice kill logic
        let amp = ol.abs().max(or.abs());
        if amp > 3.16e-5 {
            v.has_sounded = true;
            v.silent_s = 0.0;
        } else if v.has_sounded || v.gate < 0.5 {
            v.silent_s += dt;
            if v.silent_s > 0.05 && (v.gate < 0.5 || v.has_sounded) {
                // only kill sustained-silence when gate off OR one-shot done
                if v.gate < 0.5 || (v.has_sounded && v.t > 0.2) {
                    v.active = false;
                }
            }
        }
        if let Some(ka) = kill_after {
            // declick: 8ms fade-out ramp before the hard kill
            let left = ka - v.t;
            if left < 0.008 {
                let f = (left / 0.008).clamp(0.0, 1.0);
                ol *= f;
                or *= f;
            }
            if v.t > ka {
                v.active = false;
            }
        }
        v.blk.push((ol, or));
    }
}

// ---------- master chain (tier1 §4): applied to the sum of all synths ----------

pub struct MasterChain {
    pub gain: f64,
    chain: Vec<Expr>,
    state: StateStore,
    sr: f64,
    bpm: f64,
}

impl MasterChain {
    pub fn new(def: crate::parser::MasterDef, sr: f64, bpm: f64) -> Self {
        let mut chain = def.chain;
        crate::resolve::resolve_master(&mut chain);
        MasterChain { gain: def.gain, chain, state: StateStore::new(), sr, bpm }
    }

    pub fn process(&mut self, l: f64, r: f64) -> (f64, f64) {
        let mut sig = Val::St2(l * self.gain, r * self.gain);
        for call in &self.chain {
            if let Expr::Call { op, args, id, .. } = call {
                let mut ctx = Ctx {
                    sr: self.sr,
                    bpm: self.bpm,
                    note: 60.0,
                    vel: 0.0,
                    gate: 0.0,
                    time: 0.0,
                    rand: 0.0,
                    vidx: 0.0,
                    dur: 0.0,
                    state: &mut self.state,
                    cur: &[],
                    prev: &[],
                    globals: &[],
                    params: &[],
                    bus_in: sig,
                    synth_outs: None,
                    params_by_name: None,
                    seed: 4242,
                };
                // +2_000_000 keeps master node ids (rng seeds) unchanged
                sig = eval_call(*op, args, *id + 2_000_000, &mut ctx);
            }
        }
        sig.stereo()
    }
}

// ---------- wavetable (tier2 §B.2): FFT mipmaps, morph, Hermite read ----------

use std::sync::{Arc, Mutex, OnceLock};

pub struct WTable {
    // frames[frame][mip][sample]; every mip is 2048 samples, band-limited by octave
    pub frames: Vec<Vec<Vec<f64>>>,
    pub mips: usize,
}

const WT_LEN: usize = 2048;
const WT_MIPS: usize = 10;

// radix-2 iterative FFT (in-place, no deps)
pub fn fft(re: &mut [f64], im: &mut [f64], inverse: bool) {
    let n = re.len();
    let mut j = 0usize;
    for i in 0..n {
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
        let mut m = n >> 1;
        while m >= 1 && j & m != 0 {
            j ^= m;
            m >>= 1;
        }
        j |= m;
    }
    let sign = if inverse { 1.0 } else { -1.0 };
    let mut len = 2;
    while len <= n {
        let ang = sign * 2.0 * std::f64::consts::PI / len as f64;
        let (wr, wi) = (ang.cos(), ang.sin());
        let mut i = 0;
        while i < n {
            let (mut cr, mut ci) = (1.0, 0.0);
            for k in 0..len / 2 {
                let (ar, ai) = (re[i + k], im[i + k]);
                let (br, bi) = (re[i + k + len / 2], im[i + k + len / 2]);
                let (tr, ti) = (br * cr - bi * ci, br * ci + bi * cr);
                re[i + k] = ar + tr;
                im[i + k] = ai + ti;
                re[i + k + len / 2] = ar - tr;
                im[i + k + len / 2] = ai - ti;
                let ncr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = ncr;
            }
            i += len;
        }
        len <<= 1;
    }
    if inverse {
        let inv = 1.0 / n as f64;
        for i in 0..n {
            re[i] *= inv;
            im[i] *= inv;
        }
    }
}

fn build_mips(frame: &[f64]) -> Vec<Vec<f64>> {
    // mip k keeps harmonics up to WT_LEN/2 / 2^k (bin cut, exact-period table: no window)
    let mut re: Vec<f64> = frame.to_vec();
    let mut im = vec![0.0; WT_LEN];
    fft(&mut re, &mut im, false);
    let mut out = Vec::with_capacity(WT_MIPS);
    for k in 0..WT_MIPS {
        let max_h = (WT_LEN / 2) >> k;
        let mut r2 = re.clone();
        let mut i2 = im.clone();
        for bin in 0..WT_LEN {
            let h = if bin <= WT_LEN / 2 { bin } else { WT_LEN - bin };
            if h > max_h.max(1) {
                r2[bin] = 0.0;
                i2[bin] = 0.0;
            }
        }
        fft(&mut r2, &mut i2, true);
        out.push(r2);
    }
    out
}

fn make_table_from_frames(raw: Vec<Vec<f64>>) -> Arc<WTable> {
    let frames: Vec<Vec<Vec<f64>>> = raw.iter().map(|f| build_mips(f)).collect();
    Arc::new(WTable { frames, mips: WT_MIPS })
}

fn builtin_table(name: &str) -> Option<Arc<WTable>> {
    let tau = 2.0 * std::f64::consts::PI;
    let gen = |f: &dyn Fn(f64) -> f64| -> Vec<f64> {
        (0..WT_LEN).map(|i| f(i as f64 / WT_LEN as f64)).collect()
    };
    match name {
        "basic_shapes" => {
            // sine -> tri -> saw -> square
            let sine = gen(&|p| (tau * p).sin());
            let tri = gen(&|p| 1.0 - 4.0 * (p - 0.5).abs());
            let saw = gen(&|p| 2.0 * p - 1.0);
            let sq = gen(&|p| if p < 0.5 { 1.0 } else { -1.0 });
            Some(make_table_from_frames(vec![sine, tri, saw, sq]))
        }
        "digital" => {
            // odd vs even harmonic combs
            let odd = gen(&|p| {
                let mut s = 0.0;
                for h in (1..64).step_by(2) {
                    s += (tau * p * h as f64).sin() / h as f64;
                }
                s * 0.7
            });
            let even = gen(&|p| {
                let mut s = (tau * p).sin() * 0.3;
                for h in (2..64).step_by(2) {
                    s += (tau * p * h as f64).sin() / h as f64;
                }
                s * 0.7
            });
            Some(make_table_from_frames(vec![odd, even]))
        }
        "vox" => {
            // 5 vowel-ish formant frames (a e i o u), sum of harmonics shaped by 2 formant peaks
            let vowels: [(f64, f64); 5] =
                [(800.0, 1150.0), (400.0, 2000.0), (250.0, 2300.0), (400.0, 800.0), (350.0, 600.0)];
            let f0 = 110.0; // shaping reference fundamental
            let mut frames = Vec::new();
            for (f1, f2) in vowels {
                let frame = gen(&|p| {
                    let mut s = 0.0;
                    for h in 1..48 {
                        let fh = f0 * h as f64;
                        let a1 = (-((fh - f1) / 90.0).powi(2)).exp();
                        let a2 = 0.6 * (-((fh - f2) / 120.0).powi(2)).exp();
                        s += (tau * p * h as f64).sin() * (a1 + a2 + 0.02 / h as f64);
                    }
                    s * 0.5
                });
                frames.push(frame);
            }
            Some(make_table_from_frames(frames))
        }
        _ => None,
    }
}

fn table_registry() -> &'static Mutex<HashMap<String, Arc<WTable>>> {
    static REG: OnceLock<Mutex<HashMap<String, Arc<WTable>>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn get_table(name: &str) -> Option<Arc<WTable>> {
    let reg = table_registry();
    if let Some(t) = reg.lock().unwrap().get(name) {
        return Some(t.clone());
    }
    let t = if name.ends_with(".wav") {
        // .wav table: 2048 samples per frame, frames concatenated
        let (data, _sr, ch) = crate::wavio::read_wav(name).ok()?;
        let mono: Vec<f64> = if ch == 2 {
            data.chunks(2).map(|c| 0.5 * (c[0] + c[1])).collect()
        } else {
            data
        };
        if mono.len() < WT_LEN || mono.len() % WT_LEN != 0 {
            return None; // E022 (size not multiple of 2048)
        }
        let frames: Vec<Vec<f64>> = mono.chunks(WT_LEN).map(|c| c.to_vec()).collect();
        Some(make_table_from_frames(frames))
    } else {
        builtin_table(name)
    }?;
    reg.lock().unwrap().insert(name.to_string(), t.clone());
    Some(t)
}

fn wt_read(tab: &WTable, frame_pos: f64, mip_f: f64, phase: f64) -> f64 {
    let nf = tab.frames.len();
    let fpos = frame_pos.clamp(0.0, 1.0) * (nf - 1) as f64;
    let f0 = fpos.floor() as usize;
    let f1 = (f0 + 1).min(nf - 1);
    let ffr = fpos - f0 as f64;
    let m0 = (mip_f.floor() as usize).min(tab.mips - 1);
    let m1 = (m0 + 1).min(tab.mips - 1);
    let mfr = (mip_f - m0 as f64).clamp(0.0, 1.0);
    let read_one = |mip: &Vec<f64>| -> f64 {
        let x = phase * WT_LEN as f64;
        let i0 = x.floor() as usize % WT_LEN;
        let fr = x - x.floor();
        let im1 = (i0 + WT_LEN - 1) % WT_LEN;
        let i1 = (i0 + 1) % WT_LEN;
        let i2 = (i0 + 2) % WT_LEN;
        hermite(mip[im1], mip[i0], mip[i1], mip[i2], fr)
    };
    let blend = |fi: usize| -> f64 {
        let a = read_one(&tab.frames[fi][m0]);
        let b = read_one(&tab.frames[fi][m1]);
        a + (b - a) * mfr
    };
    let va = blend(f0);
    let vb = blend(f1);
    va + (vb - va) * ffr
}

// ---------- convolve (fase 4): IRs rendered from synth defs, zero audio files ----------

fn defs_registry() -> &'static Mutex<HashMap<String, SynthDef>> {
    static REG: OnceLock<Mutex<HashMap<String, SynthDef>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(HashMap::new()))
}

/// render.rs registers every parsed def so convolve(ir: name) can render it
pub fn register_defs(defs: &[SynthDef]) {
    let mut reg = defs_registry().lock().unwrap();
    for d in defs {
        reg.insert(d.name.clone(), d.clone());
    }
}

fn ir_registry() -> &'static Mutex<HashMap<String, Arc<Vec<f64>>>> {
    static REG: OnceLock<Mutex<HashMap<String, Arc<Vec<f64>>>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(HashMap::new()))
}

thread_local! {
    static IR_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// render a synth def to a mono impulse response: one c4 note at full velocity,
/// fixed seed (the IR is a deterministic property of the patch), energy
/// normalized so convolution preserves overall level
pub fn render_ir(name: &str, dur_s: f64, sr: f64) -> Option<Arc<Vec<f64>>> {
    let key = format!("{}|{}|{}", name, (dur_s * 1000.0) as u64, sr as u64);
    if let Some(ir) = ir_registry().lock().unwrap().get(&key) {
        return Some(ir.clone());
    }
    let def = defs_registry().lock().unwrap().get(name)?.clone();
    let depth = IR_DEPTH.with(|d| d.get());
    if depth >= 1 {
        eprintln!("E033: convolve ir '{}' uses convolve itself - not supported", name);
        return None;
    }
    IR_DEPTH.with(|d| d.set(depth + 1));
    let mut inst = SynthInstance::new(def, sr, 120.0);
    inst.set_seed(42);
    inst.note_on(60.0, 1.0);
    let n = (dur_s * sr).ceil() as usize;
    let empty = HashMap::new();
    let mut buf: Vec<(f64, f64)> = Vec::with_capacity(n);
    inst.process_chunk(n, &empty, &mut buf);
    IR_DEPTH.with(|d| d.set(depth));
    let mut ir: Vec<f64> = buf.iter().map(|(l, r)| 0.5 * (l + r)).collect();
    let e: f64 = ir.iter().map(|v| v * v).sum();
    if e < 1e-12 {
        eprintln!("E033: convolve ir '{}' rendered silence", name);
        return None;
    }
    let g = 1.0 / e.sqrt();
    for v in ir.iter_mut() {
        *v *= g;
    }
    let arc = Arc::new(ir);
    ir_registry().lock().unwrap().insert(key, arc.clone());
    Some(arc)
}

const CONV_B: usize = 512; // partition size (2b = 1024-point FFTs)

fn conv_partition(ir: &[f64], b: usize) -> (Vec<f64>, Vec<(Vec<f64>, Vec<f64>)>) {
    let n = ir.len();
    let mut head = vec![0.0; b];
    head[..b.min(n)].copy_from_slice(&ir[..b.min(n)]);
    // partitions k >= 1: fft of ir[kb..(k+1)b] zero-padded to 2b
    let mut parts = Vec::new();
    let mut k = 1;
    while k * b < n {
        let mut re = vec![0.0; 2 * b];
        let end = ((k + 1) * b).min(n);
        re[..end - k * b].copy_from_slice(&ir[k * b..end]);
        let mut im = vec![0.0; 2 * b];
        fft(&mut re, &mut im, false);
        parts.push((re, im));
        k += 1;
    }
    (head, parts)
}

fn make_conv(name: &str, name2: Option<&str>, dur_s: f64, sr: f64) -> ConvState {
    let empty_ch =
        || ([Vec::new(), Vec::new()], [Vec::new(), Vec::new()], [Vec::new(), Vec::new()]);
    let ir = match render_ir(name, dur_s, sr) {
        Some(ir) => ir,
        None => {
            let (a, b, c) = empty_ch();
            return ConvState {
                b: CONV_B,
                ir_head: a,
                parts: [Vec::new(), Vec::new()],
                in_ring: [Vec::new(), Vec::new()],
                fdl: [Vec::new(), Vec::new()],
                fdl_w: [0, 0],
                ytime: b,
                overlap: c,
                pos: 0,
                dead: true,
            };
        }
    };
    // ir2: independent right-channel IR (stereo decorrelation for bodies)
    let ir_r = name2.and_then(|n2| render_ir(n2, dur_s, sr)).unwrap_or_else(|| ir.clone());
    let b = CONV_B;
    let (head_l, parts_l) = conv_partition(&ir, b);
    let (head_r, parts_r) = conv_partition(&ir_r, b);
    let np = parts_l.len().max(parts_r.len());
    let zero_spec = || (vec![0.0; 2 * b], vec![0.0; 2 * b]);
    ConvState {
        b,
        ir_head: [head_l, head_r],
        parts: [parts_l, parts_r],
        in_ring: [vec![0.0; b], vec![0.0; b]],
        fdl: [
            (0..np.max(1)).map(|_| zero_spec()).collect(),
            (0..np.max(1)).map(|_| zero_spec()).collect(),
        ],
        fdl_w: [0, 0],
        ytime: [vec![0.0; b], vec![0.0; b]],
        overlap: [vec![0.0; b], vec![0.0; b]],
        pos: 0,
        dead: false,
    }
}

impl ConvState {
    /// one input sample on channel ch: direct FIR head + ready FFT-tail block
    #[inline]
    fn tick(&mut self, ch: usize, x: f64) -> f64 {
        let b = self.b;
        self.in_ring[ch][self.pos] = x;
        let ring = &self.in_ring[ch];
        let head = &self.ir_head[ch];
        let mut acc = 0.0;
        // direct part: lags 0..b-1 (split the ring at pos to avoid modulo)
        for k in 0..=self.pos {
            acc += head[k] * ring[self.pos - k];
        }
        for k in self.pos + 1..b {
            acc += head[k] * ring[self.pos + b - k];
        }
        acc + self.ytime[ch][self.pos]
    }

    /// call once per sample after both channels ticked
    fn advance(&mut self) {
        self.pos += 1;
        if self.pos < self.b {
            return;
        }
        self.pos = 0;
        let b = self.b;
        if self.parts[0].is_empty() && self.parts[1].is_empty() {
            return;
        }
        for ch in 0..2 {
            // spectrum of the block just completed
            let mut re = vec![0.0; 2 * b];
            re[..b].copy_from_slice(&self.in_ring[ch]);
            let mut im = vec![0.0; 2 * b];
            fft(&mut re, &mut im, false);
            let np = self.fdl[ch].len();
            self.fdl[ch][self.fdl_w[ch]] = (re, im);
            // frequency-domain delay line: partition k pairs with the block
            // pushed k-1 steps ago; accumulate, one IFFT for the whole tail
            let mut yr = vec![0.0; 2 * b];
            let mut yi = vec![0.0; 2 * b];
            for (k, (hr, hi)) in self.parts[ch].iter().enumerate() {
                let idx = (self.fdl_w[ch] + np - k) % np;
                let (xr, xi) = &self.fdl[ch][idx];
                for i in 0..2 * b {
                    yr[i] += xr[i] * hr[i] - xi[i] * hi[i];
                    yi[i] += xr[i] * hi[i] + xi[i] * hr[i];
                }
            }
            self.fdl_w[ch] = (self.fdl_w[ch] + 1) % np;
            fft(&mut yr, &mut yi, true);
            for i in 0..b {
                self.ytime[ch][i] = yr[i] + self.overlap[ch][i];
                self.overlap[ch][i] = yr[b + i];
            }
        }
    }
}

// ---------- SDN hall: scattering delay network room (survey 2.9.2) ----------

const HALL_N: usize = 6;

fn make_hall(size: f64, decay_s: f64, sr: f64) -> HallState {
    // shoebox: studio (size 0) .. concert hall (size 1)
    let sc = 0.55 + 1.75 * size;
    let (w, l, h) = (6.4 * sc, 8.2 * sc, 4.8 * sc);
    let src = [0.42 * w, 0.30 * l, 0.42 * h];
    let lst = [0.50 * w, 0.64 * l, 0.40 * h];
    let ears = [[lst[0] - 0.09, lst[1], lst[2]], [lst[0] + 0.09, lst[1], lst[2]]];
    // walls: (axis, plane coordinate)
    let walls: [(usize, f64); 6] =
        [(0, 0.0), (0, w), (1, 0.0), (1, l), (2, 0.0), (2, h)];
    // first-reflection point on each wall: image-source construction
    let mut nodes = [[0.0f64; 3]; HALL_N];
    for (k, (ax, plane)) in walls.iter().enumerate() {
        let mut img = src;
        img[*ax] = 2.0 * plane - src[*ax];
        let t = if (lst[*ax] - img[*ax]).abs() > 1e-9 {
            (plane - img[*ax]) / (lst[*ax] - img[*ax])
        } else {
            0.5
        };
        let t = t.clamp(0.05, 0.95);
        for a in 0..3 {
            nodes[k][a] = img[a] + t * (lst[a] - img[a]);
        }
        nodes[k][*ax] = *plane;
    }
    let dist = |a: &[f64; 3], b: &[f64; 3]| -> f64 {
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
    };
    let spd = sr / 343.0;
    let mut dline = vec![0.0; HALL_N * HALL_N];
    let mut lines: Vec<Vec<Vec<f64>>> = Vec::with_capacity(HALL_N);
    for k in 0..HALL_N {
        let mut row = Vec::with_capacity(HALL_N);
        for j in 0..HALL_N {
            if j == k {
                row.push(vec![0.0; 4]);
                continue;
            }
            let d = (dist(&nodes[k], &nodes[j]) * spd).max(2.0);
            dline[k * HALL_N + j] = d;
            row.push(vec![0.0; d.ceil() as usize + 8]);
        }
        lines.push(row);
    }
    let mut dsrc = Vec::new();
    let mut gsrc = Vec::new();
    let mut srcl = Vec::new();
    let mut dear = [Vec::new(), Vec::new()];
    let mut gear = [Vec::new(), Vec::new()];
    let mut max_ear = 0.0f64;
    for k in 0..HALL_N {
        let ds = dist(&src, &nodes[k]);
        dsrc.push((ds * spd).max(2.0));
        gsrc.push(1.0 / ds.max(0.5));
        srcl.push(vec![0.0; (ds * spd).ceil() as usize + 8]);
        for e in 0..2 {
            let de = dist(&nodes[k], &ears[e]);
            let d = (de * spd).max(1.0);
            dear[e].push(d);
            // 1/(ds+de) total spreading for the first-order path
            gear[e].push(ds.max(0.5) / (ds + de).max(1.0));
            max_ear = max_ear.max(d);
        }
    }
    // wall absorption from target T60: g per bounce along the mean free path
    let vol = w * l * h;
    let surf = 2.0 * (w * l + w * h + l * h);
    let mfp = 4.0 * vol / surf;
    let bps = 343.0 / mfp;
    let g_wall = 10f64.powf(-3.0 / (decay_s * bps).max(0.05)).min(0.9999);
    HallState {
        lines,
        lw: 0,
        dline,
        src: srcl,
        dsrc,
        gsrc,
        dear,
        gear,
        ebuf: [
            vec![0.0; max_ear.ceil() as usize + 8],
            vec![0.0; max_ear.ceil() as usize + 8],
        ],
        g_wall: vec![g_wall; HALL_N],
        damp: vec![0.0; HALL_N * HALL_N],
        p_in: vec![[0.0; HALL_N]; HALL_N],
    }
}

impl HallState {
    fn tick(&mut self, x: f64, kd: f64) -> (f64, f64) {
        let lw = self.lw;
        // 1. collect this sample's ear pressure (accumulated by past writes)
        let el = self.ebuf[0].len();
        let er = self.ebuf[1].len();
        let wl = std::mem::replace(&mut self.ebuf[0][lw % el], 0.0);
        let wr = std::mem::replace(&mut self.ebuf[1][lw % er], 0.0);
        // 2. push the source sample into the source->node lines
        for k in 0..HALL_N {
            let n = self.src[k].len();
            self.src[k][lw % n] = x;
        }
        // 3. read all incoming waves (node<->node) + source injection
        for k in 0..HALL_N {
            let sn = self.src[k].len();
            let ps = ring_read(&self.src[k], lw % sn, self.dsrc[k]) * self.gsrc[k];
            for j in 0..HALL_N {
                if j == k {
                    continue;
                }
                let buf = &self.lines[j][k];
                let n = buf.len();
                let d = self.dline[j * HALL_N + k].min(n as f64 - 4.0);
                // source pressure splits evenly onto the incoming variables
                self.p_in[k][j] = ring_read(buf, lw % n, d) + 0.5 * ps;
            }
        }
        // 4. scatter at each node: S = 2/(N-1)*J - I (lossless), then wall
        // absorption + per-line damping lowpass on the outgoing waves
        for k in 0..HALL_N {
            let mut sum = 0.0;
            for j in 0..HALL_N {
                if j != k {
                    sum += self.p_in[k][j];
                }
            }
            let pk = 2.0 / (HALL_N as f64 - 1.0) * sum;
            let gw = self.g_wall[k];
            for j in 0..HALL_N {
                if j == k {
                    continue;
                }
                let y = (pk - self.p_in[k][j]) * gw;
                let ds = &mut self.damp[k * HALL_N + j];
                *ds = flush_denorm(*ds + kd * (y - *ds));
                let buf = &mut self.lines[k][j];
                let n = buf.len();
                buf[lw % n] = *ds;
            }
            // node pressure travels on to both ears (write-ahead accumulate)
            for e in 0..2 {
                let d = self.dear[e][k].ceil() as usize;
                let n = self.ebuf[e].len();
                self.ebuf[e][(lw + d.max(1)) % n] += pk * self.gear[e][k] * gw;
            }
        }
        self.lw = self.lw.wrapping_add(1);
        (wl, wr)
    }
}

// ---------- sample playback (tier4 §1): shared cache, Hermite repitch ----------

pub struct SampleData {
    pub data: Vec<f64>, // interleaved
    pub ch: usize,
    pub sr: f64,
}

fn sample_registry() -> &'static Mutex<HashMap<String, Arc<SampleData>>> {
    static REG: OnceLock<Mutex<HashMap<String, Arc<SampleData>>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn get_sample(path: &str) -> Option<Arc<SampleData>> {
    let reg = sample_registry();
    if let Some(s) = reg.lock().unwrap().get(path) {
        return Some(s.clone());
    }
    let (data, sr, ch) = crate::wavio::read_wav(path).ok()?;
    // E030: 64MB cap per patch (f64 in-memory)
    if data.len() * 8 > 64 * 1024 * 1024 {
        eprintln!("E030: sample {} exceeds 64MB cap", path);
        return None;
    }
    let s = Arc::new(SampleData { data, ch, sr: sr as f64 });
    reg.lock().unwrap().insert(path.to_string(), s.clone());
    Some(s)
}

fn sample_frame(s: &SampleData, pos: f64) -> (f64, f64) {
    let nf = (s.data.len() / s.ch) as isize;
    if nf < 4 {
        return (0.0, 0.0);
    }
    let i0 = pos.floor() as isize;
    let fr = pos - pos.floor();
    let get = |i: isize, c: usize| -> f64 {
        let i = i.clamp(0, nf - 1) as usize;
        s.data[i * s.ch + c.min(s.ch - 1)]
    };
    let l = hermite(get(i0 - 1, 0), get(i0, 0), get(i0 + 1, 0), get(i0 + 2, 0), fr);
    if s.ch == 2 {
        let r = hermite(get(i0 - 1, 1), get(i0, 1), get(i0 + 1, 1), get(i0 + 2, 1), fr);
        (l, r)
    } else {
        (l, l)
    }
}
