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
        f.write("\n".join(json.dumps(l, ensure_ascii=False) for l in fil.lignes) + "\n")
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

script("wf_a1b2c3d4-e5f", "rendu-terrain", "Rendre le terrain de la planete et mesurer le cout de chaque passe")
f = Fil("a1c0de00000000001", FIN - timedelta(minutes=9))
f.consigne("Rendre le terrain xxxxx de la carte orbite-04 en perspective isometrique, puis verifier que le relief reste lisible a xxxxx pixels.")
f.dit("Je commence par generer la carte de hauteur, puis je compare deux eclairages avant de choisir.")
f.geste("Bash", {"command": "cd C:/demo/orbite && python outils/terrain.py --seed 7 --grille 220 --sortie rendus/terrain.png"}, "grille 220x220, 48400 sommets, 38 ms\nrendus/terrain.png")
f.geste("Read", {"file_path": "C:/demo/orbite/rendus/terrain.png"}, "", image="terrain")
f.dit("Le relief se lit bien, mais la crete au centre sature : la hauteur depasse xxxxx. Je compresse la courbe plutot que de reduire l'echelle.")
f.geste("Edit", {"file_path": "C:/demo/orbite/outils/terrain.py", "old_string": "z = h", "new_string": "z = tanh(h / 1.6) * 1.6"}, "Le fichier a ete modifie.")
f.geste("Bash", {"command": "cd C:/demo/orbite && python outils/terrain.py --seed 7 --grille 220 --sortie rendus/terrain-2.png"}, "grille 220x220, 48400 sommets, 41 ms\nrendus/terrain-2.png")
f.dit("Mesure : 41 ms contre 38 ms, la compression coute 3 ms. J'accepte, le relief n'est plus ecrase.")
ecrire("wf_a1b2c3d4-e5f", "rendu:isometrique", "Rendu", f)

script("wf_b2c3d4e5-f6a", "audit-reseau", "Auditer la topologie du maillage et trouver les noeuds isoles")
g = Fil("b2c0de00000000002", FIN - timedelta(minutes=14))
g.consigne("Auditer la topologie du maillage xxxxx : trouver les noeuds qui isolent le graphe et dire lequel couper en dernier.")
g.dit("Hypothese de depart : les noeuds de pont sont ceux dont le retrait deconnecte le plus de voisins.")
g.geste("Bash", {"command": "cd C:/demo/orbite && python outils/topologie.py --noeuds 32 --sortie rendus/reseau.png"}, "32 noeuds, 118 liens, 3 points d'articulation\nrendus/reseau.png")
g.geste("Read", {"file_path": "C:/demo/orbite/rendus/reseau.png"}, "", image="reseau")
g.dit("Trois points d'articulation, dont deux sont les noeuds dores au centre. Le troisieme, n17, ne tient que par un seul lien : c'est lui qu'il faut doubler en premier.")
g.geste("Grep", {"pattern": "n17", "path": "C:/demo/orbite/config"}, "config/liens.toml:41: n17 -> n03 (xxxxx)")
g.dit("Reponse : doubler n17 -> n03 avant toute coupure. Le reste du graphe survit a la perte de n'importe quel autre noeud.")
ecrire("wf_b2c3d4e5-f6a", "audit:topologie", "Audit", g)

script("wf_c3d4e5f6-a7b", "mesure-latence", "Mesurer la latence sous charge et comparer deux files d'attente")
h = Fil("c3c0de00000000003", FIN - timedelta(seconds=40))
h.consigne("Mesurer la latence p50 / p95 / p99 sous charge xxxxx pour les deux files d'attente et dire laquelle tient 120 secondes.")
h.dit("Je lance les deux files sur la meme charge, dans cet ordre puis dans l'ordre inverse, pour ne pas confondre la file avec le chauffage de la machine.")
h.geste("Bash", {"command": "cd C:/demo/orbite && python outils/charge.py --files fifo,prio --duree 120 --sortie rendus/latence.png"}, "fifo : p50 74 ms  p95 181 ms  p99 243 ms\nprio : p50 71 ms  p95 142 ms  p99 198 ms\nrendus/latence.png")
h.geste("Read", {"file_path": "C:/demo/orbite/rendus/latence.png"}, "", image="latence")
h.dit("La file prioritaire gagne sur le p95 (142 contre 181 ms) mais la courbe verte, la plus lente, montre une derive que je ne m'explique pas encore. Je relance dans l'ordre inverse.")
h.geste("Bash", {"command": "cd C:/demo/orbite && python outils/charge.py --files prio,fifo --duree 120 --sortie rendus/latence-inverse.png"}, "prio : p50 72 ms  p95 144 ms  p99 201 ms\nfifo : p50 75 ms  p95 183 ms  p99 241 ms")
ecrire("wf_c3d4e5f6-a7b", "mesure:latence", "Mesure", h)

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


