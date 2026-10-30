"""Fabrique un jeu de donnees de demonstration, entierement invente.

    python demo/generer.py            ecrit demo/donnees/
    VIGIE_DONNEES=demo/donnees VIGIE_MAINTENANT=<epoch> cargo run --release

Aucune donnee reelle : un projet fictif « orbite », trois chantiers, des
images techniques dessinees ici meme.
"""
import base64, io, json, math, os, random, shutil
from datetime import datetime, timedelta, timezone
import numpy as np
from PIL import Image, ImageDraw, ImageFilter

RACINE = os.path.join(os.path.dirname(os.path.abspath(__file__)), "donnees")
SESSION = "5e1d0c2a-7a41-4c6e-9b0d-0d3c4f2a9e11"
PROJET = "C--demo-orbite"
CWD = "C:\\demo\\orbite"
FIN = datetime(2026, 10, 30, 14, 30, 0, tzinfo=timezone.utc)
random.seed(7)
np.random.seed(7)
FOND, TRAIT, OR = (12, 14, 18), (120, 200, 255), (236, 190, 90)


def png64(im):
    b = io.BytesIO()
    im.save(b, "PNG")
    return base64.b64encode(b.getvalue()).decode()


def terrain():
    n = 220
    x = np.linspace(-3, 3, n)
    X, Y = np.meshgrid(x, x)
    Z = np.sin(X * 1.3) * np.cos(Y * 1.1) + 0.5 * np.sin(2.7 * X + Y) + 0.25 * np.cos(4 * Y - X)
    Z += 1.4 * np.exp(-((X - 0.8) ** 2 + (Y + 0.4) ** 2) * 1.2)
    W, H = 1280, 800
    im = Image.new("RGB", (W, H), FOND)
    d = ImageDraw.Draw(im)

    def p(i, j):
        u, v, z = X[i, j], Y[i, j], Z[i, j]
        return (W / 2 + (u - v) * 150, 560 + (u + v) * 62 - z * 105)

    for j in range(0, n, 6):
        d.line([p(i, j) for i in range(n)], fill=(40, 90, 150), width=1)
    for i in range(0, n, 6):
        t = int(60 + 150 * (i / n))
        d.line([p(i, j) for j in range(n)], fill=(t // 2, t, 255 - t // 3), width=2)
    d.text((24, 20), "heightmap  seed=0007  grid=220x220", fill=(170, 190, 210))
    return im


def reseau():
    W, H = 1280, 800
    im = Image.new("RGB", (W, H), FOND)
    d = ImageDraw.Draw(im)
    pts = [(W / 2 + 330 * math.cos(a) * (0.55 + 0.45 * random.random()),
            H / 2 + 280 * math.sin(a) * (0.55 + 0.45 * random.random()))
           for a in np.linspace(0, 2 * math.pi, 26, endpoint=False)]
    pts += [(W / 2 + random.gauss(0, 60), H / 2 + random.gauss(0, 50)) for _ in range(6)]
    for i, a in enumerate(pts):
        for j in range(i + 1, len(pts)):
            b = pts[j]
            dist = math.dist(a, b)
            if dist < 230:
                d.line([a, b], fill=(40 + int(dist / 3), 70 + int(dist / 2), 130), width=1)
    for i, (x, y) in enumerate(pts):
        r = 9 if i < 26 else 14
        c = OR if i >= 26 else TRAIT
        d.ellipse([x - r, y - r, x + r, y + r], fill=c)
        d.text((x + r + 3, y - 6), "n%02d" % i, fill=(150, 165, 185))
    d.text((24, 20), "topology  nodes=32", fill=(170, 190, 210))
    return im


def latence():
    W, H = 1280, 800
    im = Image.new("RGB", (W, H), FOND)
    d = ImageDraw.Draw(im)
    for k in range(6):
        y = 120 + k * 110
        d.line([(90, y), (W - 50, y)], fill=(34, 40, 52))
        d.text((30, y - 6), "%d ms" % (250 - k * 50), fill=(130, 140, 155))
    for s, col in enumerate([TRAIT, OR, (150, 230, 170)]):
        v = 0.0
        pts = []
        for i in range(120):
            v += random.gauss(0, 0.5)
            base = 60 + s * 55 + 22 * math.sin(i / 9 + s) + v
            pts.append((90 + i * 9.7, 700 - base * 2.1))
        d.line(pts, fill=col, width=3)
    d.text((90, 40), "p50 / p95 / p99  window=120s", fill=(170, 190, 210))
    return im


def chaleur():
    W, H = 1280, 800
    y, x = np.mgrid[0:H, 0:W]
    f = np.zeros((H, W))
    for cx, cy, s, a in [(380, 300, 160, 1.0), (860, 480, 210, 0.8), (640, 640, 120, 0.6), (1000, 220, 100, 0.7)]:
        f += a * np.exp(-((x - cx) ** 2 + (y - cy) ** 2) / (2 * s * s))
    f = np.clip(f, 0, 1)
    img = np.stack([14 + 235 * f ** 1.3, 18 + 170 * f ** 2, 30 + 90 * (1 - f) * f * 4], -1)
    im = Image.fromarray(np.clip(img, 0, 255).astype("uint8"))
    d = ImageDraw.Draw(im)
    for k in range(1, 8):
        d.line([(k * 160, 0), (k * 160, H)], fill=(255, 255, 255, 20), width=1)
    for k in range(1, 5):
        d.line([(0, k * 160), (W, k * 160)], fill=(255, 255, 255, 20), width=1)
    d.text((24, 20), "occupation  cellules=8x5", fill=(230, 235, 245))
    return im


def spectre():
    W, H = 1280, 800
    im = Image.new("RGB", (W, H), FOND)
    d = ImageDraw.Draw(im)
    for k in range(5):
        y = 100 + k * 130
        d.line([(80, y), (W - 40, y)], fill=(30, 36, 48))
    for i in range(96):
        h = 40 + 560 * (math.exp(-((i - 22) / 9) ** 2) + 0.55 * math.exp(-((i - 58) / 12) ** 2) + 0.12 * random.random())
        c = (int(70 + 160 * h / 600), int(140 + 90 * h / 600), 255 - int(60 * h / 600))
        d.rectangle([84 + i * 12, 700 - h, 84 + i * 12 + 8, 700], fill=c)
    d.text((84, 40), "spectre  fenetre=hann  n=96", fill=(170, 190, 210))
    return im


def ruban():
    W, H = 1280, 800
    y, x = np.mgrid[0:H, 0:W]
    u = (x - W / 2) / 400
    v = (y - H / 2) / 400
    r = np.sqrt(u * u + v * v)
    a = np.arctan2(v, u)
    f = 0.5 + 0.5 * np.sin(9 * r - 3 * a + 2 * np.sin(5 * r))
    img = np.stack([20 + 60 * f, 40 + 140 * f * np.exp(-r / 2.6), 70 + 185 * f * np.exp(-r / 3.2)], -1)
    return Image.fromarray(np.clip(img, 0, 255).astype("uint8")).filter(ImageFilter.GaussianBlur(0.6))


def orbites():
    W, H = 1280, 800
    im = Image.new("RGB", (W, H), FOND)
    d = ImageDraw.Draw(im)
    for k in range(1, 8):
        r = 60 + k * 48
        d.ellipse([W / 2 - r, H / 2 - r * 0.62, W / 2 + r, H / 2 + r * 0.62], outline=(36, 52, 80))
        a = k * 1.1
        px, py = W / 2 + r * math.cos(a), H / 2 + r * 0.62 * math.sin(a)
        d.ellipse([px - 8, py - 8, px + 8, py + 8], fill=OR if k % 3 == 0 else TRAIT)
    d.ellipse([W / 2 - 22, H / 2 - 22, W / 2 + 22, H / 2 + 22], fill=(255, 220, 140))
    d.text((24, 20), "orbites  corps=7  pas=0.5 s", fill=(170, 190, 210))
    return im


DATES = []
IMAGES = {"terrain": terrain(), "reseau": reseau(), "latence": latence(), "chaleur": chaleur(),
          "spectre": spectre(), "ruban": ruban(), "orbites": orbites()}


def natif(texte):
    """Hors de Windows, les chemins de la demonstration sont ceux d'un poste Unix."""
    if os.name == "nt" and not os.environ.get("DEMO_UNIX"):
        return texte
    for nom in ("orbite", "balise"):
        # Une barre inverse s'ecrit doublee dans le JSON : deux, donc quatre ici.
        texte = texte.replace("C:\\\\demo\\\\" + nom, "/home/demo/" + nom).replace("C:/demo/" + nom, "/home/demo/" + nom)
    return texte


def stamp(t):
    return t.strftime("%Y-%m-%dT%H:%M:%S.") + "%03dZ" % (t.microsecond // 1000)


class Fil:
    def __init__(self, ident, debut):
        self.id, self.t, self.lignes, self.parent, self.n = ident, debut, [], None, 0

    def _l(self, type_, message, dt=7):
        self.t += timedelta(seconds=dt)
        self.n += 1
        uid = "%s-%04d" % (self.id, self.n)
        self.lignes.append({"parentUuid": self.parent, "isSidechain": True, "agentId": self.id,
                            "type": type_, "message": message, "uuid": uid, "timestamp": stamp(self.t),
                            "userType": "external", "cwd": CWD, "sessionId": SESSION, "version": "2.1.308"})
        self.parent = uid

    def consigne(self, texte):
        self._l("user", {"role": "user", "content": texte}, 0)

    def dit(self, texte):
        self._l("assistant", {"role": "assistant", "content": [{"type": "text", "text": texte}]})

    def geste(self, nom, entree, resultat, image=None):
        tid = "toolu_%s%03d" % (self.id[:6], self.n)
        self._l("assistant", {"role": "assistant", "content": [{"type": "tool_use", "id": tid, "name": nom, "input": entree}]}, 5)
        if image:
            corps = [{"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": png64(IMAGES[image])}}]
        else:
            corps = resultat
        self._l("user", {"role": "user", "content": [{"type": "tool_result", "tool_use_id": tid, "content": corps}]}, 3)


def ecrire(wf, label, phase, fil):
    d = os.path.join(RACINE, "projects", PROJET, SESSION, "subagents", "workflows", wf)
    os.makedirs(d, exist_ok=True)
    with open(os.path.join(d, "agent-%s.jsonl" % fil.id), "w", encoding="utf-8", newline="\n") as f:
        f.write(natif("\n".join(json.dumps(l, ensure_ascii=False) for l in fil.lignes) + "\n"))
    with open(os.path.join(d, "agent-%s.meta.json" % fil.id), "w") as f:
        json.dump({"agentType": "workflow-subagent", "description": label, "workflowPhase": phase, "spawnDepth": 1}, f)
    with open(os.path.join(d, "journal.jsonl"), "a") as f:
        f.write(json.dumps({"type": "started", "key": "v2:" + fil.id, "agentId": fil.id, "label": label, "phase": phase}) + "\n")
    DATES.append((d, fil.lignes[0]["timestamp"], fil.lignes[-1]["timestamp"]))


def script(wf, nom, desc):
    d = os.path.join(RACINE, "projects", PROJET, SESSION, "workflows", "scripts")
    os.makedirs(d, exist_ok=True)
    with open(os.path.join(d, "%s-%s.js" % (nom, wf)), "w", encoding="utf-8") as f:
        f.write("export const meta = {\n  name: '%s',\n  description: '%s',\n}\n" % (nom, desc))


shutil.rmtree(RACINE, ignore_errors=True)

script("wf_a1b2c3d4-e5f", "terrain-render", "Render the planet terrain and measure the cost of each pass")
f = Fil("a1c0de00000000001", FIN - timedelta(minutes=9))
f.consigne("Render the xxxxx terrain of map orbit-04 in isometric perspective, then check that the relief stays readable at xxxxx pixels.")
f.dit("I start by generating the height map, then I compare two lightings before choosing.")
f.geste("Bash", {"command": "cd C:/demo/orbite && python tools/terrain.py --seed 7 --grid 220 --output renders/terrain.png"}, "grid 220x220, 48400 vertices, 38 ms\nrenders/terrain.png")
f.geste("Read", {"file_path": "C:/demo/orbite/renders/terrain.png"}, "", image="terrain")
f.dit("The relief reads well, but the central ridge saturates: the height goes above xxxxx. I will compress the curve rather than shrink the scale.")
f.geste("Edit", {"file_path": "C:/demo/orbite/tools/terrain.py", "old_string": "z = h", "new_string": "z = tanh(h / 1.6) * 1.6"}, "The file has been updated.")
f.geste("Bash", {"command": "cd C:/demo/orbite && python tools/terrain.py --seed 7 --grid 220 --output renders/terrain-2.png"}, "grid 220x220, 48400 vertices, 41 ms\nrenders/terrain-2.png")
f.dit("Measured: 41 ms against 38 ms, the compression costs 3 ms. I accept it, the relief is no longer flattened.")
ecrire("wf_a1b2c3d4-e5f", "render:isometric", "Render", f)

script("wf_b2c3d4e5-f6a", "network-audit", "Audit the mesh topology and find the nodes that isolate it")
g = Fil("b2c0de00000000002", FIN - timedelta(minutes=14))
g.consigne("Audit the xxxxx mesh topology: find the nodes that isolate the graph and say which one to cut last.")
g.dit("Starting hypothesis: the bridge nodes are the ones whose removal disconnects the most neighbours.")
g.geste("Bash", {"command": "cd C:/demo/orbite && python tools/topology.py --nodes 32 --output renders/network.png"}, "32 nodes, 118 links, 3 articulation points\nrenders/network.png")
g.geste("Read", {"file_path": "C:/demo/orbite/renders/network.png"}, "", image="reseau")
g.dit("Three articulation points, two of which are the golden nodes in the middle. The third, n17, hangs by a single link: that is the one to double first.")
g.geste("Grep", {"pattern": "n17", "path": "C:/demo/orbite/config"}, "config/links.toml:41: n17 -> n03 (xxxxx)")
g.dit("Answer: double n17 -> n03 before any cut. The rest of the graph survives the loss of any other node.")
ecrire("wf_b2c3d4e5-f6a", "audit:topology", "Audit", g)

script("wf_c3d4e5f6-a7b", "latency-measure", "Measure latency under load and compare two queues")
h = Fil("c3c0de00000000003", FIN - timedelta(seconds=40))
h.consigne("Measure p50 / p95 / p99 latency under xxxxx load for the two queues and say which one holds for 120 seconds.")
h.dit("I run both queues on the same load, in this order and then in the reverse order, so that I do not confuse the queue with the machine warming up. The model I compare against is described at https://example.org/queueing-notes.")
h.geste("Bash", {"command": "cd C:/demo/orbite && python tools/load.py --queues fifo,prio --duration 120 --output renders/latency.png"}, "fifo: p50 74 ms  p95 181 ms  p99 243 ms\nprio: p50 71 ms  p95 142 ms  p99 198 ms\nrenders/latency.png")
h.geste("Read", {"file_path": "C:/demo/orbite/renders/latency.png"}, "", image="latence")
h.dit("The priority queue wins on p95 (142 against 181 ms) but the green curve, the slowest one, shows a drift I cannot explain yet. I run it again in the reverse order.")
h.geste("Bash", {"command": "cd C:/demo/orbite && python tools/load.py --queues prio,fifo --duration 120 --output renders/latency-reverse.png"}, "prio: p50 72 ms  p95 144 ms  p99 201 ms\nfifo: p50 75 ms  p95 183 ms  p99 241 ms")
ecrire("wf_c3d4e5f6-a7b", "measure:latency", "Measure", h)

def agent(ident, depart, consigne, pas, wf, label, phase):
    """Un agent de plus : pas = [(texte,) | (outil, entree, resultat[, image])]."""
    a = Fil(ident, depart)
    a.consigne(consigne)
    for p in pas:
        if len(p) == 1:
            a.dit(p[0])
        else:
            a.geste(*p)
    ecrire(wf, label, phase, a)


B = "cd C:/demo/orbite && python tools/"
# Le rendu : deux agents de plus, en parallele du premier.
agent("a1c0de00000000002", FIN - timedelta(minutes=9, seconds=-20), "Check that the relief stays readable when the map is reduced to xxxxx pixels.",
      [("I reduce the map to the three scales of the interface and compare the contrast of the ridges.",),
       ("Bash", {"command": B + "scales.py --maps renders/terrain.png --reduce 1,0.5,0.25"}, "1.00  contrast 0.82\n0.50  contrast 0.74\n0.25  contrast 0.51"),
       ("Bash", {"command": B + "heatmap.py --input renders/terrain.png --output renders/heat.png"}, "cells 8x5, max 0.93\nrenders/heat.png"),
       ("Read", {"file_path": "C:/demo/orbite/renders/heat.png"}, "", "chaleur"),
       ("At a quarter size the contrast drops to 0.51: the secondary ridges disappear. The three hot spots stay, and that is what matters for reading.",)],
      "wf_a1b2c3d4-e5f", "render:scales", "Render")
agent("a1c0de00000000003", FIN - timedelta(minutes=8), "Add soft shading under the relief without going over xxxxx ms per frame.",
      [("Soft shading needs a second blur pass. I first measure what the first one costs.",),
       ("Bash", {"command": B + "shadow.py --radius 6 --output renders/shadow.png"}, "pass 1: 9 ms\npass 2: 11 ms\nrenders/shadow.png"),
       ("Read", {"file_path": "C:/demo/orbite/renders/shadow.png"}, "", "ruban"),
       ("Edit", {"file_path": "C:/demo/orbite/tools/shadow.py", "old_string": "radius = 6", "new_string": "radius = 4"}, "The file has been updated."),
       ("Bash", {"command": B + "shadow.py --radius 4 --output renders/shadow-2.png"}, "pass 1: 6 ms\npass 2: 7 ms\nrenders/shadow-2.png"),
       ("A radius of 4 instead of 6: 13 ms in total, under the 16 ms budget, and the shadow is still visible.",)],
      "wf_a1b2c3d4-e5f", "render:shading", "Render")
# L'audit : deux agents de plus.
agent("b2c0de00000000003", FIN - timedelta(minutes=13), "Look for the redundant links of the xxxxx mesh, the ones that can be removed without isolating anyone.",
      [("A link is redundant if its two ends stay reachable another way. I test it link by link.",),
       ("Bash", {"command": B + "redundancy.py --links config/links.toml"}, "118 links, 21 redundant, 3 critical\noutput: renders/redundancy.txt"),
       ("Grep", {"pattern": "critical", "path": "C:/demo/orbite/renders/redundancy.txt"}, "n17-n03 critical\nn09-n22 critical\nn30-n31 critical"),
       ("Three critical links, and n17-n03 is the one the main audit already pointed to: the two methods agree.",)],
      "wf_b2c3d4e5-f6a", "audit:redundancy", "Audit")
agent("b2c0de00000000004", FIN - timedelta(minutes=12), "Simulate the loss of each node and say how many neighbours stay reachable.",
      [("I remove the nodes one by one and count the largest remaining component.",),
       ("Bash", {"command": B + "losses.py --nodes 32 --output renders/losses.png"}, "worst case: n17, 4 nodes isolated\nmedian: 0 nodes isolated\nrenders/losses.png"),
       ("Read", {"file_path": "C:/demo/orbite/renders/losses.png"}, "", "spectre"),
       ("The median is zero: the mesh holds almost everywhere. Only n17 breaks something, and it is four nodes.",)],
      "wf_b2c3d4e5-f6a", "audit:losses", "Audit")
# La mesure en cours : un agent de plus.
agent("c3c0de00000000004", FIN - timedelta(seconds=55), "Check that the heating of the machine does not distort the measurement of the priority queue.",
      [("I note the processor frequency during each pass to see whether it drifts.",),
       ("Bash", {"command": B + "frequency.py --duration 120 --output renders/orbits.png"}, "start 4.1 GHz, end 3.9 GHz\ndrift: -4.8 %\nrenders/orbits.png"),
       ("Read", {"file_path": "C:/demo/orbite/renders/orbits.png"}, "", "orbites"),
       ("A drift of less than 5 %: it does not explain the 39 ms gap on the p95.",)],
      "wf_c3d4e5f6-a7b", "measure:frequency", "Measure")

# Un second projet, termine : l'etalonnage d'une balise.
SESSION = "8c3a91f4-2b6d-4d57-a0e2-6f1b7c4d2a08"
PROJET = "C--demo-balise"
CWD = "C:\\demo\\balise"
script("wf_d4e5f6a7-b8c", "beacon-calibration", "Calibrate the beacon and check its drift over a night")
k = Fil("d4c0de00000000001", FIN - timedelta(hours=2, minutes=10))
k.consigne("Calibrate the xxxxx beacon against the reference source, then check its drift over xxxxx hours.")
k.dit("I start with the three-point calibration, then I let the drift measurement run.")
k.geste("Bash", {"command": "cd C:/demo/balise && python tools/calibrate.py --points 3 --output renders/spectrum.png"}, "point 1: -0.4 dB\npoint 2: +0.1 dB\npoint 3: +0.3 dB\nrenders/spectrum.png")
k.geste("Read", {"file_path": "C:/demo/balise/renders/spectrum.png"}, "", image="spectre")
k.dit("Two clear peaks, one at 22 and the other at 58. The second is wider: it is the harmonic, not a defect.")
k.geste("Bash", {"command": "cd C:/demo/balise && python tools/drift.py --hours 8 --output renders/drift.png"}, "maximum drift: 0.07 dB over 8 h\nrenders/drift.png")
k.geste("Read", {"file_path": "C:/demo/balise/renders/drift.png"}, "", image="latence")
k.dit("Maximum drift of 0.07 dB over eight hours, well under the 0.5 dB tolerance. The beacon is calibrated.")
ecrire("wf_d4e5f6a7-b8c", "calibration:beacon", "Calibration", k)

os.makedirs(os.path.join(RACINE, "vigie"), exist_ok=True)
print("ecrit", RACINE, "VIGIE_MAINTENANT =", int(FIN.timestamp()) + 20)

# Les etats (en cours, interrompu, termine) se lisent sur les dates des fichiers :
# on les date comme la transcription, creation au premier geste, ecriture au dernier.
import subprocess
for d, debut, fin in DATES:
    ecrit = datetime.strptime(fin, '%Y-%m-%dT%H:%M:%S.%fZ').replace(tzinfo=timezone.utc).timestamp()
    for chemin in [d] + [os.path.join(d, n) for n in os.listdir(d)]:
        if os.name != 'nt':
            # Ailleurs que sous Windows, la date de creation ne se pose pas : celle d'ecriture suffit a dire en cours ou termine.
            os.utime(chemin, (ecrit, ecrit))
            continue
        ps = ("$i=Get-Item -LiteralPath '%s'; $i.CreationTimeUtc=[datetime]'%s'; $i.LastWriteTimeUtc=[datetime]'%s'"
              % (chemin, debut.replace("Z", ""), fin.replace("Z", "")))
        subprocess.run(["powershell", "-NoProfile", "-Command", ps], check=True)
