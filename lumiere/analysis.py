# analise de audio por frame de video: espectro log, bandas, rms,
# centroide, fluxo espectral, crest, largura estereo, envelope global.
import wave
import numpy as np


def analyze(path, fps, score=None):
    w = wave.open(path)
    sr = w.getframerate()
    n = w.getnframes()
    ch = w.getnchannels()
    raw = np.frombuffer(w.readframes(n), dtype=np.int16).astype(np.float32) / 32768.0
    if ch == 2:
        L, R = raw[0::2], raw[1::2]
    else:
        L = R = raw
    mono = (L + R) * 0.5
    dur = n / sr
    nf = int(dur * fps)
    hop = sr / fps

    NFFT = 4096
    win = np.hanning(NFFT).astype(np.float32)
    NB = 96
    freqs = np.fft.rfftfreq(NFFT, 1.0 / sr)
    edges = np.geomspace(30, 16000, NB + 1)
    bin_idx = np.searchsorted(freqs, edges)

    spec = np.zeros((nf, NB), np.float32)
    rms = np.zeros(nf, np.float32)
    bands = np.zeros((nf, 3), np.float32)      # low/mid/high
    centroid = np.zeros(nf, np.float32)
    width = np.zeros(nf, np.float32)
    crest = np.zeros(nf, np.float32)
    flux = np.zeros(nf, np.float32)

    lo_m = freqs < 250
    mid_m = (freqs >= 250) & (freqs < 2500)
    hi_m = freqs >= 4000
    prev_mag = None
    for i in range(nf):
        c = int(i * hop)
        seg = mono[c:c + NFFT]
        if len(seg) < NFFT:
            seg = np.pad(seg, (0, NFFT - len(seg)))
        mag = np.abs(np.fft.rfft(seg * win)).astype(np.float32)
        for b in range(NB):
            s = mag[bin_idx[b]:bin_idx[b + 1]]
            spec[i, b] = s.mean() if len(s) else 0.0
        tot = mag.sum() + 1e-9
        centroid[i] = float((freqs * mag).sum() / tot)
        bands[i, 0] = mag[lo_m].sum() / tot
        bands[i, 1] = mag[mid_m].sum() / tot
        bands[i, 2] = mag[hi_m].sum() / tot
        if prev_mag is not None:
            d = mag - prev_mag
            flux[i] = float(np.sqrt((d[d > 0] ** 2).sum()) / tot)
        prev_mag = mag

        sw = mono[c:c + int(hop) + 1]
        if len(sw):
            r = float(np.sqrt((sw ** 2).mean()))
            rms[i] = r
            crest[i] = float(np.abs(sw).max() / (r + 1e-9))
        sl = L[c:c + int(hop) + 1]
        sr_ = R[c:c + int(hop) + 1]
        if len(sl):
            side = float(np.sqrt(((sl - sr_) ** 2).mean()))
            midr = float(np.sqrt(((sl + sr_) ** 2).mean())) + 1e-9
            width[i] = side / midr

    # normalizacao do espectro (log + escala 0..1)
    ls = np.log10(spec + 1e-6)
    lo, hi = np.percentile(ls, 5), np.percentile(ls, 99.5)
    spec_n = np.clip((ls - lo) / (hi - lo + 1e-9), 0, 1).astype(np.float32)

    # envelope global (para o campo de arranjo)
    ne = 1200
    env = np.zeros(ne, np.float32)
    step = len(mono) / ne
    for i in range(ne):
        s = mono[int(i * step):int((i + 1) * step)]
        env[i] = np.sqrt((s ** 2).mean()) if len(s) else 0
    env = env / (env.max() + 1e-9)

    # espectrograma miniatura (thumb do subject)
    tw, th = 120, 84
    idx_t = np.linspace(0, nf - 1, tw).astype(int)
    idx_b = np.linspace(0, NB - 1, th).astype(int)
    thumb = spec_n[idx_t][:, idx_b].T[::-1]

    feats = {
        'sr': sr, 'dur': dur, 'nf': nf, 'fps': fps,
        'mono': mono, 'spec': spec_n, 'rms': rms, 'bands': bands,
        'centroid': centroid, 'width': width, 'crest': crest, 'flux': flux,
        'env': env, 'thumb': thumb,
    }

    if score is not None:
        feats.update(layer_activity(score, nf, fps))
    return feats


def layer_activity(score, nf, fps):
    order = score['order']
    nt = len(order)
    tidx = {t: i for i, t in enumerate(order)}
    gate = np.zeros((nt, nf), np.float32)
    last_note = np.full((nt, nf), -1, np.int16)
    counts = np.zeros(nt, np.int32)
    for (tr, t0, t1, notes, vel) in score['events']:
        i = tidx[tr]
        a = max(0, int(t0 * fps))
        b = min(nf, max(a + 1, int(t1 * fps)))
        gate[i, a:b] = np.maximum(gate[i, a:b], vel)
        last_note[i, a:] = max(notes)
        counts[i] += 1
    # release exponencial (ballistics de VU)
    act = np.zeros_like(gate)
    dec = np.exp(-1.0 / (0.22 * fps))
    for i in range(nt):
        v = 0.0
        g = gate[i]
        o = act[i]
        for f in range(nf):
            v = g[f] if g[f] > v else v * dec
            o[f] = v
    return {'layers': act, 'layer_last': last_note, 'layer_counts': counts}
