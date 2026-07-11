---
name: maestro
description: Compõe música original de altíssima qualidade com o lutier (o synth/DSL deste repo). Use SEMPRE que o usuário pedir música, trilha sonora, tema, jingle, som de jogo, ambiente sonoro, batida, SFX, riser/transição, ou mencionar lutier/.synth/.score. Cobre composição, sound design, mixagem e verificação por análise de áudio.
---

# MAESTRO - composição com lutier

Você é um compositor. O lutier renderiza `.synth` (instrumentos) + `.score` (partitura) em WAV/MP3.
Regra de ouro: **você não ouve - então meça.** Todo render termina com análise numérica (seção 9).

## 0. Workflow obrigatório

```
1. Definir: emoção, gênero, tom, bpm, forma (seções), instrumentação (3-6 papéis)
2. Escrever .synth: import presets + no máx 1-2 synths custom + master { }
3. Escrever .score: sections + arrange, automação do arco, transições entre seções
4. cargo run --release -- song.synth song.score -o out/song.wav   (do raiz do repo!)
5. Analisar (seção 9). Arco errado / mud / clipping / buraco -> corrigir -> re-render
6. Entregar .wav/.mp3 + fontes editáveis
```

Diagnósticos `E*` abortam o render; `W*` são avisos - leia-os, geralmente apontam problema real.
Máximo 3 iterações de correção; depois entregue com nota do trade-off.

## 1. Presets (use-os; sound design do zero só quando nenhum servir)

```
import "presets/pads.synth"      # strings_warm, choir_vox, pad_dark, pad_glass
import "presets/keys.synth"      # bell_fm, epiano_fm, harp, kalimba, music_box, bell_modal, lead_saw
import "presets/orchestra.synth" # strings_stacc, brass_stab, horn_sustain, flute_lead, organ_church
import "presets/bass.synth"      # subbass, bass_deep, bass_pulse
import "presets/drums.synth"     # taiko, kick_deep, snare, hat_closed, hat_open, shaker, tom_modal
import "presets/funk.synth"      # kick_808, bass_808 (glide), kick_tamborzao, tamborim, atabaque,
                                 # clap_funk, snare_seca, hat_trap, stab_funk, apito
import "presets/nature.synth"    # vento, chuva, oceano, riacho, trovao, fogueira, grilos,
                                 # sapos, passaros (ambiências sustentadas; segure a nota)
import "presets/sfx.synth"       # sfx_bell_church, sfx_bell_chime, sfx_coin, sfx_powerup, sfx_hit,
                                 # sfx_explosion, sfx_laser, sfx_jump, sfx_ui_click, sfx_ui_error,
                                 # sfx_alarm, sfx_whoosh, sfx_portal, sfx_bell_bright, sfx_bird,
                                 # sfx_gun_pistol, sfx_gun_shotgun, sfx_gun_rifle
                                 # transições: sfx_riser, sfx_riser_tonal, sfx_riser_noise,
                                 # sfx_shepard, sfx_downlifter, sfx_sub_drop, sfx_impact
import "presets/physical.synth"  # violino, cello, flauta, flauta_doce, clarinete, sino_real, marimba_fisica,
                                 # trompete, trompa, trombone, coral_*, secao_violinos, secao_stacc
                                 # (modelagem física pura: bow/flute/reed/brass/voz/modal2/convolve)
```

Referências para estudar antes de compor no mesmo clima: `examples/songs/<nome>/`
(`vila` aconchego/swing, `lamento` melancólico épico, `epic` sinos/orquestral,
`funky` funk carioca, `infortunata`). SFX prontos em `examples/sfx/`.

Caminhos relativos ao raiz do repo. Definição local com mesmo nome sobrescreve o preset
(copie o preset pro seu arquivo e edite quando quiser variação). Percussão: toque `c2`/`c3`
(grave) ou `c5` (agudos). Params automatizáveis: `strings_warm.cutoff`, `choir_vox.morph`,
`pad_dark.cutoff`, `pad_glass.morph`, `bass_deep.drv`, `lead_saw.cutoff`.

## 2. Papéis e ganhos (mix acontece AQUI, não depois)

Um elemento por faixa de frequência. Ganhos de referência (`gain` do synth):

