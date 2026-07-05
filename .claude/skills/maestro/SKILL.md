---
name: maestro
description: Compõe música original de altíssima qualidade com o lutier (o synth/DSL deste repo). Use SEMPRE que o usuário pedir música, trilha sonora, tema, jingle, som de jogo, ambiente sonoro, ou mencionar lutier/.synth/.score. Cobre composição, sound design, mixagem e verificação por análise de áudio.
---

# MAESTRO - composição com lutier

Você é um compositor. O lutier renderiza `.synth` (instrumentos) + `.score` (partitura) em WAV/MP3.
Regra de ouro: **você não ouve - então meça.** Todo render termina com análise numérica (seção 7).

## 0. Workflow obrigatório

```
1. Definir: emoção, gênero, tom, bpm, forma (seções), instrumentação (3-6 papéis)
2. Escrever .synth: import presets + no máx 1-2 synths custom + master { }
3. Escrever .score: sections + arrange, automação do arco
4. cargo run --release -- song.synth song.score -o out/song.wav   (do raiz do repo!)
5. Analisar (seção 7). Arco dinâmico errado / mud / clipping → corrigir → re-render
6. Entregar .wav/.mp3 + fontes editáveis
```

Diagnósticos `E*` abortam o render; `W*` são avisos - leia-os, geralmente apontam problema real.

## 1. Presets (use-os; sound design do zero só quando nenhum servir)

```
import "presets/pads.synth"      # strings_warm, choir_vox, pad_dark, pad_glass
import "presets/keys.synth"      # bell_fm, epiano_fm, harp, kalimba, music_box, bell_modal, lead_saw
import "presets/orchestra.synth" # strings_stacc, brass_stab, horn_sustain, flute_lead, organ_church
import "presets/bass.synth"      # subbass, bass_deep, bass_pulse
import "presets/drums.synth"     # taiko, kick_deep, snare, hat_closed, hat_open, shaker, tom_modal
import "presets/sfx.synth"       # sfx_bell_church, sfx_bell_chime, sfx_coin, sfx_powerup, sfx_hit,
                                 # sfx_explosion, sfx_laser, sfx_jump, sfx_ui_click, sfx_ui_error,
                                 # sfx_alarm, sfx_whoosh, sfx_riser, sfx_portal, sfx_bell_bright,
                                 # sfx_gun_pistol, sfx_gun_shotgun, sfx_gun_rifle, sfx_bird
import "presets/physical.synth"  # violino, cello, flauta, clarinete, sino_real, marimba_fisica
                                 # (modelagem fisica pura: bow/flute/reed/modal2/convolve)
```

Músicas de referência (estude antes de compor no mesmo clima): `examples/songs/<nome>/`
- `ofortuna` (coral épico), `lamento` (melancólico épico), `epic` (sinos/orquestral),
`funk` (funk carioca, tamborzão 3-3-2), `vila` (aconchego, swing). SFX prontos:
`examples/sfx/` (showcase com todos os efeitos; para SFX solo use
`bus_gain 6db` no master - one-shot precisa encostar no limiter).

Caminhos relativos ao raiz do repo. Definição local com mesmo nome sobrescreve o preset
(copie o preset pro seu arquivo e edite quando quiser variação). Percussão: toque `c2`/`c3`.
Params automatizáveis: `strings_warm.cutoff`, `choir_vox.morph`, `pad_dark.cutoff`,
`pad_glass.morph`, `bass_deep.drv`, `lead_saw.cutoff`.

## 2. Papéis e ganhos (mix acontece AQUI, não depois)

Um elemento por faixa de frequência. Ganhos de referência (`gain` do synth):

| papel | preset típico | gain | região |
|---|---|---|---|
| fundação | subbass / bass_deep | -6db | 30-300hz, sempre mono |
| cama harmônica | strings_warm / pad_* | -10 a -13db | 200hz-4khz, largo |
| melodia | bell_fm / lead_saw / harp | -6 a -9db | 500hz-5khz, destacada |
| pulso | taiko / kick_deep | -2 a -4db | 40-120hz + transiente |
| brilho/ritmo | hat_* / shaker / music_box | -14 a -16db | 6khz+ |

Sempre feche com master (segura loudness e nunca clipa):

```txt
master {
  bus_gain -1db
  compressor(threshold: -17db, ratio: 2.5, attack: 12ms, release: 200ms, makeup: auto)
  limiter(ceiling: -1db, lookahead: 5ms, release: 80ms)
}
```

## 3. Receitas de emoção (testadas)

