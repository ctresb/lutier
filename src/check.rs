// Semantic passes over the parsed .synth file (tier3 §1.2, subset with error codes).
// Messages: "<CODE> synth '<name>': <msg> - <hint>". E* are fatal, W* are warnings.
use crate::parser::{Expr, MasterDef, ParsedFile, SynthDef, Unit};
use std::collections::{HashMap, HashSet};

const BUILTIN_NAMES: &[&str] = &["note", "velocity", "gate", "time", "rand", "voice_idx"];

const OSC_NO_FM: &[&str] = &["saw", "square", "pulse", "noise"];

fn walk<'a>(e: &'a Expr, f: &mut dyn FnMut(&'a Expr)) {
    f(e);
    match e {
        Expr::Bin { l, r, .. } => {
            walk(l, f);
            walk(r, f);
        }
        Expr::Neg(x) => walk(x, f),
        Expr::Call { args, .. } => {
            for (_, a) in args {
                walk(a, f);
            }
        }
        Expr::Env { start, segs, sustain, release, .. } => {
            if let Some(s) = start {
                walk(s, f);
            }
            for s in segs {
                walk(&s.target, f);
                walk(&s.time, f);
            }
            if let Some(s) = sustain {
                walk(s, f);
            }
            for s in release {
                walk(&s.target, f);
                walk(&s.time, f);
            }
        }
        _ => {}
    }
}

// arg names whose Ident values are enum-like keywords, not signal references
const ENUM_ARGS: &[&str] = &[
    "shape", "color", "side", "steal", "makeup", "table", "exciter", "modes", "loop",
    "window", "pingpong", "oversample", "slope", "root", "ir", "ir2", "tipo",
];

/// true when Ident values of this arg are enum-like keywords, not signal refs
pub fn is_enum_arg(k: &str) -> bool {
    ENUM_ARGS.contains(&k)
}

/// names referenced by an expression (skips enum-like keyword args)
fn idents(e: &Expr) -> Vec<String> {
    let mut out = Vec::new();
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
                    if ENUM_ARGS.contains(&k.as_str()) && matches!(a, Expr::Ident(_)) {
                        continue;
                    }
                    go(a, out);
                }
            }
            Expr::Env { start, segs, sustain, release, .. } => {
                if let Some(s) = start {
                    go(s, out);
                }
                for s in segs.iter().chain(release.iter()) {
                    go(&s.target, out);
                    go(&s.time, out);
                }
                if let Some(s) = sustain {
                    go(s, out);
                }
            }
            _ => {}
        }
    }
    go(e, &mut out);
    out
}

/// does this expression contain a delay node (breaks feedback cycles)?
fn has_delay(e: &Expr) -> bool {
    let mut found = false;
    walk(e, &mut |x| {
        if let Expr::Call { name, .. } = x {
            if name == "delay" || name == "delay1" || name == "delay_fx" {
                found = true;
            }
        }
    });
    found
}