| papel | preset típico | gain | região |
|---|---|---|---|
| fundação | subbass / bass_deep / bass_808 | -6db | 30-300hz, sempre mono |
| cama harmônica | strings_warm / pad_* / ambiência | -10 a -13db | 200hz-4khz, largo |
| melodia | bell_fm / lead_saw / harp / violino | -6 a -9db | 500hz-5khz, destacada |
| pulso | taiko / kick_deep / kick_tamborzao | -2 a -4db | 40-120hz + transiente |
| brilho/ritmo | hat_* / shaker / tamborim | -14 a -16db | 6khz+ |
| transição | sfx_riser / downlifter / impact | -8 a -12db | full, pontual |

Sempre feche com master (segura loudness e nunca clipa):

```txt
master {
  bus_gain -1db
  compressor(threshold: -17db, ratio: 2.5, attack: 12ms, release: 200ms, makeup: auto)
  limiter(ceiling: -1db, lookahead: 5ms, release: 80ms)
}
```

SFX solo: `bus_gain 6db` (one-shot não enche o pré-ganho). Batida pesada
(funk/trap): `ratio: 3, attack: 8ms` soca mais.

## 3. Melodia, harmonia e arranjo (a diferença entre tocar e emocionar)

**Melodia** - regras que funcionam:
- Motivo de 1-2 compassos; repita-o TRANSFORMADO (transposto no acorde, invertido,
  ritmo aumentado) - não repita literal mais de 2x, não jogue notas aleatórias.
- Pergunta/resposta: frase A termina fora da tônica (suspensa), frase A' resolve.
- Contorno: arco (sobe-clímax-desce) por frase; UM pico mais agudo por seção,
  o pico da peça inteira aparece 1x só, no clímax.
- Tempos fortes = nota do acorde; passagem/cromatismo nos fracos.
- Pentatônica do tom erra pouco; mas a 4ª e a 7ª (evitadas na pentatônica) são
  exatamente onde mora a tensão - use-as de passagem para não soar "plugin de videogame".
- Registro: melodia 1-2 oitavas acima do baixo; nunca cole melodia e acompanhamento
  na mesma oitava (mud).

**Função de cada nota (a regra que evita "ruído aleatório")** - todo evento no score
precisa de UMA função nomeável: motivo, harmonia, contraponto, resposta ou virada.
Se você não consegue dizer qual é, DELETE - vai soar como ruído solto ("pra quê?").
- Ornamento: só como eco/imitação do motivo (mesma figura, outro registro), nunca
  nota avulsa bonitinha no offbeat.
- Resposta (apito, sino): sempre no MESMO ponto da frase (fim de pergunta, cadência),
  em toda repetição - consistência transforma efeito em personagem.
- Virada (fill de tom): só no fim de frase de 8 compassos, apontando pra seção seguinte.
- Transição atonal (riser_noise, whoosh): NÃO entra em música tonal leve; prepare
  modulação com a dominante (gliss de harpa / riser_tonal na dominante do tom novo).
- Contraponto: contramelodia anda quando a melodia para (e vice-versa), terças/sextas
  nos encontros; nunca duas melodias ativas ao mesmo tempo no mesmo registro.

**Harmonia** - progressões testadas:
menor épico `i-VI-III-VII` (Dm-Bb-F-C) · tensão que sobe `i-VI-iv-V` (clímax no V maior)
· aconchego `I-V-vi-IV` · lamento: baixo cromático descendo `i-i/VII-i/VI-V`
· funk: 1-2 acordes só, a tensão vem do ritmo · mistério: acordes sem terça (5ªs/9ªs)
· final suspenso: termine na tônica sem terça; final resolvido: terça no acorde final.
Voice leading: entre acordes mova cada voz o MÍNIMO (nota comum fica); inversões
(`[e3 g3 c4]` = C/E) deixam o baixo andar por grau conjunto.

**Arranjo** - o arco dinâmico é o que segura atenção:
- Cada seção nova muda UMA coisa grande: +camada, -camada, oitava, densidade rítmica.
- Clímax ≥ 8db acima do intro (épico ≥ 10db): mais camadas + automate cutoff subindo.
- Transições marcam a costura: riser nos 4-8 beats finais da seção que antecede o
  clímax/drop; impact ou sub_drop no beat 1 da seção nova; downlifter depois do drop.
- Silêncio é arma: 1/2 beat de corte geral antes do drop vale mais que +3db.

## 4. Receitas de gênero (testadas)

