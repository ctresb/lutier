# do i wanna lovania

Mashup: a melodia de **Megalovania** (a.mid, Toby Fox) sobre a alma de
**Do I Wanna Know?** (b.mid, Arctic Monkeys). Renderizado no lutier a partir
de `remix.synth` + `remix.score`. Este documento registra o processo inteiro,
as decisoes e as boas praticas que fizeram funcionar, para servir de receita
em mashups futuros.

## 1. Analise das fontes (nunca pule)

Antes de escrever uma nota, os dois MIDIs foram parseados e dumpados
(parser proprio, sem dependencias). O objetivo: decidir **o que cada fonte
doa** para o mashup. Regra de ouro: um mashup bom pega de A e de B coisas de
categorias DIFERENTES. Se os dois doarem melodia, briga; se os dois doarem
groove, mingau.

O que foi extraido de cada um:

| fonte | doou | dado concreto |
|---|---|---|
| a.mid (Megalovania) | melodia + identidade | riff em 16ths: `X X d(oitava) a g# g f d f g`, onde X = 2 notas na fundamental do compasso (D, C, B, Bb no original); 118bpm; re menor |
| b.mid (DIWK) | harmonia, andamento, groove, clima | 88bpm; sol menor; progressao i-VI-iv-V (Gm, Eb, Cm, D); bumbo em TODO beat (stomp) + palma/caixa no 2 e 4; riff vocal grave e arrastado; baixo esparso (1 nota por acorde) |

Descobertas que so o dump revela:
- A cabeca do riff de Megalovania (as 2 primeiras notas de cada compasso)
  segue o baixo. E um mecanismo de transposicao embutido na melodia: da para
  apontar essas cabecas para QUALQUER progressao.
- Nos compassos 3 e 4 da frase original a cabeca vira uma nota so em
  offbeat (0.5). Manter isso preserva a sintaxe da frase, nao so as notas.
- O groove de DIWK nao esta no baixo (que e quase todo pausa), esta no
  bumbo 4x4 + palma. O baixo esparso E parte do clima; encher ele mataria.

## 2. Decisoes estruturais

**Tom: re menor (o tom de A).** A melodia e a protagonista; ela nao se
dobra. A progressao de B foi transposta para o tom de A:
`Gm Eb Cm D  ->  Dm Bb Gm A` (i-VI-iv-V preservada). Transpor a melodia
para o tom de B tambem funcionaria, mas mover 4 acordes custa menos que
mover uma melodia iconica inteira.

**Andamento: 88bpm (o de B).** O pedido era "A com o clima de B", e clima
mora no andamento. A melodia em 16ths a 88bpm fica arrastada e pesada,
exatamente o efeito desejado. Nao se calcula media entre os bpms: escolhe-se
UM dos dois mundos e se compromete.

**Casamento melodia x progressao nova.** As cabecas do riff apontam para as
fundamentais da nova progressao: `d d | bb bb | (g) | (a)`, compassos 3-4
com cabeca unica em offbeat como no original. A cauda cromatica
`a g# g f d f g` fica identica em todos os compassos:
- sobre Bb: a = setima maior de passagem, resolve na descida cromatica;
- sobre A (V): g# = terca do acorde. A linha cromatica de Megalovania
  ENCAIXA na dominante por acidente feliz. Sempre teste a cauda do motivo
  contra cada acorde novo; as dissonancias em tempo fraco quase sempre
  passam, as em tempo forte precisam de justificativa.

**O riff de B vira personagem, nao cama.** O riff vocal de DIWK foi
transposto nota a nota para re menor (5 semitons abaixo) e usado em tres
papeis ao longo da peca: hook solo na intro, pedal no build, contraponto
uma oitava acima no drop. Mashup nao e colagem: o material de B precisa
circular pela peca com funcoes diferentes.

## 3. Forma e arco

```
stomp 8 | riff 16 | verse 16 | verse2 16 | build 8 | drop 32 |
breakdown 16 | build2 8 | drop2 32 | outro 16        (168 beats @ 88)
```

- Cada secao muda UMA coisa grande (regra do maestro). stomp = so bateria;
  riff = +hook; verse = +melodia grave e contida; verse2 = +coro e chimbal;
  drop = melodia oitavada; breakdown = tira tudo, motivo em ritmo aumentado
  no epiano; drop2 = strings NO LUGAR do pad + taiko + stabs.
- Drop2 troca camada em vez de empilhar: "mais epico" nao e "mais coisas",
  e camada mais rica no mesmo espaco espectral.
- Breakdown cita o motivo em ritmo aumentado (2x mais lento). Transformar o
  motivo (aumentacao, oitava, instrumento) rende mais que repetir literal.
