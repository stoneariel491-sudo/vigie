> Note : ce document est tiré du journal de travail de l'auteur. Les dossiers `design/`, `REPRISE.md` et `SOCLE.md` qu'il cite appartiennent à une archive privée et ne sont pas dans ce dépôt.

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
`wolfram.mp4`, effacée de `atelier-d` à 15:16, a changé deux rendus du même
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
- Un partisan lancé dans un clone sous un autre dépôt (`atelier-d`) : sa
  direction, la console, a échappé à l'arbitre, et c'est elle que l'auteur a
  choisie (`design/tours/tour-00/DECISION.md`).
- Le témoin de `cout_barre.py` a d'abord échoué (2 boîtes comptées sur 3) :
  l'outil sautait une barre absente. Un témoin qui échoue au premier essai
  est une bonne nouvelle, c'est qu'il regarde.

## Navigateur interne, tour 01, le 2026-10-29

Ce que le tour du navigateur a éprouvé, ce qui marche, ce qui a échoué, et
pourquoi. Chaque recette renvoie au document qui l'a éprouvée ; les chiffres
sont les siens. Les mots « vu », « lu » et « non vérifié » sont ceux de leurs
auteurs.

### Poser une page web dans la vigie

**WebView2 par `wry` en fenêtre enfant d'une fenêtre winit.** `wry` 0.57.1 prend
n'importe quelle fenêtre qui porte un `HasWindowHandle` et crée un enfant
(`WS_CHILD | WS_CLIPCHILDREN`) : +459 264 octets mesurés par deux crates à part
(winit seul, 468 992 octets ; winit + wry, 928 256), aucune DLL, runtime déjà sur
le poste (`faisabilite.md` §1, §2.1). La raison de la contrainte : l'enfant se
peint **par-dessus** la surface wgpu. Un rectangle net, rien dessous, rien par-dessus
(ni la visionneuse, ni le voile, ni la bulle) : tout ce que la vigie dessine sur
la zone de la page impose de cacher la page. Le rendu vers une texture n'a pas
d'API officielle (`CapturePreview` encode un PNG, trop lent pour une animation).

**Poser les bornes en pixels physiques.** `wry` convertit le logique en physique
par l'échelle de la fenêtre ; la vigie travaille déjà en physique. Piège vu : le
script de capture de l'essai rendait 1114 × 758 au lieu de 1393 × 947 à 125 % tant
qu'il n'appelait pas `SetProcessDPIAware` (`capturer.ps1`, `faisabilite.md` §2.1).

**Une page native échappe à `--capture`.** Le cadre autour d'elle se rend par la
vigie (zone grise à la place de la page) ; la page se photographie par
`PrintWindow` sur **une** fenêtre, choisie par le numéro de son processus
(`passage-b/photographier.ps1`, `essai-reel.ps1`), jamais la fenêtre de Stone
(`artisan.md` §4). La fenêtre d'un essai ignore l'entrée physique : `Échap` n'y est
vu que par l'évènement que la page s'envoie.

### Retrouver et surligner le passage

**Le fragment de texte d'abord, sans script, puis une sonde.** `#:~:text=` marche
sur les pages servies en entier (MDN, Wikipédia, une spécification, une doc),
centre le passage et le surligne en natif. Il est tout ou rien et **échoue en
silence** : sur le texte arrivé après le chargement (crates.io, deux pages de
contrôle à 1,5 s et 6 s), et sur un texte que le moteur ne trouve pas (rustdoc,
« Vec is… » : cause non établie). Il prend **la première occurrence**, qui peut
être une table des matières. Le tiret, la virgule et l'esperluette du texte se
codent (`%2D`, `%2C`). Les navigations lancées par l'hôte (premier chargement,
`load_url`) comptent comme initiées par l'utilisateur (`faisabilite.md` §3).

**Relancer le script, ne pas l'essayer une fois.** `window.find` relancé toutes
les 250 ms jusqu'à 15 s, `readyState` complet exigé : trouvé en 0,82 s sur
crates.io, en 1,76 s et 6,25 s sur les pages à 1,5 s et 6 s. ~~Une seule tentative
à 600 ms~~ échouait sur les deux (`B-tardive-rapide.jsonl`, gardé comme preuve de
la faute). Limites de `window.find` : non standard, il **sélectionne** au lieu de
surligner et ne centre pas ; la voie par *CSS Custom Highlight* (un `Range` par
`TreeWalker`, `::highlight()`) est restée **non essayée** (`faisabilite.md` §3.2).
Pas de passage trouvé : la page s'ouvre en haut et la vigie le dit.

