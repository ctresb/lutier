// lutier DAW — piano roll minimalista sobre a engine lutier.
// Estado -> gera .synth/.score -> POST /render -> toca o WAV.

"use strict";

// ---------- instrumentos (nome -> preset que o define) ----------

const INSTRUMENTS = {
  // keys
  kalimba: "keys", epiano_fm: "keys", bell_fm: "keys", harp: "keys",
  music_box: "keys", bell_modal: "keys", lead_saw: "keys",
  // pads
  strings_warm: "pads", choir_vox: "pads", pad_dark: "pads", pad_glass: "pads",
  // orquestra
  strings_stacc: "orchestra", brass_stab: "orchestra", horn_sustain: "orchestra",
  flute_lead: "orchestra", organ_church: "orchestra",
  // física
  violino: "physical", cello: "physical", flauta: "physical", clarinete: "physical",
  sino_real: "physical", marimba_fisica: "physical", trompete: "physical",
  trompa: "physical", trombone: "physical", coral_misto: "physical",
  // graves
  subbass: "bass", bass_deep: "bass", bass_pulse: "bass",
  // bateria
  kick_deep: "drums", snare: "drums", hat_closed: "drums", hat_open: "drums",
  shaker: "drums", taiko: "drums", tom_modal: "drums",
};

const COLORS = ["#ffcc44", "#66ccff", "#88dd66", "#ff8866",
                "#cc99ff", "#ffaacc", "#77ddcc", "#ddbb88"];

// ---------- geometria ----------

const BEAT_W = 96;          // px por beat (1/16 = 24px)
const ROW_H = 18;           // px por semitom
const MIDI_TOP = 95;        // b6
const MIDI_BOT = 24;        // c1
const ROWS = MIDI_TOP - MIDI_BOT + 1;
const NAMES = ["c", "c#", "d", "d#", "e", "f", "f#", "g", "g#", "a", "a#", "b"];

const midiName = (m) => NAMES[m % 12] + (Math.floor(m / 12) - 1);
const midiFreq = (m) => 440 * Math.pow(2, (m - 69) / 12);
const isBlack = (m) => NAMES[m % 12].includes("#");
const fmt = (x) => (Math.round(x * 100) / 100).toString();
const clamp = (x, a, b) => Math.min(b, Math.max(a, x));

// ---------- estado ----------

let state = {
  tempo: 110,
  beats: 32,
  tracks: [],   // {id, name, inst, mute, notes:[{beat, midi, dur, vel}]}
  sel: 0,       // faixa selecionada
};
let nextId = 1;
let selNote = null;         // nota selecionada {trackIdx, noteIdx}
let renderCache = { key: null, url: null };

// ---------- elementos ----------

const $ = (id) => document.getElementById(id);
const elGrid = $("grid"), elKeys = $("keys"), elRuler = $("ruler");
const elTracks = $("track-list"), elPlayhead = $("playhead");
const elStatus = $("status"), player = $("player");
const inTempo = $("in-tempo"), inBeats = $("in-beats");
const inSnap = $("in-snap"), inVel = $("in-vel"), outVel = $("out-vel");

const snap = () => parseFloat(inSnap.value);
const defVel = () => parseInt(inVel.value, 10) / 100;
const curTrack = () => state.tracks[state.sel];

// ---------- persistência ----------

function save() {
  localStorage.setItem("lutier-daw", JSON.stringify(state));
  renderCache.key = null; // estado mudou: próximo play re-renderiza
}

function load() {
  try {
    const raw = localStorage.getItem("lutier-daw");
    if (raw) {
      state = JSON.parse(raw);
      nextId = 1 + Math.max(0, ...state.tracks.map((t) => t.id));
    }
  } catch (_) { /* estado corrompido: começa limpo */ }
  if (!state.tracks.length) addTrack("kalimba");
  state.sel = clamp(state.sel, 0, state.tracks.length - 1);
  inTempo.value = state.tempo;
  inBeats.value = state.beats;
}

// ---------- faixas ----------

function addTrack(inst) {
  const id = nextId++;
  state.tracks.push({
    id,
    name: "faixa " + id,
    inst: inst || "kalimba",
    mute: false,
    notes: [],
  });
  state.sel = state.tracks.length - 1;
  selNote = null;
}

function trackColor(i) { return COLORS[state.tracks[i].id % COLORS.length]; }

