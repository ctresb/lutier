// Parser: tokens -> AST for .synth files.
use crate::lexer::Tok;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Curve {
    Lin,
    Exp,
    Log,
    Pow(f64),
}

#[derive(Debug, Clone)]
pub struct Seg {
    pub target: Expr,
    pub time: Expr,
    pub curve: Curve,
}

#[derive(Debug, Clone)]
pub enum Expr {
    // value + unit already normalized: db->linear scalar, s->ms, khz->hz, ct->st, %->scalar
    Num { v: f64, unit: Unit },
    Str(String),
    Ident(String),
    Bin { op: char, l: Box<Expr>, r: Box<Expr> },
    Neg(Box<Expr>),
    Call { name: String, op: Op, args: Vec<(String, Expr)>, id: usize },
    /// constant table literal: list of tuples of literals, e.g.
    /// [(0.5, 12s, 0.8), (1.0, 8s, 1.0)] - consumed whole by nodes (modes: etc)
    Table(Vec<Vec<(f64, Unit)>>),
    // ---- resolved forms, produced by resolve::resolve_synth at load time,
    // ---- never by the parser (slot-indexed variable reads, no string hashing)
    /// this-sample value of let slot i (already computed in eval order)
    VarCur(usize),
    /// previous-sample value of let slot i (feedback/forward reference)
    VarPrev(usize),
    /// global slot i (fully evaluated before voices run)
    VarGlobal(usize),
    /// param slot i
    VarParam(usize),
    /// bus/master chain input (the summed voice signal)
    BusIn,
    /// per-voice builtin
    Builtin(BuiltinVar),
    Env { start: Option<Box<Expr>>, segs: Vec<Seg>, sustain: Option<Box<Expr>>, release: Vec<Seg>, id: usize },
}

/// node opcode, computed from the call name at parse time so the per-sample
/// engine dispatches on an enum (jump table) instead of matching strings
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Op {
    Hz,
    PitchOp,
    Unipolar,
    Min,
    Max,
    Clamp,
    Abs,
    Gain,
    Pan,
    Sine,
    Triangle,
    Saw,
    Square,
    Pulse,
    Wavetable,
    Noise,
    Lowpass,
    Highpass,
    Bandpass,
    Notch,
    Lfo,
    Saturate,
    Clip,
    Drive,
    Delay1,
    Delay,
    Sample,
    Pluck,
    Strings,
    Modal,
    Modal2,
    Nwave,
    Grain,
    Follower,
    Rms,
    Ringmod,
    Widen,
    Haas,
    DelayFx,
    Chorus,
    Reverb,
    Compressor,
    Duck,
    Limiter,
    Convolve,
    Bow,
    Flute,
    Reed,
    Breath,
    Leslie,
    Hall,
    Brass,
    Voz,
    Unknown,
}

