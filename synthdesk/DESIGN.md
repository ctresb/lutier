# DESIGN.md - identidade visual do synthdesk

Lei visual do projeto. Todo componente novo (DOM ou canvas) segue
EXATAMENTE o que esta aqui. Herdado do lumiere (lumiere/scene.py):
terminal de analise monocromatico, fosforo CRT frio, brackets.

## 1. Essencia

- Preto quase absoluto + UM tom de fosforo frio. Nenhuma outra cor.
- Hierarquia = brilho, nunca matiz. Quem importa mais, brilha mais.
- Tudo MAIUSCULO, monospace, com tracking. Prefixos `//`, `::`, `/`.
- Moldura fina + brackets de canto = assinatura. Sem border-radius,
  sem gradiente decorativo, sem sombra de elevacao.
- Densidade calma: muito espaco preto, poucos elementos, precisos.

## 2. Cor: a formula do fosforo

Um unico eixo de brilho v (0..255) com tint frio do lumiere:

```
r = v * 0.94    g = v * 0.97    b = min(255, v * 1.01 + 6)
```

Canvas: `ph(v, alpha)` em `src/core/palette.ts`. DOM: tokens em
`src/style/tokens.css` (base rgb(214 228 238) + alpha). NUNCA hex cru.

Escala semantica (nivel v -> uso):

| v   | canvas (COL)   | css token       | uso |
|-----|----------------|-----------------|-----|
| bg  | `#030405`      | `--bg`          | fundo universal |
| 255 a .115 | `dot`   | -               | ponto da grade (26/255 do lumiere) |
| 48  | `lineFaint`    | `--line-faint`  | moldura interna, ticks, regua sutil |
| 110 | `line`         | `--line`        | moldura padrao de painel/modulo |
| 160 | `lineMid`      | `--line-strong` | moldura selecionada, port idle |
| 105 | `textFaint`    | `--text-faint`  | hints, indices, labels 0/1 |
| 160 | `textDim`      | `--text-dim`    | labels secundarios, chaves (`STATUS:`) |
| 205 | `text`         | `--text`        | texto padrao |
| 228 | `bracket`      | `--bracket`     | brackets de canto |
| 247 | `textBright`   | `--text-bright` | valores, nome selecionado, ponteiro |

Brilho/halo NUNCA e desenhado a mao (nada de shadowBlur ou passada
larga): o bloom global do shader (secao 10) e a unica fonte de glow.

Superficie DOM: `--surface = rgb(6 8 10 / 0.86)` (deixa a mesa vazar).
Fundo de modulo canvas: `rgb(5 7 9 / 0.94)` (opaco pra tapar grade).

## 3. Tipografia

- Fonte UNICA do projeto: Lilex variavel, local
  (`src/font/lilex-var.ttf`, @font-face em tokens.css). Nada de outra
  familia; fallback e so `monospace`. Canvas usa
  `${size}px Lilex, monospace` (prims.text).
- Corpo 12px. Escala inteira: 9 / 10 / 11 / 12 / 13 (canvas usa as
  mesmas em unidades de mundo; escala junto com o zoom).
- Tracking por contexto: barras HUD `0.08em`; titulo central `0.22em`;
  titulos de painel `0.16em`; itens/labels `0.10-0.14em`; tags 9px
  `0.14em`.
- Numeros SEMPRE tabulares (`font-variant-numeric: tabular-nums`),
  com padding fixo: coords `+0000.0`, contagem `NODES 02`, knob `0.500`
  (3 casas), zoom `100%`.
- Chave apagada, valor brilhante: `STATUS:` em dim + `IDLE` em bright.
- Reticencias `...` em textFaint fecham listas/blocos (tique lumiere).

## 4. Painel DOM (module box e futuros)

Anatomia exata (`.panel` em `src/style/modulebox.css`):

1. borda `1px solid --line`, fundo `--surface`, padding `14px`;
2. 4 brackets de canto: 10x10px, tracos de 1px em `--bracket`,
   posicionados `-1px` pra COBRIR o canto da borda (els `.bk-*`);
3. header: titulo 12px bright tracking 0.16em + contador dim a direita
   (`01 TYPES`);
4. regua `1px --line` a `10px` abaixo do header;
5. corpo;
6. hint no rodape: `border-top 1px --line-faint`, texto 10px
   `--text-faint`, tracking 0.12em, line-height 1.7.

Item de lista (`.mod-item`): grid `24px minmax(0,1fr) auto`, gap 8px,
padding `9px 8px`, borda 1px TRANSPARENTE. Indice 10px faint, nome 12px
(ellipsis se nao couber, nunca vazar), tag 9px dim com borda faint e
`white-space: nowrap`. Conteudo tem que caber DENTRO do outline.

