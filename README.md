<p align="center">
  <img src="assets/banner.png" alt="lutier" width="100%">
</p>

# lutier

`lutier` é uma engine musical offline escrita em Rust, sem dependências externas. Você descreve instrumentos numa DSL própria (`.synth`), escreve a partitura em outra (`.score`) e a engine renderiza áudio determinístico em WAV (e MP3, se `ffmpeg` estiver no PATH).

A ideia central: música, trilha e SFX como código. Instrumentos versionáveis, partitura legível, render reprodutível bit a bit. Nenhum áudio pré-gravado: todo som nasce da linguagem.

## Índice

- [Quick start](#quick-start)
- [Uso](#uso)
- [A DSL `.synth`](#a-dsl-synth)
- [A DSL `.score`](#a-dsl-score)
- [Presets](#presets)
- [Exemplos](#exemplos)
- [Estrutura do repositório](#estrutura-do-repositório)
- [Testes](#testes)
- [Composição com agentes](#composição-com-agentes)
- [Licença](#licença)

## Quick start

Pré-requisitos:

- Rust estável com Cargo.
- `ffmpeg` (opcional) para exportar MP3 além do WAV.

```sh
cargo build --release
./target/release/lutier examples/songs/vila/vila.synth examples/songs/vila/vila.score -o out/vila.wav
```

Saída esperada:

```txt
rendered 59.4s
wrote out/vila.wav
wrote out/vila.mp3
```

Sem `ffmpeg`, o render continua normalmente e só o WAV é gerado.

## Uso

```sh
cargo run --release -- <patch.synth> <song.score> -o out/song.wav
```

| Flag | Uso |
|---|---|
| `-o <arquivo.wav>` | Caminho do WAV de saída. |
| `--seed <n>` | Seed determinística usada por `rand` e `humanize`. |
| `--bench` | Imprime tempo de render e fator realtime. |

## A DSL `.synth`

Um arquivo `.synth` descreve instrumentos e a cadeia de master. Ele pode importar presets e sobrescrever nomes localmente.

```txt
import "presets/keys.synth"

master {
  bus_gain -1db
  compressor(threshold: -18db, ratio: 2, attack: 15ms, release: 180ms, makeup: auto)
  limiter(ceiling: -1db, lookahead: 5ms, release: 60ms)
}
```

### Blocos

| Bloco | Função |
|---|---|
| `synth nome { ... }` | Define um instrumento. |
| `poly`, `mono`, `gain`, `kill after` | Controlam vozes, ganho e fim de nota. |
| `param` | Expõe parâmetros para `set` e `automate` no score. |
| `global` | Calcula sinal compartilhado entre vozes. |
| `voice` | Calcula sinal por nota, com `note`, `velocity`, `gate`, `time`, `dur`, `rand`, `voice_idx`. `dur` é a duração agendada da nota em segundos: `env { 0 -> 1 in dur }` faz a rampa ocupar a nota inteira (risers exatos). |
| `bus` | Processa a soma das vozes. Bom lugar para chorus, delay, reverb, widen e duck. |
| `mod` | Matriz de modulação para targets internos. |
| `master` | Processamento final do render. |

### Recursos

- **Osciladores:** `sine`, `triangle`, `saw`, `square`, `pulse`, `noise`, `wavetable`, `pluck`, `modal`, `sample`, `grain`.
- **Filtros:** SVF TPT `lowpass`, `highpass`, `bandpass`, `notch`, com clamp de cutoff.
- **Envelopes:** `env {}` multi-segmento e açúcar `adsr()`.
- **Modulação:** `lfo`, `follower`, `rms`, `ringmod`, matriz `mod`.
- **Não-lineares:** `saturate`, `clip`, `drive`.
- **Delay e feedback:** `delay1`, `delay`, `delay_fx`.
- **Mix e dinâmica:** `gain`, `pan`, `widen`, `duck`, `compressor`, `limiter`, `reverb`, `chorus`.
- **Unidades:** `hz`, `khz`, `ms`, `s`, `db`, `%`, `st`, `ct`, `beat`.

## A DSL `.score`

Um arquivo `.score` agenda notas e automações em beats.

```txt
tempo 104
section manha 32
track kal kalimba
swing 56
humanize 8ms 10%
0    b4 0.5 0.75
1    d5 0.5 0.70
2    g4 1   0.75

arrange manha
```

| Sintaxe | Exemplo | Função |
|---|---|---|
| `tempo <bpm>` | `tempo 104` | Define BPM inicial. |
| `tempo <beat> <bpm>` | `tempo 32 118` | Muda BPM no beat indicado. |
| `track <nome> <synth>` | `track ep epiano_fm` | Cria ou seleciona uma faixa. |
| Nota | `0 c4 1 0.8` | Beat, nota, duração em beats, velocity. |
| Acorde | `0 [c3 e3 g3] 4 0.5` | Várias notas no mesmo evento. |
| Repetição | `0 c5 0.1 0.4 x16 @1` | Repete `x16`, a cada `1` beat. |
| `set` | `set cutoff 1200` | Define parâmetro fixo. |
| `automate` | `automate cutoff 0 500 -> 32 4000 curve log` | Automatiza parâmetro. |
| `swing` | `swing 56` | Atrasa offbeats de colcheia. |
| `humanize` | `humanize 8ms 10%` | Jitter determinístico de tempo e velocity. |
| `section` | `section intro 32` | Eventos relativos à seção. |
| `arrange` | `arrange intro verso intro` | Monta a timeline final. |

Notas usam nomes como `c4`, `a#3`, `eb2`. Comentário com `#` só conta no início de token, então `a#2` continua válido.

## Presets

Instrumentos prontos em `presets/`, importáveis com `import "presets/<nome>.synth"`:

| Arquivo | Conteúdo |
|---|---|
| `pads.synth` | `strings_warm`, `choir_vox`, `pad_dark`, `pad_glass`. |
| `keys.synth` | `bell_fm`, `epiano_fm`, `harp`, `kalimba`, `music_box`, `bell_modal`, `lead_saw`. |
| `orchestra.synth` | `strings_stacc`, `brass_stab`, `horn_sustain`, `flute_lead`, `organ_church`. |
| `bass.synth` | `subbass`, `bass_deep`, `bass_pulse`. |
| `drums.synth` | `taiko`, `kick_deep`, `snare`, `hat_closed`, `hat_open`, `shaker`, `tom_modal`. |
| `funk.synth` | Funk carioca/trap: `kick_808`, `bass_808` (glide), `kick_tamborzao`, `tamborim`, `atabaque`, `clap_funk`, `snare_seca`, `hat_trap`, `stab_funk`, `apito`. |
| `nature.synth` | Ambiências procedurais: `vento`, `chuva`, `oceano`, `riacho`, `trovao`, `fogueira`, `grilos`, `sapos`, `passaros`. |
| `physical.synth` | Instrumentos por modelagem física: `violino`, `cello`, `flauta`, `flauta_doce`, `clarinete`, `sino_real`, `marimba_fisica`, metais e corais. |
| `sfx.synth` | SFX de jogo: UI, moeda, sino, pulo, powerup, laser, hit, whoosh, portal, alarme, explosão e armas. Transições que escalam com a duração da nota: `sfx_riser`, `sfx_riser_tonal`, `sfx_riser_noise`, `sfx_shepard`, `sfx_downlifter`, `sfx_sub_drop`, `sfx_impact`. |

## Exemplos

Tudo em `examples/`, separado do código da engine:

| Pasta | Conteúdo |
|---|---|
| `examples/songs/<nome>/` | Músicas completas: `vila`, `lamento`, `epic`, `funky`, `infortunata`, `tamborzao`. Cada pasta tem o par `.synth` + `.score` e o `.mp3` renderizado. |
| `examples/sfx/` | Showcase com todos os SFX de jogo (`showcase.synth` + `showcase.score` + `showcase.mp3`). |
| `examples/sfx/oneshots/` | Cada SFX renderizado individualmente em `.mp3`. |
| `examples/ambience/` | Vitrine dos sons da natureza (`natureza.synth` + `natureza.score` + `natureza.mp3`). |

Os `.mp3` de cada exemplo já estão versionados ao lado dos fontes. Para re-renderizar qualquer um:

```sh
./target/release/lutier examples/songs/vila/vila.synth examples/songs/vila/vila.score -o out/vila.wav
./target/release/lutier examples/sfx/showcase.synth examples/sfx/showcase.score -o out/sfx_showcase.wav
```

Renders avulsos vão para `out/`, que fica fora do versionamento.

## Estrutura do repositório

```txt
src/          engine (lexer, parser, checker, engine DSP, score, render, CLI)
presets/      instrumentos prontos (.synth)
examples/     músicas e SFX (fontes + mp3 renderizado)
tests/        testes DSP e golden (regressão de áudio)
assets/       banner e logo
out/          áudio renderizado (gitignored)
```

| Caminho | Responsabilidade |
|---|---|
| `src/lexer.rs` | Tokens da DSL `.synth`. |
| `src/parser.rs` | AST, imports, expressões, synths, buses, master e mod matrix. |
| `src/check.rs` | Passes semânticos com diagnósticos `E*` e `W*`. |
| `src/engine.rs` | Interpretador de dataflow, estado por nó/voz, DSP, osciladores e efeitos. |
| `src/score.rs` | Parser `.score`, tempo map, sections, arrange, chords, swing, humanize e automação. |
| `src/render.rs` | Render offline, roteamento, sidechain e master chain. |
| `src/wavio.rs` | Escrita WAV. |
| `src/main.rs` | CLI `lutier`. |

## Testes

```sh
cargo test
```

A suíte tem duas camadas:

- `tests/dsp.rs`: estabilidade e comportamento de cada primitivo DSP.
- `tests/golden.rs`: renderiza patches de fixture em `tests/golden/` e compara o fingerprint sonoro (hash + RMS) para detectar regressão audível. Os `.fp` não são versionados: são criados na primeira execução e regravados com `UPDATE_GOLDEN=1 cargo test`.

## Composição com agentes

O repo inclui uma skill local em `.claude/skills/maestro/SKILL.md` com um fluxo de composição para agentes: escolher emoção, gênero, tom, BPM, forma e instrumentação, renderizar, medir RMS/peak/width e iterar antes de entregar o áudio.

Uso esperado: abrir este repo no agente e pedir algo como "gera uma trilha de vila aconchegante" ou "faz um SFX de portal". O agente escreve `.synth` e `.score`, roda o render e valida o áudio numericamente.

## Licença

<img src="assets/logo.svg" alt="lutier" width="220">

Licença própria: [Lutier License v1.1](LICENSE).

Resumo não jurídico:

- Crédito sempre obrigatório, inclusive em obras que usam músicas ou SFX feitos com o Lutier.
- Código, engine, DSLs, presets e sistema: uso não comercial, salvo licença comercial separada.
- Música ou SFX dentro de obra maior (jogo, vídeo, filme, app, peça, anúncio): pode comercializar sem royalties, mantendo crédito.
- Vender a música em si exige transformação humana substancial. Saída crua, loopada, cortada ou só processada em lote não conta.

Crédito sugerido:

```txt
Músicas feitas com o Lutier, de C3B - github.com/ctresb
```

Curto:

```txt
Músicas: Lutier (github.com/ctresb)
```

Usos fora da licença: negociar licença comercial à parte.
