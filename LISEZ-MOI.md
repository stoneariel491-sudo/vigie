# Vigie

Une fenêtre pour regarder penser les agents, pendant qu'ils travaillent.

Le panneau des chantiers dit **où** en est le travail. Il ne dit pas ce que
l'agent pense, et c'est pourtant là que se trouve ce qui s'apprend : les
hypothèses écartées, la mesure qui contredit une attente, le moment exact où
quelqu'un se corrige.

## L'installer

```
.\outils\installer.ps1
```

Vigie s'installe pour l'utilisateur, sans droits d'administrateur, comme une
application : dans `%LOCALAPPDATA%\Programs\Vigie`, avec une entrée dans le
menu Démarrer, un raccourci sur le Bureau, et une ligne dans **Paramètres →
Applications installées** (son icône, sa version, l'éditeur « Stone », et
**Désinstaller**). Relancer le script met à jour ; il ferme d'abord la vigie
ouverte, qui garde son état en se fermant. `outils\desinstaller.ps1` retire ce
que l'installation a posé et rien d'autre : les données restent, sauf avec
`-AussiLesDonnees`. Pour développer, `cargo run --release` suffit.

Avant le 2026-10-27, le raccourci du Bureau visait un exécutable du dossier de
développement : la vigie n'était dans aucune liste d'applications, ne se
désinstallait pas, et Windows lui-même ne la connaissait pas (l'outil qui
pilote les applications répondait qu'aucune ne portait ce nom).

### Ce qui fait une application

- **Une seule fenêtre.** Un second lancement, depuis le menu ou le Bureau,
  ne rouvre pas une seconde vigie : il ramène la première au premier plan, et
  la restaure si elle était réduite. Un mutex nommé dit qu'elle existe, le nom
  de classe de sa fenêtre (`Stone.Vigie.Fenetre`) la retrouve ; le titre
  « Vigie » ne suffirait pas, un dossier ouvert dans l'Explorateur porte le
  même. Les drapeaux d'essai et de capture y échappent, pour pouvoir juger un
  état pendant que la vigie est ouverte.
- **Elle se souvient.** À la fermeture, d'où qu'elle vienne (la croix,
  Échap, la fin de session), elle garde sa taille, sa place, son zoom et ce
  qu'on regardait — le direct, la liste d'un projet, la page d'un chantier ou
  le fil d'un agent — dans `%APPDATA%\Vigie\fenetre.json`, et les retrouve au
  lancement suivant. Réduite au moment de fermer, elle garde la taille
  d'avant. Une place hors de tout écran (un second écran débranché depuis)
  n'est pas reprise, et un souvenir incohérent est écarté plutôt que de faire
  une fenêtre de zéro pixel.
- **Elle démarre tout de suite.** Le catalogue garde sur le disque
  (`%APPDATA%\Vigie\catalogue.json`) ce qui ne change plus : la première
  ligne de chaque agent, le bilan des chantiers finis, l'en-tête des scripts.
  Mesuré le 2026-10-27 sur 177 chantiers et 1 470 agents : **62,4 s** pour le
  premier parcours sans mémoire, **0,0 s** au lancement suivant. La racine git
  d'un dossier n'y est pas, un dépôt créé depuis la changerait ; un agent dont
  la première ligne n'est pas encore écrite non plus, pour être relu.
- **Elle se nomme.** L'exécutable porte son nom, sa version et son éditeur :
  le Gestionnaire des tâches dit « Vigie », pas « vigie.exe ».

## Ce qu'elle montre

Rien ne se configure. La vigie prend le chantier le plus récent, s'attache à
l'agent qui écrit, et bascule toute seule quand un autre prend la main — y
compris après une interruption, qui est fréquente.

Quand plusieurs chantiers tournent dans plusieurs projets, celui qu'on veut
regarder n'est pas forcément le dernier lancé. Une **barre latérale**, une
rangée par projet, donne la liste des chantiers de chacun ; un clic ouvre la
**page du chantier** : ses agents, quand chacun a travaillé, et toutes ses
images. Un clic sur un agent suit cet agent-là, depuis son début. La rangée
**Direct** garde le comportement d'origine. La barre se masque et se
réaffiche à la main, comme celle de l'application Claude (plus bas).

```
liste du projet  →  page du chantier  →  fil d'un agent
                 ←  Retour arrière    ←
```

## Ce qui a été demandé

Le 27 octobre 2026, Stone :

> « fais en sorte de terminer le travail d'amélioration que l'on avait
> commencer sur vigie du genre que l'on peut choisir qu'elle worflow on
> regarde avec une belle interface et uutile et que tout soit là les images
> surtout donc c'est surtout une ouverture »

Ce qui était déjà là : les onglets et la liste des chantiers (3 octobre), et
les images que l'agent **ouvre**. Ce qui manquait, et que ce travail ajoute :

1. **Le raccourci du Bureau lançait une version d'avant les onglets.**
   `dist\Vigie.exe` datait du 3 octobre à 01 h 09, une heure avant le commit
   des onglets : le choix du chantier existait dans le code, pas dans la
   fenêtre qu'on ouvrait. Il est refait ; l'ancien est gardé sous
   `dist\ancienne\`.
2. **Le choix de l'agent.** Le direct ne montre que l'agent qui écrit ; dans
   un chantier de design à cinq chercheurs, ce n'est presque jamais celui qui
   fait les images. La page du chantier les nomme tous.
3. **Les images produites.** Un agent qui rend une maquette, assemble une
   planche ou tourne une animation écrit un fichier ; il ne l'ouvre pas
   toujours. Ces fichiers n'entraient pas dans la vigie. Ils y entrent,
   vidéos comprises, dans le fil de l'agent qui les a faits et sur la planche
   du chantier.
4. **L'ouverture.** Un clic sur une image l'ouvre dans l'application du
   système, une vidéo dans le lecteur : la vigie montre tout ce que font les
   agents, et renvoie à l'outil qui sait zoomer au pixel ou lire le son.

## Les trois registres

Aucune étiquette ne les nomme. La couleur et la graisse suffisent, et une
étiquette répétée mille fois devient du bruit.

| | à quoi ça se voit | pourquoi |
|---|---|---|
| ce qu'il **pense** | encre-2, normal, en paragraphe | c'est l'intérieur, donc une voix plus basse |
| ce qu'il **dit** | encre-1, demi-gras, **un rail** à gauche, de l'air autour | c'est adressé au dehors, et ça finira dans son rapport |
| ce qu'il **fait** | une ligne de table : l'heure, le verbe (l'outil) en encre-2 dans sa colonne de 78 points, le détail coupé d'une ellipse en encre-3 | on descend la colonne des verbes sans relire les chemins |

Un seul registre porte un rail. En marquer trois rendrait la page rayée et ne
dirait plus rien ; en marquer un fait que l'œil trouve d'emblée ce qui compte.

### La console

Le 2026-10-27, devant les quatre directions du tour de design 00, Stone a
choisi « D · console » : la vigie comme
un instrument qu'on lit comme un moniteur. Un geste tient une ligne de
20 points à 13 points de corps, sans air entre deux gestes ; l'en-tête porte
le mot de l'état à droite du titre et compte ce que le fil contient
(`design-de-l-application · mon-projet · 75 gestes, 2 messages, 12 images`) ;
la pastille ne respire que devant ce qui travaille. Une image recopiée par un
`cp` ne se montre pas deux fois : elle devient une ligne, `Image
tan_manim.png · la même que Tangente_ManimCE_v0.22.0.png`. La palette est
celle du système du tour : trois
encres, aucune teinte, l'alerte toujours avec un mot ; et deux thèmes,
sombre et clair, que la touche `T` fait tourner avec celui de Windows.

## La page d'un chantier

Entre la liste d'un projet et le fil d'un agent, il manquait le chantier
lui-même. Sa page reprend la grille de la liste.

- **L'en-tête** dit son état, ses agents, ses images, quand il a été lancé et
  dans quel projet : `en cours · 5 agents · 19 images · lancé aujourd'hui à
  09:49 · mon-projet`.
- **Les agents**, en chronologie : rangés sous leur phase (`Balayage`,
  `Synthèse`, `Contre-épreuve`), chacun sa ligne, et sur un axe d'heures
  commun au chantier une barre de son début à sa dernière écriture, une
  marque par entrée de sa transcription et une marque haute par image. Ce
  qui s'est fait en même temps se voit en même temps. Sous le nom, la
  première ligne de son bilan, sinon la dernière chose qu'il a dite.
- Tant que le chantier tourne, une première ligne, **Celui qui écrit**, suit
  l'agent qui écrit et passe à celui qui prend la main : le direct, mais dans
  ce chantier.
- **Les pièces jointes**, enfin : toutes les images du chantier, autant de
  front que la colonne en tient à 130 points au moins (six au plus), par
  agent dans l'ordre du chantier puis dans l'ordre où chacun les a faites.
  Chaque image tient **entière** dans un cadre en 16:10, sans être rognée :
  couper une capture pour la faire entrer dans un carré cacherait ce qu'on
  veut juger. Sous elle, son nom, l'agent qui l'a faite et l'heure ; pour une
  vidéo, sa durée d'abord.

Un clic sur une vignette l'ouvre dans **la visionneuse** (plus bas), sans
quitter la page ; de là, **Entrée** suit l'agent qui l'a faite et place son
fil sur l'image, un peu d'air au-dessus pour la pensée qui la précède. Un clic
sur une ligne d'agent suit cet agent depuis son début : on l'a choisi pour le
lire, et vingt entrées ne suffiraient pas.

Le titre de l'en-tête remonte d'un niveau, comme celui d'une fenêtre du
Finder : du fil à la page, de la page à la liste. Il s'éclaire sous la souris,
et c'est le seul signe qui dise qu'il se clique.

### Le bilan d'un agent

Un bilan est un texte, ou un objet quand le chantier demande une réponse
structurée. Vu le 2026-10-27 sur un chantier de figures : `{"verdict": …}`
pour les juges, `{"figures": [...]}` pour les lots. La règle : le champ qui
conclut (`verdict`, `resume`, `summary`, `bilan`, `conclusion`, `message`),
sinon le premier texte, sinon le compte d’une liste (« 2 figures »). Les
dièses d'un titre, les accents graves d'un chemin et les astérisques d'une
emphase sont ôtés : ils se liraient en clair.

### La fiche

La page se dresse dans un fil à part, avec **le même lecteur** que le fil d'un
agent : une image est sur la planche si et seulement si elle est dans un fil.
Chaque transcription est lue une fois, puis seulement ce qui s'y ajoute,
toutes les secondes et demie. Pendant la première lecture, la fiche partielle
part toutes les demi-secondes : les agents se voient avant les images du
dernier. L'en-tête ajoute alors « lecture des agents » : ses comptes ne sont
pas encore ceux du chantier, et une capture prise sur une fiche partielle
montrait « 1 agent · 13 images » pour un chantier de cinq (vu le 2026-10-27 ;
`--capture` attend désormais la fiche complète). Mesuré le même jour,
catalogue en cours de parcours :

| chantier | agents | images | première lecture |
|---|---|---|---|
| `design-de-l-application` | 5 | 24, vidéos comprises | 3,3 s |
| un chantier de figures | 7 | 146 | 4,2 s |

Une vignette est gardée à 480 px au plus, ce qui laisse la marge d'un zoom à
trois de front ; 146 vignettes tiennent ainsi une centaine de mégaoctets. La
planche s'arrête à deux cents, et le dit dans son titre.

## Les images

Quand un agent ouvre une image, la vigie la montre **dans le fil, à l'endroit
où il l'a regardée**. Ce peut être la capture d'une maquette ou un rendu qu'il
juge. Sans elle, « le contraste du titre est trop faible » ne se vérifie qu'en
allant chercher le fichier à la main.

Quand un de ses gestes **écrit** une image ou une vidéo, la vigie la montre
aussi, juste après ce geste. La transcription n'en garde que la commande et ce
qu'elle a imprimé ; l'image, elle, est sur le disque, et la vigie l'y lit.

- **Le nom.** Tout nom de fichier image (`png`, `jpg`, `webp`, `gif`, `bmp`)
  ou vidéo (`mp4`, `webm`, `mov`, `mkv`) qui paraît dans un geste ou dans son
  résultat, ramené au dossier où la commande tournait : le premier `cd` qui
  l'ouvre, sinon le dossier de l'agent.
- **Un nom à blancs.** Rien dans `capture finale 2.png` ne dit où le nom
  commence. Chaque nom vient donc avec ses variantes, en remontant de quatre
  mots au plus et sans jamais traverser un guillemet ni la ponctuation du
  shell : `09:52 capture finale 2.png`, `capture finale 2.png`, `finale
  2.png`, `2.png`. La plus longue qui existe sur le disque l'emporte ; entre
  guillemets, le nom entier est lu d'un coup.
- **La règle.** Un nom ne suffit pas à dire que l'agent a produit le fichier :
  il peut lister le travail d'un autre, ou une capture d'un ancien chantier.
  Le fichier est à lui s'il a été écrit **pendant** le geste, entre son appel
  et son résultat ; ou si le geste le **nomme lui-même** et qu'il a été écrit
  depuis que l'agent a commencé, parce que manim range sa vidéo dans un
  sous-dossier qui n'est souvent nommé qu'à la commande suivante. Un nom qui
  ne paraît que dans la sortie d'un `ls`, écrit à un autre moment, n'est pas
  de lui. Trois secondes de marge, pour l'écart d'horloge et la fin d'une
  écriture.
- **Une seule fois.** Une planche ouverte trois fois ne se voit qu'une, là où
  elle est apparue.
- **Un seul agent.** Deux agents qui travaillent dans le même dossier au même
  moment voient passer les mêmes fichiers : l'un écrit `a.png`, l'autre la
  liste. Sur la planche, l'image va à celui qui l'a **nommée lui-même** (il
  l'a ouverte, ou sa commande portait son nom) plutôt qu'à celui qui ne l'a
  vue que dans la sortie d'un geste ; à preuve égale, au premier agent du
  chantier. Le compte d'images d'un agent est celui de la planche, si bien
  que les comptes s'additionnent au total du chantier (3 + 13 + 9 + 4 = 29
  pour `design-de-l-application`, 146 pour le chantier de figures).
- **La légende** dit ce qu'est l'image : `planche.png · produite · 2000 × 640`,
  ou `ouverte` pour celle que l'agent a regardée, ou `vidéo de 3 s`. La taille
  est celle du fichier, pas celle des pixels gardés : une capture de 2 000 px
  réduite à 1 280 n'est pas affichée plus petite qu'elle n'est.
- **Un clic** l'ouvre dans la visionneuse, dans la vigie elle-même. La main
  remplace la flèche au-dessus de toute image du fil.

### Les vidéos

Une vidéo se montre par **son image du milieu**, et non la première : une
animation commence presque toujours sur un fond vide. `ffprobe` donne sa
durée et sa taille, `ffmpeg` extrait l'image ; tous deux sont cherchés dans le
PATH, sans fenêtre de console. Sans eux, une vidéo reste une ligne : on perd
l'aperçu, pas le fil.

Le signe de lecture est **peint dans les pixels** : un disque sombre, un
triangle blanc, et un anneau clair d'un pixel et demi. L'anneau a été ajouté
après coup : sur les rendus manim à fond noir du cours, le disque sombre seul
disparaissait (vu le 2026-10-27 sur `ParDefaut.mp4`).

## La visionneuse

Le 2026-10-27, Stone :

> « fais en sorte que l'on puisse lire les videos de penser et les images de
> maniere native dans l'app pas avec un tier et aussi que lorsque l'on clique
> par exemple sur une photo et video dans un genre dw galerie elle fasse comme
> sur apple elle souvre comme proprement sans devoir repartir en bah ni
> pertuber quoi que ce soir »

puis, pendant le travail :

> « je pense que tu peux utiliser le travaille fait sur veilleurse et
> t'inspirer ici du glassmorphisme pour vraiment rendre le tout au top »

Avant, un clic sur une image l'ouvrait dans la visionneuse de Windows, et un
clic sur une vignette quittait la page pour le fil de l'agent. Désormais, tout
s'ouvre dans la vigie, comme Coup d'œil ou Photos sur un Mac :

- **L'image grandit depuis sa vignette** jusqu'à sa place, et s'y rend en se
  fermant. Le mouvement est le ressort à amortissement critique du
  défilement : un clic pendant l'ouverture change la cible et rien d'autre,
  sans saut. Si la vignette n'est plus à l'écran, l'image s'éteint en
  rétrécissant vers le centre.
- **Dessous, rien ne bouge.** Ni le défilement, ni la page ; le fil ne
  recolle pas au bas tant qu'elle est ouverte ; la molette et les clics ne
  passent pas à travers.
- **L'image entière, jamais étirée** au-delà de sa taille naturelle à
  l'échelle de l'écran. La vignette se montre d'abord ; l'image pleine se lit
  sur le disque dans un fil à part et la remplace, à sa vraie taille (bornée
  par la plus grande texture de la carte, 8 192 px au moins) et avec ses
  réductions, de moitié en moitié jusqu'à 64 px, pour rester fine en petit.
- **Le zoom**, demandé par Stone le 2026-10-28 :

  > « je pens avoir une idée ces images ne sont pas vectoriel ni zoomable fait
  > en sorte qu'on puisse zoomer proprement comme sur mac »

  Comme dans Aperçu : un cran de molette grossit d'un quart autour du point
  visé, qui reste sous la souris ; le pincement du pavé tactile (que Windows
  envoie avec `Ctrl`) aussi ; un double-clic passe de l'image ajustée à sa
  vraie taille et revient ; `+`, `-` et `0` zooment au clavier. Zoomée, elle
  se glisse à la main ou à deux doigts, et ses bords ne se décollent pas de
  ceux de la fenêtre. Le curseur dit ce qu'on peut faire : la loupe, puis la
  main ouverte, puis fermée. Le zoom va jusqu'à huit pixels d'écran par pixel
  de l'image, et ses pixels restent nets : chaque texel garde son carré, seul
  son bord se fond sur un pixel d'écran (`images.wgsl`). Une capture ne devient
  pas vectorielle : on voit ses vrais pixels, pas plus. Zoomée, l'image passe
  sous les capsules ; un voile opaque à leur forme garde leurs textes lisibles,
  le verre ne réfractant que la scène. `--loupe=z[,x,y]` le rend en capture.
- **Les voisines** : `←` `→` parcourent toutes les images de la planche, ou
  toutes celles du fil. `Échap` ferme (et ne quitte plus la vigie quand une
  image est ouverte). `Entrée`, depuis la planche, ouvre le fil de l'agent
  sur cette image.
- **Les réglages de Windows** : « Effets d'animation » coupé, elle s'ouvre et
  se ferme d'un coup.

### Les vidéos, lues dans la vigie

Une vidéo ouverte se lit tout de suite, comme dans Coup d'œil, **dans une
texture de la fenêtre** : ffmpeg ne fait que décoder, en pixels RGBA bruts sur
sa sortie, à la taille d'affichage (1 600 px de large au plus) ; la vigie les
cadence et les affiche. L'horloge est celle du mur : une image en retard est
sautée, jamais montrée tard. Quatre images d'avance au plus, et le décodeur
s'arrête de lui-même quand la file est pleine : c'est ce qui le met en pause.

**Le son** passe par `waveOut`, l'interface audio de Windows elle-même,
appelée sans aucune caisse : ffmpeg rend du PCM 16 bits stéréo à 48 kHz, un fil
le verse dans huit tampons de 100 ms. `cpal` a été essayé d'abord : il tire la
caisse `windows` entière, dont la compilation a fait tomber `rustc`
(dépassement de pile, pendant que quatre autres compilations tournaient).

La capsule des commandes est celle de QuickTime : le signe de lecture et le
temps écoulé, la barre, la durée. `Espace` ou un clic sur la vidéo la lit ou
la suspend ; un clic sur la barre y va ; au bout, elle s'arrête sur sa
dernière image, et la relancer la reprend au début.

### Les sons, lus dans la vigie

Stone, le 2026-10-29, à propos des ateliers sur la voix :

> « j'ai une idée pour viger vu qu'il y a des fois des ateliers sur la voix et
> tout est ce possible de faire comme sur un mackbook et de faire en sorte que
> l'on puisse directement lire la voix genre les documents audio tu comprendras »

Un fichier audio (`mp3`, `wav`, `m4a`, `aac`, `flac`, `ogg`, `opus`) qu'un
agent produit apparaît dans le fil et dans la planche comme une image ou une
vidéo : sa vignette est **son onde**, sa légende « son de 10 s » (jamais une
taille en pixels). Un clic l'ouvre dans la visionneuse, sous le même verre :

- **L'onde en grand**, à 1 600 × 400, sur une plaque sombre : des barres
  claires, centrées, et un filet au milieu pour que le silence se voie comme
  une ligne et non comme un trou. Le plus haut pic vaut la pleine hauteur,
  sauf dans un son presque muet, qu'un plancher d'un dixième de la pleine
  échelle empêche de se lire comme un cri.
- **Il se lit tout de suite**, comme une vidéo, avec la même capsule de
  commandes : lecture et pause (`Espace`), temps écoulé, barre de position,
  durée. La part pas encore entendue de l'onde s'assombrit, et un trait fin
  marque l'instant.
- **Un clic sur l'onde** va à l'endroit du son qu'il touche ; `←` `→`
  parcourent les sons voisins du fil ou de la planche (A, B, C d'un même
  atelier, l'un après l'autre).
- **Aucun décodeur d'images** : un son se lit sans processus ffmpeg d'images ;
  l'horloge et la sortie audio de Windows (`waveOut`, comme le son d'une
  vidéo) font tout. Un mp3 avec une pochette reste un son : c'est l'extension
  qui le dit, pas le flux d'images que ffprobe y trouve.
- **L'en-tête d'un fil** compte les sons à part : « 1 geste, 2 messages, 0 image,
  6 sons ».
- `VIGIE_MUET=1` coupe toute sortie audio : les captures et les essais ne
  jouent rien dans les haut-parleurs (une capture de la visionneuse sur un son
  avait lu une seconde de voix, le 2026-10-29). `--clic-vis=x,y` clique dans la
  visionneuse ouverte, par la même fonction que la souris.

Vérifié sur de vrais enregistrements de voix (six
`.wav` de 4 et 10 s, copiés dans un chantier de test) : les six apparaissent
avec leur onde, l'un s'ouvre, et un clic à 60 % de l'onde amène la lecture à
6,16 s sur 10,24 s. **Non entendu** pendant l'essai : la voix elle-même (la
visionneuse la lit par la sortie audio de Windows, mais l'essai de la lecture
est muet exprès). Le compte de la page d'un chantier (« 25 images ») compte
encore les sons parmi les images.

### Le verre

La matière est celle de veilleuse, arbitrée par l'auteur
(`DECISION.md` du dépôt veilleuse, « Pour l'intégrateur ») et reprise dans
`src/verre.wgsl`, dont les fonctions de forme et de biseau sont celles de
`veilleuse/src/galet.wgsl` à la lettre.

- **Le fond.** La visionneuse ouverte, la vue de la vigie se dessine dans une
  texture au lieu de l'écran. Deux passes de flou gaussien, en linéaire, la
  portent à demi-résolution (comme le givre de veilleuse : le coût divisé par
  huit) avec σ = 18 px à l'échelle de l'écran ; puis la vibrance 1,35 et la
  teinte noire 0,80 posée sur les octets encodés. La vue nette se fond dans ce
  verre à mesure que l'image s'ouvre.
- **Les plaques.** Le nom, le rang, l'aide et les commandes d'une vidéo sont
  posés sur des capsules de verre : un bord en superellipse (exposant 4, le
  coin continu), un congé et une épaisseur de 12 px pour 148 de haut, l'indice
  1,5168 du N-BK7, la réfraction de Snell au biseau, la réflexion de Fresnel
  exacte moyennée sur huit prises, et la lampe d'écran sur le chant, au niveau
  85 (`NIVEAU_DEPLOYE` de veilleuse).
- **Ce qui change de veilleuse.** L'îlot se pose sur le bureau ; une plaque se
  pose ici sur un fond déjà assombri. Sa teinte est donc plus légère (0,62,
  choisie, non mesurée), pour qu'elle se lise comme une lame plus claire. Et sa
  réflexion lit le fond tel qu'il est composé, pas la vue nette : lire la vue
  nette faisait paraître sous la plaque du titre une bande de texte presque
  net, alors qu'autour tout est flou (vu sur la première capture du verre).
  Enfin, Apple anime le rayon du flou à l'ouverture ; ici le flou est fixe et
  c'est son mélange qui monte, une passe de moins par trame.

### Ce que garde le fil

- La transcription garde l'image entière, en base64, dans le résultat de
  l'outil de lecture. La vigie la décode dans son fil de suivi, jamais dans
  celui du rendu : un PNG de deux mégaoctets coûte plusieurs dizaines de
  millisecondes, et une image qui saute est ce qu'on ne veut pas.
- Une image du fil est une **pièce jointe** : sa vignette entière dans une
  boîte de 128 × 72 points, jamais agrandie au-delà de sa taille naturelle, et
  à côté son nom et ce qu'elle est (`produite · 1205 × 360`). Un clic sur la
  ligne l'ouvre dans la visionneuse, qui grandit depuis la vignette.
- Sa texture est faite **à la taille exacte de l'affichage**, un texel par
  pixel : aucun flou d'échantillonnage, aucune moire sur le texte d'une
  capture.
- **Trente images au plus** gardent leurs pixels. Les plus anciennes laissent
  une ligne : trente captures de 1 280 px font déjà plus de cent mégaoctets.

## La barre des projets

Le 2026-10-27, revenu sur les planches du tour 00, Stone : « finalement le C
est pas mal », puis « La console + la barre de C ». La console emprunte à la
direction matériau sa barre latérale, et rien d'autre. Au lancement du
passage B : « je pense qu'une barre que l'on peut enlever comme ici dans
claude est parfaite ».

- **Ce qu'elle porte.** En haut, **Direct**, qui ne défile jamais : c'est le
  retour. Puis le groupe **Projets**, une rangée de 28 points par projet (la
  cible de clic du système), **du plus récemment lancé au plus ancien** ; un
  point dit qu'un de ses chantiers est en cours. Le projet ouvert prend la
  plaque et l'encre-1, les autres sont en encre-3. Trop de projets pour la
  hauteur : la molette sur la barre fait glisser les projets seuls.
- **Sa largeur** est fixe, 176 points, et elle a été tranchée par la planche
  et par le calcul de la mise en page : à la moitié
  d'écran, 188 points ne laissaient que 2 vignettes de front, 176 en laissent
  3, contre 4 barre masquée.
- **Se masquer et revenir, à la main.** Un bouton toujours visible en haut à
  gauche de la fenêtre, le dessin que Claude, le Finder et Mail donnent à ce
  bouton (un cadre et son panneau, plein quand la barre se voit), et
  **`Ctrl` + `B`**. Survolé, le bouton dit à sa droite ce qu'il fait et sa
  touche (« Masquer la barre · Ctrl + B »). La barre masquée, les gestes et les
  images reprennent toute la largeur ; la parole garde sa mesure, 464 points,
  75 signes au plus. Le choix est gardé au souvenir de la fenêtre et se
  retrouve au lancement suivant.
- **Se replier d'elle-même.** Sous 640 points de large, la barre se replie ;
  le bouton ou `Ctrl` + `B` la rappellent **par-dessus** le contenu, qu'elle
  voile, son bord à l'encre des repères, et un clic à côté, `Échap` ou le
  choix d'un projet, à la souris comme par `Ctrl` + chiffre, la renvoient. Ce
  rappel ne touche pas au choix gardé pour une fenêtre large.
- **Pendant le premier parcours**, le groupe se dit « Projets · lecture » :
  la barre se remplit, et elle le dit.

- **Son mouvement** est le ressort du défilement (amortissement critique,
  interruptible). Le contenu se remet en page une fois, à sa largeur
  d'arrivée, et le ressort ne fait que le translater : remettre mille blocs en
  page à chaque trame ne se paierait pas. Sous le mouvement réduit, tout se
  pose d'un coup.

**Pourquoi `Ctrl` + `B`.** C'est la touche de Visual Studio Code pour sa
barre latérale, que la main de Stone connaît ; et un rapport d'utilisateur
de l'application Claude de bureau la lui attribue aussi (« the pinned state
(Ctrl+B) », <https://claudeissues.com/issue/75523-desktop-expose-a-persistent-keep-sidebar-open-setting-the-pinned-state-ctrl-b-is>,
consulté le 2026-10-27 de seconde main ; Anthropic ne publie pas la liste,
donc **non vérifié** à la source). `B` est à la même place en AZERTY et en
QWERTY, et la touche est lue par sa position physique, comme les chiffres.
macOS dit `Ctrl` + `Cmd` + `S` pour « Afficher la barre latérale », une touche
que Windows n'a pas.

### Le projet d'un chantier

Le dossier où la session a été lancée ne dit pas le projet : une session
ouverte dans le dossier personnel range sous lui des chantiers qui
travaillent dans d'autres dépôts. Le vrai lieu est le champ
`cwd` des transcriptions d'agent, et il varie d'un agent à l'autre dans un
même chantier. La règle :

1. On lit la première ligne qui porte un `cwd` dans **douze agents au plus**,
   répartis sur la liste. Mesuré le 2026-10-03 sur les 139 chantiers présents,
   contre le vote de tous leurs agents : douze donnent le même projet partout,
   huit se trompent une fois, cinq trois fois, un seul quatorze fois. Lire
   tous les agents coûterait 5 455 ouvertures de fichier.
2. Les deux écritures d'un chemin (`C:\Users\…` et `/c/Users/…`) se
   confondent.
3. Le dossier personnel, `AppData` (les brouillons temporaires) et `.claude`
   ne votent pas : ce sont des endroits d'où l'on part, pas des projets.
4. Chaque lieu est ramené au dépôt git qui le contient :
   `veilleuse\recherche\verre-taille` compte pour `veilleuse`.
5. Le lieu le plus fréquent l'emporte ; à égalité, celui de l'agent le plus
   ancien, qui a ouvert le travail. Sans aucun lieu, le chantier va au
   **dossier personnel**. Le Bureau s'appelle « Bureau ».

### L'état d'un chantier

Un chantier terminé laisse un bilan, `<session>/workflows/wf_<id>.json`. La
règle :

| | quand |
|---|---|
| **fini** | le bilan existe et son statut est `completed` |
| **interrompu** | le bilan existe avec un autre statut (`killed`), ou il n'existe pas et rien n'a été écrit dans le dossier depuis trente minutes |
| **en cours** | pas de bilan, et une écriture depuis moins de trente minutes |

Trente minutes, parce qu'un outil au premier plan peut tenir dix minutes sans
rien écrire et qu'une longue réponse en prend encore quelques-unes : à quinze,
on déclarerait interrompu un chantier vivant. Le prix est connu : une session
morte reste « en cours » une demi-heure de trop.

Le **nom** vient du bilan, sinon de l'en-tête `export const meta = { name: … }`
du script (qui peut vivre sous un autre dossier de projet que les
transcriptions), sinon du nom du fichier du script, sinon de l'identifiant.
La **description** vient du bilan ou du script ; sans elle, la ligne montre
l'identifiant. L'**heure** est celle du lancement, la création du dossier du
chantier ; la dernière écriture ne sert qu'à l'état. Le **nombre d'agents** est celui des
transcriptions sur le disque : il compte les relances, ce que le bilan ne fait
pas (29 pour un chantier relevé, contre 24 dans son bilan).

### La liste d'un projet

Elle reprend la grille du fil : l'heure dans la gouttière, le nom dans la
colonne, la description en dessous, l'état et le nombre d'agents calés à
droite. Les chantiers vont **du plus récemment lancé au plus ancien**, pour la
même raison que les onglets : une liste qui se réordonne pendant qu'on la
lit ne se lit pas. Ils sont rangés sous des
titres de jour — « Aujourd’hui », « Hier », « Vendredi 25 septembre » — comme
Mail range son courrier : cent chantiers en vrac ne se lisent pas, cent
chantiers datés se parcourent d'un coup d'œil. Le chantier suivi porte le
rail, comme la parole dans le fil : un seul signe pour « c'est celui-là ».

Un clic sur une ligne **suit ce chantier** : la vigie s'attache à l'agent qui
bouge dans ce chantier, avec la même reprise de vingt entrées que le direct.
Le fil affiché se vide et repart ; ce que le fil de suivi envoyait encore
pour l'ancien choix est écarté (`suivi::garder`). L'en-tête dit qui parle, et
dans quel chantier : `<chantier> · fini`.

La souris agit au **relâchement**, et seulement sur ce qui était sous elle à
l'appui : on peut se raviser en glissant hors de la cible. La ligne survolée
prend une plaque du gris des filets, et la main remplace la flèche au-dessus
de ce qui se clique. Un second clic sur l'onglet actif remonte en haut de la
liste.

### Le clavier

| touche | effet | pourquoi celle-là |
|---|---|---|
| `Début` | revient au Direct, d'où que l'on soit | le Direct est la première rangée, le « début » ; la touche ne servait à rien |
| `←` `→` | projet précédent, suivant dans la barre | la convention des barres d'onglets, gardée |
| `↑` `↓` | ligne précédente, suivante dans une liste ou dans les agents d'une page | la ligne choisie se voit toujours |
| `Entrée` | ouvre la ligne en évidence : la page d'un chantier, le fil d'un agent | |
| `Retour arrière` | remonte d'un niveau : du fil à la page du chantier, de la page à la liste du projet, la ligne qu'on quitte en évidence. Depuis le direct, ouvre la page du chantier en cours | on revient là d'où l'on est parti, comme le Finder |
| `Ctrl` + `1`…`8` | la rangée de ce rang ; `Ctrl` + `9`, la dernière | la convention des navigateurs sous Windows |
| `Ctrl` + `Tab` | projet suivant ; avec `Maj`, précédent | idem |
| `Ctrl` + `B` | masque ou réaffiche la barre ; en fenêtre étroite, la rappelle | la barre latérale de VS Code, et de Claude (plus haut) |
| `T` | thème suivant : sombre, clair, celui de Windows | le choix est gardé au souvenir |

Les chiffres sont lus par la **touche physique** : sur ce poste, le premier
clavier est l'AZERTY français, où la rangée du haut donne `& é "` sans
majuscule. Sans `Ctrl`, ils resteraient en conflit avec le zoom (`-` est sur
la touche du 6). `Échap` quitte, comme avant : changer son sens surprendrait
la main ; seules une image ouverte et une barre rappelée le prennent d'abord
pour se fermer.

### Le catalogue

Un fil à part dresse le catalogue toutes les **trois secondes**, comme le
dernier chantier du direct. Rien n'est relu d'un chantier terminé une fois
lu : la première ligne d'un agent, la racine d'un chemin, un bilan et un
en-tête de script ne changent plus. Le premier parcours lit les chantiers du
plus récent au plus ancien et envoie une liste partielle toutes les
demi-secondes. Mesuré le 2026-10-03 : **54 s** à froid (fichiers hors du cache
du système, pendant que d'autres chantiers compilaient), **0,4 s** à chaud,
pour 140 chantiers et 1 250 agents lus.

## Ce qui se règle, et pourquoi

- **Molette**, `Page haut` / `Page bas` pour remonter. `Fin` pour recoller au
  direct. **`Ctrl` + molette**, ou `+` et `-`, pour zoomer ; `0` revient à
  l'échelle du système.
- La colonne est **centrée**, jamais collée à gauche : sur un écran large, un
  texte aligné à gauche laisse un vide à droite qui n'a aucun sens. Elle ne se
  décentre que sur une fenêtre étroite, où centrer reviendrait à mordre le bord.
- **Le texte garde sa mesure, le reste prend la place.** Le 2026-10-27, Stone,
  devant une fenêtre agrandie : « dans vigie on a beaucoup d'application vide
  je pense que tu peux genre utiliser la logique d'ultra belle application
  pour faire un bel équilibre ». Sur ses 1 535 px, la colonne de 760 points
  laissait 570 px de vide. Désormais un **bloc large**, centré, de 1 180 points
  au plus, porte la bande d'onglets, l'en-tête, les listes et la page d'un
  chantier ; le fil seul garde sa colonne de lecture, centrée dans le bloc,
  parce qu'au-delà de 78 signes une ligne se lit mal. La planche prend autant
  de colonnes que la place en donne, une vignette faisant au moins 240 points :
  4 sur la fenêtre agrandie, 3 dans la fenêtre de départ, 2 en fenêtre étroite.
  Tout s'aligne sur le même bord gauche, et rien ne
  saute quand on passe d'une vue à l'autre.
- **L'échelle du système est appliquée.** Sans elle tout paraît minuscule sur
  un écran dense, et l'on croit l'application mal faite.
- Tant qu'on n'est pas revenu au bas, l'arrivée d'un bloc **ne fait plus sauter
  la page** — sinon lire un passage ancien serait impossible.
- Le défilement est un **ressort à amortissement critique**, intégré par sa
  solution exacte. Un coup de molette pendant le mouvement change la cible et
  rien d'autre : la position et la vitesse restent, donc il n'y a ni reprise à
  zéro ni saut.
- **Une commande longue est coupée à une ligne.** Dans la première version, une
  commande occupait six lignes contre quatre pour un raisonnement entier. Le
  détail complet vit de toute façon dans l'archive de raisonnement.

## Les drapeaux

| | |
|---|---|
| `--capture=chemin.png` | rend, écrit, et quitte. Sans lui, aucun relecteur ne peut juger sans ouvrir la fenêtre à la main |
| `--essai` | pose des signes connus — grec, mathématiques, ponctuation — et une mire d'image, et écarte le flux vivant |
| `--onglet=<projet>` | ouvre la liste de ce projet (la casse ne compte pas). Un nom inconnu montre l'état vide |
| `--chantier=wf_<id>` | ouvre la page de ce chantier. L'emporte sur `--onglet` |
| `--agent=<rôle>` | avec `--chantier`, suit cet agent depuis son début. Le rôle (`chercheur-forme`) ou l'identifiant |
| `--defiler=px` | fait descendre la vue de tant de pixels avant la capture, pour juger le bas d'une page |
| `--attente=s` | le temps laissé au fil après un clic rejoué, avant la capture ; 2,5 s par défaut, il en faut plus pour un agent relu depuis son début |
| `--clic=x,y` | rejoue un clic à ce point (en pixels de la capture), une fois le catalogue entier, par les mêmes fonctions que la souris |
| `--touches=bas,bas,entree,retour` | rejoue des touches par la même fonction que le clavier : `debut`, `retour`, `gauche`, `droite`, `haut`, `bas`, `entree`, et `1`…`9` pour `Ctrl` + chiffre |
| `--survol=x,y` | pose la souris là pour la capture, pour juger la plaque et le survol |
| `--echelle=f` | remplace le facteur d'affichage du système (1, 1,25, 1,5, 2) : un seul écran suffit à juger chaque échelle |
| `--taille=LxH` | la taille de la fenêtre en points, 1000 × 760 par défaut ; 480 × 760 pour une fenêtre étroite |
| `--instant=s` | capture à s secondes sans attendre le catalogue ni la fiche : un premier lancement, une page en lecture |
| `--theme=sombre\|clair\|systeme` | impose le thème |
| `--viser=nom.png` | avec `--agent`, place le fil sur cette image, comme un clic sur sa vignette |
| `--ouvrir` | avec `--viser`, ouvre cette image dans la visionneuse et attend son image pleine |
| `--barre=masquee\|visible` | impose le choix de la barre que le souvenir donnerait |
| `--mouvement-reduit`, `--contraste-eleve` | imposent les deux réglages de Windows pour une capture, sans toucher à ceux du poste |
| `VIGIE_MAINTENANT=<s>` | variable : fige l'heure de la vigie (secondes Unix), pour qu'une copie figée reste « aujourd'hui » et « en cours » |
| `VIGIE_DONNEES=<dossier>` | variable : lit les chantiers sous `<dossier>\projects` et garde la mémoire sous `<dossier>\vigie`, jamais dans `%APPDATA%\Vigie`, qui est celui de la vigie installée |
| `VIGIE_PERDRE=<s>` | variable : détruit le périphérique graphique à s secondes, une fois, dans la première vigie seulement ; le témoin de la relance (ci-dessous) |

Les quatre derniers avant `VIGIE_PERDRE` datent du 2026-10-27 (tour de design 00). Leur essai : à `--echelle=1.5
--taille=640x400`, la capture doit faire 960 × 600 et annoncer l'échelle 1,50 ;
son témoin, la même capture sans `--echelle`, tombe sur ce poste à 125 %
(800 × 500). Avant `VIGIE_DONNEES`, toute capture écrivait le catalogue dans
`%APPDATA%\Vigie`, la mémoire partagée avec la vigie installée.

**Le périphérique perdu.** Une carte graphique peut se perdre sous la vigie :
réinitialisée par le pilote, ou à court de mémoire quand d'autres programmes
la prennent (wgpu-core traite un manque de mémoire comme une perte). Jusqu'au
2026-10-27, le premier tampon créé ensuite naissait invalide et la vigie
paniquait (`Buffer with 'ecran' label is invalid`).
Elle se relance désormais elle-même, avec les mêmes drapeaux, trois fois au
plus (`relancer`, dans `main.rs`) ; une capture attend la sienne et rend son
code ; la fenêtre, qui garde son souvenir en sortant, le relit en se
relançant (vu par le code et sur une capture, pas dans une vraie fenêtre). Le
journal dit la carte au
lancement, la perte et sa raison, puis la relance. Le témoin :
`VIGIE_PERDRE=1.5` sur une capture du fil doit écrire la capture et
« relance 1 sur 3 » ; quand le rappel de perte ne lève plus le drapeau, la
même capture panique sur un tampon né invalide, comme avant la
correction.

Avec `--capture`, la vigie attend le catalogue entier (une minute et demie au
plus) avant de juger, la fiche d'une page de chantier, et `--attente` de plus
après un clic rejoué, pour que le fil se remplisse. Un fil attend d'être lu
jusqu'au bout de ce que son fichier contenait à l'ouverture (une minute et
demie au plus), puis une seconde de calme ; le journal le dit (« fil lu, à
3,0 s »). La fenêtre d'une capture ne prend pas le premier plan, et toute
entrée réelle (touche, clic, molette, souris) y est ignorée et notée au
journal (« entree reelle ignoree ») : une touche tapée ailleurs ramenait la
capture du fil à la page du chantier, sans trace. Les captures d'essai ne
sont pas dans le dépôt (`.gitignore` les laisse dehors) : elles montrent la
pensée réelle des agents.

`--essai` est le **témoin du recours de police**. Barlow n'a ni grec ni
symboles, et la pensée d'un agent qui fait de l'optique est pleine de θ et de
φ. Sans ce témoin, il faudrait attendre qu'un agent en écrive pour découvrir
qu'ils rendaient un carré vide. Il en a trouvé un : le nabla manquait aussi à
Segoe, d'où le troisième recours.

La **mire** est le témoin des images : deux rampes, rouge puis verte, et un
damier, dont chaque pixel est connu. Au milieu d'une rampe, la capture doit
lire 127. Le témoin a été prouvé capable de tomber le 2026-10-03 : en
déclarant la texture non sRGB, la conversion gamma est appliquée deux fois et
la capture lit 187 (la théorie prévoit ~186) ; restaurée, elle relit 127.

Le chemin de lecture a ses propres essais (`cargo test --release`). Le premier
reprend exactement la forme relevée dans une transcription. Le second,
`une_vraie_ligne_de_transcription`, se lance sur une vraie ligne extraite
dans un fichier :

```
VIGIE_LIGNE=chemin cargo test --release -- --ignored
```

Le 2026-10-03, il a décodé une capture de 400 × 410 ouverte par un agent.

Les onglets ont les leurs, chacun sur un cas dont la réponse vient d'ailleurs
que du code testé :

| essai | ce qu'il garde |
|---|---|
| `plan::essais` | ce qui est sous la souris : un onglet, rien entre deux, rien hors de la bande, « Direct » au-dessus d'un onglet qui glisse dessous, aucune ligne cachée sous l'en-tête ; et le défilement qui montre une ligne en bougeant le moins possible |
| `dates::essais` | le calendrier (réponses du module `datetime` de Python), les jours nommés à la française, l'année seulement quand elle change |
| `catalogue::essais` | les deux écritures d'un chemin, les non-lieux, le dépôt qui l'emporte sur le dossier personnel (la forme relevée sur un vrai dépôt), l'égalité, la règle d'état à 29 et 31 minutes, l'en-tête exact d'un vrai script, le tri des projets |
| `suivi::essais` | qu'un choix abandonné ne laisse rien passer, même deux choix coup sur coup ; qu'une image écrite pendant un geste entre dans le fil, et qu'une image listée par un `ls` à un autre moment n'y entre pas |
| `medias::essais` | les noms lus dans les vraies commandes et sorties du chantier de design (`ffmpeg … planche.png`, un `ls -la`, un `find`), le dossier d'un `cd`, la règle de production dans ses deux cas et ce qu'elle exclut, le signe de lecture visible sur du noir |
| `chantier::essais` | les trois formes de bilan : un texte, un objet à verdict, un objet qui n'est qu'une liste ; une image listée par un agent et écrite par un autre va à celui qui l'a écrite |
| `medias::essais` (suite) | un nom à blancs entre guillemets et dans une sortie, la variante la plus longue d'abord, et le disque qui tranche entre `capture 1.png` et `1.png` |
| `dates::essais` (suite) | l'horodatage d'une transcription en instant (réponses de `datetime.fromisoformat`) |
| `plan::essais` (suite) | une vignette et une image du fil se touchent sous l'en-tête, pas au-dessus |
| `vues::essais_barre` | la cible de la barre dans les quatre cas (large ou étroite, masquée ou rappelée) ; le mouvement réduit qui la pose en une trame quand le ressort n'en fait qu'une partie ; les rangées de 28 points sous le bouton et sous « Projets », et rien à l'écran barre rentrée |
| `app::essais` (suite) | la barre masquée et la barre visible se retrouvent telles quelles au souvenir |
| `palette::essais` | chaque encre tient son seuil sur son fond dans les deux thèmes (7 pour encre-1, 4,5 pour les autres et l'alerte) ; le creux du souffle de la pastille tient 3:1, refait octet par octet |
| `suivi::essais` (suite) | la parole se lit sans ses accents graves ni ses astérisques d'emphase |
| `chantier::essais` (suite) | une image recopiée sous un autre nom ne compte qu'une fois sur la planche |
| `vues::essais_coupe` | un chemin trop long garde son dossier et son nom ; un mot sans chemin, une adresse web et un chemin court ne bougent pas |
| `catalogue::essais` (suite) | un chantier en cours qui se tait dix minutes se dit « muet depuis 10 min » et perd sa pastille ; à 9 min 59 s il est encore en cours ; un chantier fini n'est jamais muet |

Les quatre essais de la barre ont été prouvés capables de tomber le
2026-10-27 par sabotage (l'étroite
ignorée, le pas du mouvement réduit ignoré, « Direct » posé sous le bouton, le
souvenir qui oublie le champ) : chacun tombe saboté et repasse rendu. Le juge
technique les a refait tomber par d'autres sabotages, avec les deux essais
venus de la console que ce registre ne citait pas (`les_encres_tiennent_leur_seuil`,
`la_parole_se_lit_sans_balisage`), par un script
qui ne compte une chute que si cargo dit `FAILED` pour cet essai. Les trois
essais du correcteur (la copie, le creux du souffle, les chemins) tombent de
même, et celui de sa seconde passe (le muet), avec la relance, aussi.


Chacun a été **prouvé capable de tomber** le 2026-10-03 : quatorze sabotages
de sa condition (la bande ignorée, la zone ignorée, le calendrier décalé d'un
jour, les non-lieux qui votent, l'égalité inversée, quinze minutes au lieu de
trente, l'écriture Git Bash oubliée, une clé lue au milieu d'un mot, le tri
inversé, le filtre des choix ouvert…), chacun fait tomber son essai, et chacun
restauré le fait repasser. Un seul a d'abord tenu debout saboté : l'essai de
l'en-tête de script prétendait garder « name » de « workflowName », mais la
majuscule de « Name » rendait le piège inopérant. Il pose désormais
« filename », et tombe.

Le chemin complet, de la souris et du clavier jusqu'au fil, a ses témoins en
capture : `--onglet=<projet> --clic=400,365` touche la ligne d'un
chantier fini (les coordonnées suivent la liste du moment) et rend son fil
(« contre:07 », `<chantier> · fini`) ; `--touches=bas,bas,entree,retour`
suit le même chantier au clavier puis revient à la liste, son rail posé.

Le clavier a aussi été vu **dans une vraie fenêtre**, le 2026-10-03 : des
touches postées à la fenêtre d'essai (`target\release\vigie.exe`, jamais
celle de `dist`), photographiée à chaque pas par `PrintWindow`. ↓ ↓ met
ce chantier en évidence, Entrée le suit, Retour arrière revient à la
liste, → passe à l'onglet suivant, Début revient au Direct — et chaque pas se
redessine sans attendre de bloc. C'était l'avant-dernier état du code, avant
le tri par lancement, qui ne touche pas au clavier ; refait sur le dernier, le
même essai n'a pas pu photographier la fenêtre (zone client de 0 × 0, sans
doute réduite ou la session verrouillée). La souris ne se rejoue pas ainsi : winit suit
la sortie du curseur, et le vrai curseur n'étant pas sur la fenêtre, un
mouvement posté est aussitôt suivi d'une sortie.

La base de polices est **vide au départ**, et chaque police est chargée
nommément. Aucune ne peut donc se substituer à Barlow sans qu'on l'ait voulu.
L'absence d'un recours n'est pas une panne : on perd le grec, pas le texte.

## Ce qui a été vu, le 2026-10-27

Sur trois chantiers du jour, par `--capture` (le même chemin de dessin que la
fenêtre) :

- `design-de-l-application` (en cours, 5 agents) : sa page, 24
  images dont les vidéos manim et Wolfram ; un clic rejoué sur la vignette de
  `wolfram.mp4` ouvre le fil de `chercheur-forme` placé sur elle ; le fil de
  `chercheur-direction-artistique` montre `ArdoiseVirgule_pendant.png ·
  produite · 960 × 540` après la commande qui l'a rendue.
- `apprendre-au-sommet` (7 agents) et un chantier de figures
  (7 agents, 146 images, bilans structurés).

Et dans une **vraie fenêtre**, lancée sans drapeau comme le raccourci, des
touches postées et la fenêtre photographiée par `PrintWindow` à chaque pas :
le direct suit `verificateur-adverse` ;
Retour arrière ouvre la page d'`apprendre-au-sommet` ; ↓ ↓ Entrée suit
`chercheur-references-du-sujet` depuis son début ; Retour arrière revient à la
page, sa ligne en évidence et son rail posé. Les photos sont rognées à droite :
PowerShell n'est pas déclaré sensible à l'échelle, et `GetClientRect` lui rend
la taille à 100 %.

Puis à la **vraie souris**, dans la fenêtre du raccourci (`dist\Vigie.exe`) :
un clic sur l'onglet du projet, un clic sur `design-de-l-application`, la molette sur la
planche, un clic sur la vignette `Ardoise_milieu.png` qui ouvre le fil de
`chercheur-direction-artistique` placé sur elle, un clic sur l'image qui
l'ouvre dans Photos (960 × 540), un clic sur le titre qui revient à la page,
rail posé, et un clic sur `wolfram.mp4` qui la joue dans le Lecteur
multimédia.

### Ce que la vraie souris a trouvé

- **La liste ne défilait pas.** Depuis le premier commit, la trame n'avançait
  que le ressort du fil. Celui de la liste et celui de la bande recevaient
  leur cible sans jamais bouger : ni la molette, ni Page bas, ni les flèches
  ne faisaient défiler une liste plus haute que la fenêtre. Et comme ces deux
  ressorts n'étaient jamais au repos, la fenêtre se redessinait sans fin : la
  page de chantier ouverte et immobile prenait **2 953 ms** de processeur en
  10 s. Corrigé, la planche défile, et la même page en prend **375 ms**, ce
  qui reste est la relecture de la fiche et du catalogue. Les captures le
  cachaient : `--capture` pose chaque ressort sur sa cible avant de dessiner.
- **Une fenêtre restaurée restait blanche** jusqu'au bloc suivant, donc pour
  toujours sur un chantier fini. La surface perdue pendant la réduction se
  reconfigurait sans redemander de trame. Elle la redemande, et un
  redimensionnement ou une fenêtre redevenue visible redessinent aussitôt :
  restaurée sur un chantier fini, elle s'affiche tout de suite.

Puis l'**application installée**, le même jour :
deux lancements de suite ne laissent qu'un processus ; fermée sur la page de
`design-de-l-application`, elle s'y rouvre, à la même place et à la même
taille.

## Ce qui n'est pas fait

- **La barre n'a pas été vue dans une vraie fenêtre ni à la vraie souris.**
  Tout ce qui en est dit est rendu par `--capture`, qui pose chaque ressort
  sur sa cible : son mouvement, le bouton sous le vrai curseur et `Ctrl` + `B`
  au vrai clavier ne sont pas vus.

- **La visionneuse n'a pas été jugée par un tiers.** Elle a été vue
  sur des captures (une image et une vidéo ouvertes depuis la planche,
  rendues par `--capture`, et la vraie fenêtre, où une vidéo se lisait déjà
  quand l'épreuve à la souris a commencé : quelqu'un s'en servait, et
  l'épreuve s'est arrêtée là pour ne pas lui prendre la main). Le son n'a été
  entendu par personne ; l'essai de la vidéo réelle est muet exprès, pour ne
  pas jouer un la à 440 Hz dans les haut-parleurs de la machine. La teinte des
  plaques (0,62) n'est pas mesurée.

- **Un décalage de la bande d'onglets vu une fois.** À la toute première
  ouverture de la version installée, sans état gardé, la bande s'est montrée
  glissée de quelques onglets vers la droite sans qu'on y touche. Ni la
  capture du même état, ni trois relances, ni un double lancement ne l'ont
  reproduit. Rien ne l'explique encore ; c'est noté pour qu'un prochain
  passage le reconnaisse.

- **Un nom de plus de quatre mots** sans guillemets se lit par sa fin
  seulement, et deux variantes qui existent toutes deux (`capture 1.png` et
  `1.png` dans le même dossier) se départagent par la plus longue, qui n'est
  pas forcément celle que la commande voulait dire.
- ~~**Les vidéos ne se lisent pas dans la vigie.** Elles s'ouvrent dans le
  lecteur du système ; la vigie n'en montre que l'image du milieu.~~ Faux
  depuis la visionneuse (plus haut) : elles se lisent dans la vigie, son
  compris (barré le 2026-10-27 par le passage B du tour 00).
- **Les textures des vignettes ne sont pas rendues** quand on fait défiler une
  longue planche : 146 vignettes à l'écran une fois chacune tiennent environ
  trente mégaoctets de carte, ce qui reste loin d'une limite.
- **Le projet se devine, il ne se lit pas.** Un chantier dont aucun agent
  n'est descendu dans un dépôt va au dossier personnel, comme un chantier
  lancé depuis `.claude\projects`. Deux dépôts de
  même nom feraient un seul onglet.
- **Le catalogue n'est pas gardé d'un lancement à l'autre.** Le premier
  parcours se refait à chaque démarrage ; à froid, il a pris 54 s, pendant
  lesquelles les chantiers arrivent du plus récent au plus ancien.
- **Rien ne se cherche.** Pas de filtre, pas de recherche dans l'historique ni
  dans la liste des chantiers.
- **Non vérifié à la main.** Les clics réels sur les onglets, les lignes, les
  vignettes, les images et le titre, et la molette sur une page, ont été vus
  le 2026-10-27. La molette sur la bande d'onglets et la main au survol ne
  l'ont pas été. `Ctrl` + chiffre et `Ctrl` + `Tab` non
  plus (l'état de `Ctrl` ne se poste pas), ni la bande après un zoom ou un
  redimensionnement.
- **Mille blocs au plus.** Les plus anciens sortent. C'est plus que ce qu'on
  relit, mais ce n'est pas une archive — l'archive, c'est
  `outils/raisonnement.py`, dans le dépôt veilleuse.
- **Une image que rien ne nomme ne se voit pas.** Un script qui écrit vingt
  fichiers sans les imprimer n'en laisse aucun nom dans la transcription.
- **La réflexion des agents est en anglais**, alors qu'ils écrivent leurs
  documents en français. C'est leur pensée interne, et on n'a pas prise dessus.