function drawTracks() {
  elTracks.replaceChildren();
  state.tracks.forEach((t, i) => {
    const row = document.createElement("div");
    row.className = "track" + (i === state.sel ? " sel" : "");
    row.addEventListener("pointerdown", () => {
      if (state.sel !== i) { state.sel = i; selNote = null; drawTracks(); drawNotes(); }
    });

    const sw = document.createElement("div");
    sw.className = "swatch";
    sw.style.background = trackColor(i);

    const name = document.createElement("input");
    name.type = "text";
    name.value = t.name;
    name.setAttribute("aria-label", "nome da faixa");
    name.addEventListener("change", () => { t.name = name.value || t.name; save(); });

    const mute = document.createElement("button");
    mute.className = "tbtn" + (t.mute ? " on" : "");
    mute.textContent = "M";
    mute.title = t.mute ? "Faixa muda — clique para ouvir" : "Silenciar faixa";
    mute.setAttribute("aria-pressed", String(t.mute));
    mute.addEventListener("click", (e) => {
      e.stopPropagation();
      t.mute = !t.mute; save(); drawTracks();
    });

    const del = document.createElement("button");
    del.className = "tbtn del";
    del.textContent = "×";
    del.title = "Apagar faixa";
    del.addEventListener("click", (e) => {
      e.stopPropagation();
      if (t.notes.length && !confirm(`Apagar "${t.name}" com ${t.notes.length} nota(s)?`)) return;
      state.tracks.splice(i, 1);
      if (!state.tracks.length) addTrack("kalimba");
      state.sel = clamp(state.sel, 0, state.tracks.length - 1);
      selNote = null;
      save(); drawTracks(); drawNotes();
    });

    const sel = document.createElement("select");
    sel.setAttribute("aria-label", "instrumento");
    for (const inst of Object.keys(INSTRUMENTS)) {
      const o = document.createElement("option");
      o.value = o.textContent = inst;
      sel.append(o);
    }
    sel.value = t.inst;
    sel.addEventListener("change", () => { t.inst = sel.value; save(); });
    sel.addEventListener("pointerdown", (e) => e.stopPropagation());

    row.append(sw, name, mute, del, sel);
    elTracks.append(row);
  });
}

// ---------- grade ----------

function drawStatic() {
  const w = state.beats * BEAT_W, h = ROWS * ROW_H;
  elGrid.style.width = w + "px";
  elGrid.style.height = h + "px";
  elRuler.style.width = w + "px";

  elKeys.replaceChildren();
  for (let r = 0; r < ROWS; r++) {
    const m = MIDI_TOP - r;
    const k = document.createElement("div");
    k.className = "key" + (isBlack(m) ? " black" : m % 12 === 0 ? " c" : "");
    k.textContent = m % 12 === 0 || !isBlack(m) ? midiName(m) : "";
    elKeys.append(k);
  }

  elRuler.replaceChildren();
  for (let b = 0; b < state.beats; b++) {
    const d = document.createElement("div");
    d.className = "ruler-beat" + (b % 4 === 0 ? " bar" : "");
    d.style.left = b * BEAT_W + "px";
    d.textContent = b % 4 === 0 ? String(b) : "·";
    elRuler.append(d);
  }
}

function drawNotes() {
  // remove tudo menos o playhead
  for (const el of [...elGrid.children]) if (el !== elPlayhead) el.remove();

  // sombreamento das linhas de teclas pretas + barras de compasso
  for (let r = 0; r < ROWS; r++) {
    if (!isBlack(MIDI_TOP - r)) continue;
    const s = document.createElement("div");
    s.className = "rowshade";
    s.style.top = r * ROW_H + "px";
    elGrid.append(s);
  }
  for (let b = 4; b < state.beats; b += 4) {
    const l = document.createElement("div");
    l.className = "barline";
    l.style.left = b * BEAT_W + "px";
    elGrid.append(l);
  }

  state.tracks.forEach((t, ti) => {
    t.notes.forEach((n, ni) => {
      const el = document.createElement("div");
      el.className = "note" + (ti !== state.sel ? " ghost" : "");
      if (selNote && selNote.trackIdx === ti && selNote.noteIdx === ni) el.classList.add("sel");
      el.style.left = n.beat * BEAT_W + "px";
      el.style.top = (MIDI_TOP - n.midi) * ROW_H + "px";
      el.style.width = Math.max(6, n.dur * BEAT_W - 1) + "px";
      el.style.background = trackColor(ti);
      el.style.opacity = ti === state.sel ? String(0.45 + 0.55 * n.vel) : "";
      el.title = `${midiName(n.midi)}  beat ${fmt(n.beat)}  dur ${fmt(n.dur)}  vel ${fmt(n.vel)}`;
      if (ti === state.sel) {
        el.dataset.idx = ni;
        const grip = document.createElement("div");
        grip.className = "grip";
        el.append(grip);
      }
      elGrid.append(el);
    });
  });
}

// ---------- interação no grid ----------

let drag = null; // {mode:'create'|'move'|'resize', noteIdx, offBeat, startMidi}

