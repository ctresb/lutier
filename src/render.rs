// Offline render: synth defs + score tracks -> stereo f64 buffer.
use crate::engine::{MasterChain, SynthInstance};
use crate::score::{parse_score, Ev, TimedEv};
use crate::{lexer, parser};

pub struct RenderResult {
    pub buf: Vec<(f64, f64)>,
    pub sr: f64,
    pub render_seconds: f64, // wall-clock time spent in the sample loop
}

pub fn render_song(
    synth_src: &str,
    score_src: &str,
    sr: f64,
    seed: u64,
) -> Result<RenderResult, String> {
    let (toks, spans) = lexer::lex_spanned(synth_src)?;    let file = parser::Parser::new_spanned(toks, spans).parse_file()?;
    let diags = crate::check::check_file(&file);
    let mut fatal = false;
    for d in &diags {
        eprintln!("{}", d);
        if d.starts_with('E') {
            fatal = true;
        }
    }
    if fatal {
        return Err("compilation failed (errors above)".into());
    }
    let defs = file.defs;
    // convolve(ir: name) renders other defs lazily; make them all reachable
    crate::engine::register_defs(&defs);
    let (bpm, tracks) = parse_score(score_src, sr)?;

    struct Track {
        inst: SynthInstance,
        evs: Vec<TimedEv>,
        cursor: usize,
        blk: Vec<(f64, f64)>,
    }

    fn apply_events(tr: &mut Track, s: u64) {
        while tr.cursor < tr.evs.len() && tr.evs[tr.cursor].sample <= s {
            match &tr.evs[tr.cursor].ev {
                Ev::On(n, v) => tr.inst.note_on(*n, *v),
                Ev::Off(n) => tr.inst.note_off(*n),
                Ev::Param(name, v) => tr.inst.set_param(name, *v),
                Ev::Bpm(v) => tr.inst.bpm = *v,
            }
            tr.cursor += 1;
        }
    }

    let mut tracks_v: Vec<Track> = Vec::new();
    for tr in &tracks {
        let def = defs
            .iter()
            .find(|d| d.name == tr.synth)
            .ok_or(format!("track references unknown synth '{}'", tr.synth))?
            .clone();
        let mut inst = SynthInstance::new(def, sr, bpm);
        inst.set_seed(seed);
        tracks_v.push(Track { inst, evs: tr.events.clone(), cursor: 0, blk: Vec::new() });
    }
    // producers (bus reads nothing) first, consumers (sidechain key: etc) after,
    // both keeping track order; consumers then see producers' this-sample values
    tracks_v.sort_by_key(|t| if t.inst.bus_reads().is_empty() { 0 } else { 1 });
    let n_prod = tracks_v.iter().filter(|t| t.inst.bus_reads().is_empty()).count();

    let mut master = file.master.map(|m| MasterChain::new(m, sr, bpm));

    let last = tracks_v
        .iter()
        .flat_map(|t| t.evs.iter().map(|e| e.sample))
        .max()
        .unwrap_or(0);
    let total = last + (sr * 4.0) as u64; // 4s tail

    let t_start = std::time::Instant::now();
    let mut buf: Vec<(f64, f64)> = Vec::with_capacity(total as usize);
    // consumers read this-sample producer values through this map (updated per sample)
    let mut synth_outs: std::collections::HashMap<String, (f64, f64)> =
        tracks_v.iter().map(|t| (t.inst.def.name.clone(), (0.0, 0.0))).collect();
    let empty_outs: std::collections::HashMap<String, (f64, f64)> =
        std::collections::HashMap::new();
    // block processing: producers are independent within a block (events applied
    // per sample inside), so they render their block buffers on parallel threads.
    // Consumers then run per sample over the block reading producer values, which
    // reproduces the sequential engine bit-for-bit.
    const BLOCK: usize = 8192;
    let mut bs = 0u64;
    while bs < total {
        let n = (BLOCK as u64).min(total - bs) as usize;
        let (prods, cons) = tracks_v.split_at_mut(n_prod);
        std::thread::scope(|sc| {
            for tr in prods.iter_mut() {
                let empty_outs = &empty_outs;
                sc.spawn(move || {
                    // chunked: events land on chunk boundaries, voices run in
                    // parallel inside process_chunk for long event-free spans
                    tr.blk.clear();
                    let mut i = 0usize;
                    while i < n {
                        let s = bs + i as u64;
                        apply_events(tr, s);
                        let next_ev =
                            tr.evs.get(tr.cursor).map(|e| e.sample).unwrap_or(u64::MAX);
                        let chunk =
                            (next_ev.saturating_sub(s).min((n - i) as u64) as usize).max(1);
                        tr.inst.process_chunk(chunk, empty_outs, &mut tr.blk);
                        i += chunk;
                    }
                });
            }
        });
        if !cons.is_empty() {
            for tr in cons.iter_mut() {
                tr.blk.clear();
            }
            for i in 0..n {
                for tr in prods.iter() {
                    if let Some(slot) = synth_outs.get_mut(&tr.inst.def.name) {
                        *slot = tr.blk[i];
                    }
                }
                for tr in cons.iter_mut() {
                    apply_events(tr, bs + i as u64);
                    let out = tr.inst.process_sample_with(&synth_outs);
                    if let Some(slot) = synth_outs.get_mut(&tr.inst.def.name) {
                        *slot = out;
                    }
                    tr.blk.push(out);
                }
            }
        }
        // sum in track order (same addition order as the sequential loop)
        for i in 0..n {
            let mut l = 0.0;
            let mut r = 0.0;
            for tr in tracks_v.iter() {
                l += tr.blk[i].0;
                r += tr.blk[i].1;
            }
            buf.push((l, r));
        }
        bs += n as u64;
    }
    let render_seconds = t_start.elapsed().as_secs_f64();

    let peak = buf.iter().map(|(l, r)| l.abs().max(r.abs())).fold(0.0f64, f64::max);
    if let Some(m) = master.as_mut() {
        // master chain present: peak normalize becomes ~-6dbfs pre-gain; limiter holds the rest
        if peak > 0.0 {
            let g = 0.5 / peak;
            for s in buf.iter_mut() {
                s.0 *= g;
                s.1 *= g;
            }
        }
        for s in buf.iter_mut() {
            let (l, r) = m.process(s.0, s.1);
            s.0 = l;
            s.1 = r;
        }
    } else if peak > 0.0 {
        let g = 0.891 / peak;
        for s in buf.iter_mut() {
            s.0 *= g;
            s.1 *= g;
        }
    }
    Ok(RenderResult { buf, sr, render_seconds })
}