fn check_synth(def: &SynthDef, synth_names: &HashSet<String>, out: &mut Vec<String>) {
    let ctx = |code: &str, msg: String| format!("{} synth '{}': {}", code, def.name, msg);
    let mut scope: HashSet<String> = HashSet::new();
    for p in &def.params {
        scope.insert(p.name.clone());
    }
    for (n, _) in &def.globals {
        scope.insert(n.clone());
    }
    let voice_names: HashSet<String> = def.voice.iter().map(|(n, _)| n.clone()).collect();
    for n in BUILTIN_NAMES {
        scope.insert(n.to_string());
    }

    // E012: globals using voice-only context
    for (n, e) in &def.globals {
        for id in idents(e) {
            if ["note", "velocity", "gate"].contains(&id.as_str()) {
                out.push(ctx("E012", format!("global '{}' uses voice context '{}' - move it into voice {{ }}", n, id)));
            }
        }
    }

    // E003 unknown names (voice scope; forward refs among lets allowed for feedback)
    for (n, e) in def.voice.iter().chain(def.globals.iter()) {
        for id in idents(e) {
            if !scope.contains(&id) && !voice_names.contains(&id) && !synth_names.contains(&id) {
                out.push(ctx(
                    "E003",
                    format!("'{}' (used in '{}') is not defined - check spelling or add a let", id, n),
                ));
            }
        }
    }
    for id in idents(&def.out) {
        if !scope.contains(&id) && !voice_names.contains(&id) && !synth_names.contains(&id) {
            out.push(ctx("E003", format!("'{}' (used in out) is not defined", id)));
        }
    }

    // E006 missing out (parser default = literal 0)
    if matches!(def.out, Expr::Num { v, .. } if v == 0.0) && !def.voice.is_empty() {
        out.push(ctx("E006", "voice has no 'out' - nothing will sound".into()));
    }

    // E002: cycle without a delay in the loop (DFS over voice lets)
    let map: HashMap<&str, &Expr> = def.voice.iter().map(|(n, e)| (n.as_str(), e)).collect();
    for (start, _) in &def.voice {
        let mut path: Vec<String> = vec![start.clone()];
        let mut stack: Vec<(String, bool)> = idents(map[start.as_str()])
            .into_iter()
            .filter(|n| map.contains_key(n.as_str()))
            .map(|n| (n.clone(), has_delay(map[start.as_str()])))
            .collect();
        let mut seen = HashSet::new();
        while let Some((cur, delayed)) = stack.pop() {
            if cur == *start {
                if !delayed {
                    out.push(ctx(
                        "E002",
                        format!("feedback cycle through '{}' has no delay - insert delay1() in the loop", start),
                    ));
                }
                break;
            }
            if !seen.insert(cur.clone()) {
                continue;
            }
            if let Some(e) = map.get(cur.as_str()) {
                let d = delayed || has_delay(e);
                for n in idents(e) {
                    if map.contains_key(n.as_str()) || n == *start {
                        stack.push((n, d));
                    }
                }
            }
        }
        let _ = path.pop();
    }

    // per-call checks
    let check_calls = |e: &Expr, place: &str, out: &mut Vec<String>| {
        walk(e, &mut |x| match x {
            Expr::Call { name, args, .. } => {
                // E023: fm on unsupported oscillator
                if OSC_NO_FM.contains(&name.as_str())
                    && args.iter().any(|(k, _)| k == "fm")
                {
                    out.push(format!(
                        "E023 synth '{}': fm: not supported on '{}' (only sine/triangle)",
                        place, name
                    ));
                }
                // E014: delay_fx feedback literal out of range
                if name == "delay_fx" {
                    if let Some((_, Expr::Num { v, .. })) = args.iter().find(|(k, _)| k == "feedback") {
                        if *v < 0.0 || *v > 0.95 {
                            out.push(format!(
                                "E014 synth '{}': delay_fx feedback {} outside 0..0.95",
                                place, v
                            ));
                        }
                    }
                }
                // W002: saturate/drive amount > 0.9
                if name == "saturate" || name == "drive" {
                    if let Some((_, Expr::Num { v, .. })) = args.iter().find(|(k, _)| k == "amount") {
                        if *v > 0.9 {
                            out.push(format!(
                                "W002 synth '{}': {} amount {:.2} > 0.9 - heavy aliasing risk",
                                place, name, v
                            ));
                        }
                    }
                }
                // E033: convolve ir must reference a synth defined in this file
                if name == "convolve" {
                    match args.iter().find(|(k, _)| k == "ir") {
                        Some((_, Expr::Ident(n))) | Some((_, Expr::Str(n))) => {
                            if !synth_names.contains(n) {
                                out.push(format!(
                                    "E033 synth '{}': convolve ir '{}' is not a synth in this file",
                                    place, n
                                ));
                            }
                        }
                        _ => out.push(format!(
                            "E033 synth '{}': convolve needs ir: <synth_name>",
                            place
                        )),
                    }
                    if let Some((_, Expr::Ident(n))) | Some((_, Expr::Str(n))) =
                        args.iter().find(|(k, _)| k == "ir2")
                    {
                        if !synth_names.contains(n) {
                            out.push(format!(
                                "E033 synth '{}': convolve ir2 '{}' is not a synth in this file",
                                place, n
                            ));
                        }
                    }
                }
                // W003: literal cutoff above ~nyquist
                if ["lowpass", "highpass", "bandpass", "notch"].contains(&name.as_str()) {
                    if let Some((_, Expr::Num { v, unit })) = args.iter().find(|(k, _)| k == "cutoff") {
                        if *unit == Unit::Hz && *v > 44100.0 * 0.5 * 0.98 {
                            out.push(format!(
                                "W003 synth '{}': cutoff {}hz above usable range",
                                place, v
                            ));
                        }
                    }
                    // W005: q > 1 without kill_after (self-oscillating voice never dies)
                    if let Some((_, Expr::Num { v, .. })) = args.iter().find(|(k, _)| k == "q") {
                        if *v > 1.0 {
                            out.push(format!(
                                "W005 synth '{}': q {} > 1 self-oscillates - make sure an envelope or 'kill after' ends the voice",
                                place, v
                            ));
                        }
                    }
                }
            }
            Expr::Env { sustain, release, .. } => {
                if sustain.is_some() && release.is_empty() {
                    out.push(format!(
                        "E013 synth '{}': env has sustain but empty release - voice would never end",
                        place
                    ));
                }
            }
            _ => {}
        });
    };
    for (_, e) in def.voice.iter().chain(def.globals.iter()) {
        check_calls(e, &def.name, out);
    }
    check_calls(&def.out, &def.name, out);
    for e in &def.bus {
        check_calls(e, &def.name, out);
    }

    // W006: physical resonators ring on their own - voices never die without kill after
    if def.kill_after.is_none() {
        const PHYSICAL: &[&str] = &["pluck", "modal", "modal2", "bow", "flute", "reed"];
        let mut phys: Option<String> = None;
        for (_, e) in &def.voice {
            walk(e, &mut |x| {
                if let Expr::Call { name, .. } = x {
                    if PHYSICAL.contains(&name.as_str()) && phys.is_none() {
                        phys = Some(name.clone());
                    }
                }
            });
        }
        if let Some(n) = phys {
            out.push(ctx(
                "W006",
                format!("'{}' voices decay on their own - add 'kill after Ns' so they get freed", n),
            ));
        }
    }

    // W001: dead lets (defined, never used by out chain or pitch mod)
    let mut used: HashSet<String> = idents(&def.out).into_iter().collect();
    if let Some(pm) = &def.pitch_mod {
        used.extend(idents(pm));
    }
    let mut changed = true;
    while changed {
        changed = false;
        for (n, e) in &def.voice {
            if used.contains(n) {
                for id in idents(e) {
                    if used.insert(id) {
                        changed = true;
                    }
                }
            }
        }
    }
    for (n, _) in &def.voice {
        if !used.contains(n) {
            out.push(ctx("W001", format!("let '{}' is never used", n)));
        }
    }
}

fn check_master(m: &MasterDef, out: &mut Vec<String>) {
    for e in &m.chain {
        walk(e, &mut |x| {
            if let Expr::Call { name, .. } = x {
                if name == "haas" {
                    out.push("W004 master: haas on the master bus compromises mono compatibility".into());
                }
            }
        });
    }
}

pub fn check_file(file: &ParsedFile) -> Vec<String> {
    let mut out = Vec::new();
    let synth_names: HashSet<String> = file.defs.iter().map(|d| d.name.clone()).collect();
    for d in &file.defs {
        check_synth(d, &synth_names, &mut out);
    }
    if let Some(m) = &file.master {
        check_master(m, &mut out);
    }
    out
}