Estados DOM:
- hover: borda vira `--line`, texto vira bright. SEM box-shadow/glow
  (regra do dono, 2026-07-12: hover DOM nao brilha).
- active/dragging: borda `--line-strong`, cursor grabbing.
- focus-visible: `outline 1px solid --focus, offset 2px`.

## 5. Componente no canvas (a BASE)

Sistema de unidades: `1u = 46` (UNIT em components/spec.ts) = um
passo da grade. TODO componente mede `unitsW x unitsH` unidades
INTEIRAS, sem excecao - com snap ligado os quatro cantos caem
exatamente nas bolinhas da grade.

Todo componente e DECLARATIVO (`ComponentSpec` = json puro): tipo,
nome, tag, tamanho em unidades, `inputs[]`/`outputs[]`, `params[]` e
`controls[]`. A base desenha moldura, header, faixa de io E os
controles declarados; hit testing e interacao dos controles tambem
sao da base (zonas derivadas por `knobsOf`/`buttonsOf`). O que e
especifico vira hook opcional: `drawExtra` (desenho custom, ex scope
e grelha), `press`/`selectorValue` (selectors), `cvOut` (grafo de
cv), `animates` (redesenho continuo, ex scope ligado). NADA de
layout hardcoded dentro de componente.

Controles declarados (`ControlSpec`): `knob` (param, x, y, r),
`toggle` (bool, quadrado), `switch` (bool, deslizante), `selector`
(chave/valor, clique circula), `readout` (leitura .3f), `label`
(texto 9px faint) e `rule` (regua de margem a margem).

Anatomia desenhada pela base (Renderer.drawNode):

1. fundo 100% opaco `rgb(5 7 9)` (grade e cabos nao vazam);
2. moldura dupla: externa 1px `--line` no limite, interna 1px
   `lineFaint` com inset de 3px (eco do frame do lumiere);
3. brackets de canto len 10 em `bracket` (bright quando hover/sel);
4. header (42px, EMPILHADO): ENERGIA a esquerda - componente
   `onOff(g, 12, 15, on, false)`: switch SEM label no header (a
   label ON/OFF embaixo existe no componente mas desalinha o header;
   o estado ja e obvio pelo cursor). Zona de clique x..x+40,
   y+6..y+34, alterna `params.on`.
   Nome (`POT_01`) 12px em x+48 y+10; tag (`CV SRC`) 9px textFaint
   embaixo do nome (x+48 y+26); toggle de lock a direita (icone 14px
   em w-26 y+14: lock-open phHex(105) destravado, lock-closed
   phHex(205) travado; HOVER clareia (190/247); zona w-32..w);
   regua 1px lineFaint em y+42. TODO componente tem o switch de
   energia: `on` e param da base (def 1, injetado no addNode);
   desligado o componente e INERTE (cv nao sai, som nao sai,
   desenhos esmaecem) - nunca criar toggle de ligar proprio;
5. miolo do componente (entre y+42 e h-36);
6. faixa de IO (36px, OBRIGATORIA, sempre embaixo): desenhada pelo
   componente `nodePort(g, x, lineY, label, active)` de
   `src/render/controls.ts` - regua 1px lineFaint em h-36, label 8px
   textFaint em linha+5, vao de 4, quadrado 10x10 em linha+16..+26.
   As folgas sao medidas contra o FRAME INTERNO (h-3.5), nao contra a
   borda externa: ~5.5 da regua ao label = ~6.5 do quadrado ao frame.
   Inputs da esquerda pra direita, outputs da direita pra esquerda,
   passo 42, label centrado no port. Nenhum componente poe ports em
   outro lugar.

Linhas de canvas: hairline `1/zoom` (1px de TELA sempre), com
`lineJoin` e `lineCap` ARREDONDADOS (setados globalmente no
renderer; nunca miter). Posicao sempre inteira; com snap de grade
ligado, multipla de 1u.

Estados: selecionado = moldura lineMid + brackets/nome bright;
hover = so brackets bright; travado = lock-closed brilhando no
toggle do header, nao move nem deleta (knobs e ports continuam
vivos). Nada de fill de selecao.

## 6. Knob (controle padrao)

Geometria canonica num miolo de 3u de largura (r = 30, centro em
x=69 y=88, meio do corpo entre header 42 e io 148):

- curso de 270 graus, inicio em 135 graus (7h30), sentido horario,
  igual pot analogico;
- 19 ticks a cada 15 graus, de r+5 ate r+8 (r+11 nos multiplos de 3),
  em lineFaint;
