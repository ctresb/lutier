# CLI: python3 -m lumiere audio.wav score.score -o out.mp4
import argparse
import os
import subprocess
import sys
import time

from PIL import Image

from .analysis import analyze
from .scene import Scene, W, H
from .scoreparse import parse_score


def main():
    ap = argparse.ArgumentParser(prog='lumiere',
                                 description='terminal de analise visual do lutier')
    ap.add_argument('audio', help='wav renderizado pelo lutier')
    ap.add_argument('score', help='.score correspondente (camadas/secoes)')
    ap.add_argument('-o', '--out', default='out/lumiere.mp4')
    ap.add_argument('--fps', type=int, default=24)
    ap.add_argument('--seed', type=int, default=7)
    ap.add_argument('--title', default=None)
    ap.add_argument('--preview', default=None,
                    help='ex: "10,50,90" - salva PNGs nesses segundos e sai')
    ap.add_argument('--crf', type=int, default=17)
    args = ap.parse_args()

    title = args.title or os.path.splitext(os.path.basename(args.score))[0]
    score = parse_score(args.score)
    print(f"[lumiere] score: {len(score['order'])} camadas, "
          f"{len(score['timeline'])} secoes, {score['total']:.1f}s @ {score['tempo']:.0f}bpm")
    t0 = time.time()
    feats = analyze(args.audio, args.fps, score)
    print(f"[lumiere] analise: {feats['nf']} frames em {time.time()-t0:.1f}s")

    scene = Scene(feats, score, title, seed=args.seed)

    if args.preview:
        for ts in args.preview.split(','):
            i = min(int(float(ts) * args.fps), feats['nf'] - 1)
            # avanca estado (particulas/log) ate o frame alvo, esparso
            for j in range(max(0, i - 48), i):
                scene.render(j)
            frame = scene.render(i)
            p = args.out.replace('.mp4', f'_preview_{ts.strip()}s.png')
            Image.fromarray(frame).save(p)
            print(f"[lumiere] preview: {p}")
        return

    os.makedirs(os.path.dirname(args.out) or '.', exist_ok=True)
    cmd = [
        'ffmpeg', '-y', '-loglevel', 'error',
        '-f', 'rawvideo', '-pix_fmt', 'rgb24', '-s', f'{W}x{H}',
        '-r', str(args.fps), '-i', '-',
        '-i', args.audio,
        '-c:v', 'libx264', '-preset', 'medium', '-crf', str(args.crf),
        '-pix_fmt', 'yuv420p', '-movflags', '+faststart',
        '-c:a', 'aac', '-b:a', '192k', '-shortest',
        args.out,
    ]
    proc = subprocess.Popen(cmd, stdin=subprocess.PIPE)
    t0 = time.time()
    nf = feats['nf']
    for i in range(nf):
        proc.stdin.write(scene.render(i).tobytes())
        if i % 240 == 0:
            el = time.time() - t0
            eta = el / max(i, 1) * (nf - i)
            print(f"[lumiere] frame {i}/{nf}  ({el:.0f}s, eta {eta:.0f}s)")
    proc.stdin.close()
    proc.wait()
    if proc.returncode != 0:
        sys.exit(f"[lumiere] ffmpeg falhou ({proc.returncode})")
    print(f"[lumiere] ok: {args.out}  ({time.time()-t0:.0f}s)")


if __name__ == '__main__':
    main()