function gridPos(e) {
  const r = elGrid.getBoundingClientRect();
  return {
    beat: (e.clientX - r.left) / BEAT_W,
    midi: MIDI_TOP - Math.floor((e.clientY - r.top) / ROW_H),
  };
}

elGrid.addEventListener("pointerdown", (e) => {
  if (e.button === 2) return;
  const t = curTrack();
  const p = gridPos(e);
  const noteEl = e.target.closest(".note");
  elGrid.focus({ preventScroll: true });

  if (noteEl) {
    const ni = parseInt(noteEl.dataset.idx, 10);
    const n = t.notes[ni];
    selNote = { trackIdx: state.sel, noteIdx: ni };
    inVel.value = Math.round(n.vel * 100);
    outVel.value = fmt(n.vel);
    drag = e.target.classList.contains("grip")
      ? { mode: "resize", noteIdx: ni }
      : { mode: "move", noteIdx: ni, offBeat: p.beat - n.beat, startMidi: n.midi };
  } else {
    const beat = clamp(Math.floor(p.beat / snap()) * snap(), 0, state.beats - snap());
    const midi = clamp(p.midi, MIDI_BOT, MIDI_TOP);
    t.notes.push({ beat, midi, dur: snap(), vel: defVel() });
    selNote = { trackIdx: state.sel, noteIdx: t.notes.length - 1 };
    drag = { mode: "resize", noteIdx: t.notes.length - 1 };
    blip(midi);
  }
  elGrid.setPointerCapture(e.pointerId);
  drawNotes();
});

elGrid.addEventListener("pointermove", (e) => {
  if (!drag) return;
  const t = curTrack();
  const n = t.notes[drag.noteIdx];
  if (!n) return;
  const p = gridPos(e);

  if (drag.mode === "resize") {
    n.dur = clamp(Math.ceil((p.beat - n.beat) / snap()) * snap(), snap(), state.beats - n.beat);
  } else {
    const beat = Math.round((p.beat - drag.offBeat) / snap()) * snap();
    n.beat = clamp(beat, 0, state.beats - n.dur);
    const midi = clamp(p.midi, MIDI_BOT, MIDI_TOP);
    if (midi !== n.midi) { n.midi = midi; blip(midi); }
  }
  drawNotes();
});

elGrid.addEventListener("pointerup", () => { if (drag) { drag = null; save(); } });
elGrid.addEventListener("lostpointercapture", () => { if (drag) { drag = null; save(); } });

function deleteNoteAt(e) {
  const noteEl = e.target.closest(".note");
  if (!noteEl || noteEl.classList.contains("ghost")) return;
  curTrack().notes.splice(parseInt(noteEl.dataset.idx, 10), 1);
  selNote = null;
  drag = null;
  save(); drawNotes();
}

elGrid.addEventListener("contextmenu", (e) => { e.preventDefault(); deleteNoteAt(e); });
elGrid.addEventListener("dblclick", deleteNoteAt);

elGrid.addEventListener("keydown", (e) => {
  if (!selNote || selNote.trackIdx !== state.sel) return;
  const n = curTrack().notes[selNote.noteIdx];
  if (!n) return;
  const step = { ArrowLeft: [-snap(), 0], ArrowRight: [snap(), 0],
                 ArrowUp: [0, 1], ArrowDown: [0, -1] }[e.key];
  if (e.key === "Delete" || e.key === "Backspace") {
    curTrack().notes.splice(selNote.noteIdx, 1);
    selNote = null;
    save(); drawNotes();
    e.preventDefault();
  } else if (step) {
    n.beat = clamp(n.beat + step[0], 0, state.beats - n.dur);
    n.midi = clamp(n.midi + step[1], MIDI_BOT, MIDI_TOP);
    if (step[1]) blip(n.midi);
    save(); drawNotes();
    e.preventDefault();
  }
});

// velocity: slider edita a nota selecionada, senão vira o padrão de novas notas
inVel.addEventListener("input", () => {
  outVel.value = fmt(defVel());
  if (selNote && selNote.trackIdx === state.sel) {
    const n = curTrack().notes[selNote.noteIdx];
    if (n) { n.vel = defVel(); save(); drawNotes(); }
  }
});

// ---------- preview de altura (blip local, sem render) ----------

let ac = null;
function blip(midi) {
  ac = ac || new AudioContext();
  const o = ac.createOscillator(), g = ac.createGain();
  o.frequency.value = midiFreq(midi);
  o.type = "triangle";
  g.gain.setValueAtTime(0.12, ac.currentTime);
  g.gain.exponentialRampToValueAtTime(0.0001, ac.currentTime + 0.12);
  o.connect(g).connect(ac.destination);
  o.start();
  o.stop(ac.currentTime + 0.13);
}

// ---------- geração de .synth / .score ----------

