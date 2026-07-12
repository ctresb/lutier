// Expansao de fx (plugins de usuario em DSL): instanciar um fx numa chain
// (bus de synth, canal do mixer, master) e uma macro de load-time - a chain
// do fx e clonada, os params viram os args da chamada (ou os defaults) e os
// node ids sao re-semeados pra cada instancia ter estado/rng proprios.
// Custo em runtime: zero (vira nos comuns na chain do dono).
use crate::parser::{Expr, FxDef, Op, ParsedFile};
use std::collections::HashMap;

/// substitui Ident(param) pelo expr do arg; pula args enum-like (shape: sine)
fn subst_params(e: &mut Expr, map: &HashMap<String, Expr>) {
    match e {
        Expr::Ident(n) => {
            if let Some(rep) = map.get(n) {
                *e = rep.clone();
            }
        }
        Expr::Bin { l, r, .. } => {
            subst_params(l, map);
            subst_params(r, map);
        }
        Expr::Neg(x) => subst_params(x, map),
        Expr::Call { args, .. } => {
            for (k, a) in args {
                if crate::check::is_enum_arg(k) && matches!(a, Expr::Ident(_)) {
                    continue;
                }
                subst_params(a, map);
            }
        }
        Expr::Env { start, segs, sustain, release, .. } => {
            if let Some(s) = start {
                subst_params(s, map);
            }
            for s in segs.iter_mut().chain(release.iter_mut()) {
                subst_params(&mut s.target, map);
                subst_params(&mut s.time, map);
            }
            if let Some(s) = sustain {
                subst_params(s, map);
            }
        }
        _ => {}
    }
}

/// ids semeiam rngs e indexam estado: cada instancia precisa dos seus
fn reseed_ids(e: &mut Expr, next_id: &mut usize) {
    match e {
        Expr::Call { args, id, .. } => {
            *next_id += 1;
            *id = *next_id;
            for (_, a) in args {
                reseed_ids(a, next_id);
            }
        }
        Expr::Env { start, segs, sustain, release, id } => {
            *next_id += 1;
            *id = *next_id;
            if let Some(s) = start {
                reseed_ids(s, next_id);
            }
            for s in segs.iter_mut().chain(release.iter_mut()) {
                reseed_ids(&mut s.target, next_id);
                reseed_ids(&mut s.time, next_id);
            }
            if let Some(s) = sustain {
                reseed_ids(s, next_id);
            }
        }
        Expr::Bin { l, r, .. } => {
            reseed_ids(l, next_id);
            reseed_ids(r, next_id);
        }
        Expr::Neg(x) => reseed_ids(x, next_id),
        _ => {}
    }
}

/// expande fx calls no topo de uma chain, recursivo (fx compondo fx)
pub fn expand_chain(
    chain: &mut Vec<Expr>,
    fx: &[FxDef],
    next_id: &mut usize,
    place: &str,
) -> Result<(), String> {
    let mut depth = 0;
    let mut expanded = false;
    loop {
        let mut any = false;
        let mut out: Vec<Expr> = Vec::with_capacity(chain.len());
        for e in chain.drain(..) {
            let fxdef = match &e {
                Expr::Call { name, op: Op::Unknown, .. } => fx.iter().find(|f| &f.name == name),
                _ => None,
            };
            match (fxdef, e) {
                (Some(f), Expr::Call { args, .. }) => {
                    for (k, _) in &args {
                        if k.starts_with('_') {
                            return Err(format!(
                                "E045 {}: fx '{}' aceita apenas args nomeados (param: valor)",
                                place, f.name
                            ));
                        }
                        if !f.params.iter().any(|(p, _)| p == k) {
                            return Err(format!(
                                "E046 {}: fx '{}' nao tem param '{}' (tem: {})",
                                place,
                                f.name,
                                k,
                                f.params.iter().map(|(p, _)| p.as_str()).collect::<Vec<_>>().join(", ")
                            ));
                        }
                    }
                    let map: HashMap<String, Expr> = f
                        .params
                        .iter()
                        .map(|(p, d)| {
                            let v = args
                                .iter()
                                .find(|(k, _)| k == p)
                                .map(|(_, e)| e.clone())
                                .unwrap_or_else(|| d.clone());
                            (p.clone(), v)
                        })
                        .collect();
                    for mut node in f.chain.iter().cloned() {
                        subst_params(&mut node, &map);
                        reseed_ids(&mut node, next_id);
                        out.push(node);
                    }
                    any = true;
                }
                (_, e) => out.push(e),
            }
        }
        *chain = out;
        if !any {
            break;
        }
        expanded = true;
        depth += 1;
        if depth > 8 {
            return Err(format!("E047 {}: expansao de fx passa de 8 niveis (fx em ciclo?)", place));
        }
    }
    // chain que expandiu mistura ids do parser (baixos) com ids novos (altos);
    // o StateStore e um Vec plano indexado por id, entao renumera a chain
    // INTEIRA pra uma faixa compacta (so chains novas: goldens intactos)
    if expanded {
        for e in chain.iter_mut() {
            reseed_ids(e, next_id);
        }
    }
    Ok(())
}

/// expande fx em todas as chains do arquivo (bus dos synths, master, mixer)
pub fn expand_file(file: &mut ParsedFile, next_id: &mut usize) -> Result<(), String> {
    let fx = file.fx.clone();
    for d in file.defs.iter_mut() {
        let place = format!("synth '{}' bus", d.name);
        expand_chain(&mut d.bus, &fx, next_id, &place)?;
    }
    if let Some(m) = file.master.as_mut() {
        expand_chain(&mut m.chain, &fx, next_id, "master")?;
    }
    if let Some(mx) = file.mixer.as_mut() {
        for ch in mx.channels.iter_mut() {
            let place = format!("channel '{}'", ch.name);
            expand_chain(&mut ch.chain, &fx, next_id, &place)?;
        }
    }
    Ok(())
}