impl Op {
    pub fn from_name(name: &str) -> Op {
        match name {
            "hz" => Op::Hz,
            "pitch" => Op::PitchOp,
            "unipolar" => Op::Unipolar,
            "min" => Op::Min,
            "max" => Op::Max,
            "clamp" => Op::Clamp,
            "abs" => Op::Abs,
            "gain" => Op::Gain,
            "pan" => Op::Pan,
            "sine" => Op::Sine,
            "triangle" => Op::Triangle,
            "saw" => Op::Saw,
            "square" => Op::Square,
            "pulse" => Op::Pulse,
            "wavetable" => Op::Wavetable,
            "noise" => Op::Noise,
            "lowpass" => Op::Lowpass,
            "highpass" => Op::Highpass,
            "bandpass" => Op::Bandpass,
            "notch" => Op::Notch,
            "lfo" => Op::Lfo,
            "saturate" => Op::Saturate,
            "clip" => Op::Clip,
            "drive" => Op::Drive,
            "delay1" => Op::Delay1,
            "delay" => Op::Delay,
            "sample" => Op::Sample,
            "pluck" => Op::Pluck,
            "string" => Op::Strings,
            "modal" => Op::Modal,
            "modal2" => Op::Modal2,
            "nwave" => Op::Nwave,
            "grain" => Op::Grain,
            "follower" => Op::Follower,
            "rms" => Op::Rms,
            "ringmod" => Op::Ringmod,
            "widen" => Op::Widen,
            "haas" => Op::Haas,
            "delay_fx" => Op::DelayFx,
            "chorus" => Op::Chorus,
            "reverb" => Op::Reverb,
            "compressor" => Op::Compressor,
            "duck" => Op::Duck,
            "limiter" => Op::Limiter,
            "convolve" => Op::Convolve,
            "bow" => Op::Bow,
            "flute" => Op::Flute,
            "reed" => Op::Reed,
            "breath" => Op::Breath,
            "leslie" => Op::Leslie,
            "hall" => Op::Hall,
            "brass" => Op::Brass,
            "voz" => Op::Voz,
            _ => Op::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BuiltinVar {
    Note,
    Velocity,
    Gate,
    Time,
    Dur,
    Rand,
    VoiceIdx,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Unit {
    Scalar,
    Hz,
    Ms,
    St,   // semitone interval
    Beat, // musical beats (converted vs bpm at engine init)
}

#[derive(Debug, Clone)]
pub enum Mode {
    Poly { n: usize, steal: String, spread: f64 },
    Mono { glide_ms: f64, legato: bool },
}

#[derive(Debug, Clone)]
pub struct ParamDef {
    pub name: String,
    pub default: Expr,
}

#[derive(Debug, Clone)]
pub struct ParsedFile {
    pub defs: Vec<SynthDef>,
    pub master: Option<MasterDef>,
}

#[derive(Debug, Clone)]
pub struct MasterDef {
    pub gain: f64, // linear
    pub chain: Vec<Expr>,
}

#[derive(Debug, Clone)]
pub struct SynthDef {
    pub name: String,
    pub mode: Mode,
    pub gain: f64, // linear
    pub kill_after: Option<f64>,
    pub params: Vec<ParamDef>,
    pub globals: Vec<(String, Expr)>,
    pub voice: Vec<(String, Expr)>,
    pub out: Expr,
    pub bus: Vec<Expr>,
    /// mod-matrix routes targeting voice.pitch, summed into `note` (semitones)
    pub pitch_mod: Option<Expr>,
}

pub struct Parser {
    toks: Vec<Tok>,
    spans: Vec<crate::lexer::Span>,
    pos: usize,
    next_id: usize,
}

impl Parser {
    pub fn new(toks: Vec<Tok>) -> Self {
        Parser { toks, spans: Vec::new(), pos: 0, next_id: 0 }
    }

    pub fn new_spanned(toks: Vec<Tok>, spans: Vec<crate::lexer::Span>) -> Self {
        Parser { toks, spans, pos: 0, next_id: 0 }
    }

    /// "line:col: " of the current token, if spans are available
    fn here(&self) -> String {
        match self.spans.get(self.pos.min(self.spans.len().saturating_sub(1))) {
            Some((l, c)) => format!("{}:{}: ", l, c),
            None => String::new(),
        }
    }

    fn id(&mut self) -> usize {
        self.next_id += 1;
        self.next_id
    }

    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn next(&mut self) -> Result<Tok, String> {
        let t = self
            .toks
            .get(self.pos)
            .cloned()
            .ok_or_else(|| format!("{}unexpected EOF", self.here()))?;
        self.pos += 1;
        Ok(t)
    }

    fn expect_sym(&mut self, s: &str) -> Result<(), String> {
        let here = self.here();
        match self.next()? {
            Tok::Sym(x) if x == s => Ok(()),
            t => Err(format!("{}expected '{}', got {:?}", here, s, t)),
        }
    }

    fn expect_id(&mut self) -> Result<String, String> {
        let here = self.here();
        match self.next()? {
            Tok::Id(x) => Ok(x),
            t => Err(format!("{}expected identifier, got {:?}", here, t)),
        }
    }

    fn eat_id(&mut self, kw: &str) -> bool {
        if let Some(Tok::Id(x)) = self.peek() {
            if x == kw {
                self.pos += 1;
                return true;
            }
        }
        false
    }

    fn eat_sym(&mut self, s: &str) -> bool {
        if let Some(Tok::Sym(x)) = self.peek() {
            if x == s {
                self.pos += 1;
                return true;
            }
        }
        false
    }

    pub fn parse_file(&mut self) -> Result<ParsedFile, String> {
        self.parse_file_depth(0)
    }

    fn parse_file_depth(&mut self, depth: usize) -> Result<ParsedFile, String> {
        let mut file = ParsedFile { defs: Vec::new(), master: None };
        while self.peek().is_some() {
            if self.eat_id("synth") {
                let d = self.parse_synth()?;
                // local definitions override earlier imports with the same name
                file.defs.retain(|x| x.name != d.name);
                file.defs.push(d);
            } else if self.eat_id("master") {
                file.master = Some(self.parse_master()?);
            } else if self.eat_id("import") {
                // import "path.synth" - inlines that file's synth defs (master ignored).
                // Paths resolve relative to the working directory (project root).
                if depth > 8 {
                    return Err("import depth > 8 (cycle?)".into());
                }
                let path = match self.next()? {
                    Tok::Str(s) => s,
                    t => return Err(format!("{}import expects a string path, got {:?}", self.here(), t)),
                };
                let src = std::fs::read_to_string(&path)
                    .map_err(|e| format!("import \"{}\": {}", path, e))?;
                let (toks, spans) = crate::lexer::lex_spanned(&src)
                    .map_err(|e| format!("import \"{}\": {}", path, e))?;
                let mut sub = Parser::new_spanned(toks, spans);
                sub.next_id = self.next_id + 100_000; // keep node ids distinct across files
                let imported = sub
                    .parse_file_depth(depth + 1)
                    .map_err(|e| format!("import \"{}\": {}", path, e))?;
                self.next_id = sub.next_id;
                for d in imported.defs {
                    // local definitions win: skip imported synth if name already present
                    if !file.defs.iter().any(|x| x.name == d.name) {
                        file.defs.push(d);
                    }
                }
            } else {
                return Err(format!("expected 'synth', 'master' or 'import', got {:?}", self.peek()));
            }
        }
        Ok(file)
    }

    fn parse_master(&mut self) -> Result<MasterDef, String> {
        self.expect_sym("{")?;
        let mut m = MasterDef { gain: 1.0, chain: Vec::new() };
        while !self.eat_sym("}") {
            if self.eat_id("bus_gain") {
                m.gain = self.parse_signed_literal_linear()?;
            } else {
                m.chain.push(self.parse_expr()?);
            }
        }
        Ok(m)
    }

    fn parse_synth(&mut self) -> Result<SynthDef, String> {
        let name = self.expect_id()?;
        self.expect_sym("{")?;
        let mut def = SynthDef {
            name,
            mode: Mode::Poly { n: 8, steal: "oldest".into(), spread: 0.0 },
            gain: 1.0,
            kill_after: None,
            params: Vec::new(),
            globals: Vec::new(),
            voice: Vec::new(),
            out: Expr::Num { v: 0.0, unit: Unit::Scalar },
            bus: Vec::new(),
            pitch_mod: None,
        };
        let mut mod_defs: Vec<(String, Expr)> = Vec::new();
        let mut mod_routes: Vec<(Expr, (String, String), Option<(Expr, Expr)>)> = Vec::new();
        loop {
            if self.eat_sym("}") {
                break;
            }
            let kw = self.expect_id()?;
            match kw.as_str() {
                "meta" => {
                    self.expect_sym("{")?;
                    while !self.eat_sym("}") {
                        self.expect_id()?;
                        match self.next()? {
                            Tok::Str(_) => {}
                            t => return Err(format!("meta expects string, got {:?}", t)),
                        }
                    }
                }
                "poly" => {
                    let n = match self.next()? {
                        Tok::Num(v, _) => v as usize,
                        t => return Err(format!("poly expects int, got {:?}", t)),
                    };
                    let mut steal = "oldest".to_string();
                    let mut spread = 0.0;
                    loop {
                        if self.eat_id("steal") {
                            steal = self.expect_id()?;
                        } else if self.eat_id("spread") {
                            spread = self.parse_signed_literal_linear()?;
                        } else {
                            break;
                        }
                    }
                    def.mode = Mode::Poly { n, steal, spread };
                }
                "mono" => {
                    let mut glide = 0.0;
                    let mut legato = false;
                    loop {
                        if self.eat_id("glide") {
                            glide = self.parse_time_literal()?;
                        } else if self.eat_id("legato") {
                            legato = true;
                        } else {
                            break;
                        }
                    }
                    def.mode = Mode::Mono { glide_ms: glide, legato };
                }
                "gain" => {
                    def.gain = self.parse_signed_literal_linear()?;
                }
                "kill" => {
                    if !self.eat_id("after") {
                        return Err("expected 'after' after 'kill'".into());
                    }
                    def.kill_after = Some(self.parse_time_literal()? / 1000.0);
                }
                "param" => {
                    let p = self.parse_param()?;
                    def.params.push(p);
                }
                "global" => {
                    self.expect_sym("{")?;
                    while self.eat_id("let") {
                        let n = self.expect_id()?;
                        self.expect_eq()?;
                        let e = self.parse_expr()?;
                        def.globals.push((n, e));
                    }
                    self.expect_sym("}")?;
                }
                "voice" => {
                    self.expect_sym("{")?;
                    loop {
                        if self.eat_id("let") {
                            let n = self.expect_id()?;
                            self.expect_eq()?;
                            let e = self.parse_expr()?;
                            def.voice.push((n, e));
                        } else if self.eat_id("out") {
                            def.out = self.parse_expr()?;
                        } else if self.eat_sym("}") {
                            break;
                        } else {
                            return Err(format!("in voice: unexpected {:?}", self.peek()));
                        }
                    }
                }
                "bus" => {
                    self.expect_sym("{")?;
                    while !self.eat_sym("}") {
                        let e = self.parse_expr()?;
                        def.bus.push(e);
                    }
                }
                "mod" => {
                    // mod matrix (tier4 §4): defs `name: expr`, routes `expr -> target.arg [range lo..hi]`
                    self.expect_sym("{")?;
                    while !self.eat_sym("}") {
                        // def: Ident ':' expr (only when followed by ':')
                        if let (Some(Tok::Id(id)), Some(Tok::Sym(c))) =
                            (self.toks.get(self.pos), self.toks.get(self.pos + 1))
                        {
                            if c == ":" {
                                let name = id.clone();
                                self.pos += 2;
                                let e = self.parse_expr()?;
                                mod_defs.push((name, e));
                                continue;
                            }
                        }
                        let src = self.parse_expr()?;
                        self.expect_sym("->")?;
                        let tgt_node = self.expect_id()?;
                        self.expect_sym(".")?;
                        let tgt_arg = self.expect_id()?;
                        let mut range = None;
                        if self.eat_id("range") {
                            let lo = self.parse_expr_no_range()?;
                            self.expect_sym("..")?;
                            let hi = self.parse_expr_no_range()?;
                            range = Some((lo, hi));
                        }
                        mod_routes.push((src, (tgt_node, tgt_arg), range));
                    }
                }
                other => return Err(format!("unexpected keyword '{}' in synth", other)),
            }
        }
        self.apply_mod_matrix(&mut def, mod_defs, mod_routes)?;
        inline_table_lets(&mut def);
        Ok(def)
    }

    fn parse_expr_no_range(&mut self) -> Result<Expr, String> {
        // a single atom/unary (range bounds are literals or idents)
        self.parse_unary()
    }

    /// lower the mod matrix onto the graph: routes become extra addends on named args
    fn apply_mod_matrix(
        &mut self,
        def: &mut SynthDef,
        defs: Vec<(String, Expr)>,
        routes: Vec<(Expr, (String, String), Option<(Expr, Expr)>)>,
    ) -> Result<(), String> {
        if defs.is_empty() && routes.is_empty() {
            return Ok(());
        }
        // defs become voice lets, prepended so routes/lets can reference them
        for (i, (n, e)) in defs.into_iter().enumerate() {
            def.voice.insert(i, (n, e));
        }
        for (src, (node, argname), range) in routes {
            let src = match range {
                Some((lo, hi)) => {
                    // remap unipolar source to lo..hi: lo + src*(hi-lo)
                    Expr::Bin {
                        op: '+',
                        l: Box::new(lo.clone()),
                        r: Box::new(Expr::Bin {
                            op: '*',
                            l: Box::new(src),
                            r: Box::new(Expr::Bin {
                                op: '-',
                                l: Box::new(hi),
                                r: Box::new(lo),
                            }),
                        }),
                    }
                }
                None => src,
            };
            if node == "voice" && argname == "pitch" {
                def.pitch_mod = Some(match def.pitch_mod.take() {
                    Some(prev) => Expr::Bin { op: '+', l: Box::new(prev), r: Box::new(src) },
                    None => src,
                });
                continue;
            }
            let target = def
                .voice
                .iter_mut()
                .find(|(n, _)| *n == node)
                .ok_or(format!("E032: mod matrix target '{}' does not exist", node))?;
            match &mut target.1 {
                Expr::Call { args, .. } => {
                    if let Some((_, a)) = args.iter_mut().find(|(k, _)| *k == argname) {
                        let orig = a.clone();
                        *a = Expr::Bin { op: '+', l: Box::new(orig), r: Box::new(src) };
                    } else {
                        return Err(format!(
                            "E032: mod matrix target '{}.{}' - argument not present on the node",
                            node, argname
                        ));
                    }
                }
                _ => {
                    return Err(format!(
                        "E032: mod matrix target '{}' is not a callable node",
                        node
                    ))
                }
            }
        }
        Ok(())
    }

    fn expect_eq(&mut self) -> Result<(), String> {
        // '=' not in symbol set of lexer? add: lexer doesn't emit '='. handled by lexer change.
        self.expect_sym("=")
    }

    fn parse_time_literal(&mut self) -> Result<f64, String> {
        // returns ms
        match self.next()? {
            Tok::Num(v, u) => match u.as_str() {
                "ms" => Ok(v),
                "s" => Ok(v * 1000.0),
                "" => Ok(v * 1000.0),
                other => Err(format!("expected time literal, got unit {}", other)),
            },
            t => Err(format!("expected time literal, got {:?}", t)),
        }
    }

    fn parse_signed_literal_linear(&mut self) -> Result<f64, String> {
        // for gain decl: -6db etc -> linear amplitude
        let neg = self.eat_sym("-");
        match self.next()? {
            Tok::Num(v, u) => {
                let v = if neg { -v } else { v };
                match u.as_str() {
                    "db" => Ok(10f64.powf(v / 20.0)),
                    "" => Ok(v),
                    "%" => Ok(v * 0.01),
                    other => Err(format!("gain: bad unit {}", other)),
                }
            }
            t => Err(format!("expected literal, got {:?}", t)),
        }
    }

    // ---- expressions ----

    pub fn parse_expr(&mut self) -> Result<Expr, String> {
        self.parse_add()
    }

    fn parse_add(&mut self) -> Result<Expr, String> {
        let mut l = self.parse_mul()?;
        loop {
            if self.eat_sym("+") {
                let r = self.parse_mul()?;
                l = Expr::Bin { op: '+', l: Box::new(l), r: Box::new(r) };
            } else if self.peek() == Some(&Tok::Sym("-".into())) {
                self.pos += 1;
                let r = self.parse_mul()?;
                l = Expr::Bin { op: '-', l: Box::new(l), r: Box::new(r) };
            } else {
                break;
            }
        }
        Ok(l)
    }

    fn parse_mul(&mut self) -> Result<Expr, String> {
        let mut l = self.parse_unary()?;
        loop {
            if self.eat_sym("*") {
                let r = self.parse_unary()?;
                l = Expr::Bin { op: '*', l: Box::new(l), r: Box::new(r) };
            } else if self.eat_sym("/") {
                let r = self.parse_unary()?;
                l = Expr::Bin { op: '/', l: Box::new(l), r: Box::new(r) };
            } else {
                break;
            }
        }
        Ok(l)
    }

    fn parse_unary(&mut self) -> Result<Expr, String> {
        if self.eat_sym("-") {
            // fold sign into literal so -6db converts correctly
            if let Some(Tok::Num(v, u)) = self.peek().cloned() {
                self.pos += 1;
                return Ok(self.make_num(-v, &u));
            }
            let e = self.parse_unary()?;
            return Ok(Expr::Neg(Box::new(e)));
        }
        self.parse_atom()
    }

    fn make_num(&self, v: f64, u: &str) -> Expr {
        let (v, unit) = match u {
            "" => (v, Unit::Scalar),
            "%" => (v * 0.01, Unit::Scalar),
            "db" => (10f64.powf(v / 20.0), Unit::Scalar),
            "hz" => (v, Unit::Hz),
            "khz" => (v * 1000.0, Unit::Hz),
            "ms" => (v, Unit::Ms),
            "s" => (v * 1000.0, Unit::Ms),
            "st" => (v, Unit::St),
            "ct" => (v / 100.0, Unit::St),
            "beat" | "beats" => (v, Unit::Beat),
            _ => (v, Unit::Scalar),
        };
        Expr::Num { v, unit }
    }

    fn parse_atom(&mut self) -> Result<Expr, String> {
        match self.next()? {
            Tok::Num(v, u) => Ok(self.make_num(v, &u)),
            Tok::Str(s) => Ok(Expr::Str(s)),
            Tok::Sym(s) if s == "(" => {
                let e = self.parse_expr()?;
                self.expect_sym(")")?;
                Ok(e)
            }
            Tok::Sym(s) if s == "[" => self.parse_table(),
            Tok::Id(name) => {
                if name == "env" {
                    return self.parse_env();
                }
                if self.eat_sym("(") {
                    let call = self.parse_call(name)?;
                    return Ok(call);
                }
                Ok(Expr::Ident(name))
            }
            t => Err(format!("unexpected token in expression: {:?}", t)),
        }
    }

    fn parse_call(&mut self, name: String) -> Result<Expr, String> {
        let mut args: Vec<(String, Expr)> = Vec::new();
        let mut pos_idx = 0usize;
        if !self.eat_sym(")") {
            loop {
                // named arg: Ident ':'  else positional
                let mut argname = String::new();
                if let (Some(Tok::Id(id)), Some(Tok::Sym(c))) =
                    (self.toks.get(self.pos), self.toks.get(self.pos + 1))
                {
                    if c == ":" {
                        argname = id.clone();
                        self.pos += 2;
                    }
                }
                if argname.is_empty() {
                    argname = format!("_{}", pos_idx);
                    pos_idx += 1;
                }
                let e = self.parse_expr()?;
                args.push((argname, e));
                if self.eat_sym(",") {
                    continue;
                }
                self.expect_sym(")")?;
                break;
            }
        }
        // adsr sugar -> env
        if name == "adsr" {
            let get = |n: &str| -> Result<Expr, String> {
                args.iter()
                    .find(|(k, _)| k == n)
                    .map(|(_, e)| e.clone())
                    .ok_or(format!("adsr: missing arg {}", n))
            };
            let a = get("attack")?;
            let d = get("decay")?;
            let s = get("sustain")?;
            let r = get("release")?;
            return Ok(Expr::Env {
                start: Some(Box::new(Expr::Num { v: 0.0, unit: Unit::Scalar })),
                segs: vec![
                    Seg { target: Expr::Num { v: 1.0, unit: Unit::Scalar }, time: a, curve: Curve::Exp },
                    Seg { target: s.clone(), time: d, curve: Curve::Exp },
                ],
                sustain: Some(Box::new(s)),
                release: vec![Seg {
                    target: Expr::Num { v: 0.0, unit: Unit::Scalar },
                    time: r,
                    curve: Curve::Exp,
                }],
                id: self.id(),
            });
        }
        let op = Op::from_name(&name);
        Ok(Expr::Call { name, op, args, id: self.id() })
    }

    /// table literal (after '['): rows are tuples '(n, n, ...)' or bare literals.
    /// Values are literals only (evaluated at parse time), units normalized by make_num.
    fn parse_table(&mut self) -> Result<Expr, String> {
        let mut rows: Vec<Vec<(f64, Unit)>> = Vec::new();
        if !self.eat_sym("]") {
            loop {
                let mut row = Vec::new();
                if self.eat_sym("(") {
                    loop {
                        row.push(self.parse_table_num()?);
                        if self.eat_sym(",") {
                            continue;
                        }
                        self.expect_sym(")")?;
                        break;
                    }
                } else {
                    row.push(self.parse_table_num()?);
                }
                if let Some(first) = rows.first() {
                    if first.len() != row.len() {
                        return Err(format!(
                            "{}table rows must have the same arity ({} vs {})",
                            self.here(),
                            first.len(),
                            row.len()
                        ));
                    }
                }
                rows.push(row);
                if self.eat_sym(",") {
                    continue;
                }
                self.expect_sym("]")?;
                break;
            }
        }
        Ok(Expr::Table(rows))
    }

    fn parse_table_num(&mut self) -> Result<(f64, Unit), String> {
        let here = self.here();
        let neg = self.eat_sym("-");
        match self.next()? {
            Tok::Num(v, u) => {
                match self.make_num(if neg { -v } else { v }, &u) {
                    Expr::Num { v, unit } => Ok((v, unit)),
                    _ => unreachable!(),
                }
            }
            t => Err(format!("{}table entries must be literals, got {:?}", here, t)),
        }
    }

    fn parse_curve_opt(&mut self) -> Result<Curve, String> {
        if self.eat_id("curve") {
            let n = self.expect_id()?;
            match n.as_str() {
                "lin" => Ok(Curve::Lin),
                "exp" => Ok(Curve::Exp),
                "log" => Ok(Curve::Log),
                "pow" => {
                    self.expect_sym("(")?;
                    let v = match self.next()? {
                        Tok::Num(v, _) => v,
                        t => return Err(format!("pow expects number, got {:?}", t)),
                    };
                    self.expect_sym(")")?;
                    Ok(Curve::Pow(v))
                }
                o => Err(format!("unknown curve {}", o)),
            }
        } else {
            Ok(Curve::Lin)
        }
    }

    fn parse_env(&mut self) -> Result<Expr, String> {
        self.expect_sym("{")?;
        let mut start: Option<Box<Expr>> = None;
        let mut segs = Vec::new();
        let mut sustain = None;
        let mut release = Vec::new();
        loop {
            if self.eat_sym("}") {
                break;
            }
            if self.eat_id("sustain") {
                sustain = Some(Box::new(self.parse_expr()?));
                continue;
            }
            if self.eat_id("release") {
                self.expect_sym("->")?;
                let target = self.parse_expr()?;
                if !self.eat_id("in") {
                    return Err("env release: expected 'in'".into());
                }
                let time = self.parse_expr()?;
                let curve = self.parse_curve_opt()?;
                release.push(Seg { target, time, curve });
                continue;
            }
            // segment: [start_expr] -> target in time [curve]
            if !self.eat_sym("->") {
                let s = self.parse_expr()?;
                if segs.is_empty() && start.is_none() {
                    start = Some(Box::new(s));
                } else {
                    return Err("env: start value only allowed on first segment".into());
                }
                self.expect_sym("->")?;
            }
            let target = self.parse_expr()?;
            if !self.eat_id("in") {
                return Err("env segment: expected 'in'".into());
            }
            let time = self.parse_expr()?;
            let curve = self.parse_curve_opt()?;
            segs.push(Seg { target, time, curve });
        }
        Ok(Expr::Env { start, segs, sustain, release, id: self.id() })
    }
}

/// tables are compile-time constants: a `let t = [...]` in global/voice is inlined
/// wherever `t` appears, then the let is dropped (tables have no signal value)
fn inline_table_lets(def: &mut SynthDef) {
    let mut tables: std::collections::HashMap<String, Expr> = std::collections::HashMap::new();
    for (n, e) in def.globals.iter().chain(def.voice.iter()) {
        if matches!(e, Expr::Table(_)) {
            tables.insert(n.clone(), e.clone());
        }
    }
    if tables.is_empty() {
        return;
    }
    fn subst(e: &mut Expr, tables: &std::collections::HashMap<String, Expr>) {
        match e {
            Expr::Ident(n) => {
                if let Some(t) = tables.get(n) {
                    *e = t.clone();
                }
            }
            Expr::Bin { l, r, .. } => {
                subst(l, tables);
                subst(r, tables);
            }
            Expr::Neg(x) => subst(x, tables),
            Expr::Call { args, .. } => {
                for (_, a) in args {
                    subst(a, tables);
                }
            }
            Expr::Env { start, segs, sustain, release, .. } => {
                if let Some(s) = start {
                    subst(s, tables);
                }
                for s in segs.iter_mut().chain(release.iter_mut()) {
                    subst(&mut s.target, tables);
                    subst(&mut s.time, tables);
                }
                if let Some(s) = sustain {
                    subst(s, tables);
                }
            }
            _ => {}
        }
    }
    for (_, e) in def.globals.iter_mut().chain(def.voice.iter_mut()) {
        if !matches!(e, Expr::Table(_)) {
            subst(e, &tables);
        }
    }
    subst(&mut def.out, &tables);
    for e in def.bus.iter_mut() {
        subst(e, &tables);
    }
    if let Some(pm) = &mut def.pitch_mod {
        subst(pm, &tables);
    }
    def.globals.retain(|(_, e)| !matches!(e, Expr::Table(_)));
    def.voice.retain(|(_, e)| !matches!(e, Expr::Table(_)));
}

// param parsing lives here as a free function used by Parser::parse_synth replacement
impl Parser {
    pub fn parse_param(&mut self) -> Result<ParamDef, String> {
        let pname = self.expect_id()?;
        self.expect_sym(":")?;
        let ty = self.expect_id()?;
        self.expect_sym("=")?;
        let neg = self.eat_sym("-");
        let (v, u) = match self.next()? {
            Tok::Num(v, u) => (if neg { -v } else { v }, u),
            t => return Err(format!("param default must be literal, got {:?}", t)),
        };
        // unit from literal, else from declared type
        let unit_str = if u.is_empty() {
            match ty.as_str() {
                "hz" => "hz",
                "ms" => "ms",
                "s" => "s",
                "db" => "db",
                _ => "",
            }
        } else {
            u.as_str()
        };
        let default = self.make_num(v, unit_str);
        // optional range / smooth / curve - parsed, ignored by offline engine
        if self.eat_id("range") {
            self.eat_sym("-");
            self.next()?; // min
            self.expect_sym("..")?;
            self.eat_sym("-");
            self.next()?; // max
        }
        if self.eat_id("smooth") {
            self.next()?;
        }
        if self.eat_id("curve") {
            let n = self.expect_id()?;
            if n == "pow" {
                self.expect_sym("(")?;
                self.next()?;
                self.expect_sym(")")?;
            }
        }
        Ok(ParamDef { name: pname, default })
    }
}