B = "cd C:/demo/orbite && python outils/"
# Le rendu : deux agents de plus, en parallele du premier.
agent("a1c0de00000000002", FIN - timedelta(minutes=9, seconds=-20), "Verifier que le relief reste lisible quand la carte est reduite a xxxxx pixels.",
      [("Je reduis la carte aux trois echelles de l'interface et je compare le contraste des cretes.",),
       ("Bash", {"command": B + "echelles.py --cartes rendus/terrain.png --reduire 1,0.5,0.25"}, "1.00  contraste 0.82\n0.50  contraste 0.74\n0.25  contraste 0.51"),
       ("Bash", {"command": B + "carte_chaleur.py --entree rendus/terrain.png --sortie rendus/chaleur.png"}, "cellules 8x5, max 0.93\nrendus/chaleur.png"),
       ("Read", {"file_path": "C:/demo/orbite/rendus/chaleur.png"}, "", "chaleur"),
       ("A un quart de taille le contraste tombe a 0.51 : les cretes secondaires disparaissent. Les trois foyers restent, c'est ce qui compte pour la lecture.",)],
      "wf_a1b2c3d4-e5f", "rendu:echelles", "Rendu")
agent("a1c0de00000000003", FIN - timedelta(minutes=8), "Ajouter l'ombrage doux sous le relief sans depasser xxxxx ms par image.",
      [("L'ombrage doux demande une seconde passe de flou. Je mesure d'abord ce que coute la premiere.",),
       ("Bash", {"command": B + "ombre.py --rayon 6 --sortie rendus/ombre.png"}, "passe 1 : 9 ms\npasse 2 : 11 ms\nrendus/ombre.png"),
       ("Read", {"file_path": "C:/demo/orbite/rendus/ombre.png"}, "", "ruban"),
       ("Edit", {"file_path": "C:/demo/orbite/outils/ombre.py", "old_string": "rayon = 6", "new_string": "rayon = 4"}, "Le fichier a ete modifie."),
       ("Bash", {"command": B + "ombre.py --rayon 4 --sortie rendus/ombre-2.png"}, "passe 1 : 6 ms\npasse 2 : 7 ms\nrendus/ombre-2.png"),
       ("Un rayon de 4 au lieu de 6 : 13 ms au total, sous le budget de 16 ms, et l'ombre se voit encore.",)],
      "wf_a1b2c3d4-e5f", "rendu:ombrage", "Rendu")
# L'audit : deux agents de plus.
agent("b2c0de00000000003", FIN - timedelta(minutes=13), "Chercher les liens redondants du maillage xxxxx, ceux qu'on peut retirer sans isoler personne.",
      [("Un lien est redondant si ses deux extremites restent joignables autrement. Je le teste lien par lien.",),
       ("Bash", {"command": B + "redondance.py --liens config/liens.toml"}, "118 liens, 21 redondants, 3 critiques\nsortie : rendus/redondance.txt"),
       ("Grep", {"pattern": "critique", "path": "C:/demo/orbite/rendus/redondance.txt"}, "n17-n03 critique\nn09-n22 critique\nn30-n31 critique"),
       ("Trois liens critiques, et n17-n03 est celui que l'audit principal a deja designe : les deux methodes concordent.",)],
      "wf_b2c3d4e5-f6a", "audit:redondance", "Audit")