- Dois drops, dois sfx diferentes: drop1 abre com sub_drop, drop2 com
  impact. Variedade nas costuras evita fadiga.
- Risers escalam com a duracao da nota e terminam NO beat do drop, nunca
  1 beat antes ou depois. Build2 poe caixa acelerando (1 -> 0.5 -> 0.25)
  sob o riser: tensao dupla, truque barato que sempre funciona.
- Outro devolve o vazio da intro e termina na tonica SEM terca (suspenso,
  combina com o clima noturno de B).

## 4. Instrumentacao por papel (mix nasce aqui)

| papel | preset | por que |
|---|---|---|
| fundacao | subbass | notas esparsas como o baixo de B (1 por acorde + aproximacao); espaco e clima |
| cama | pad_dark (verses) -> strings_warm (drop2) | escuro por padrao; troca de camada = novo patamar epico |
| voz de apoio | choir_vox | o "falsete de apoio" do DIWK; morph automatizado |
| melodia | lead_saw | cutoff automatizado de 1300 a 5200: a peca inteira clareia rumo ao fim |
| hook/contraponto | bass_deep com drv 0.4 | timbre sujo de guitarra grave |
| eco/citacao | epiano_fm | breakdown e outro, registro medio |
| pulso | kick_deep 4x4 | o stomp de B e sagrado: todo beat, sem excecao |
| backbeat | clap_funk (+ snare_seca so nos drops) | palma no 2 e 4 vinda de B |
| brilho | hat_closed 8ths, shaker | velocity SEMPRE < 0.4 (acima disso chia) |
| pontuacao | taiko so no drop2 | tempos fortes + virada no fim de cada frase de 16 |

Boas praticas de mix que valeram ouro:
- Registros sem colisao: hook em 2-3, melodia em 4-5, pad em 3, sub em 1-2.
  No drop o hook sobe uma oitava para nao brigar com o subbass.
- Percussao inteira dirigida por velocity (ghost 0.3, normal 0.6,
  acento 0.95), nunca por gain.
- Master com compressor ratio 3, attack 8ms (batida pesada soca mais)
  + limiter -1db. Sempre.

## 5. Verificacao numerica (obrigatoria, 2 iteracoes)

Todo render passa pelo script de analise (RMS por janela, bandas, peak,
width). O que os numeros pegaram que o ouvido interno nao pega:

**Iteracao 1: arco raso.** Intro -25db, drop -21db: so 4db de arco (minimo
8). Causa: intro quente (kick 0.9, clap 0.8) roubando headroom do drop.
Correcao: baixar velocities do comeco (kick 0.72/0.58, pad 0.3), subir as
do drop (lead 0.9+, coro 0.45). Nao se conserta arco raso aumentando o
drop apenas: abaixa-se a intro TAMBEM, senao a normalizacao global come o
ganho.

**Iteracao 2 (versao estendida): aprovada.**
```
intro -31db -> drop1 -23db -> drop2 -20.5db
```
- intro -> drop2 = 10.5db (epico pede >= 10) ok
- drop2 ~1.5db acima do drop1 (hierarquia de climax certa) ok
- breakdown em -26db, nivel de verse: respiro, nao buraco ok
- riser rampa de RMS ate a borda exata do drop2 ok
- peak 0.847, width 0.017-0.02 (estereo vivo), cauda morre natural ok

Criterio de parada: maximo 3 iteracoes de correcao; passou dos criterios,
entrega.

## 6. Receita resumida para o proximo mashup

1. Dumpe os dois MIDIs. Liste o que cada um doa (categorias diferentes).
2. Melodia manda no tom; clima manda no bpm. Escolha UM lado para cada.
3. Transporte a progressao de B para o tom de A e teste a cauda do motivo
   de A contra cada acorde novo.
4. Ache o mecanismo interno da melodia de A (cabecas que seguem o baixo,
   sequencias, pergunta/resposta) e aponte-o para a harmonia nova.
5. O material de B circula: hook, pedal, contraponto. Nunca so cama.
6. Forma em dois atos com breakdown citando o motivo transformado.
7. Uma mudanca grande por secao; climax final troca camada, nao empilha.
8. Verifique com numeros: arco >= 8db (epico >= 10), sem buraco, riser na
   borda, peak ~0.85+, width > 0.01. Corrija abaixando a intro, nao so
   subindo o drop.

## Arquivos

- `remix.synth` / `remix.score`: fontes editaveis
- `remix.mp3`: render final (168 beats, ~118s)
- Visualizacao: `python3 -m lumiere out/remix.wav examples/songs/megalovania_dwk/remix.score -o out/remix_lumiere.mp4 --title DO_I_WANNA_LOVANIA`
- Reproduzivel bit a bit com `--seed 42`
