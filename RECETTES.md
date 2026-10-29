# Recettes

Ce qui a été essayé sur cette machine pour rendre, mesurer et juger la vigie,
ce qui marche, ce qui ne marche pas, et pourquoi. Chaque recette renvoie au
document qui l'a éprouvée ; les chiffres sont les siens.

## Tour 00, le 2026-10-27

### Figer les données, et les refaire à l'identique

**Copier la copie figée avec ses dates de création.** La vigie date le
lancement d'un chantier par la date de création d'un fichier
(`catalogue.rs`, `created()`). Une copie qui la perd fait passer deux
chantiers à « en cours » et toutes les heures à celle de la copie. `cp -r` et
`shutil.copytree` la perdent (447 à 498 pixels d'écart sur la liste) ;
`cp -a` garde la modification, pas la création. Ce qui marche :

    MSYS2_ARG_CONV_EXCL='*' robocopy <source> <copie> /E /COPY:DAT /DCOPY:DAT /NFL /NDL /NJH /NJS /NP || [ $? -lt 8 ]

0 pixel d'écart sur 57 captures (`arbitrage.md` §1). `MSYS2_ARG_CONV_EXCL`
empêche Git Bash de prendre `/E` pour un chemin ; robocopy rend 0 à 7 quand il
a copié. Sur une copie déjà faite autrement, `passage-b/juge-correction/dates.py`
repose les dates par PowerShell (`SetCreationTimeUtc`) et compte d'abord les
dates fausses : une copie fraîche doit en avoir, sinon la relecture ne prouve
rien.

**Relire la copie contre son manifeste** (`donnees/MANIFESTE.json`, taille et
SHA-256 de 217 fichiers) avant chaque rendu. Le témoin change un octet et doit
trouver la faute ; réduit à la seule taille, il ne la trouve plus
(`passage-b/epreuve.md`).

**Figer l'heure** par `VIGIE_MAINTENANT` : deux minutes après la dernière
écriture de la copie, les chantiers qui tournaient sont « en cours », ce qui
rend jouable le coup d'œil sur un agent vivant ; à +12, +25 et +31 minutes,
le muet et l'interrompu (`passage-b/correcteur-2/`).

**Ce que la copie ne fige pas** : les fichiers nommés par les transcriptions.
`wolfram.mp4`, effacée de `cours-llm` à 15:16, a changé deux rendus du même
jour (`passage-b/juge-design.md` §1).

### Rendre sans que la capture mente

**Poser le survol hors de toute cible** (`--survol=1,1`) : la fenêtre de
capture voit le vrai curseur. La vigie lit le premier `--survol` de sa ligne de
commande : un script qui pose le sien avant celui qu'on lui passe l'écrase
(`passage-b/artisan.md` §4).

**Exiger le journal de chaque action.** La vigie écrit ce qu'un clic a touché
(`clic en (479, 384) : Ligne(3), vue Chantier("wf_609ba535-c75")`) et le
nombre de blocs du fil ; le script exige ce texte et refait la capture, trois
fois au plus. C'est ce qui attrape `--agent` ignoré
(`passage-b/epreuve.md`).

**Attendre un état, pas une durée.** Une capture qui partait après trois
secondes prenait le fil à 48 blocs sur 90 sur une machine chargée. La vigie dit
maintenant quand le fil a lu son fichier (`Nouvelle::Lu`) et la capture
l'attend (`passage-b/CORRECTIONS.md`, objection 21).

**Une capture ne prend pas la main.** La fenêtre d'une capture prenait le
premier plan et obéissait au vrai clavier : `Retour arrière` ou un clic sur le
titre la faisaient passer du fil à la page, journal muet
(`passage-b/juge-technique.md` §4). Elle naît maintenant sans activation
(`with_active(false)`) et ignore l'entrée réelle ; **non vérifié par une vraie
entrée**.

**Comparer deux rendus en masquant la pastille.** Elle respire avec l'horloge
et `--capture` la prend où elle en est : deux rendus de suite ne diffèrent que
dans sa boîte de 16 × 20 points (`systeme.md` §8).

**Provoquer la perte du périphérique** par `VIGIE_PERDRE=1.5` : la vigie
détruit son périphérique graphique pendant la capture. C'est le témoin de la
relance ; sans garde, elle panique sur un tampon né invalide, le message même
de la panique vue 7 fois en rendu (`passage-b/CORRECTIONS.md`, objection 22).

### Mesurer sur les pixels

**Le contraste se lit sur la capture d'origine, jamais sur une planche
réduite** : la réduction lisse les petits glyphes et les éclaircit
(`crit-tire.md`). `outils/contraste.py` prend le cœur du glyphe contre la
couleur la plus fréquente de la zone, en lumière linéaire ; sans gamma, 2,05
au lieu de 4,54. Une zone sans encre est refusée (code 2), sinon une zone mal
posée passerait pour un contraste (`systeme.md` §8).

**Refaire un chiffre par un second chemin** : `contraste.py --paire` sur les
octets lus aux mêmes pixels redonne la mesure de zone (5,24 et 3,93 des deux
côtés pour le bord de la barre, `CORRECTIONS.md`).

**Le coup d'œil au quart** : réduire la capture au quart de sa largeur par
Lanczos et mesurer la hauteur d'encre de chaque ligne
(`passage-b/correcteur-2/quart.py`). Le lieu passé de 13 à 20 points y passe
de 3 à 5 pixels, la hauteur du titre. Lire au quart reste un jugement à l'œil ;
la mesure dit seulement qu'il égale le titre.

**Séparer la mise en page de la couleur** : comparer les masques d'encre de
deux captures (où il y a quelque chose, où il n'y a que le fond). La même image
en négatif diffère par tous ses pixels et par 0 de structure ; déplacée, de
plus de 0,3 (`arbitrage/triptyques.py`, `arbitrage.md` §4).

**Trancher une dimension par ce qu'elle coûte, par deux chemins** : la
largeur de la barre (176 points) sort du calcul de la mise en page
(`passage-b/artisan/cout_barre.py`) et du compte des vignettes sur les pixels
de 24 captures, qui s'accordent. La première largeur essayée, 188, ne laissait
que 2 vignettes de front à la moitié d'écran.

**Mesurer dans une vraie fenêtre** : `passage-b/juge-technique/fenetre.ps1`
lance et arrête ses propres fenêtres par leur PID. La cadence se lit en
copiant le tiers central de la zone client en boucle (141 à 145 copies par
seconde) ; c'est une borne basse des images présentées, et le témoin sans clic
voit parfois une image nouvelle sur une page immobile.

### Juger à l'aveugle

**Ranger les options par l'empreinte de leur contenu avant de les permuter.**
La graine 17 permute 3 options en `[0, 1, 2]` : sans ce rang, l'ordre de la
ligne de commande serait passé tel quel aux juges (`systeme.md` §8).

**Tirer les lettres une fois**, sur un écran de référence, et les garder pour
tous les écrans. Tirées écran par écran, la lettre A d'une planche n'est plus
l'option A de la suivante (`crit-tire.md`, `crit-inverse.md`).

**L'envers dans un appel séparé**, sans mémoire de l'endroit ; puis l'arbitre
met chaque appréciation de l'une en face de l'autre et déclare nulle celle
qui change de signe avec l'ordre (trois sur seize, `arbitrage.md` §2). Une
direction qui se reconnaît au premier coup d'œil n'est aveugle qu'en partie.

**Sceller la prédiction de l'arbitre** hors du dépôt, son SHA-256 dans
`ETAT.md` ; le greffier la recalcule après la décision (concordante au
tour 00, `design/LOI-DU-DESIGN.md` §4).

### Prouver qu'un essai sait tomber

**Ne compter comme chute que `FAILED` sur l'essai visé.** Une erreur de
compilation n'est pas une chute : le premier sabotage du correcteur
(`let _ = &mut vues;`) ne compilait pas, et le script l'a dit au lieu de le
compter. Un sabotage qui compile (`… || true`) fait tomber l'essai
(`passage-b/correcteur/saboter.txt`, `saboter-1.txt`).

**Saboter le chemin que l'essai garde, pas un voisin.** Ôter le seul garde de
`dessiner_dans` laissait la capture relancée : deux autres gardes prenaient la
perte avant lui. Le sabotage qui fait foi empêche le rappel de perte de lever
son drapeau (`passage-b/correcteur-2/saboter-1.txt`).

**Saboter sur une copie de l'arbre commité**, avec sa propre cible de
compilation, jamais dans l'arbre où d'autres agents travaillent.

**Rejouer dans un arbre propre** : un `git clone` hors du dépôt, la copie
figée recopiée et ses dates reposées, les essais, puis le rendu commité **tel
quel, sans reprise** (`passage-b/juge-correction/seconde/rejouer.sh`). C'est
ce qui a montré que le rendu du correcteur mourait deux fois sur deux sur une
panique, avant la relance.

### Le mouvement et les réglages

**Mouvement réduit par le même chemin** : sous le réglage de Windows ou
`--mouvement-reduit`, le pas des ressorts devient si long que la solution
exacte est déjà sur sa cible ; tout se pose d'un coup sans un second code
(`passage-b/artisan.md` §1).

**Remettre en page une fois, translater ensuite** : la barre qui se masque
remet le contenu en page à sa largeur d'arrivée, et le ressort ne fait que le
translater. Le prix est connu et **non jugé à l'œil** : le contenu déborde à
droite pendant ~300 ms.

### Écrire depuis Git Bash

**Python vers un fichier** : sous Windows, la sortie redirigée prend la page
de code de la console, et « ≥ » fait planter le script. Écrire en octets
UTF-8 (`sys.stdout.buffer.write(texte.encode("utf-8"))`) ou préfixer
`PYTHONIOENCODING=utf-8` (`passage-b/CORRECTIONS.md`, seconde passe).

**`python -I`** pour tout outil qui lit des captures ou des copies : rien du
dossier courant ne s'importe.

### Ce qui a échoué, pour ne pas le refaire

- Un drapeau supposé par la consigne : `planche_aveugle.py --inverse` n'existe
  pas ; la crit à l'envers a écrit son propre `crit-inverse/planches.py`.
- Un partisan lancé dans un clone sous un autre dépôt (`cours-llm`) : sa
  direction, la console, a échappé à l'arbitre, et c'est elle que l'auteur a
  choisie (`design/tours/tour-00/DECISION.md`).
- Le témoin de `cout_barre.py` a d'abord échoué (2 boîtes comptées sur 3) :
  l'outil sautait une barre absente. Un témoin qui échoue au premier essai
  est une bonne nouvelle, c'est qu'il regarde.