- trilho apagado: arco full-curso r em lineFaint;
- trilho percorrido: arco de 135 ate 135+270*v, lineMid (bright em
  hover);
- corpo: circulo r-6 em `--line` (bracket em hover);
- ponteiro: linha 1.6px de r=5 ate r-8, textBright (o halo vem do
  bloom global, sem shadowBlur);
- pino central: disco r2 textDim;
- leitura: valor `v.toFixed(3)` 13px bright, centrado, y = cy+r+8
  (respiro real ate a regua do io);
- extremos: `0` e `1` 9px faint nas pontas do curso (y = cy+r-2).

## 6.1 Toggle e Switch (controles bool padrao)

Dois tipos, ambos em `src/render/controls.ts`:

TOGGLE (`toggle(g, x, y, on, label)`): quadrado 10x10 outline
(textBright ligado, lineMid desligado); marcado = quadradinho 4x4
PREENCHIDO no centro. Nunca check de tracos. MESMO visual do port
ativo: quadrado + quadradinho e a linguagem universal de
"ligado/conectado". Ex: ACTIVE do speaker.

SWITCH (`switchCtl(g, x, y, on, label)`): retangulo 24x12 com cursor
8x8 que desliza - ESQUERDA = off, cursor escuro (textFaint) e caixa
lineMid; DIREITA = on, cursor claro (textBright) e caixa textBright.

ON/OFF (`onOff(g, x, y, on, label)`): switch + texto de estado
("ON"/"OFF") 8px centrado EMBAIXO; `label: boolean` desliga o texto.
E o padrao de energia do header (secao 5.4).

Labels de ambos: `string | false`, 9px a direita, com o offset
validado NO OLHO contra o render real (a lilex renderiza ~2px mais
baixo que o TextMetrics promete; nunca alinhar so pela metrica).
Zonas de clique derivadas pela base (linha inteira, cursor pointer).
Selectors (ex DEV do speaker, WAVE do oscillator): chave faint
esquerda + valor a direita, clique circula.

Interacao: arrasto vertical (sens 0.006/px, shift = x0.12), duplo
clique reseta pro default, cursor `ns-resize`.

## 7. Ports e cabos

Port: quadrado de 10x10 (mesmo tamanho do checkbox, sempre) em
lineMid, vazio. Ativo (hover / snap / conectado): contorno textBright
+ quadradinho 4x4 preenchido, identico ao checkbox marcado. Cursor
`crosshair`.

Conexao: como numa mesa analogica, TUDO E TENSAO - qualquer out
pluga em qualquer in (cv em audio, audio em cv), sem restricao de
tipo. As unicas regras: out -> in, nunca no mesmo componente, e um
in aceita um cabo (o novo substitui). O grafo de cv resolve em cadeia
via `cvOut(node, port, graph, seen)` com guarda de ciclo.

Cabo: RETO, sempre. Polilinha solida port -> waypoints -> port,
1.1px em ph(185) (selecionado: 1.6px textBright). Nada de curva,
nada de tracejado. Pontas: quadradinhos 5x5 preenchidos bright nos
ports (NUNCA bolinha). Cabos sao desenhados POR CIMA dos componentes
(ordem: grade, componentes, cabos, preview) e o hit testing
acompanha: port > waypoint > cabo > knob/botao/corpo.

Waypoints (pontos de roteamento): QUADRADOS RETOS (7x7; outline
lineMid no cabo em repouso, preenchido textBright com o cabo
selecionado; nunca losango, nunca bolinha). Interacao: clicar num
trecho da linha INSERE um waypoint naquele segmento e ja entra
arrastando (status ROUTING); soltar sem mover remove o ponto (vira
so selecao). Arrastar waypoint existente move; BOTAO DIREITO em cima
remove o angulo na hora (sem menu); duplo clique tambem remove.
Snap do waypoint e ORTOGONAL, nao de grade: perto de alinhar com um
vizinho da polilinha (limiar 8px de tela), cola no eixo dele e o
segmento vira uma reta horizontal/vertical certinha. Coordenadas de
mundo, inteiras.

Preview de patch: linha reta solida ph(150), textBright quando tem
snap valido. Sem dash.

## 7.1 Movimento na mesa

- snap de grade LIGADO por padrao (footer mostra `SNAP ON`); toggle
  pela tecla `g` ou pelo menu de contexto do desk. Desligado, os
  pontinhos da grade SOMEM (a mesa fica lisa) e a posicao continua
  inteira (nunca fracionaria). Vale so pra COMPONENTES: waypoint de
  cabo tem snap proprio, ortogonal (secao 7);
