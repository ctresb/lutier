# parser minimo de .score: tempo, sections, tracks, eventos, arrange.
# so o suficiente para visualizacao (automate/swing/humanize/set ignorados).
import re

EVENT_RE = re.compile(
    r"^\s*([\d.]+)\s+(\[[^\]]+\]|[a-g][#b]?\d)\s+([\d.]+)\s+([\d.]+)"
    r"(?:\s+x(\d+)\s+@([\d.]+))?\s*$")

NOTE_NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B']
_SEMI = {'c': 0, 'd': 2, 'e': 4, 'f': 5, 'g': 7, 'a': 9, 'b': 11}


def note_midi(s):
    p = _SEMI[s[0]]
    i = 1
    if len(s) > 1 and s[1] in '#b':
        p += 1 if s[1] == '#' else -1
        i = 2
    return p + (int(s[i:]) + 1) * 12


def note_name(m):
    return f"{NOTE_NAMES[m % 12]}{m // 12 - 1}"


def parse_score(path):
    tempo = 120.0
    sections = {}
    track_synth = {}
    track_order = []
    arrange = []
    cur_sec = None
    cur_track = None
    for raw in open(path):
        line = raw.split('#')[0].rstrip()
        if not line.strip():
            continue
        parts = line.split()
        if parts[0] == 'tempo' and cur_sec is None:
            tempo = float(parts[1])
        elif parts[0] == 'section':
            cur_sec = parts[1]
            sections[cur_sec] = {'len': float(parts[2]), 'events': []}
        elif parts[0] == 'track':
            tid, synth = parts[1], parts[2]
            if tid not in track_synth:
                track_synth[tid] = synth
                track_order.append(tid)
            cur_track = tid
        elif parts[0] == 'arrange':
            arrange = parts[1:]
        elif parts[0] in ('automate', 'swing', 'humanize', 'set', 'master'):
            continue
        else:
            m = EVENT_RE.match(line)
            if m and cur_sec and cur_track:
                beat = float(m.group(1))
                tok = m.group(2)
                dur = float(m.group(3))
                vel = float(m.group(4))
                reps = int(m.group(5) or 1)
                step = float(m.group(6) or 0)
                if tok.startswith('['):
                    notes = [note_midi(x) for x in tok[1:-1].split()]
                else:
                    notes = [note_midi(tok)]
                for r in range(reps):
                    sections[cur_sec]['events'].append(
                        (cur_track, beat + r * step, notes, dur, vel))

    spb = 60.0 / tempo
    timeline = []
    events = []
    t = 0.0
    for name in arrange:
        sec = sections[name]
        for (tr, b, notes, dur, vel) in sec['events']:
            events.append((tr, t + b * spb, t + (b + dur) * spb, notes, vel))
        timeline.append((name, t, t + sec['len'] * spb))
        t += sec['len'] * spb

    return {
        'tempo': tempo, 'spb': spb,
        'tracks': track_synth, 'order': track_order,
        'timeline': timeline, 'events': events, 'total': t,
    }