**Demander la même langue des deux côtés.** La lecture sans script et le moteur
doivent envoyer le même `Accept-Language` (`--lang` et `--accept-lang` du moteur) :
Unity a répondu sa version française au moteur, la citation n'y était plus, et
la vigie a d'abord accusé la page. ~~« passage non retrouvé »~~ était une faute de
l'artisan (`artisan.md` §4).

**Peindre la surbrillance avec une feuille adoptée.** Une feuille de style que
le script de la vigie ajoute au document (`adoptedStyleSheets`, pas de `<style>`,
donc non bloquée par une politique de contenu) peint `::target-text` et la
surbrillance en ambre `#ffc857`, encre `#1a1a1a`, soulignée de 2 px. Mesuré sur
une photo vivante : 11,31:1 encre sur surligné, **1,54:1** surligné contre page
blanche. La couleur seule ne tient donc pas la surface ; le soulignement la porte.
Limite : une page qui réécrit `adoptedStyleSheets` efface le style, pas le
surlignage (`CORRECTIONS.md` §3, §7).

**Mesurer la surbrillance avec son témoin d'abord.** `contraste_surbrillance.py`
passe son témoin (21:1 et 1:1) avant de mesurer la photo.

### Ne pas croire la page

**Une sonde qui ne lit rien que la page écrit.** L'état « retrouvé » vit dans une
fermeture ; la poignée est une propriété non configurable au nom tiré au sort ; les
outils du moteur sont pris avant la page ; un évènement clavier fabriqué
(`isTrusted` faux) ne ferme rien ; « retrouvé » exige le texte dans le DOM,
affiché, à l'écran, surligné. ~~`window.__vigie_etat` et `scrollY > 0` comme
preuve~~ : une page les fabriquait (T1). Reste **N2** : une citation à `opacity:0`
rend encore `Trouve` (le script teste une boîte non nulle, pas l'opacité).

**Le blindage, posé dans tous les cadres avant la page.** Un script de
démarrage remplace `getDisplayMedia` par un refus non modifiable, refuse
`clipboard.writeText` et `write`, rend faux `execCommand('copy'/'cut')`, neutralise
`alert`, `confirm`, `prompt`. La raison : le gestionnaire de permissions du moteur
ne voit pas `getDisplayMedia` (le sélecteur de partage d'écran s'ouvrait sans
geste, il liste les fenêtres de Stone) et `with_clipboard(false)` ne couvre pas
`writeText`. **Trou connu** : `showOpenFilePicker`, `showSaveFilePicker`,
`showDirectoryPicker` ouvrent une boîte native qui bloque la fermeture de la
vigie (N1) ; HID, USB, Bluetooth : non tranché (`CORRECTIONS.md` §2,
`juge-correction.md` §3).

**Un gardien de navigation qui est une fonction jugée par un essai.**
`navigation_permise` lit l'hôte comme le moteur : IPv4 sous toutes ses écritures
(décimale `2130706433`, hexadécimale, octale, moins de quatre parties, pleine
chasse, pourcentages, tabulations), IPv6 dont la mappée, point final, `.localhost`,
`.local`, toutes les plages privées. ~~`starts_with("127.")`~~ prenait
`127.evil.example` pour la boucle locale. L'accord pour le local vaut pour **l'hôte
et le port vus**, et chaque saut de redirection est rejugé (T4, T5). **Trou
connu** : le moteur ne soumet pas les `<iframe>` à ce gardien (T4 cadres).

**Refuser une adresse privée à la résolution, pas avant.** `ureq` 3 avec un
résolveur remplacé : une vérification faite avant la résolution laisse passer un
nom qui change de réponse entre les deux (le rebond de DNS). Le mandataire de
l'environnement est ignoré exprès ; la lecture accepte le texte d'une page de 2 Mio
au plus en 10 s au plus, cinq sauts au plus ; `Accept-Encoding: identity`
(`artisan.md` §1, `CORRECTIONS.md` §4). L'essai qui prouve le refus à la résolution
exige un réseau (`localtest.me`) : **hors ligne il passe sans rien prouver**
(`artisan.md` §6, point 6).

**Un profil de page vide, à part, supprimé.** Profil InPrivate dans un dossier à
lui. Trois essais sur sept laissaient le dossier après la fermeture (le moteur
tient encore ses fichiers) : ce qui reste est confié à un petit processus, et un
balayage au lancement retire les profils orphelins, jamais celui d'une autre vigie
ouverte. L'essai de ce processus a d'abord échoué sur un défaut réel : les
guillemets de Rust mangeaient la ligne de `cmd` (« La syntaxe du nom de fichier …
est incorrecte »), corrigé par `raw_arg`. Piège de l'essai : Windows supprime un
dossier dont un fichier est ouvert (sémantique POSIX de `remove_dir_all`), il faut
un processus dont le **dossier courant** est le profil (`artisan.md` §4, §7).

### Prouver qu'un essai tombe, et juger sans croire

**Rejouer en vrai sur deux exécutables.** Pour une faille du moteur de page, que les
essais Rust ne rejouent pas : construire l'ancien (`git archive` du commit fautif
dans un dossier à part, cible à part), lancer les **mêmes pages hostiles** par le même
script, et recopier **ce que la page a dit au serveur**
(`corrections/rapports-des-pages.txt`). Ancien : `Trouve`, « EN ATTENTE apres 3 s »,
« ACCORDEE » ; nouveau : `NonRetrouve`, `NotAllowedError`. Quand toutes les pages
sont locales, Stone « a accepté le local » : la photo ne prouve rien pour le gardien,
c'est l'essai qui prouve (`CORRECTIONS.md` §1, T4). **Ce qui a mal tourné** : le
rejeu de l'ancien exécutable a écrit « x » dans le presse-papiers de Stone, que
personne n'a pu restaurer. **À faire avant un tel rejeu** : lire et sauver le
presse-papiers.