- snap magnetico de bordas (estilo janelas do macos): arrastando
  perto da lateral de outro componente (limiar 12px de tela), cola
  flush - horizontais e verticais - com alinhamento secundario das
  bordas perpendiculares. Prioridade: borda > grade > livre;
- NENHUM componente fica em cima de outro: ao soltar (ou colocar)
  sobreposto, ele vai pro espaco vazio mais proximo em que cabe
  (busca em aneis de 1u, menor distancia ganha). Bordas encostadas
  (gap zero) sao permitidas e desejaveis;
- EMENDA: largar UM componente que tenha in E out em cima de um cabo
  religa a conexao atraves dele (A -> B vira A -> comp -> B, primeiro
  in e primeiro out; waypoints do cabo antigo somem). Vale pro drop
  do components box e pro arrasto de um componente solto (nao pra
  grupos);
- SHIFT+D duplica a selecao (params copiados, cabos INTERNOS ao
  conjunto preservados com waypoints, deslocado 1u, anti-sobreposicao
  aplicada; a selecao passa a ser as copias);
- DELETE/BACKSPACE em componentes abre MODAL de confirmacao (painel
  lumiere: titulo DELETE, mensagem REMOVE N COMPONENTS..., CANCEL +
  DELETE primario; enter confirma, esc/clique fora cancela; teclado
  do app fica bloqueado com o modal aberto). Cabo selecionado deleta
  direto, sem modal;
- MULTISELECT: cmd + arrasto no vazio abre a caixa de selecao
  (outline lineMid + fill ph(255, 0.05): translucido sutil mas
  visivel), selecao viva por intersecao, status SELECTING. A caixa
  IGNORA componentes travados e some com fadeout de 180ms ao soltar
  (instantaneo com prefers-reduced-motion). cmd + clique alterna um
  componente na selecao. Grupo selecionado se move junto (o agarrado
  dita o snap, o resto preserva posicao relativa; membros do grupo
  nao snapam entre si) e delete remove todos os destravados. Clique
  no vazio limpa a selecao.

## 7.2 Menu de contexto

Botao direito abre painel lumiere (mesma anatomia da secao 4, classe
`.ctx-menu`) na posicao do cursor, clampado na viewport. Itens: icone
14px + label 11px tracking 0.1em; hover = borda `--line` + texto
bright (sem glow); disabled = textFaint + cursor not-allowed.

Desenhos de componente (grelhas, ilustracoes do miolo): svg em
`src/graphics/*.svg` com as cores de fosforo FIXAS em hex (phHex) e
stroke-width 1, mesmos tons do outline do knob (claro ph(110),
escuro ph(48), detalhe ph(160)); rasterizados no canvas via
`rasterSvg` de `src/render/raster.ts` (cache + redraw no onload);
estado inativo = globalAlpha 0.4 no drawImage, nunca cores novas.
Ex: sound-emitter.svg do speaker.

Icones: TODOS feitos a mao pelo dono em `src/icons/*.svg` (20x20,
stroke/fill `currentColor`, geometria com offsets .5 pra 1px crisp)
e usados SO pelo componente `icon()` de `src/ui/icon.ts` (16px no
menu). Nenhuma biblioteca de icones entra no projeto, nunca. Em uso:
lock-closed / lock-open (estado do LOCK COMPONENT), trash (DELETE),
grid-enable / grid-disable (acao do item de snap: ligado mostra
grid-disable + "DISABLE GRID SNAP", desligado o inverso). Icone novo
= svg novo do dono em src/icons. No canvas, icone entra rasterizado
via Renderer.iconImage (tint phHex, cache por nome+cor).

Conteudo por alvo: componente = DELETE COMPONENT (disabled se
travado; lock/unlock e o toggle do header, NAO entra no menu);
cabo/waypoint = DELETE CABLE; mesa vazia = ENABLE/DISABLE GRID SNAP.

## 7.3 Toolbox

Painel no canto superior DIREITO (mesma anatomia do components box:
brackets, header TOOLBOX + contador `01 TOOLS`, regua). Item de
ferramenta: indice 10px faint + icone 16px do dono + nome 12px;
hover = borda `--line` + texto bright (sem glow); clique executa.
Ferramentas atuais: CENTRALIZE (icone centralize.svg) - enquadra o
patch inteiro na vista (bbox dos componentes + 100px de folga, zoom
0.15..1.5; mesa vazia reseta a camera).

## 8. Mesa (grade e origem)

- passo base 46 (o mesmo do palco do lumiere);
- ponto: quadrado ~0.85px de tela, alpha 0.115 (26/255);
- LOD: passo dobra enquanto `passo * zoom < 23px`; pontos de indice
  (par, par) ficam firmes, o resto esvaece com
  `fade = clamp((passo*zoom - 20) / 26)`;