| emoção | tom/modo | bpm | receita |
|---|---|---|---|
| melancólico/épico | menor natural | 60-80 | strings+choir+bell_fm+harp+subbass+taiko; dominante maior que NÃO resolve (Dm→A→volta Dm); arco RMS: clímax ≥ 3x o intro |
| tensão/dungeon | frígio ou menor | 70-100 | pad_dark + subbass em pedal + tom_modal esparso; semitons (a#2 sobre a2); silêncio é arma |
| aconchego/vila | maior / mixolídio | 90-110 | kalimba ou music_box + harp + epiano; swing 54-58; humanize 8ms 10% |
| ação/combate | menor / dórico | 120-150 | ostinato de bass_pulse em semicolcheias + taiko 4x4 + lead_saw; sem swing; automate cutoff subindo na entrada |
| mistério/magia | tons inteiros ou lídio | 60-90 | pad_glass + bell_modal; acordes sem terça (só 5ªs e 9ªs) |
| triste/perda | menor, melodia descendente | 50-70 | epiano ou harp solo primeiro, cordas entram na 2ª seção; termine na tônica sem terça |

Progressões que funcionam: menor épico `i-VI-III-VII` (Dm-Bb-F-C); tensão que sobe
`i-VI-iv-V` (clímax no V maior); aconchego `I-V-vi-IV`; lamento clássico: baixo cromático
descendente `i-i/VII-i/VI-V`.

Melodia: pentatônica do tom erra pouco; note alvo do acorde nos tempos fortes; frase de
2 compassos repetida com final diferente (pergunta/resposta); clímax = nota mais aguda da peça.

## 4. Estrutura do .score (sintaxe completa)

```txt
tempo 72                    # (tempo 32 118 = muda pra 118 no beat 32)
track <nome> <synth>        # nome livre, synth = nome do synth
automate cutoff 0 500 -> 64 3800 curve log -> 128 600 curve exp
swing 56                    # só offbeats de colcheia; 50=reto, 66=shuffle
humanize 8ms 10%            # jitter determinístico de tempo/velocity
set drv 0.5                 # param fixo no beat 0

section intro 32            # eventos RELATIVOS ao início da seção
  track str strings_warm    # dentro de section, track troca o alvo
  0 [d3 f3 a3] 8 0.5        # acorde; dur 8 beats; velocity 0.5
  0 d2 0.5 0.9 x16 @1       # x16 = repete 16x, @1 = a cada 1 beat
section climax 32
  ...
arrange intro climax intro  # timeline = concatenação; NÃO misture com eventos absolutos (E021)
```

Notas: `c0..b8`, `#`/`b` (`a#2`, `db4`). Automação: declarar ANTES das sections, beats
absolutos da timeline final.

## 5. Sintaxe .synth essencial (para custom/edição)

```txt
synth nome {
  poly 8 steal oldest spread 60%      # ou: mono glide 30ms legato
  gain -8db
  kill after 5s                       # obrigatório se q>1 ou pluck/modal (voz não morre)
  param cutoff: hz = 900 range 100..8000 smooth 15ms curve log

  global { let m = lfo(shape: sine, rate: 8beat, amount: 0.2) }   # 1x por synth

  voice {                             # 1x por voz; contexto: note, velocity, gate, time, rand
    let osc = saw(freq: note, unison: 5, spread: 12ct, width: 0.7, gain: 0.4)
    let amp = adsr(attack: 1s, decay: 400ms, sustain: 0.8, release: 2s)
    out lowpass(osc, cutoff: cutoff, q: 0.3, slope: 24db) * amp * velocity
  }

  bus {                               # pós-soma de vozes; efeitos caros aqui
    chorus(voices: 3, depth: 40%, rate: 0.4hz, mix: 35%)
    delay_fx(time: 3/8beat, feedback: 45%, mix: 25%, pingpong: true, damp: 4khz)
    reverb(size: 0.85, decay: 3s, damp: 4khz, predelay: 20ms, mix: 25%)
    widen(amount: 40%)
    duck(key: outro_synth, threshold: -24db, ratio: 8, attack: 3ms, release: 180ms, amount: 80%)
  }

  mod {                               # mod matrix (açúcar sobre o grafo)
    vib: lfo(shape: sine, rate: 5.2hz, amount: 1)
    vib * 0.12 -> voice.pitch         # vibrato em semitons
    velocity -> tone.cutoff range 800..4000   # alvo = arg nomeado de um let
  }
}
```

Geradores: `sine/triangle(freq, gain, fm:)`, `saw(freq, unison, spread, width, gain)`,
`pulse(freq, width)`, `wavetable(freq, table: "basic_shapes"|"vox"|"digital", pos)`,
`noise(color: white|pink|brown|blue|violet|velvet|crackle, density:)`,
`pluck(freq, damp, decay, position)`, `modal(freq, modes: bell|bar|membrane|pipe, strike, decay)`,
`sample("arq.wav", pitch: note, root: c3, loop: off|forward|pingpong)` (legado),
`grain(source: "arq.wav", position, size, density, pitch, root, jitter, spread)` (legado).

Física (fase 5 - preferir a samples SEMPRE):
- `modal2(freq, modes: tabela, doublet: 0.15%, strike: 0.3, hard: 0.6, decay: 1, noise: 0.1)` -
  banco modal com tabela do usuário `[(ratio, decay, amp), ...]`, doublets com batimento
  (o "mmm" de sino), martelo físico (hard = dureza do contato). Razões de sino real:
  hum 0.5, prime 1.0, tierce 1.183, quint 1.506, nominal 2.0, deciem 2.662, undeciem 3.011.
  Marimba (barra): 1 / 3.932 / 9.538 / 20.06.
- `nwave(dur: 2.5ms, sharp: 0.9, reflect: 4ms, reflect_gain: 0.4, air: 9khz)` - onda de choque
  coerente (subida instantânea + rampa): o crack de tiro que noise filtrado não dá.
  Receita de tiro: nwave + bandpass(noise, q alto) de corpo + drive/clip como cola.
- `bow(freq, pressure, velocity, position, damp, noise: 0.15)` - corda com arco, juncao
  stick-slip EXATA (MSW: Stribeck resolvida por sample contra o feedback de onda, com
  histerese) + ruido de slip correlacionado (noise:). Afinacao +-3ct em qualquer
  pressao/nota. `velocity` E a arcada: envelope com sustain = nota segurada, release =
  arco para. Passe por convolve de corpo (corpo_violino + corpo_violino_r em ir:/ir2:).
- `flute(freq, pressure, breath)` - flauta física. **pressure útil 0.9..1.2**; abaixo disso
  assobia ou cala. Envelope na pressure = ataque de língua.
- `reed(freq, pressure, stiffness, breath)` - clarinete (tubo fechado, harmônicos ímpares).
  pressure útil 0.7..1.0. Melhor no registro grave (d3..c5).
- `breath(pressure, turbulence)` - fonte de sopro DC + ruído para sopros custom.
- `convolve(sig, ir: nome_de_synth, ir2: outra, dur: 150ms, mix: 100%)` - convolui com IR
  RENDERIZADA de outro synth do arquivo (corpo ressonante, sala). `ir2:` = IR independente
  no canal direito (decorrelacao L/R real de corpos). O synth da IR deve ser one-shot
  (ex.: modal2 com kill after curto); é tocado 1x em c4 e normalizado por energia.
  Use no `bus`. Tabelas: `let m = [(1.0, 8s, 1.0), ...]` são constantes de compilação.
- `brass(freq, pressure, lip: 1.0, bell: 1.5khz, rasp: 0.4, breath: 0.02)` - metal fisico:
  valvula de labio + tubo + campana, com brassiness NO LOOP (escuro -> rasgado continuo
  com a dinamica). pressure util 0.5..1.3 (envelope = sopro). rasp = quanto rasga no forte.
  bell: trompete ~1.8khz, trombone ~1.1khz, trompa ~900hz. Presets: trompete/trompa/trombone.
- `voz(freq, vowel: 0..4, tipo: soprano|alto|tenor|baixo, ens: N, vib, vib_rate, jitter,
  shimmer, breath, tension, spread)` - voz fonte-filtro: fonte glotal + 4 formantes
  publicados por naipe. `ens: 8` = 8 cantores INDEPENDENTES (jitter/shimmer/vibrato/trato
  proprios) = coral de verdade num no. vowel: 0=a 1=e 2=i 3=o 4=u (automatizavel).
  Presets: coral_misto/coral_soprano/coral_alto/coral_baixo.
- `leslie(sig, speed: 0..1, depth, mix)` - gabinete rotativo FISICO (doppler circular + AM
  sincronizada + crossover 800hz, 2 rotores com inercia). speed 1 = tremolo, 0 = chorale.
  Para orgao SEMPRE prefira leslie a chorus (chorus em aditivo = desafinacao percebida).
- `hall(sig, size: 0..1, decay: 2s, damp: 5khz, mix: 25%)` - sala SDN (Scattering Delay
  Network): early reflections geometricas + cauda por recirculacao. ATENCAO: com 6 nos
  a densidade modal e baixa - pode ressoar modos isolados em mix alto; prefira mix <= 10%
  ou continue com reverb() FDN por instrumento ate a versao com difusao.

Processadores: `lowpass/highpass/bandpass/notch(sig, cutoff, q 0..1.2, slope: 24db)`,
`saturate/drive(sig, amount)`, `clip(sig, level)`, `env { 1 -> 0.5 in 80ms curve exp -> 0 in 200ms }`,
`follower(sig, attack, release)`, `rms(sig, window)`, `ringmod(a, b)`, `pan(sig, pos)`,
`gain`, `min/max/clamp/abs`, `hz(pitch)`, `delay(sig, time, feedback)` (curto, p/ física),
`delay1(sig)` (feedback de 1 sample).

Unidades: `hz khz ms s db % st ct beat` (`3/8beat` sync ao bpm; em `rate:` de lfo, beat = PERÍODO).

## 6. Armadilhas conhecidas (custam iterações - decore)

- **FM**: modulador em ratio usa `hz(note) * 2`, NUNCA `note * 2` (note é pitch MIDI, não hz).
- Pitch relativo: `note - 12st` (oitava abaixo). `note + 7st` = quinta.
- `pluck`/`modal`/`q > 1` → voz pode nunca silenciar → **sempre `kill after Ns`**.
- `feedback` de delay_fx máx 0.95 (E014). `mix` de reverb em pad: 20-35%; mais vira sopa.
- `section` + eventos absolutos no mesmo arquivo = E021. `automate` duplicado no track = E020.
- Envelope custom: `env { 1 -> 0 in 60ms curve exp }` - decay percussivo. adsr para sustentados.
- Cadeia típica de out: `out tone * amp * velocity` - esquecer `* amp` = nota infinita.
- Percussão ignora afinação fina; toque c2 (grave) ou c5 (hats), velocity varia o groove (0.3 ghost, 0.95 acento).
- Reverbs/delays ficam no `bus` (1 instância), nunca no `voice` (N instâncias, caro e turvo).
- Sidechain: `duck(key: nome_do_synth, ...)` no bus de quem ABAIXA (pad), key = quem dispara (kick).
- Render roda ~30-70x tempo real (multi-thread); música de 2min ≈ 2-4s. Se demorar muito
  mais, suspeite de dezenas de tracks com reverb próprio (compartilhe bus/master).
- `flute` fora de pressure 0.9..1.2 assobia num modo errado ou cala; `reed` trava acima de ~1.2.
- `bow` sem envelope em `velocity` = arcada eterna (nota nunca articula).
- `convolve` no `voice` roda por VOZ (caro); quase sempre pertence ao `bus`.
- Tabela `[( ... )]` só aceita literais; a let da tabela é inlined e não vira sinal.
- Física (pluck/modal/modal2/bow/flute/reed) sem `kill after` = W006 (voz pode não morrer).
- `brass` afina por busca de modo: ataques fortes tem "blat" de 20-60ms (fisico, nao bug).
- Secao de cordas = N instancias de `bow` com detune/ataque/vibrato/pan proprios (presets
  secao_violinos/secao_stacc), NUNCA saw + chorus (correlacao demais, ouvido rotula synth).

## 7. Verificação (obrigatória antes de entregar)

```bash
python3 - <<'EOF'
import wave,struct,math
w=wave.open('out/song.wav');n=w.getnframes();d=w.readframes(n)
xs=struct.unpack('<%dh'%(n*2),d);sr=44100
dur=n//sr
win=max(dur//12,2)
for t in range(0,dur,win):
    seg=xs[t*sr*2:(t+win)*sr*2]
    if not seg: break
    rms=math.sqrt(sum(x*x for x in seg[::7])/len(seg[::7]))/32768
    db=20*math.log10(max(rms,1e-6))
    print(f"{t:4d}s {db:6.1f}db {'#'*max(0,int((db+60)/2))}")
L=xs[0::2];R=xs[1::2]
peak=max(max(map(abs,L)),max(map(abs,R)))/32768
side=sum(abs(l-r) for l,r in zip(L[::200],R[::200]))/len(L[::200])/32768
print(f"peak={peak:.3f} (esperado ~0.89 c/ master)  width={side:.4f} (>0.01 = estéreo vivo)")
EOF
```

Critérios de aceitação:
- **Arco**: diferença intro→clímax ≥ 8db (épico ≥ 10db). Flat = chato → automatize cutoff/adicione camadas por seção.
- **peak ≈ 0.891** com master (limiter em -1db). Peak baixo (<0.5) = mixagem fraca antes do master.
- **width > 0.01**; se ~0, faltou spread/chorus/widen.
- Nenhuma seção com rms < -50db no meio da peça (buraco), exceto silêncio intencional.
- Fim: cauda de reverb morre naturalmente (render já soma 4s de cauda).

Se falhar: corrija o `.score`/`.synth` e re-renderize. Máximo 3 iterações; depois entregue com nota do trade-off.

## 8. Reprodutibilidade e variações

- `--seed 42`: mesmo arquivo = mesmo áudio bit a bit (noise/unison determinísticos).
- Variação de uma peça: mude seed, troque 1 instrumento de papel, transponha o score, ou re-arranje as sections (`arrange intro drop intro` → loop de jogo perfeito).
- Loop para jogo: termine a última seção no acorde do início e corte a cauda de 4s no pós-processamento, ou peça fade.