**Énumérer les fenêtres visibles des processus.** La photo du harnais ne montre que
la fenêtre de la vigie. Une boîte native du moteur (`#32770 « Select a file this
site can read »`) ne se voit que par l'énumération des fenêtres visibles des
processus de la vigie et de son moteur (`juge-correction/fenetres.ps1`).

**Les sabotages disent la vérité sur les essais.** 20 essais neufs de l'artisan
sabotés, 20 sur 20 conformes (`passage-b/saboter.txt`) ; 44 sabotages du juge
technique, 11 sans essai qui tombe ; 129 essais passent, 8 sabotages sur 9 tombent
chez B8. Quatre leçons :
- un essai qui « tient debout » ne teste que l'absence d'une valeur
  (`le_processus_courant_est_vivant` ne testait que l'absence d'un numéro) ;
- un essai de borne doit comparer à un nombre **en clair**, pas à la constante qu'il
  garde : `DELAI` comparé à lui-même, ou `SAUTS_MAX + 1`, passe avec
  `SAUTS_MAX = 500` ;
- un sabotage qui ne change aucune sortie est un **mutant équivalent** (le seuil de
  huit mots : sous huit mots la citation est son propre début) ; il se prouve par un
  argument et par le seuil changé sans qu'un essai tombe, et on n'écrit pas
  d'essai pour lui ;
- le sabotage doit viser la ligne que l'essai garde, pas sa voisine (B8 a d'abord
  gâté la garde de la seconde rangée, que l'essai n'atteint pas) ; un sabotage
  neutre qui ne doit **pas** faire tomber prouve que le harnais ne déclare pas tout
  tombé.

### La seconde passe : ce que le rejeu en vrai a trouvé et que les essais ne voyaient pas

