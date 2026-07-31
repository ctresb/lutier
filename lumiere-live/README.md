# lumiere-live

Visualizador de sinal em tempo real no estilo do LUMIERE (o terminal
de analise de `lumiere/scene.py`), pensado pra ficar no rodape de uma
livestream: janela de 1920x200.

- captura de audio nativa (cpal): clique no painel INPUT SOURCE cicla
  o input (botao direito volta).
- DESKTOP AUDIO (SYSTEM): captura o som do sistema sem driver. No mac
  via ScreenCaptureKit (na primeira vez o macos pede permissao de
  GRAVACAO DE TELA; conceda e selecione o input de novo). No windows
  os devices `DESKTOP: ...` sao loopback WASAPI de cada saida.
  Alternativa classica: driver de loopback (BlackHole/VB-Cable)
  aparece como input normal.
- analise em rust a ~60hz: fft hann 2048, espectro log 28hz..18khz,
  waveform, goniometro, rms/peak/flux/crest/largura estereo.
- paineis: input + metricas, waveform, espectrograma, entidade de
  particulas com goniometro (a nuvem de pontinhos), wireframe 3d do
  `meiaum.glb` girando devagar, mapa de frequencia.
- estetica lumiere: fosforo frio monocromatico, brackets, bloom CRT
  (webgl2 em 1/4 de res + plus-lighter), scanlines, vinheta, grain.
  O gradiente (22DFEE 163BDD 7320D9 D920AA F38F61 E4F361) entra so em
  detalhes: filete sob o header, linha do frequency map, matiz da
  nuvem do goniometro e a lut de intensidade do espectrograma.

## rodar

```sh
pnpm install
pnpm tauri dev
```

Sem backend (so o visual, com audio mock): `pnpm dev` e abre
http://localhost:1430 no navegador.

## build

mac (gera .app e .dmg em `src-tauri/target/release/bundle/`):

```sh
pnpm tauri build
```

windows (.exe): rodar `pnpm tauri build` numa maquina windows com
rust + pnpm, ou usar o workflow `lumiere-live build` do github
actions (workflow_dispatch ou tag `lumiere-live-v*`), que sobe os
artefatos dos dois sistemas.

## troca de mesh

Substitua `src/assets/meiaum.glb` por qualquer .glb (gltf 2.0 com
indices); o parser normaliza e extrai as arestas sozinho.
