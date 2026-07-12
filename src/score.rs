// .score parsing: tempo map, sections, swing, humanize, automation.
#[derive(Debug, Clone)]
pub enum Ev {
    On(f64, f64, f64), // note, vel, dur_s (scheduled length; voices read it as `dur`)
    Off(f64),
    Param(String, f64),
    Bpm(f64),
}

#[derive(Debug, Clone)]
pub struct TimedEv {
    pub sample: u64,
    pub ev: Ev,
}

pub struct Track {
    pub synth: String,
    pub events: Vec<TimedEv>,
}

/// automacao de canal do mixer: eventos Param (gain/pan/send.<canal> em db,
/// params de canal em valor cru) + Bpm broadcast
pub struct MixTrack {
    pub channel: String,
    pub events: Vec<TimedEv>,
}

pub fn note_name_to_midi(s: &str) -> Result<f64, String> {
    let b = s.as_bytes();
    if b.is_empty() {
        return Err("empty note".into());
    }
    let base = match b[0].to_ascii_lowercase() {
        b'c' => 0,
        b'd' => 2,
        b'e' => 4,
        b'f' => 5,
        b'g' => 7,
        b'a' => 9,
        b'b' => 11,
        _ => return Err(format!("bad note: {}", s)),
    };
    let mut i = 1;
    let mut acc = 0i32;
    while i < b.len() && (b[i] == b'#' || b[i] == b'b') {
        acc += if b[i] == b'#' { 1 } else { -1 };
        i += 1;
    }
    let oct: i32 = s[i..].parse().map_err(|_| format!("bad octave in {}", s))?;
    Ok((12 * (oct + 1) + base + acc) as f64)
}

// ---- score v2 (tier2 §A): tempo map, sections/arrange, chords, swing, humanize, automate ----

#[derive(Debug, Clone, Copy, PartialEq)]
enum ACurve {
    Lin,
    Exp,
    Log,
    Pow(f64),
}

fn acurve_val(start: f64, target: f64, p: f64, curve: ACurve) -> f64 {
    let p = p.clamp(0.0, 1.0);
    match curve {
        ACurve::Lin => start + (target - start) * p,
        ACurve::Exp => target + (start - target) * (-6.9 * p).exp(),
        ACurve::Log => start + (target - start) * ((1.0 + 9.0 * p).ln() / 10f64.ln()),
        ACurve::Pow(n) => start + (target - start) * p.powf(n),
    }
}

/// piecewise-constant tempo: (beat, bpm) points, first at beat 0
struct TempoMap {
    points: Vec<(f64, f64)>,
}

impl TempoMap {
    fn time_of_beat(&self, beat: f64) -> f64 {
        let mut t = 0.0;
        for i in 0..self.points.len() {
            let (b0, bpm) = self.points[i];
            let b1 = self.points.get(i + 1).map(|p| p.0).unwrap_or(f64::INFINITY);
            if beat <= b0 {
                break;
            }
            let span = beat.min(b1) - b0;
            t += span * 60.0 / bpm;
            if beat <= b1 {
                break;
            }
        }
        t
    }
    fn bpm_at(&self, beat: f64) -> f64 {
        let mut bpm = self.points.first().map(|p| p.1).unwrap_or(120.0);
        for &(b, v) in &self.points {
            if beat >= b {
                bpm = v;
            }
        }
        bpm
    }
}

#[derive(Debug, Clone)]
struct NoteEv {
    track: usize,
    beat: f64,
    notes: Vec<f64>,
    dur: f64,
    vel: f64,
}

#[derive(Debug, Clone)]
struct AutoPoint {
    beat: f64,
    value: f64,
    curve: ACurve, // curve of the segment ENDING at this point
}

#[derive(Debug, Clone)]
struct Automation {
    track: usize,
    param: String,
    points: Vec<AutoPoint>,
}

