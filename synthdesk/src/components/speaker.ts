import { deskAudio } from '../audio/audio'
import { rasterSvg } from '../render/raster'
import soundEmitter from '../graphics/sound-emitter.svg?raw'
import type { ComponentSpec } from './spec'

// o speaker NAO gera nem processa som: e so a saida da mesa pro
// dispositivo do computador. escolhe o device e liga/desliga.
const CX = 69
const CY = 106

export const speaker: ComponentSpec = {
  type: 'speaker',
  title: 'SPEAKER',
  tag: 'AUDIO OUT',
  prefix: 'SPK',
  category: 'PRIMITIVES',
  unitsW: 3,
  unitsH: 4,
  inputs: [{ id: 'in', kind: 'audio', label: 'IN' }],
  outputs: [],
  // ligar/desligar e o switch de energia da base, no header
  params: [{ id: 'device', label: 'DEVICE', def: 0 }],
  controls: [
    { kind: 'selector', id: 'device', x: 14, y: 48, label: 'DEV' },
    { kind: 'rule', y: 63 },
  ],

  press(node, button) {
    if (button === 'device') {
      node.params.device = ((node.params.device ?? 0) + 1) % deskAudio.count()
    }
  },

  selectorValue(node) {
    const label = deskAudio.deviceLabel(node.params.device ?? 0)
    return label.length > 13 ? `${label.slice(0, 12)}…` : label
  },

  drawExtra(g, node) {
    // grelha do falante: asset svg com os tons de outline do knob
    const on = (node.params.on ?? 1) > 0
    const img = rasterSvg('graphic/sound-emitter', soundEmitter)
    if (img) {
      if (!on) g.globalAlpha = 0.4
      g.drawImage(img, CX - 32, CY - 32, 64, 64)
      g.globalAlpha = 1
    }
  },
}