- origem: cruz de 14px em lineFaint + label `/ORIGIN` 10px faint;
- mesa vazia fica VAZIA: nenhum texto de estado (o dono vetou o
  "DESK EMPTY"); o components box ja diz tudo.

## 9. Barras HUD

Altura 42px, grid `1fr auto 1fr`, padding lateral 18px, fundo
`--surface`, separadas da mesa por `1px --line`. Header: sujeito a
esquerda, titulo central tracking 0.22em, timecode `HH:MM:SS` +
caixinha `LIVE` (borda line-strong, dot 6px pulsando steps 1.1s) a
direita. Footer 11px: status, coords X/Y/Z centrais, contadores +
`LINK FEED:` a direita.

Texto bright de HUD leva `text-shadow: var(--glow-text)` (halo de
fosforo estatico; e halo de emissao, nao efeito de hover).

## 10. Pos-processamento CRT (arquitetura de performance)

Regra numero um: a cena 2d NITIDA vai direto pra tela (canvas #desk,
zero copia). O bloom e uma camada separada e barata por cima:

- canvas #glow (webgl2, `src/render/crt.ts`) fixo sobre o desk com
  `mix-blend-mode: plus-lighter` (soma = sharp + glow, o compositor
  do lumiere) e `pointer-events: none`, z39;
- backing de 1/4 da resolucao da cena (o CSS estica de graca); por
  frame sujo: um drawImage de downscale + upload de 1/16 dos bytes +
  um fragment em 1/4 de res. NUNCA upload full-res, NUNCA mipmap por
  frame;
- shader: `glow = ring(1.2) * 0.5 + ring(3.2) * 0.5` (anel de 9 taps
  bilineares; raio pequeno = halo de fosforo, grande = halo de
  vidro), saida `glow * 0.55`;
- aberracao cromatica ~2px horizontal SO no glow: canal r amostra o
  halo deslocado pra esquerda, b pra direita. O traco nitido nunca
  separa - texto continua branco e legivel (regra central do lumiere);
- roda SO em frame sujo (dirty flag); mesa parada = zero GPU;
- HDR: em tela com headroom (`dynamic-range: high`) o backbuffer do
  glow vira float16 (`drawingBufferStorage(RGBA16F)`) em display-p3 +
  `configureHighDynamicRange({mode:'extended'})` quando existir, e o
  shader ganha `uGain 0.75` + nucleo quente `glow^3 * 1.6` que passa
  do branco SDR: os brilhos ESTOURAM em tela HDR. Em tela SDR os
  uniforms voltam pra `0.55 / 0.0` e o resultado e bit-identico ao de
  sempre. Qualquer falha no caminho HDR degrada pra SDR em silencio.

Vinheta (z40, radial ate `rgb(0 0 0 / 0.34)`), scanlines (z41,
repeating-linear 2px + 1px `rgb(0 0 0 / 0.16)`) e grain (z42, SVG
feTurbulence 240x240 fractalNoise 0.85/2 oitavas, opacity 0.05,
inset -120px, anim `steps(3)` 0.9s so em transform) sao overlays CSS
compostos em GPU, custo zero de JS. Sem WebGL2 o #glow e removido e
fica tudo igual, so sem bloom.

Proibido reintroduzir: shadowBlur no canvas 2d, passadas de halo a
mao, upload de textura full-res, generateMipmap por frame, filtro
CSS no #desk. Performance e parte da identidade.

`prefers-reduced-motion`: grain e blink do LIVE param.

## 11. Cursors

pan grabbing | corpo de modulo grab (grabbing arrastando) |
knob ns-resize | port crosshair | cabo pointer | resto default.

## 12. Proibido

- cor fora do eixo de fosforo (nenhum accent, nenhum semantic color);
- glow/box-shadow em hover de elemento DOM (hover = borda + brilho de
  texto, so);
- border-radius, gradiente decorativo, blur de fundo;
- emoji ou icone figurativo (vocabulario e texto, tracos e geometria);
- animacao continua decorativa fora de grain/LIVE;
- texto misto (minusculas) em labels; hex cru fora de palette/tokens;
- conteudo vazando de outline (tudo cabe dentro da moldura, ellipsis
  se precisar);
- componente fora da base: tamanho que nao seja unidades inteiras,
  ports fora da faixa de IO, layout de header proprio;
- icone de biblioteca (tabler, lucide, qualquer uma): icone e svg
  feito a mao em src/icons; componentes sobrepostos na mesa.
