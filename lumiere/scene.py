# renderer do terminal de analise: HUD monocromatico 1920x1080,
# paineis com brackets, particulas espectrais, glow, grain, scanlines.
import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont

from .scoreparse import note_name

W, H = 1920, 1080
MENLO = "/System/Library/Fonts/Menlo.ttc"
HIRAGINO = "/System/Library/Fonts/Hiragino Sans GB.ttc"

KATA = list("アイウエオカキクケコサシスセソタチツテトナニヌネノハヒフヘホマミムメモヤユヨラリルレロワヲン0123789")
HEXCH = list("0123456789ABCDEF")

POEM = ["the music is not", "the notes", "", "it is what moves", "between", "the notes"]
JPCOL = "音は消えて意味は残る"   # "o som se vai, o sentido fica"
NOTES_PANEL = ["MELODY OF A", "BONES OF B", "", "SIGNAL FADES", "GROOVE LINGERS"]


def _font(size, bold=False):
    return ImageFont.truetype(MENLO, size, index=1 if bold else 0)


class Scene:
    def __init__(self, feats, score, title, seed=7):
        self.f = feats
        self.s = score
        self.title = title.upper()
        self.rng = np.random.default_rng(seed)
        self.f10 = _font(10)
        self.f12 = _font(12)
        self.f13 = _font(13)
        self.f15 = _font(15)
        self.f18 = _font(18, bold=True)
        self.f22 = _font(22, bold=True)
        try:
            self.fjp = ImageFont.truetype(HIRAGINO, 15)
        except OSError:
            self.fjp = None

        # geometria
        self.frame = (300, 68, 1724, 1012)          # moldura principal
        self.colL = (322, 90, 586)                  # x0, y_top, x1 coluna esq
        self.colR = (1392, 90, 1702)
        self.center = (608, 130, 1372, 818)         # palco da entidade
        self.field = (608, 842, 1372, 962)          # campo de arranjo
        self.cx = (self.center[0] + self.center[2]) // 2

        # particulas persistentes
        self.MAXP = 9000
        self.px = np.zeros(self.MAXP, np.float32)
        self.py = np.zeros(self.MAXP, np.float32)
        self.pvx = np.zeros(self.MAXP, np.float32)
        self.pvy = np.zeros(self.MAXP, np.float32)
        self.plife = np.zeros(self.MAXP, np.float32)
        self.pbri = np.zeros(self.MAXP, np.float32)
        self.pcur = 0

        self.rings = []          # (t0,) pulsos de grave
        self.prev_low = 0.0
        self.siglog = []         # transicoes de secao vistas
        self.seen_secs = set()

        # sparklines por secao (do envelope global)
        self.sec_spark = {}
        env, total = feats['env'], score['total']
        for (name, t0, t1) in score['timeline']:
            a = int(t0 / total * len(env))
            b = max(a + 2, int(t1 / total * len(env)))
            e = env[a:b]
            idx = np.linspace(0, len(e) - 1, 56).astype(int)
            self.sec_spark[name] = e[idx]

        self._build_static()
        self._precompute_post()

    # ---------- estatico ----------

    def _panel(self, d, x0, y0, x1, y1, title=None):
        col = (120,)
        d.rectangle([x0, y0, x1, y1], outline=95)
        for (cx, cy, dx, dy) in ((x0, y0, 1, 1), (x1, y0, -1, 1),
                                 (x0, y1, 1, -1), (x1, y1, -1, -1)):
            d.line([cx, cy, cx + 10 * dx, cy], fill=225, width=1)
            d.line([cx, cy, cx, cy + 10 * dy], fill=225, width=1)
        if title:
            d.text((x0 + 14, y0 + 10), title, font=self.f15, fill=235)
            d.line([x0 + 14, y0 + 34, x1 - 14, y0 + 34], fill=80)

    def _build_static(self):
        img = Image.new('L', (W, H), 0)
        d = ImageDraw.Draw(img)
        fx0, fy0, fx1, fy1 = self.frame

        # moldura principal + header/footer
        d.rectangle([fx0, fy0, fx1, fy1], outline=110)
        d.rectangle([fx0 + 2, fy0 + 2, fx1 - 2, fy1 - 2], outline=45)
        d.line([fx0, fy0 + 46, fx1, fy0 + 46], fill=95)
        d.line([fx0, fy1 - 34, fx1, fy1 - 34], fill=95)
        d.text((fx0 + 18, fy0 + 13), f"SUBJECT // {self.title}", font=self.f18, fill=245)
        tw = d.textlength("LUMIERE :: ANALYSIS TERMINAL", font=self.f15)
        d.text(((fx0 + fx1) / 2 - tw / 2, fy0 + 15), "LUMIERE :: ANALYSIS TERMINAL",
               font=self.f15, fill=200)
        d.text((fx0 + 18, fy1 - 26), "STATUS: RESONATING", font=self.f13, fill=180)
        d.text((fx1 - 168, fy1 - 26), "LINK FEED: LIVE", font=self.f13, fill=180)

        # coluna esquerda
        x0, _, x1 = self.colL
        self._panel(d, x0, 130, x1, 323)          # thumb
        d.text((x0 + 14, 138), self.title[:18], font=self.f13, fill=235)
        self._panel(d, x0, 343, x1, 536, "PROFILE LOG")
        self._panel(d, x0, 556, x1, 749, "SIGNAL METRICS")
        self._panel(d, x0, 769, x1, 962, "NOTES")
        for i, ln in enumerate(NOTES_PANEL):
            d.text((x0 + 16, 815 + i * 22), ln, font=self.f13, fill=190)
        d.text((x0 + 16, 919), "...", font=self.f13, fill=140)

        # coluna direita
        x0, _, x1 = self.colR
        self._panel(d, x0, 130, x1, 323, "WAVEFORM ANALYSIS")
        self._panel(d, x0, 343, x1, 536, "FREQUENCY MAP")
        self._panel(d, x0, 556, x1, 749, "SIGNAL LOG")
        self._panel(d, x0, 769, x1, 962, "LAYER MATRIX")

        # campo de arranjo
        self._panel(d, *self.field, "ARRANGEMENT FIELD")

        # margens externas
        d.text((70, 380), "/ROOT", font=self.f15, fill=210)
        for i, ln in enumerate(POEM):
            d.text((1760, 520 + i * 24), ln, font=self.f13, fill=185)
        d.text((1760, 520 + len(POEM) * 24 + 8), "...", font=self.f13, fill=130)
        d.text((60, 828), "DATA FRAGMENTS", font=self.f13, fill=185)

        # labels fixos do perfil
        x0 = self.colL[0]
        prof = [("ID", "DIWL-88"), ("CLASS", "REMIX"), ("ORIGIN", "DUAL SRC"),
                ("KEY", "D MINOR"), ("TEMPO", f"{self.s['tempo']:.0f} BPM")]
        for i, (k, v) in enumerate(prof):
            d.text((x0 + 16, 386 + i * 22), k, font=self.f13, fill=160)
            d.text((x0 + 120, 386 + i * 22), v, font=self.f13, fill=230)
        d.text((x0 + 16, 386 + 5 * 22), "STATE", font=self.f13, fill=160)
        d.text((x0 + 16, 386 + 6 * 22), "SYNC", font=self.f13, fill=160)

        met = ["SMR", "FLUX", "CREST", "PHASE", "COHER"]
        for i, k in enumerate(met):
            d.text((x0 + 16, 602 + i * 26), k, font=self.f13, fill=160)

        # coordenadas (labels) no palco
        cx0, cy0 = self.center[0], self.center[1]
        d.text((cx0 + 18, cy0 + 12), "COORDINATES", font=self.f13, fill=210)
        for i, k in enumerate("XYZ"):
            d.text((cx0 + 18, cy0 + 38 + i * 20), k, font=self.f13, fill=160)

        # grade de pontos do palco
        arr = np.array(img, np.float32)
        gx = np.arange(self.center[0] + 24, self.center[2] - 12, 46)
        gy = np.arange(self.center[1] + 24, self.center[3] - 12, 46)
        for y in gy:
            arr[int(y), gx.astype(int)] = np.maximum(arr[int(y), gx.astype(int)], 26)
        self.static = arr

    def _precompute_post(self):
        yy, xx = np.mgrid[0:H, 0:W].astype(np.float32)
        r2 = (((xx - W / 2) / (W / 2)) ** 2 + ((yy - H / 2) / (H / 2)) ** 2)
        self.vign = (1.0 - 0.32 * r2 ** 1.2).astype(np.float32)
        scan = np.ones(H, np.float32)
        scan[1::3] = 0.94
        scan[2::3] = 0.985
        self.scan = scan[:, None]

    # ---------- dinamico ----------

    def _spawn(self, i):
        f = self.f
        spec = f['spec'][i]
        c0, cy0, c1, cy1 = self.center
        top, bot = cy1 - 300 - 260, cy1 - 30   # coluna da entidade
        nb = len(spec)
        energy = spec ** 1.4
        n_new = int(40 + energy.sum() * 120)
        bins = self.rng.choice(nb, size=n_new, p=energy / energy.sum()) \
            if energy.sum() > 1e-6 else np.zeros(n_new, int)
        yy = np.interp(bins, [0, nb - 1], [bot, cy0 + 70])
        hw = 12 + spec[bins] * 120
        xx = self.cx + self.rng.standard_normal(n_new) * hw * 0.42
        # simetria espelhada (fantasma bilateral)
        mirror = self.rng.random(n_new) < 0.5
        xx = np.where(mirror, 2 * self.cx - xx, xx)
        k = (self.pcur + np.arange(n_new)) % self.MAXP
        self.px[k] = np.clip(xx, c0 + 8, c1 - 8)
        self.py[k] = yy + self.rng.standard_normal(n_new) * 6
        self.pvx[k] = self.rng.standard_normal(n_new) * 2.4
        self.pvy[k] = -10 - self.rng.random(n_new) * 26
        self.plife[k] = 0.35 + self.rng.random(n_new) * 1.0
        self.pbri[k] = 120 + spec[bins] * 150
        self.pcur = (self.pcur + n_new) % self.MAXP

    def _particles(self, dyn, i, dt):
        alive = self.plife > 0
        self.plife[alive] -= dt
        self.px[alive] += self.pvx[alive] * dt
        self.py[alive] += self.pvy[alive] * dt
        a = self.plife > 0
        if a.any():
            xi = self.px[a].astype(int)
            yi = self.py[a].astype(int)
            ok = (xi > self.center[0] + 4) & (xi < self.center[2] - 4) & \
                 (yi > self.center[1] + 4) & (yi < self.center[3] - 4)
            b = (self.pbri[a] * np.clip(self.plife[a] / 0.4, 0, 1))[ok]
            np.add.at(dyn, (yi[ok], xi[ok]), b)
            hot = b > 150
            np.add.at(dyn, (yi[ok][hot], xi[ok][hot] + 1), b[hot] * 0.6)
        # nucleo da entidade: feixe vertical respirando com o rms
        i_rms = self.f['rms'][i]
        low = self.f['bands'][i, 0]
        c0, cy0, c1, cy1 = self.center
        bw2 = 7 + low * 42
        xs = np.arange(self.cx - 90, self.cx + 90)
        xprof = np.exp(-0.5 * ((xs - self.cx) / bw2) ** 2)
        ys = np.arange(cy0 + 80, cy1 - 26)
        ny = len(ys)
        yr = (ys - (cy0 + 80)) / max(ny, 1)
        yprof = (0.35 + 0.65 * yr ** 1.3) * (i_rms * 1400)
        fade = min(36, ny)
        yprof[-fade:] *= np.linspace(1.0, 0.12, fade)
        yprof[:24] *= np.linspace(0.15, 1.0, min(24, ny))
        dyn[cy0 + 80:cy1 - 26, self.cx - 90:self.cx + 90] += \
            np.outer(yprof, xprof).astype(np.float32)

    def _rings(self, d, i, t):
        f = self.f
        low = f['bands'][i, 0] * f['rms'][i]
        if low - self.prev_low > 0.012 and (not self.rings or t - self.rings[-1] > 0.18):
            self.rings.append(t)
        self.prev_low = low
        self.rings = [r for r in self.rings if t - r < 1.4]
        ey = self.center[3] - 240
        for r0 in self.rings:
            age = t - r0
            rr = 30 + age * 260
            fade = int(max(0, 110 * (1 - age / 1.4)))
            if fade > 4:
                bb = [self.cx - rr * 1.7, ey - rr * 0.42, self.cx + rr * 1.7, ey + rr * 0.42]
                for a0 in range(0, 360, 24):
                    d.arc(bb, a0, a0 + 13, fill=fade)

    def _entity_extras(self, d, i, t):
        # orbitas tracejadas + ticks + glifos flutuantes
        ey = self.center[3] - 270
        for (rx, ry, spd, ph) in ((300, 74, 9, 0), (218, 50, -14, 120)):
            off = (t * spd + ph) % 360
            bb = [self.cx - rx, ey - ry, self.cx + rx, ey + ry]
            for a0 in range(0, 360, 18):
                d.arc(bb, a0 + off, a0 + off + 9, fill=70)
        f = self.f
        hi = f['bands'][i, 2]
        rng = np.random.default_rng(i * 31 + 5)
        ng = int(8 + hi * 90)
        c0, cy0, c1, cy1 = self.center
        for _ in range(ng):
            gx = rng.integers(c0 + 30, c1 - 30)
            gy = rng.integers(cy0 + 30, cy1 - 30)
            if abs(int(gx) - self.cx) < 130:
                continue
            if rng.random() < 0.75 or self.fjp is None:
                ch = HEXCH[rng.integers(16)]
                fnt = self.f12 if rng.random() < 0.8 else self.f10
            else:
                ch = KATA[rng.integers(len(KATA))]
                fnt = self.fjp
            d.text((int(gx), int(gy)), ch, font=fnt, fill=int(50 + rng.random() * 110))
        # coluna vertical jp
        if self.fjp:
            for k, chj in enumerate(JPCOL):
                fade = 120 if (i // 8 + k) % 7 else 40
                d.text((c1 - 52, cy0 + 66 + k * 24), chj, font=self.fjp, fill=fade)

    def _left_margin(self, d, i, t):
        # orbital /ROOT
        ox, oy = 150, 500
        for rr in (22, 44, 68):
            for a0 in range(0, 360, 20):
                d.arc([ox - rr, oy - rr * 0.62, ox + rr, oy + rr * 0.62],
                      a0, a0 + 11, fill=75)
        ang = t * 0.55
        pxx = ox + 68 * np.cos(ang)
        pyy = oy + 42 * np.sin(ang)
        d.ellipse([pxx - 3, pyy - 3, pxx + 3, pyy + 3], fill=220)
        d.ellipse([ox - 3, oy - 3, ox + 3, oy + 3], fill=255)
        d.line([ox, oy, ox + 90, oy - 66], fill=90)
        d.text((ox + 96, oy - 76), "R", font=self.f12, fill=170)
        dv = self.f['flux'][i] * 100
        d.text((70, 408), f"Δ {dv:0.5f}%", font=self.f13, fill=190)
        # data fragments
        rng = np.random.default_rng(i // 3)
        base_on = rng.random((7, 14)) < 0.16 + self.f['bands'][i, 2] * 0.5
        for r in range(7):
            for c in range(14):
                if base_on[r, c]:
                    d.point((66 + c * 13, 856 + r * 13), fill=200)
                else:
                    d.point((66 + c * 13, 856 + r * 13), fill=45)
        # matriz de pontos direita
        rng2 = np.random.default_rng(i // 5 + 99)
        for r in range(6):
            for c in range(10):
                v = 170 if rng2.random() < 0.2 else 40
                d.point((1770 + c * 12, 800 + r * 12), fill=v)

    def _thumb(self, dyn, i):
        x0 = self.colL[0]
        th = self.f['thumb']
        h, w = th.shape
        px = int(i / max(1, self.f['nf'] - 1) * (w - 1))
        img = (th * 150).astype(np.float32)
        img[:, px] = np.maximum(img[:, px], 230)
        y0, x0b = 166, x0 + 16
        sy, sx = 118 // h + 1, 232 // w + 1
        big = np.kron(img, np.ones((2, 2), np.float32))[:118, :232]
        dyn[y0:y0 + big.shape[0], x0b:x0b + big.shape[1]] += big

    def _profile(self, d, i, t):
        f, s = self.f, self.s
        x0 = self.colL[0]
        sec = next(((n, t0, t1) for (n, t0, t1) in s['timeline'] if t0 <= t < t1),
                   (s['timeline'][-1][0], 0, s['total']))
        name, t0, t1 = sec
        sync = (t - t0) / max(t1 - t0, 1e-9) * 100
        d.text((x0 + 120, 386 + 5 * 22), name.upper(), font=self.f13, fill=245)
        d.text((x0 + 120, 386 + 6 * 22), f"{sync:3.0f}%", font=self.f13, fill=245)

        rms_db = 20 * np.log10(max(f['rms'][i], 1e-6))
        vals = [f"{rms_db:5.1f} dB", f"{f['flux'][i]*100:5.2f}%",
                f"{f['crest'][i]:5.2f}x",
                "LOCKED" if f['width'][i] < 0.32 else "DRIFT",
                f"{min(f['width'][i]*260,99):3.0f}%"]
        fracs = [np.clip((rms_db + 50) / 40, 0, 1), np.clip(f['flux'][i] * 9, 0, 1),
                 np.clip(f['crest'][i] / 9, 0, 1),
                 0.85 if f['width'][i] < 0.32 else 0.3,
                 np.clip(f['width'][i] * 2.6, 0, 1)]
        for k, (v, fr) in enumerate(zip(vals, fracs)):
            y = 602 + k * 26
            d.text((x0 + 82, y), v, font=self.f13, fill=240)
            bx = x0 + 186
            d.rectangle([bx, y + 4, bx + 60, y + 11], outline=85)
            d.rectangle([bx, y + 4, bx + int(60 * fr), y + 11], fill=200)
        return name, t0

    def _coords(self, d, i, t, secname):
        cx0, cy0 = self.center[0], self.center[1]
        f = self.f
        d.text((cx0 + 44, cy0 + 38), f"{f['centroid'][i]:8.3f}", font=self.f13, fill=235)
        d.text((cx0 + 44, cy0 + 58), f"{f['rms'][i]*1000:8.3f}", font=self.f13, fill=235)
        d.text((cx0 + 44, cy0 + 78), f"{f['width'][i]*100:8.3f}", font=self.f13, fill=235)
        beat = t / self.s['spb']
        state = "RISING" if i > 12 and f['rms'][i] > f['rms'][i - 12] else "FALLING"
        yb = self.center[3] - 86
        d.text((cx0 + 18, yb), f"// SECTION_{secname.upper()}", font=self.f13, fill=190)
        d.text((cx0 + 18, yb + 20), f"// BEAT: {beat:7.2f}", font=self.f13, fill=190)
        d.text((cx0 + 18, yb + 40), f"// STATUS: {state}", font=self.f13, fill=190)

    def _waveform(self, d, i):
        x0, _, x1 = self.colR
        y0, y1 = 178, 305
        f = self.f
        c = int(i / f['fps'] * f['sr'])
        seg = f['mono'][max(0, c - 8000):c + 8000]
        if len(seg) < 100:
            return
        wpx = x1 - x0 - 32
        idx = np.linspace(0, len(seg) - 1, wpx).astype(int)
        ys = seg[idx]
        my = (y0 + y1) / 2
        sc = (y1 - y0) * 0.48 / max(np.abs(ys).max(), 0.05)
        pts = [(x0 + 16 + k, my - v * sc) for k, v in enumerate(ys)]
        d.line(pts, fill=225, width=1)
        d.line([x0 + 16, my, x1 - 16, my], fill=55)

    def _freqmap(self, d, i):
        x0, _, x1 = self.colR
        y0, y1 = 391, 520
        spec = self.f['spec'][i]
        wpx = x1 - x0 - 90
        idx = np.linspace(0, len(spec) - 1, wpx).astype(int)
        ys = spec[idx]
        pts = [(x0 + 16 + k, y1 - 8 - v * (y1 - y0 - 24)) for k, v in enumerate(ys)]
        d.line(pts, fill=220, width=1)
        for k in range(0, wpx, 5):     # dither de preenchimento
            top = pts[k][1]
            for yy in np.arange(top + 5, y1 - 8, 7):
                d.point((x0 + 16 + k, yy), fill=60)
        j = max(0, i - int(self.f['fps']))
        db = self.f['bands'][i] - self.f['bands'][j]
        for k in range(3):
            d.text((x1 - 66, y0 + 14 + k * 24), f"Δ {abs(db[k])*99:04.1f}",
                   font=self.f13, fill=210)
        d.text((x1 - 66, y0 + 14 + 3 * 24), "...", font=self.f13, fill=130)

    def _signallog(self, d, i, t):
        for (name, t0, t1) in self.s['timeline']:
            if t >= t0 and name not in self.seen_secs:
                self.seen_secs.add(name)
                self.siglog.append((t0, name))
        x0, _, x1 = self.colR
        y = 604
        for (t0, name) in self.siglog[-4:]:
            mm, ss = divmod(int(t0), 60)
            d.text((x0 + 16, y), f"{mm:02d}:{ss:02d}:{int((t0%1)*24):02d}",
                   font=self.f12, fill=170)
            sp = self.sec_spark[name]
            bx = x0 + 110
            pts = [(bx + k * 2, y + 8 - v * 12) for k, v in enumerate(sp)]
            d.line(pts, fill=190, width=1)
            d.text((x1 - 60, y), name[:6].upper(), font=self.f10, fill=140)
            y += 28

    def _layers(self, d, i):
        x0, _, x1 = self.colR
        f = self.f
        rows = [(k, tid) for k, tid in enumerate(self.s['order'])
                if f['layer_counts'][k] >= 4]
        nsfx = len(self.s['order']) - len(rows)
        n = len(rows)
        step = min(13, max(11, 140 // max(n, 1)))
        for j, (k, tid) in enumerate(rows):
            y = 811 + j * step
            if y > 934:
                break
            act = f['layers'][k, i]
            nm = self.s['tracks'][tid].upper()[:14]
            on = act > 0.04
            d.text((x0 + 14, y), nm, font=self.f10, fill=225 if on else 95)
            bx = x0 + 140
            bw = 90
            d.rectangle([bx, y + 2, bx + bw, y + 8], outline=70)
            if on:
                d.rectangle([bx, y + 2, bx + int(bw * min(act, 1)), y + 8], fill=210)
            ln = f['layer_last'][k, i]
            lbl = note_name(int(ln)) if on and ln >= 0 else "--"
            d.text((bx + bw + 10, y), lbl, font=self.f10, fill=200 if on else 70)
            st = "ACT" if on else "IDL"
            d.text((x1 - 40, y), st, font=self.f10, fill=210 if on else 80)
        if nsfx:
            d.text((x0 + 14, min(811 + n * step + 2, 946)),
                   f"... +{nsfx} SFX TRANSIENTS", font=self.f10, fill=110)

    def _fieldpanel(self, d, i, t):
        x0, y0, x1, y1 = self.field
        env = self.f['env']
        total = self.s['total']
        gx0, gx1 = x0 + 16, x1 - 16
        gy0, gy1 = y0 + 44, y1 - 14
        wpx = gx1 - gx0
        idx = np.linspace(0, len(env) - 1, wpx).astype(int)
        ys = env[idx]
        pts = [(gx0 + k, gy1 - v * (gy1 - gy0)) for k, v in enumerate(ys)]
        d.line(pts, fill=170, width=1)
        for (name, t0, t1) in self.s['timeline']:
            bx = gx0 + t0 / total * wpx
            d.line([bx, gy0 - 4, bx, gy1], fill=75)
            d.text((bx + 3, gy0 - 6), name[:5].upper(), font=self.f10, fill=110)
        phx = gx0 + min(t / total, 1) * wpx
        d.line([phx, gy0 - 8, phx, gy1], fill=255, width=1)
        e = env[min(int(t / total * len(env)), len(env) - 1)]
        d.ellipse([phx - 3, gy1 - e * (gy1 - gy0) - 3, phx + 3, gy1 - e * (gy1 - gy0) + 3],
                  fill=255)
        d.text((x1 - 116, y0 + 10), f"Δ {self.f['rms'][i]:0.4f}", font=self.f13, fill=200)

    def _header_dyn(self, d, i, t):
        fx0, fy0, fx1, _ = self.frame
        mm, ss = divmod(int(t), 60)
        fr = int((t % 1) * self.f['fps'])
        tc = f"00:{mm:02d}:{ss:02d}:{fr:02d}"
        d.text((fx1 - 210, fy0 + 14), tc, font=self.f15, fill=235)
        d.rectangle([fx1 - 78, fy0 + 12, fx1 - 16, fy0 + 34], outline=200)
        d.text((fx1 - 52, fy0 + 15), "REC", font=self.f13, fill=245)
        if int(t * 2) % 2 == 0:
            d.ellipse([fx1 - 70, fy0 + 19, fx1 - 62, fy0 + 27], fill=255)

    # ---------- frame ----------

    def render(self, i):
        f = self.f
        t = i / f['fps']
        dt = 1.0 / f['fps']
        dimg = Image.new('L', (W, H), 0)
        d = ImageDraw.Draw(dimg)

        secname, _ = self._profile(d, i, t)
        self._coords(d, i, t, secname)
        self._waveform(d, i)
        self._freqmap(d, i)
        self._signallog(d, i, t)
        self._layers(d, i)
        self._fieldpanel(d, i, t)
        self._header_dyn(d, i, t)
        self._left_margin(d, i, t)
        self._entity_extras(d, i, t)
        self._rings(d, i, t)

        dyn = np.array(dimg, np.float32)
        self._spawn(i)
        self._particles(dyn, i, dt)
        self._thumb(dyn, i)

        # bloom CRT: halo curto (fosforo) + halo largo (vidro)
        dimg8 = Image.fromarray(np.clip(dyn, 0, 255).astype(np.uint8), 'L')
        blur1 = np.array(dimg8.filter(ImageFilter.GaussianBlur(2.2)), np.float32)
        blur2 = np.array(
            dimg8.resize((W // 4, H // 4), Image.BILINEAR)
            .filter(ImageFilter.GaussianBlur(4.0))
            .resize((W, H), Image.BILINEAR), np.float32)
        sharp = self.static + dyn
        glow = blur1 * 0.5 + blur2 * 0.5
        post = self.vign * self.scan
        noise = self.rng.standard_normal((H, W)).astype(np.float32) * 1.9 + 3

        # aberracao cromatica so no halo (texto continua branco e nitido)
        r_ch = np.clip((sharp + np.roll(glow, -2, axis=1)) * post + noise, 0, 255)
        g_ch = np.clip((sharp + glow) * post + noise, 0, 255)
        b_ch = np.clip((sharp + np.roll(glow, 2, axis=1)) * post + noise, 0, 255)
        out = np.empty((H, W, 3), np.uint8)
        out[..., 0] = (r_ch * 0.94).astype(np.uint8)
        out[..., 1] = (g_ch * 0.97).astype(np.uint8)
        out[..., 2] = np.clip(b_ch * 1.01, 0, 255).astype(np.uint8)
        return out