fn split_u64(s: u64) -> u64 {
    // splitmix64 for deterministic humanize
    let mut z = s.wrapping_add(0x9E3779B97F4A7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

fn uniform_pm1(seed: u64) -> f64 {
    (split_u64(seed) >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
}

fn parse_note_token(s: &str) -> Result<f64, String> {
    note_name_to_midi(s)
}

pub fn parse_score(src: &str, sr: f64) -> Result<(f64, Vec<Track>, Vec<MixTrack>), String> {
    // pass 1: tempo points
    let mut tempo_points: Vec<(f64, f64)> = Vec::new();
    let clean_line = |raw: &str| -> String {
        let mut cut = raw.len();
        for (i, c) in raw.char_indices() {
            if c == '#' && (i == 0 || raw.as_bytes()[i - 1].is_ascii_whitespace()) {
                cut = i;
                break;
            }
        }
        raw[..cut].trim().to_string()
    };
    for raw in src.lines() {
        let line = clean_line(raw);
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.first() == Some(&"tempo") {
            match parts.len() {
                2 => tempo_points.push((0.0, parts[1].parse().map_err(|_| "bad tempo")?)),
                3 => tempo_points.push((
                    parts[1].parse().map_err(|_| "bad tempo beat")?,
                    parts[2].parse().map_err(|_| "bad tempo bpm")?,
                )),
                _ => return Err("tempo: expected 'tempo <bpm>' or 'tempo <beat> <bpm>'".into()),
            }
        }
    }
    if tempo_points.is_empty() {
        tempo_points.push((0.0, 120.0));
    }
    tempo_points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    if tempo_points[0].0 > 0.0 {
        let bpm0 = tempo_points[0].1;
        tempo_points.insert(0, (0.0, bpm0));
    }
    let tmap = TempoMap { points: tempo_points };
    let bpm0 = tmap.bpm_at(0.0);

    // pass 2: structure
    struct TrackMeta {
        synth: String,
        swing: f64,    // percent, 50 = straight
        human_ms: f64, // ± time jitter
        human_vel: f64, // ± velocity fraction
        sets: Vec<(String, f64)>,
    }
    let mut metas: Vec<TrackMeta> = Vec::new();
    let mut notes: Vec<NoteEv> = Vec::new();          // absolute-mode events
    let mut autos: Vec<Automation> = Vec::new();
    let mut global_swing = 50.0;

    // sections: name -> (len_beats, events with relative beats)
    let mut sections: std::collections::HashMap<String, (f64, Vec<NoteEv>)> =
        std::collections::HashMap::new();
    let mut section_order: Vec<String> = Vec::new();
    let mut arrange: Vec<String> = Vec::new();
    let mut cur_section: Option<String> = None;
    let mut cur_track: Option<usize> = None;
    let mut had_absolute = false;

    // sufixos db e % sao ergonomia: "set gain -3db" le -3 (gain de mixer E em db)
    let parse_num = |s: &str, what: &str, lineno: usize| -> Result<f64, String> {
        s.trim_end_matches("db")
            .trim_end_matches('%')
            .parse()
            .map_err(|_| format!("line {}: bad {}", lineno + 1, what))
    };

    // contexto de canal do mixer: (nome, sets); set/automate caem aqui
    let mut mix_metas: Vec<(String, Vec<(String, f64)>)> = Vec::new();
    let mut mix_autos: Vec<Automation> = Vec::new();
    let mut cur_mixer: Option<usize> = None;

    for (lineno, raw) in src.lines().enumerate() {
        let line = clean_line(raw);
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        match parts[0] {
            "tempo" => {} // handled in pass 1
            "mixer" => {
                // mixer <canal>: set/automate seguintes vao pro canal do mixer
                let name = parts.get(1).ok_or("mixer needs channel name")?.to_string();
                cur_track = None;
                if let Some(i) = mix_metas.iter().position(|(n, _)| *n == name) {
                    cur_mixer = Some(i);
                } else {
                    mix_metas.push((name, Vec::new()));
                    cur_mixer = Some(mix_metas.len() - 1);
                }
            }
            "track" => {
                cur_mixer = None;
                let synth = parts.get(2).or(parts.get(1)).ok_or("track needs synth name")?;
                // reuse track if same synth already declared (sections switch back)
                if let Some(i) = metas.iter().position(|m| m.synth == *synth) {
                    cur_track = Some(i);
                } else {
                    metas.push(TrackMeta {
                        synth: synth.to_string(),
                        swing: global_swing,
                        human_ms: 0.0,
                        human_vel: 0.0,
                        sets: Vec::new(),
                    });
                    cur_track = Some(metas.len() - 1);
                }
            }
            "swing" => {
                let v = parse_num(parts.get(1).ok_or("swing needs value")?, "swing", lineno)?;
                match cur_track {
                    Some(i) => metas[i].swing = v,
                    None => global_swing = v,
                }
            }
            "humanize" => {
                let i = cur_track.ok_or(format!("line {}: humanize before track", lineno + 1))?;
                let t = parts.get(1).ok_or("humanize needs time")?;
                let ms: f64 = t
                    .trim_end_matches("ms")
                    .parse()
                    .map_err(|_| format!("line {}: bad humanize time", lineno + 1))?;
                let v = parse_num(parts.get(2).unwrap_or(&"0"), "humanize vel", lineno)?;
                metas[i].human_ms = ms;
                metas[i].human_vel = v / 100.0;
            }
            "set" => {
                let name = parts.get(1).ok_or("set needs name")?.to_string();
                let v = parse_num(parts.get(2).ok_or("set needs value")?, "set value", lineno)?;
                match (cur_mixer, cur_track) {
                    (Some(m), _) => mix_metas[m].1.push((name, v)),
                    (None, Some(i)) => metas[i].sets.push((name, v)),
                    _ => return Err(format!("line {}: set before track/mixer", lineno + 1)),
                }
            }
            "automate" => {
                // automate <param> <beat> <val> [-> <beat> <val> [curve c]]...
                let i = match (cur_mixer, cur_track) {
                    (Some(m), _) => m,
                    (None, Some(i)) => i,
                    _ => return Err(format!("line {}: automate before track/mixer", lineno + 1)),
                };
                let param = parts.get(1).ok_or("automate needs param")?.to_string();
                let dup = if cur_mixer.is_some() {
                    mix_autos.iter().any(|a| a.track == i && a.param == param)
                } else {
                    autos.iter().any(|a| a.track == i && a.param == param)
                };
                if dup {
                    return Err(format!(
                        "E020 line {}: duplicate automate for param '{}' on this track",
                        lineno + 1,
                        param
                    ));
                }
                let mut pts: Vec<AutoPoint> = Vec::new();
                let mut j = 2;
                let mut first = true;
                while j < parts.len() {
                    if !first {
                        if parts[j] != "->" {
                            return Err(format!("line {}: automate expected '->'", lineno + 1));
                        }
                        j += 1;
                    }
                    let beat = parse_num(parts.get(j).ok_or("automate: beat")?, "beat", lineno)?;
                    let value = parse_num(parts.get(j + 1).ok_or("automate: value")?, "value", lineno)?;
                    j += 2;
                    let mut curve = ACurve::Lin;
                    if parts.get(j) == Some(&"curve") {
                        curve = match *parts.get(j + 1).ok_or("automate: curve name")? {
                            "lin" => ACurve::Lin,
                            "exp" => ACurve::Exp,
                            "log" => ACurve::Log,
                            s if s.starts_with("pow(") => ACurve::Pow(
                                s.trim_start_matches("pow(")
                                    .trim_end_matches(')')
                                    .parse()
                                    .map_err(|_| format!("line {}: bad pow", lineno + 1))?,
                            ),
                            o => return Err(format!("line {}: unknown curve {}", lineno + 1, o)),
                        };
                        j += 2;
                    }
                    pts.push(AutoPoint { beat, value, curve });
                    first = false;
                }
                if pts.is_empty() {
                    return Err(format!("line {}: automate needs points", lineno + 1));
                }
                pts.sort_by(|a, b| a.beat.partial_cmp(&b.beat).unwrap());
                if cur_mixer.is_some() {
                    mix_autos.push(Automation { track: i, param, points: pts });
                } else {
                    autos.push(Automation { track: i, param, points: pts });
                }
            }
            "section" => {
                let name = parts.get(1).ok_or("section needs name")?.to_string();
                let len: f64 = parts.get(2).and_then(|s| s.parse().ok()).ok_or("section needs len")?;
                sections.insert(name.clone(), (len, Vec::new()));
                section_order.push(name.clone());
                cur_section = Some(name);
            }
            "arrange" => {
                for p in &parts[1..] {
                    arrange.push(p.to_string());
                }
                cur_section = None;
            }
            _ => {
                // note line: <beat> <note>|[chord] <dur> <vel> [x<count>] [@<stride>]
                let ti = cur_track.ok_or(format!("line {}: note before any track", lineno + 1))?;
                let beat: f64 = parse_num(parts[0], "beat", lineno)?;
                // chord?
                let mut idx = 1;
                let mut chord: Vec<f64> = Vec::new();
                if parts[1].starts_with('[') {
                    let mut done = false;
                    while idx < parts.len() && !done {
                        let mut tok = parts[idx];
                        tok = tok.trim_start_matches('[');
                        if tok.ends_with(']') {
                            tok = tok.trim_end_matches(']');
                            done = true;
                        }
                        if !tok.is_empty() {
                            chord.push(parse_note_token(tok)?);
                        }
                        idx += 1;
                    }
                    if !done {
                        return Err(format!("line {}: unterminated chord", lineno + 1));
                    }
                } else {
                    chord.push(parse_note_token(parts[1])?);
                    idx = 2;
                }
                if parts.len() < idx + 2 {
                    return Err(format!("line {}: need: beat note dur vel", lineno + 1));
                }
                let dur: f64 = parse_num(parts[idx], "dur", lineno)?;
                let vel: f64 = parse_num(parts[idx + 1], "vel", lineno)?;
                let mut count = 1u32;
                let mut stride = dur;
                for p in &parts[idx + 2..] {
                    if let Some(c) = p.strip_prefix('x') {
                        count = c.parse().map_err(|_| format!("line {}: bad x", lineno + 1))?;
                    } else if let Some(st) = p.strip_prefix('@') {
                        stride = st.parse().map_err(|_| format!("line {}: bad @", lineno + 1))?;
                    }
                }
                for k in 0..count {
                    let ev = NoteEv {
                        track: ti,
                        beat: beat + k as f64 * stride,
                        notes: chord.clone(),
                        dur,
                        vel,
                    };
                    match &cur_section {
                        Some(sec) => sections.get_mut(sec).unwrap().1.push(ev),
                        None => {
                            had_absolute = true;
                            notes.push(ev);
                        }
                    }
                }
            }
        }
    }

    // E021: mixing section/arrange with absolute events
    if !sections.is_empty() && had_absolute {
        return Err("E021: file mixes section/arrange with absolute top-level events".into());
    }

    // expand arrangement
    if !sections.is_empty() {
        let order: Vec<String> = if arrange.is_empty() { section_order } else { arrange };
        let mut offset = 0.0;
        for name in &order {
            let (len, evs) = sections
                .get(name)
                .ok_or(format!("arrange references unknown section '{}'", name))?;
            for ev in evs {
                let mut e = ev.clone();
                e.beat += offset;
                notes.push(e);
            }
            offset += len;
        }
    }

    // build tracks
    let mut tracks: Vec<Track> = metas
        .iter()
        .map(|m| Track { synth: m.synth.clone(), events: Vec::new() })
        .collect();
    for (i, m) in metas.iter().enumerate() {
        for (name, v) in &m.sets {
            tracks[i].events.push(TimedEv { sample: 0, ev: Ev::Param(name.clone(), *v) });
        }
    }

    // canais do mixer: sets viram Param no sample 0
    let mut mix_tracks: Vec<MixTrack> = mix_metas
        .iter()
        .map(|(n, sets)| MixTrack {
            channel: n.clone(),
            events: sets
                .iter()
                .map(|(p, v)| TimedEv { sample: 0, ev: Ev::Param(p.clone(), *v) })
                .collect(),
        })
        .collect();

    // bpm-change events broadcast to all tracks (bus FX re-read bpm per block)
    for &(b, v) in tmap.points.iter().skip(1) {
        let s = (tmap.time_of_beat(b) * sr) as u64;
        for tr in tracks.iter_mut() {
            tr.events.push(TimedEv { sample: s, ev: Ev::Bpm(v) });
        }
        for tr in mix_tracks.iter_mut() {
            tr.events.push(TimedEv { sample: s, ev: Ev::Bpm(v) });
        }
    }

    // note events: swing -> humanize -> tempo map -> samples
    let mut per_track_idx: Vec<u64> = vec![0; metas.len()];
    for ev in &notes {
        let m = &metas[ev.track];
        let mut beat = ev.beat;
        // swing: only eighth-note offbeats (x.5 ± 1/32)
        if (m.swing - 50.0).abs() > 1e-9 {
            let frac = beat.rem_euclid(1.0);
            if (frac - 0.5).abs() <= 1.0 / 32.0 {
                let even = beat - frac;
                beat = even + (m.swing / 50.0) * 0.5;
            }
        }
        let eidx = per_track_idx[ev.track];
        per_track_idx[ev.track] += 1;
        // deterministic humanize: seed from (track name, event index)
        let mut tseed = 0xcbf29ce484222325u64;
        for b in m.synth.bytes() {
            tseed = (tseed ^ b as u64).wrapping_mul(0x100000001b3);
        }
        let t_jit = if m.human_ms > 0.0 {
            uniform_pm1(tseed ^ eidx.wrapping_mul(2)) * m.human_ms / 1000.0
        } else {
            0.0
        };
        let v_jit = if m.human_vel > 0.0 {
            uniform_pm1(tseed ^ (eidx.wrapping_mul(2) + 1)) * m.human_vel
        } else {
            0.0
        };
        let vel = (ev.vel * (1.0 + v_jit)).clamp(0.0, 1.0);
        let t0 = (tmap.time_of_beat(beat) + t_jit).max(0.0);
        let t1 = tmap.time_of_beat(beat + ev.dur) + t_jit;
        let dur_s = t1.max(t0 + 0.001) - t0;
        for &n in &ev.notes {
            tracks[ev.track]
                .events
                .push(TimedEv { sample: (t0 * sr) as u64, ev: Ev::On(n, vel, dur_s) });
            tracks[ev.track]
                .events
                .push(TimedEv { sample: (t1.max(t0 + 0.001) * sr) as u64, ev: Ev::Off(n) });
        }
    }

    // automation -> block-rate Param events (64-sample blocks; param smoothing covers zipper)
    const BLOCK: f64 = 64.0;
    let expand_auto = |a: &Automation, events: &mut Vec<TimedEv>| {
        let first = a.points.first().unwrap();
        let last = a.points.last().unwrap();
        events.push(TimedEv { sample: 0, ev: Ev::Param(a.param.clone(), first.value) });
        let s0 = (tmap.time_of_beat(first.beat) * sr) as u64;
        let s1 = (tmap.time_of_beat(last.beat) * sr) as u64;
        let mut s = s0;
        while s <= s1 {
            // beat at sample s: invert tempo map by scanning points (piecewise linear)
            let t = s as f64 / sr;
            let mut beat = 0.0;
            let mut acc = 0.0;
            for i in 0..tmap.points.len() {
                let (b0, bpm) = tmap.points[i];
                let b1 = tmap.points.get(i + 1).map(|p| p.0).unwrap_or(f64::INFINITY);
                let span_t = (b1 - b0) * 60.0 / bpm;
                if acc + span_t >= t || b1.is_infinite() {
                    beat = b0 + (t - acc) * bpm / 60.0;
                    break;
                }
                acc += span_t;
            }
            // value at beat
            let mut val = first.value;
            for w in a.points.windows(2) {
                let (p0, p1) = (&w[0], &w[1]);
                if beat >= p1.beat {
                    val = p1.value;
                } else if beat >= p0.beat {
                    let p = (beat - p0.beat) / (p1.beat - p0.beat).max(1e-9);
                    val = acurve_val(p0.value, p1.value, p, p1.curve);
                    break;
                }
            }
            events.push(TimedEv { sample: s, ev: Ev::Param(a.param.clone(), val) });
            s += BLOCK as u64;
        }
        events.push(TimedEv { sample: s1 + 1, ev: Ev::Param(a.param.clone(), last.value) });
    };
    for a in &autos {
        let mut evs = std::mem::take(&mut tracks[a.track].events);
        expand_auto(a, &mut evs);
        tracks[a.track].events = evs;
    }
    for a in &mix_autos {
        let mut evs = std::mem::take(&mut mix_tracks[a.track].events);
        expand_auto(a, &mut evs);
        mix_tracks[a.track].events = evs;
    }

    for tr in tracks.iter_mut() {
        tr.events.sort_by_key(|e| e.sample);
    }
    for tr in mix_tracks.iter_mut() {
        tr.events.sort_by_key(|e| e.sample);
    }
    Ok((bpm0, tracks, mix_tracks))
}