agent("b2c0de00000000004", FIN - timedelta(minutes=12), "Simuler la perte de chaque noeud et dire combien de voisins restent joignables.",
      [("Je retire les noeuds un par un et je compte la plus grande composante restante.",),
       ("Bash", {"command": B + "pertes.py --noeuds 32 --sortie rendus/pertes.png"}, "pire cas : n17, 4 noeuds isoles\nmediane : 0 noeud isole\nrendus/pertes.png"),
       ("Read", {"file_path": "C:/demo/orbite/rendus/pertes.png"}, "", "spectre"),
       ("La mediane est a zero : le maillage tient presque partout. Seul n17 casse quelque chose, et c'est quatre noeuds.",)],
      "wf_b2c3d4e5-f6a", "audit:pertes", "Audit")
# La mesure en cours : un agent de plus.
agent("c3c0de00000000004", FIN - timedelta(seconds=55), "Verifier que le chauffage de la machine ne fausse pas la mesure de la file prioritaire.",
      [("Je note la frequence du processeur pendant chaque passe pour voir si elle derive.",),
       ("Bash", {"command": B + "frequence.py --duree 120 --sortie rendus/orbites.png"}, "debut 4.1 GHz, fin 3.9 GHz\nderive : -4.8 %\nrendus/orbites.png"),
       ("Read", {"file_path": "C:/demo/orbite/rendus/orbites.png"}, "", "orbites"),
       ("Une derive de moins de 5 % : elle ne suffit pas a expliquer l'ecart de 39 ms sur le p95.",)],
      "wf_c3d4e5f6-a7b", "mesure:frequence", "Mesure")

# Un second projet, termine : l'etalonnage d'une balise.
SESSION = "8c3a91f4-2b6d-4d57-a0e2-6f1b7c4d2a08"
PROJET = "C--demo-balise"
CWD = "C:\\demo\\balise"
script("wf_d4e5f6a7-b8c", "etalonnage-balise", "Etalonner la balise et verifier sa derive sur une nuit")
k = Fil("d4c0de00000000001", FIN - timedelta(hours=2, minutes=10))
k.consigne("Etalonner la balise xxxxx avec la source de reference puis verifier sa derive sur xxxxx heures.")
k.dit("Je commence par l'etalonnage a trois points, puis je laisse tourner la mesure de derive.")
k.geste("Bash", {"command": "cd C:/demo/balise && python outils/etalonner.py --points 3 --sortie rendus/spectre.png"}, "point 1 : -0.4 dB\npoint 2 : +0.1 dB\npoint 3 : +0.3 dB\nrendus/spectre.png")
k.geste("Read", {"file_path": "C:/demo/balise/rendus/spectre.png"}, "", image="spectre")
k.dit("Deux pics nets, l'un a 22 et l'autre a 58. Le second est plus large : c'est l'harmonique, pas un defaut.")
k.geste("Bash", {"command": "cd C:/demo/balise && python outils/derive.py --heures 8 --sortie rendus/derive.png"}, "derive maximale : 0.07 dB sur 8 h\nrendus/derive.png")
k.geste("Read", {"file_path": "C:/demo/balise/rendus/derive.png"}, "", image="latence")
k.dit("Derive maximale de 0.07 dB sur huit heures, bien sous la tolerance de 0.5 dB. La balise est etalonnee.")
ecrire("wf_d4e5f6a7-b8c", "etalonnage:balise", "Etalonnage", k)

os.makedirs(os.path.join(RACINE, "vigie"), exist_ok=True)
print("ecrit", RACINE, "VIGIE_MAINTENANT =", int(FIN.timestamp()) + 20)

# Les etats (en cours, interrompu, termine) se lisent sur les dates des fichiers :
# on les date comme la transcription, creation au premier geste, ecriture au dernier.
import subprocess
for d, debut, fin in DATES:
    for chemin in [d] + [os.path.join(d, n) for n in os.listdir(d)]:
        ps = ("$i=Get-Item -LiteralPath '%s'; $i.CreationTimeUtc=[datetime]'%s'; $i.LastWriteTimeUtc=[datetime]'%s'"
              % (chemin, debut.replace("Z", ""), fin.replace("Z", "")))
        subprocess.run(["powershell", "-NoProfile", "-Command", ps], check=True)
