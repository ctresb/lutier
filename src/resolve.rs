// Load-time name resolution: rewrites Expr::Ident into slot-indexed variants
// (VarCur/VarPrev/VarGlobal/VarParam/Builtin/BusIn) so the per-sample engine
// never hashes strings. Node ids are left untouched (they seed per-node rngs;
// changing them would change the audio). Resolution replicates the runtime
// lookup order of the interpreter exactly: cur > globals > params > builtins
// > prev, per scope - so output stays bit-identical to the name-based path.
use crate::parser::{BuiltinVar, Expr, Seg, SynthDef};

fn builtin(name: &str) -> Option<BuiltinVar> {
    Some(match name {
        "note" => BuiltinVar::Note,
        "velocity" => BuiltinVar::Velocity,
        "gate" => BuiltinVar::Gate,
        "time" => BuiltinVar::Time,
        "dur" => BuiltinVar::Dur,
        "rand" => BuiltinVar::Rand,
        "voice_idx" => BuiltinVar::VoiceIdx,
        _ => return None,
    })
}

/// scope descriptor for one expression position
struct Scope<'a> {
    /// let names of this slot space (voice lets, or global lets)
    lets: &'a [String],
    /// lets with index < avail resolve to VarCur; the rest to VarPrev
    avail: usize,
    globals: &'a [String],
    params: &'a [String],
    /// resolve "__bus_in" to BusIn (bus/master chains)
    bus: bool,
}

fn subst(e: &mut Expr, sc: &Scope) {
    match e {
        Expr::Ident(n) => {
            if sc.bus {
                // bus/master scope: only the chain input resolves at load; synth
                // names, params and builtins go through the runtime fallback
                // (synth_outs > params > builtins), same order as the interpreter
                if n == "__bus_in" {
                    *e = Expr::BusIn;
                }
                return;
            }
            if let Some(i) = sc.lets[..sc.avail.min(sc.lets.len())].iter().position(|x| x == n) {
                *e = Expr::VarCur(i);
            } else if let Some(i) = sc.globals.iter().position(|x| x == n) {
                *e = Expr::VarGlobal(i);
            } else if let Some(i) = sc.params.iter().position(|x| x == n) {
                *e = Expr::VarParam(i);
            } else if let Some(b) = builtin(n) {
                *e = Expr::Builtin(b);
            } else if let Some(i) = sc.lets.iter().position(|x| x == n) {
                *e = Expr::VarPrev(i);
            }
            // anything else stays Ident: synth names in bus key: routing
            // (resolved per sample against synth_outs), or unknown -> 0.0
        }
        Expr::Bin { l, r, .. } => {
            subst(l, sc);
            subst(r, sc);
        }
        Expr::Neg(x) => subst(x, sc),
        Expr::Call { args, .. } => {
            for (k, a) in args {
                // enum-like keyword args (shape: sine, color: pink...) must stay Ident
                if crate::check::is_enum_arg(k) && matches!(a, Expr::Ident(_)) {
                    continue;
                }
                subst(a, sc);
            }
        }
        Expr::Env { start, segs, sustain, release, .. } => {
            if let Some(s) = start {
                subst(s, sc);
            }
            for s in segs.iter_mut().chain(release.iter_mut()) {
                let Seg { target, time, .. } = s;
                subst(target, sc);
                subst(time, sc);
            }
            if let Some(s) = sustain {
                subst(s, sc);
            }
        }
        _ => {}
    }
}

/// resolve all names in a synth def; returns nothing, mutates in place.
/// Call once per SynthInstance (defs are cloned per instance).
pub fn resolve_synth(def: &mut SynthDef) {
    let gnames: Vec<String> = def.globals.iter().map(|(n, _)| n.clone()).collect();
    let vnames: Vec<String> = def.voice.iter().map(|(n, _)| n.clone()).collect();
    let pnames: Vec<String> = def.params.iter().map(|p| p.name.clone()).collect();
    let no_lets: Vec<String> = Vec::new();

    // globals: earlier globals are cur, later ones are prev (feedback across samples)
    for j in 0..def.globals.len() {
        let sc = Scope { lets: &gnames, avail: j, globals: &no_lets, params: &pnames, bus: false };
        subst(&mut def.globals[j].1, &sc);
    }
    // voice lets: sequential, let k sees lets 0..k this-sample
    for k in 0..def.voice.len() {
        let sc = Scope { lets: &vnames, avail: k, globals: &gnames, params: &pnames, bus: false };
        subst(&mut def.voice[k].1, &sc);
    }
    // out: sees every let
    let sc_out =
        Scope { lets: &vnames, avail: vnames.len(), globals: &gnames, params: &pnames, bus: false };
    subst(&mut def.out, &sc_out);
    // pitch mod runs before the lets: every let reference is previous-sample
    if let Some(pm) = &mut def.pitch_mod {
        let sc = Scope { lets: &vnames, avail: 0, globals: &gnames, params: &pnames, bus: false };
        subst(pm, &sc);
    }
    // bus: only __bus_in resolves; synth names stay Ident (sidechain key: routing),
    // params/builtins resolve like the interpreter's fallback order
    for e in def.bus.iter_mut() {
        let sc = Scope { lets: &no_lets, avail: 0, globals: &no_lets, params: &pnames, bus: true };
        subst(e, &sc);
        // inject the chain input once at load instead of every sample
        if let Expr::Call { args, .. } = e {
            if !args.iter().any(|(k, _)| k == "_0") {
                args.insert(0, ("_0".to_string(), Expr::BusIn));
            }
        }
    }
}

/// resolve a master chain (no params, no globals; only __bus_in + builtins)
pub fn resolve_master(chain: &mut [Expr]) {
    let none: Vec<String> = Vec::new();
    for e in chain.iter_mut() {
        let sc = Scope { lets: &none, avail: 0, globals: &none, params: &none, bus: true };
        subst(e, &sc);
        if let Expr::Call { args, .. } = e {
            if !args.iter().any(|(k, _)| k == "_0") {
                args.insert(0, ("_0".to_string(), Expr::BusIn));
            }
        }
    }
}