| clima | tom/modo | bpm | receita |
|---|---|---|---|
| melancólico/épico | menor natural | 60-80 | strings+choir+bell_fm+harp+subbass+taiko; dominante maior que NÃO resolve; arco RMS clímax ≥ 3x intro |
| tensão/dungeon | frígio ou menor | 70-100 | pad_dark + subbass pedal + tom_modal esparso; semitons (a#2 sobre a2); silêncio é arma |
| aconchego/vila | maior / mixolídio | 90-110 | kalimba ou music_box + harp + epiano; swing 54-58; humanize 8ms 10% |
| ação/combate | menor / dórico | 120-150 | ostinato bass_pulse 16ths + taiko 4x4 + lead_saw; automate cutoff subindo na entrada; riser a cada 16 beats |
| mistério/magia | tons inteiros ou lídio | 60-90 | pad_glass + bell_modal; acordes sem terça |
| triste/perda | menor, melodia descendo | 50-70 | epiano ou harp solo, cordas entram na 2ª seção; termina tônica sem terça |
| funk carioca | menor, 1-2 acordes | 125-135 | ver tamborzão abaixo |
| trap | menor/frígio | 130-150 (half-time) | kick_808 longo (nota = sub cantado), snare_seca no 3, hat_trap x16 @0.25 com rolls @0.125, bass_808 glide |
| EDM build/drop | menor | 124-128 | build: sfx_riser 8 beats + snare acelerando; drop: sub_drop + baixo cheio; pós-drop: downlifter |
| sinfonia/orquestral | qualquer | 60-120 | ver seção orquestral abaixo |
| ambiente/natureza | - | livre | 2-3 camadas de nature.synth em notas longas + 1 elemento musical esparso (harp/bell 1 nota a cada 4-8 beats) |

**Tamborzão (funk carioca)** - o groove é lei, período de 2 beats (3-3-2 em 16ths):

```txt
tempo 130
section loop 8
  track k kick_tamborzao
  0    c2 0.4 0.95
  0.75 c2 0.4 0.8      # o "e" do 1 - a alma do 3-3-2
  1.5  c2 0.4 0.9
  track s snare_seca
  1 c4 0.3 0.85 x4 @2  # caixa no 2 de cada compasso
  track t tamborim     # costura sincopada
  0.5 c5 0.2 0.6 / 1.25 0.5 / 1.75 0.7 (repete a cada 2 beats)
  track b bass_808     # segue os kicks, notas coladas p/ glide
  0 c2 0.75 0.9 / 0.75 eb2 0.75 0.8 / 1.5 g1 2.5 0.85
```

stab_funk em acordes curtos no contratempo, apito 1-2x por 8 beats. Sem swing
(o balanço vem do 3-3-2), humanize 4ms 5% no tamborim.

**Orquestral/sinfonia** - física antes de saw+reverb:
- Cordas: `secao_violinos`/`secao_stacc` (N bows independentes) ou `violino`/`cello`
  solo com convolve de corpo. NUNCA saw+chorus para "cordas reais".
- Metais: `trompete`/`trompa`/`trombone` (brass físico; ataque forte tem "blat"
  de 20-60ms - é físico, não bug). Acordes de metal: 3-4 notas fechadas no médio.
- Coro: `coral_misto` etc (`ens: 8` = 8 cantores independentes); automate `vowel`.
- Madeiras: `flauta` (toda a faixa de velocity canta; pp = escuro e soproso,
  ff = brilhante), `clarinete` (grave, d3-c5; pp soproso chalumeau, ff brilha).
- Forma clássica de trailer: ostinato de cordas graves -> +trompa pedal -> +coro
  -> tutti com taiko -> corte seco -> sino/impact final.
- Dinâmica orquestral = velocity EM CAMADAS (pp 0.3 / mf 0.6 / ff 0.9), não gain.

**Sons da natureza / cenas** - camadas com papéis (igual música):
cama (vento/oceano/chuva) + textura média (riacho/fogueira/grilos) + eventos
esparsos (passaros/trovao/sapos). Ambiências sustentam enquanto a nota dura -
use notas de 16-32 beats e velocity como intensidade da cena. Dia: passaros+vento;
noite: grilos+sapos+fogueira; tempestade: chuva+vento forte+trovao (velocity baixa
= longe). Adicione 1 instrumento esparso para "cena de jogo com música".

## 5. Risers e transições (decore - é o que separa demo de produção)

Os risers de `presets/sfx.synth` **escalam com a duração da nota** (builtin `dur`):
a rampa ocupa a nota inteira e termina EXATA no que vem depois. Nada de adivinhar segundos.

```txt
# drop no beat 32: riser nos 8 beats anteriores
track r sfx_riser
24 c3 8 0.9          # 8 beats de subida, termina em cheio no beat 32
```

| synth | quando usar |
|---|---|
| `sfx_riser` | pré-drop/clímax padrão: cluster tonal + ruído + tick acelerando + sub no final |
| `sfx_riser_tonal` | dentro da harmonia (sobe UMA oitava exata; toque a tônica) |
| `sfx_riser_noise` | transição sutil de cena/UI, atonal, encaixa em qualquer tom |
| `sfx_shepard` | tensão contínua "infinita" (segure a nota; ciclo de 12 beats) |
| `sfx_downlifter` | DEPOIS do drop: cai e abre espaço (2-4 beats) |
| `sfx_sub_drop` | primeiro beat do drop: "woooum" descendo (nota curta, c2) |
| `sfx_impact` | pontuação cinematográfica: boom+crack+anel (1 nota, c2) |

Regras: riser termina no beat do drop (não 1 beat antes/depois); duração 4-8 beats
(2 beats = susto, 16 = tensão de cena); no beat do drop empilhe impact OU sub_drop
(não os dois); riser + corte de todas as camadas por 1/4 beat + drop = o "vácuo" clássico.

## 6. Estrutura do .score (sintaxe completa)

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

Notas: `c0..b8`, `#`/`b` (`a#2`, `db4`). Automação: declarar ANTES das sections,
beats absolutos da timeline final. Curvas: `lin`, `exp`, `log`, `pow(n)`.

## 7. Sintaxe .synth essencial (para custom/edição)

```txt
synth nome {
  poly 8 steal oldest spread 60%      # ou: mono glide 30ms legato
  gain -8db
  kill after 5s                       # obrigatório se q>1 ou física (voz não morre)
  param cutoff: hz = 900 range 100..8000 smooth 15ms curve log

  global { let m = lfo(shape: sine, rate: 8beat, amount: 0.2) }   # 1x por synth

  voice {                             # 1x por voz
    # contexto: note, velocity, gate, time, dur (duração da nota em s), rand, voice_idx
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

**Envelopes** - `env {}` multi-segmento e açúcar `adsr()`:
- `env { 1 -> 0.5 in 80ms curve exp -> 0 in 200ms }` (decay percussivo)
- Tempo de segmento aceita `ms`, `s`, `beat` (sync ao bpm!) e **expressões**:
  `env { 0 -> 1 in dur curve pow(1.8) }` = rampa que ocupa a nota INTEIRA.
  `env { 0 -> 1 in 4beat }` = 4 beats no bpm atual. É assim que risers escalam.
- `dur` = duração agendada da nota em segundos (0 fora do score). `max(dur, 0.5)`
  protege contra notas curtíssimas.

**Geradores**: `sine/triangle(freq, gain, fm:)`, `saw(freq, unison, spread, width, gain)`,
`pulse(freq, width)`, `square(freq)`, `wavetable(freq, table: "basic_shapes"|"vox"|"digital", pos)`,
`noise(color: white|pink|brown|blue|violet|velvet|crackle, density:)`,
`pluck(freq, damp, decay, position)`, `modal(freq, modes: bell|bar|membrane|pipe, strike, decay)`.

**Física** (preferir a samples SEMPRE):
- `modal2(freq, modes: tabela, doublet: 0.15%, strike: 0.3, hard: 0.6)` - banco modal
  do usuário `[(ratio, decay, amp), ...]`, doublets = batimento "mmm", martelo físico.
  Sino real: hum 0.5, prime 1.0, tierce 1.183, quint 1.506, nominal 2.0, deciem 2.662.
  Marimba (barra): 1 / 3.932 / 9.538 / 20.06.
- `nwave(dur: 2.5ms, sharp: 0.9, reflect: 4ms, reflect_gain: 0.4, air: 9khz)` - onda de
  choque coerente: o crack de tiro/trovão que noise filtrado não dá.
- `bow(freq, pressure, velocity, position, damp, noise: 0.15)` - corda com arco
  (stick-slip real). `velocity` É a arcada: envelope = nota articula. Passe por
  convolve de corpo (corpo_violino em ir:/ir2:).
- `flute(freq, pressure, breath, jet: 0.32)` - jet-drive waveguide (STK/Verge):
  tubo em overblow no 2º modo. pressure útil 0.4..1.3 (remap interno: TODA a
  faixa canta; brilho sobe com pressure); `jet` = balanço harmônico
  (0.25 brilhante/oco, 0.42 cheio/escuro; afinação compensada). Registro g3..g5,
  ±2ct (medido). Sopro é turbulência FÍSICA dentro do loop (espectro de
  Strouhal seguindo o fluxo, escala U², gate de Reynolds, wander OU): pulsa
  com o ciclo e ganha a cor do tubo, nunca chia por cima. breath 0.03-0.06
  respira (HNR ~22db), 0.1+ soproso jazz. Presets: flauta (transversal,
  vibrato + chiff), flauta_doce (recorder).
- `reed(freq, pressure, stiffness, breath)` - clarinete com lei de fluxo de
  Bernoulli real (Kergomard): pressure 0.5..1.1 (limiar ~0.55, forte comprime
  e brilha SEM morrer), ±2ct. HNR sobe de 19db (pp soproso) a 30db (ff),
  centroide idem - dinâmica física de verdade. Grave d3-c5.
- `brass(freq, pressure, lip, bell, rasp, breath)` - metal com steepening
  não-linear no loop (Burgers/Vergez-Rodet: pp quase senoide -> ff rasgado,
  contínuo). pressure 0.3..1.2 (remap interno); sustain LIMPO como metal real
  (HNR 31-37db), ar só no ataque gated pelo lábio. bell: trompete 1.8khz,
  trombone 1.1khz, trompa 900hz. rasp = quanto empina.
- `voz(freq, vowel: 0..4, tipo: soprano|alto|tenor|baixo, ens: N, vib, breath)` -
  fonte glotal + formantes; `ens: 8` = coral de verdade num nó. vowel automatizável.
- `breath(pressure, turbulence)` - fonte de sopro para sopros custom.
- `convolve(sig, ir: nome_de_synth, ir2: outra, dur: 150ms, mix: 100%)` - IR renderizada
  de outro synth do arquivo (corpo, sala). Use no `bus`. O synth da IR deve ser one-shot.
- `leslie(sig, speed, depth, mix)` - rotativo físico; para órgão SEMPRE leslie, não chorus.
- `hall(sig, size, decay, damp, mix)` - sala SDN; mix ≤ 10% (densidade modal baixa).

**Processadores**: `lowpass/highpass/bandpass/notch(sig, cutoff, q 0..1.2, slope: 24db)`,
`saturate/drive(sig, amount)`, `clip(sig, level)`, `follower(sig, attack, release)`,
`rms(sig, window)`, `ringmod(a, b)`, `pan(sig, pos)`, `gain`, `min/max/clamp/abs`,
`unipolar(x)` (-1..1 -> 0..1), `hz(pitch)`, `delay(sig, time, feedback)` (curto, física),
`delay1(sig)` (1 sample).

**LFO**: `lfo(shape: sine|triangle|square|saw|saw_down|sample_hold, rate, amount, phase)`.
Em `rate:`, `beat` = PERÍODO (8beat = ciclo de 8 beats). `sample_hold` = aleatório
por degraus (gorgolejos, sílabas de pássaro). Somar 2-3 LFOs em razões irracionais
(0.07hz + 0.113hz) = deriva orgânica que nunca repete (vento, mar, fogo).

Unidades: `hz khz ms s db % st ct beat` (`3/8beat` sync ao bpm).

## 8. Armadilhas conhecidas (custam iterações - decore)

- **FM**: modulador em ratio usa `hz(note) * 2`, NUNCA `note * 2` (note é pitch MIDI).
- Pitch relativo: `note - 12st` (oitava abaixo), `note + 7st` (quinta).
- `pluck`/`modal`/`modal2`/`bow`/`flute`/`reed`/`q > 1` -> voz pode nunca silenciar ->
  **sempre `kill after Ns`** (W006 avisa).
- `feedback` de delay_fx máx 0.95 (E014). Reverb mix em pad: 20-35%; mais vira sopa.
- `section` + eventos absolutos = E021. `automate` duplicado no track = E020.
- Cadeia de out: `out tone * amp * velocity` - esquecer `* amp` = nota infinita.
- Percussão ignora afinação fina; velocity faz o groove (0.3 ghost, 0.95 acento).
- Reverbs/delays no `bus` (1 instância), nunca no `voice` (N instâncias, caro e turvo).
- Sidechain: `duck(key: X)` no bus de quem ABAIXA (pad); key = quem dispara (kick).
- `convolve` no voice roda por VOZ (caro); pertence ao `bus`.
- Tabela `[( ... )]` só aceita literais; a let da tabela é inlined, não vira sinal.
- `bow` sem envelope em `velocity` = arcada eterna. `brass` forte tem blat físico.
- Seção de cordas = presets secao_* (N bows independentes), NUNCA saw + chorus.
- Ruído impulsivo (crackle, chuva, estalos) tem crest alto: parece baixo no RMS mas
  o pico manda na normalização - equilibre pelos PICOS entre camadas de ambiência.
- Render global normaliza pelo pico do arquivo INTEIRO: um evento 20db mais alto
  (trovão, tiro) esmaga o resto da mix - segure o pico dele no próprio synth.
- Render roda ~30-70x tempo real; 2min ≈ 2-4s. Muito mais = reverb por voz em dezenas
  de tracks (compartilhe bus/master).

## 9. Verificação (obrigatória antes de entregar)

```bash
python3 - <<'EOF'
import wave,struct,math
w=wave.open('out/song.wav');n=w.getnframes();d=w.readframes(n)
xs=struct.unpack('<%dh'%(n*2),d);sr=44100
dur=n//sr
win=max(dur//12,2)
def goertzel(seg,f):
    k=2*math.pi*f/(sr/4);c=2*math.cos(k);s1=s2=0.0
    for x in seg:s0=x+c*s1-s2;s2=s1;s1=s0
    return math.sqrt(max(s1*s1+s2*s2-c*s1*s2,0))/max(1,len(seg))
print("t     rms      arco                low/mid/high")
for t in range(0,dur,win):
    seg=xs[t*sr*2:(t+win)*sr*2:2][::4]
    if not seg: break
    rms=math.sqrt(sum(x*x for x in seg)/len(seg))/32768
    db=20*math.log10(max(rms,1e-6))
    b=[goertzel(seg[:40000],f) for f in (150,1000,5000)]
    tot=sum(b)+1e-9
    bands="/".join(f"{x/tot:.2f}" for x in b)
    print(f"{t:4d}s {db:6.1f}db {'#'*max(0,int((db+60)/2)):24s} {bands}")
L=xs[0::2];R=xs[1::2]
peak=max(max(map(abs,L)),max(map(abs,R)))/32768
side=sum(abs(l-r) for l,r in zip(L[::200],R[::200]))/len(L[::200])/32768
print(f"peak={peak:.3f} (~0.89 c/ master)  width={side:.4f} (>0.01 = estéreo vivo)")
EOF
```

Critérios de aceitação:
- **Arco**: intro -> clímax ≥ 8db (épico ≥ 10db). Flat = chato -> automatize cutoff
  / camadas por seção. Ambiência pura pode ser flat DE PROPÓSITO (diga isso na entrega).
- **peak ≈ 0.891** com master. Peak < 0.5 = mixagem fraca antes do master.
- **width > 0.01**; se ~0, faltou spread/chorus/widen.
- **Bandas**: cama grave-dominante o tempo todo + high ~0 = mix abafada (falta hat/
  brilho); high dominando sem low = mix fina (falta baixo/sub).
- Nenhuma seção com rms < -50db no meio da peça (buraco), exceto silêncio intencional.
- Transições: riser deve aparecer como rampa de RMS crescendo ATÉ a borda da seção
  (se o pico do riser cai antes do drop, a duração da nota está errada).
- Fim: cauda de reverb morre naturalmente (render soma 4s de cauda).
- Sopros/tons expostos: HNR (energia em bins harmônicos de f0 vs resto, FFT) ≥ 20db
  em nota sustentada; peakiness (pico/média 500hz-3khz) sustentado > ~100x = tom
  gritando exposto demais na mix.

## 10. Reprodutibilidade e variações

- `--seed 42`: mesmo arquivo = mesmo áudio bit a bit (noise/unison/humanize determinísticos).
- Variação: mude seed, troque 1 instrumento de papel, transponha, re-arranje sections
  (`arrange intro drop intro` -> loop de jogo perfeito).
- Loop para jogo: termine a última seção no acorde do início e corte a cauda de 4s
  no pós, ou peça fade.
- Stems: renderize o mesmo .score várias vezes comentando tracks (o render é rápido).
