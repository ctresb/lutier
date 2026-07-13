# synthdesk

Mesa modular virtual sobre a engine lutier. Sintetizador analogico
simulado por nodes: modulos na mesa, cabos entre ports, tudo com a
identidade visual do lumiere (HUD terminal monocromatico, brackets,
scanlines, grain, fosforo CRT).

## Rodar

```sh
pnpm install
pnpm tauri dev      # janela nativa (compila o backend rust + lutier)
pnpm dev            # so o frontend no browser (LINK FEED: STANDALONE)
```

## Arquitetura

Frontend sem framework: TypeScript puro + um unico canvas 2d.

- `src/core/camera.ts` pan em mundo + zoom ancorado no cursor (15%..400%)
- `src/core/graph.ts` nodes, ports, cabos e hit testing
- `src/core/input.ts` maquina de estados de interacao (pan, move, tune,
  patch, route)
- `src/core/layout.ts` snap magnetico de bordas + anti-sobreposicao
  (busca o vazio mais proximo em aneis de 1u)
- `src/render/renderer.ts` passada unica com dirty flag: so redesenha
  quando algo muda; grade de pontos com LOD (passo dobra no zoom out)
- `src/components/` a BASE (`spec.ts`: tamanho em unidades de 46px,
  inputs/outputs, knobs, sliders) + registry; componente so desenha o
  miolo - moldura, header e a faixa de io padronizada sao da base.
  Componentes por categoria (o box agrupa): PRIMITIVES speaker,
  device / CONTROLLERS volume, gain, channel, sequencer / OPERATORS
  math, mix / GENERATORS oscillator, noise / EFFECTS reverb /
  PROPERTIES envelope
- `src/core/project.ts` projeto .synthproj: save/load que grava TUDO
  (componentes com posicao/params/lock/nome, cabos com waypoints,
  contadores de id, camera, snap). Dialog nativo no tauri (comandos
  save_project/load_project + plugin dialog), file system access no
  browser (fallback download/input). AUTOSAVE a cada 2s: sempre no
  localStorage (reload restaura a sessao) e, com arquivo ja definido,
  no proprio arquivo; asterisco no SUBJECT = mudanca sem salvar.
  Atalhos: cmd+s salva, cmd+shift+s salva como, cmd+o abre; toolbox
  tem SAVE PROJECT / LOAD PROJECT
- `src/core/vars.ts` variaveis globais: todo componente expoe cada
  param como NOME_PARAM (OSC_01_ACTIVE, OSC_01_WAVE, DEV_01_NOTE...)
  via `window.desk.vars` (list/get/set) - superficie de automacao;
  set aceita numero cru, TRUE/FALSE e rotulo de selector (SQUARE)
- DEVICE: instrumento tocavel. IN = timbre (a cadeia plugada),
  NOTE/GATE = quem toca (sequencer por cabo, teclado Z-M com o
  device selecionado, ou vars), ENV = propriedades (componente
  envelope com attack/decay/sustain/release). A nota transpoe os
  osciladores do cone de entrada (pilha de pitch na engine)
- SEQUENCER: 8 passos, PITCH e RATE; playhead exato reportado pela
  engine via port.postMessage (sem drift visual)
- caminho de audio ESTEREO na engine; CHANNEL faz balance L/R real
- `src/audio/` a engine de audio: `processor.ts` e um AudioWorklet
  (via Blob URL) que roda o dsp do patch inteiro por amostra no audio
  thread - fase dos osciladores persiste por id (plugar cabo nao
  reinicia nada), crossfade de ~5ms em mudanca de topologia (zero
  clique), polyblep em saw/square, knobs suavizados, ciclos de cabo
  viram feedback com 1 amostra de atraso, dc blocker + soft clip na
  saida. `audio.ts` mantem um AudioContext PERSISTENTE por speaker
  (so morre quando o speaker sai da mesa), serializa o subgrafo e
  reconcilia com a engine em frame sujo; gesto do usuario destrava
  contextos suspensos pela politica de autoplay
- `src/ui/icon.ts` componente de icone; svgs feitos a mao em src/icons
- `src/ui/contextmenu.ts` botao direito (lock/delete/snap)
- `src-tauri/` casca tauri 2 com o crate lutier linkado (comando
  `engine_info`); o grafo vai compilar pra .synth/.score nas proximas fases

Pos processamento CRT (`src/render/crt.ts`): a cena nitida vai direto
pra tela; o bloom (com aberracao cromatica so no halo) e um canvas
webgl2 de 1/4 de resolucao somado por cima via plus-lighter, so em
frame sujo. Scanlines/vinheta/grain sao overlays CSS. Fonte unica:
Lilex variavel local. A lei visual completa esta em DESIGN.md.

## Controles

- scroll: pan | pinch ou cmd+scroll: zoom no cursor | cmd+0: reset
- arrastar componente do COMPONENTS BOX pra mesa (ou enter com foco)
- arrastar corpo: mover (cola nas bordas vizinhas e na grade; g liga/
  desliga o snap de grade) | knob: arrastar vertical (shift = fino,
  duplo clique = reset)
- arrastar de um port: puxa cabo reto; soltar num port compativel conecta
- clicar num trecho de cabo: insere ponto de roteamento e arrasta;
  arrastar ponto move, duplo clique remove
- cmd+arrasto no vazio: caixa de selecao (componentes E cantos de
  cabo); arrastar qualquer item selecionado move o grupo inteiro;
  delete apaga cantos selecionados sem confirmacao
- knob de freq do oscillator: arrasto muda so a unidade (hz inteiro);
  shift ajusta o decimo (0.1 em 0.1)
- botao direito: lock/unlock e delete do componente, delete de cabo,
  snap to grid (na mesa vazia)
- delete: remove componente (destravado) ou cabo | esc: cancela
- speaker: saida pro dispositivo do computador, nada mais. ACTIVE
  liga/desliga, DEV circula os dispositivos de saida; o som chega
  pelo port IN quando o grafo tiver fontes de audio.

## Proximas fases

1. mais modulos (vco, vcf, vca, env, lfo, mixer, output)
2. compilar o grafo pra .synth e tocar via lutier no backend
3. audio em tempo real (stream de blocos do rust pro frontend)