function genSynth() {
  const presets = new Set(state.tracks.map((t) => INSTRUMENTS[t.inst] || "keys"));
  let out = "# gerado pelo lutier DAW\n";
  for (const p of [...presets].sort()) out += `import "presets/${p}.synth"\n`;
  out += "\nmaster {\n  bus_gain -1db\n";
  out += "  compressor(threshold: -18db, ratio: 2, attack: 15ms, release: 180ms, makeup: auto)\n";
  out += "  limiter(ceiling: -1db, lookahead: 5ms, release: 60ms)\n}\n";
  return out;
}

function genScore(includeMuted) {
  let out = `# gerado pelo lutier DAW\ntempo ${state.tempo}\n`;
  state.tracks.forEach((t, i) => {
    if (!t.notes.length || (!includeMuted && t.mute)) return;
    out += `\n# ${t.name}\ntrack t${i + 1} ${t.inst}\n`;
    for (const n of [...t.notes].sort((a, b) => a.beat - b.beat || b.midi - a.midi)) {
      out += `${fmt(n.beat)} ${midiName(n.midi)} ${fmt(n.dur)} ${fmt(n.vel)}\n`;
    }
  });
  return out;
}

function download(name, text) {
  const a = document.createElement("a");
  a.href = URL.createObjectURL(new Blob([text], { type: "text/plain" }));
  a.download = name;
  a.click();
  URL.revokeObjectURL(a.href);
}

$("btn-score").addEventListener("click", () => download("song.score", genScore(true)));
$("btn-synth").addEventListener("click", () => download("song.synth", genSynth()));

// ---------- transporte ----------

const btnPlay = $("btn-play"), btnStop = $("btn-stop");
let rafId = 0;

function setStatus(msg, cls) {
  elStatus.textContent = msg;
  elStatus.className = cls || "";
}

async function play() {
  const hasNotes = state.tracks.some((t) => !t.mute && t.notes.length);
  if (!hasNotes) { setStatus("adicione notas na grade para tocar", "err"); return; }

  const synth = genSynth(), score = genScore(false);
  const key = synth + " " + score;

  if (renderCache.key !== key) {
    btnPlay.disabled = true;
    setStatus("renderizando…", "busy");
    try {
      const res = await fetch("/render", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ synth, score }),
      });
      if (!res.ok) {
        const err = await res.json().catch(() => ({ error: "falha no render (" + res.status + ")" }));
        setStatus(err.error, "err");
        return;
      }
      if (renderCache.url) URL.revokeObjectURL(renderCache.url);
      renderCache = { key, url: URL.createObjectURL(await res.blob()) };
    } catch (e) {
      setStatus("servidor fora do ar? rode: python3 daw/server.py", "err");
      return;
    } finally {
      btnPlay.disabled = false;
    }
  }

  player.src = renderCache.url;
  await player.play();
  btnStop.disabled = false;
  setStatus("tocando", "");
  elPlayhead.hidden = false;
  cancelAnimationFrame(rafId);
  const tick = () => {
    elPlayhead.style.left = (player.currentTime * state.tempo / 60) * BEAT_W + "px";
    rafId = requestAnimationFrame(tick);
  };
  tick();
}

function stop() {
  player.pause();
  player.currentTime = 0;
  cancelAnimationFrame(rafId);
  elPlayhead.hidden = true;
  btnStop.disabled = true;
  setStatus("", "");
}

btnPlay.addEventListener("click", play);
btnStop.addEventListener("click", stop);
player.addEventListener("ended", stop);

document.addEventListener("keydown", (e) => {
  if (e.code === "Space" && !/INPUT|SELECT|TEXTAREA/.test(document.activeElement.tagName)) {
    e.preventDefault();
    player.paused ? play() : stop();
  }
});

// ---------- toolbar ----------

inTempo.addEventListener("change", () => {
  state.tempo = clamp(parseInt(inTempo.value, 10) || 110, 20, 300);
  inTempo.value = state.tempo;
  save();
});

inBeats.addEventListener("change", () => {
  state.beats = clamp(parseInt(inBeats.value, 10) || 32, 4, 512);
  inBeats.value = state.beats;
  for (const t of state.tracks) {
    t.notes = t.notes.filter((n) => n.beat < state.beats);
    for (const n of t.notes) n.dur = Math.min(n.dur, state.beats - n.beat);
  }
  save(); drawStatic(); drawNotes();
});

$("btn-add-track").addEventListener("click", () => {
  addTrack("kalimba");
  save(); drawTracks(); drawNotes();
});

// ---------- boot ----------

load();
drawStatic();
drawTracks();
drawNotes();
outVel.value = fmt(defVel());
// rola para o meio do teclado (c4 visível)
document.getElementById("editor").scrollTop = (MIDI_TOP - 72) * ROW_H;
