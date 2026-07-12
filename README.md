<p align="center">
  <img src="assets/banner.png" alt="lutier" width="100%">
</p>

# lutier

`lutier` é uma engine sonora offline escrita em Rust, sem dependências externas. Você constrói qualquer som numa DSL própria (`.synth`): instrumentos por modelagem física, sintetizadores, efeitos, ambiências e SFX. Uma segunda DSL (`.score`) agenda os eventos no tempo, e a engine renderiza áudio determinístico em WAV (e MP3, se `ffmpeg` estiver no PATH).

A ideia central: som como código. Não é só música: é um lutier digital. Você descreve a física de uma corda friccionada, o jato de ar de uma flauta ou o corpo de um violino, versiona tudo em texto e o render sai reprodutível bit a bit. Nenhum áudio pré-gravado: todo som nasce da linguagem.

## Índice

- [Quick start](#quick-start)
- [Uso](#uso)
- [A DSL `.synth`](#a-dsl-synth)
- [A DSL `.score`](#a-dsl-score)
- [Presets](#presets)
- [Exemplos](#exemplos)
- [Lumiere: análise visual](#lumiere-análise-visual)
- [DAW web local](#daw-web-local)
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
./target/release/lutier tests/fixtures/demo.synth tests/fixtures/demo.score -o out/demo.wav
```

Saída esperada:

```txt
rendered 14.0s
wrote out/demo.wav
wrote out/demo.mp3
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

- **Osciladores:** `sine`, `triangle`, `saw`, `square`, `pulse`, `noise`, `nwave`, `wavetable`, `sample`, `grain`.
- **Modelagem física:** `pluck`, `string` (corda dedilhada EKS/waveguide, 2 polarizações), `bow` (arco com fricção térmica), `flute`, `reed`, `brass`, `voz` (formantes vocais), `modal`, `modal2`, `breath`.
- **Filtros:** SVF TPT `lowpass`, `highpass`, `bandpass`, `notch`, com clamp de cutoff.
- **Envelopes:** `env {}` multi-segmento e açúcar `adsr()`.
- **Modulação:** `lfo`, `follower`, `rms`, `ringmod`, matriz `mod`.
- **Não-lineares:** `saturate`, `clip`, `drive`.
- **Delay e feedback:** `delay1`, `delay`, `delay_fx`.
- **Mix e dinâmica:** `gain`, `pan`, `widen`, `haas`, `duck`, `compressor`, `limiter`, `reverb`, `hall`, `chorus`, `leslie`, `convolve` (IR gerada na linguagem, ex.: corpos de instrumento).
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
| `physical.synth` | Instrumentos por modelagem física: `violino`, `cello`, `flauta`, `flauta_doce`, `clarinete`, `sino_real`, `marimba_fisica`, metais (`trompete`, `trompa`, `trombone`) e corais. |
| `strings.synth` | Cordas v2 com `string()` e `bow()` e corpos `modal2` com modos medidos da literatura: `violino`, `viola`, `cello`, `contrabaixo` (arco e pizzicato), `violao`, `violao_aco`, `guitarra`, `baixo_eletrico`, `baixo_slap`, `banjo`, `ukulele`, `bandolim` e seções. Não importar junto com `physical.synth`: os dois definem `corpo_violino` e `secao_violinos`. |
| `sfx.synth` | SFX de jogo: UI, moeda, sino, pulo, powerup, laser, hit, whoosh, portal, alarme, explosão e armas. Transições que escalam com a duração da nota: `sfx_riser`, `sfx_riser_tonal`, `sfx_riser_noise`, `sfx_shepard`, `sfx_downlifter`, `sfx_sub_drop`, `sfx_impact`. |

## Exemplos

Os scores de referência vivem em `tests/fixtures/` e fazem dupla função: são os goldens de regressão e a documentação viva de escrita. `demo` é uma peça curta com os presets (cordas físicas, bateria, baixo), `features` exercita a DSL de score inteira, `physics` toca cada primitivo de modelagem física e `showcase` percorre os SFX.

```sh
./target/release/lutier tests/fixtures/demo.synth tests/fixtures/demo.score -o out/demo.wav
./target/release/lutier tests/fixtures/showcase.synth tests/fixtures/showcase.score -o out/sfx_showcase.wav
```

Renders avulsos vão para `out/`, que fica fora do versionamento.

## Lumiere: análise visual

`lumiere/` é um terminal de análise visual em Python: recebe o WAV renderizado e o `.score` correspondente e gera um MP4 1080p24 com HUD monocromático estilo terminal, mostrando espectro, camadas do score, seções e medidores enquanto o áudio toca.

Pré-requisitos: Python 3 com `numpy` e `Pillow`, além de `ffmpeg` no PATH.

```sh
python3 -m lumiere out/demo.wav tests/fixtures/demo.score -o out/demo.mp4
```

| Flag | Uso |
|---|---|
| `-o <arquivo.mp4>` | Caminho do vídeo de saída. |
| `--fps <n>` | Frames por segundo (padrão 24). |
| `--title <texto>` | Título exibido no HUD (padrão: nome do score). |
| `--theme mono\|brasil` | Tema de cor. |
| `--preview "10,50,90"` | Salva PNGs nesses segundos e sai, sem renderizar o vídeo. |
| `--seed <n>` / `--crf <n>` | Seed das partículas e qualidade do encode. |

## DAW web local

`daw/` é uma interface web local para editar e ouvir patches sem sair do navegador: editor de `.synth` e `.score` com piano roll, tempo, grade e botão de render que chama o binário `lutier` e devolve o WAV.

```sh
cargo build --release
python3 daw/server.py        # abre em http://localhost:8737
```

O servidor só serve a UI estática e expõe `POST /render`; todo o áudio continua sendo gerado pela engine Rust.

## Estrutura do repositório

```txt
src/          engine (lexer, parser, checker, resolver, engine DSP, score, render, CLI)
presets/      instrumentos prontos (.synth)
lumiere/      análise visual (wav + score -> mp4)
daw/          interface web local (editor + render)
tests/        testes DSP e golden (fixtures de score em tests/fixtures/)
assets/       banner e logo
out/          áudio renderizado (gitignored)
```

| Caminho | Responsabilidade |
|---|---|
| `src/lexer.rs` | Tokens da DSL `.synth`. |
| `src/parser.rs` | AST, imports, expressões, synths, buses, master e mod matrix. |
| `src/check.rs` | Passes semânticos com diagnósticos `E*` e `W*`. |
| `src/resolve.rs` | Resolução de nomes em load-time para slots indexados (engine não hasheia strings por sample). |
| `src/engine.rs` | Interpretador de dataflow, estado por nó/voz, DSP, osciladores, modelagem física e efeitos. |
| `src/score.rs` | Parser `.score`, tempo map, sections, arrange, chords, swing, humanize e automação. |
| `src/render.rs` | Render offline, roteamento, sidechain e master chain. |
| `src/fp.rs` | Fingerprint de áudio (hash + RMS por bloco) para os testes golden. |
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