**Une boîte modale du moteur désactive la fenêtre de la vigie.** La boîte native
(`#32770`, sélecteur de fichiers, de HID) est posée par le processus du moteur avec
**la fenêtre de la vigie pour propriétaire** : le propriétaire passe à inactif,
`CloseMainWindow` est ignoré, aucun clic n'arrive. Refuser les sélecteurs à la source
(blindage) **et** refermer toute fenêtre d'un autre processus dont la vigie est
propriétaire (`EnumWindows` + `GetWindow(GW_OWNER)` + `WM_CLOSE`, puis `EnableWindow`),
chaque couche prouvée seule (`seconde/saboter.py`, `seconde/rejeu.py`). Piège de l'essai :
la fenêtre est désactivée un instant **avant** que la boîte ne soit montrée ; sans pause
le sabotage du filtre ne tombait qu'une fois sur deux.

**Piloter la page par le protocole de devtools du moteur, pour l'essai seulement.**
`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9333` dans
l'environnement du seul processus d'essai ; `node` 24 (WebSocket natif) envoie de vrais
clics et de vraies touches de confiance (`Input.dispatchMouseEvent`,
`Input.dispatchKeyEvent`, `modifiers:1` pour Alt) sans toucher le clavier de personne
(`seconde/cdp.js`, `essai_cdp.ps1`). C'est ce qui a montré qu'**Échap et Alt + flèche
n'avaient jamais été lus dans la page** : `isTrusted` est un attribut inaltérable de
chaque évènement, pas du prototype ; `getOwnPropertyDescriptor(Event.prototype,
'isTrusted')` rend `undefined`. Il se lit sur l'évènement. Une fenêtre réduite ne se
redessine plus, donc la vigie n'y interroge plus sa page : la restaurer sans l'activer
(`seconde/etat_fenetre.ps1`, `SW_SHOWNOACTIVATE`).

**L'historique du moteur, pas de la page.** `GoBack`/`GoForward`, `CanGoBack`/`CanGoForward`
relus à `HistoryChanged`. `history.length` compte aussi les entrées en avant.

**Ce que le filtre de ressources ne voit pas.** `WebResourceRequested` (filtre `*`, toutes
sources) livre les cadres, les images, les scripts, `fetch` ; **pas les `WebSocket`**,
même avec `ws://*` : le blindage les refuse. Construire la page vide, poser les
gardiens, puis `load_url` : avec `with_url` la première requête part avant eux.

**Un essai de chaînes ne prouve pas un script.** Le script de la sonde appelait `indexOf`
au lieu de sa copie `indexDe` : exception à chaque tentative, plus rien jamais retrouvé,
tous les essais Rust verts. Le rejeu sur la page honnête (`visible.html` doit rendre
`Trouve`) l'a vu. Garder le témoin positif avec chaque contrôle négatif.

### Juger trois options sur les mêmes données

**Tout refaire sur un chantier, un lien, une citation.** Les trois partisans avaient
rendu des données différentes : une remarque qui compare deux options pouvait venir
du cas autant que du dessin. L'arbitre a rendu les trois (188 captures) sur la copie
figée de A2 (empreintes relues contre `MANIFESTE.json`, 0 écart), chaque option avec
la vigie construite par son partisan (`arbitrage.md` §0).

**Retour au pixel d'un état d'ouverture.** Fil seul contre fil après ouverture puis
fermeture : 0 pixel sur 15 paires. Piège du banc : `--volet=<adresse>` puis `Échap`
laissait le volet ouvert dans la capture (333 821 à 1 281 451 pixels de différence),
le drapeau repose l'état après la touche ; par un clic sur la ligne puis `Échap`, 0
(`arbitrage.md` §4).

**Écarter une préférence qui change de signe avec l'ordre.** Quatre sur les crits :
même fait lu en qualité puis en défaut (la plaque de survol de D1, le schéma refusé
de D1, le point d'ambre de D3, l'état de chargement de D2) ; l'arbitre garde le fait
mesuré, jamais la préférence (`arbitrage.md` §1).

**Le banc a une dépendance cachée.** `donnees/projects` est ignoré par git
(`.gitignore:11`) : sans lui `rendre.sh` et les rejeux de B8 ne tournent pas
(« page NON OUVERTE »). Quatre captures de la barre latérale diffèrent d'un clone à
l'autre (5 626 pixels, deux noms de projet dans un autre ordre) ; cause **supposée**
: les dates des fichiers copiés (`juge-correction.md` §2). La recette du tour 00 (copier
avec robocopy et ses dates de création) s'applique.
