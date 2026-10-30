//! Vigie — regarder penser les agents, en direct.
//!
//! Le panneau des chantiers dit OU en est le travail. Il ne dit pas ce que
//! l'agent pense, et c'est pourtant la que se trouve ce qui s'apprend : les
//! hypotheses ecartees, la mesure qui contredit une attente, le moment ou
//! quelqu'un se corrige.
//!
//! Trois registres, et ils ne se lisent pas de la meme facon. Ce qu'il PENSE,
//! d'une voix plus basse parce que c'est l'interieur. Ce qu'il DIT, qui porte
//! parce que c'est adresse au dehors et finira dans son rapport. Ce qu'il
//! FAIT, en une ligne qu'on survole. Aucune etiquette ne les nomme : la
//! couleur et la graisse suffisent, et une etiquette repetee mille fois
//! devient du bruit.
//!
//! Rien ne se configure. La vigie prend le chantier le plus recent, s'attache
//! a l'agent qui ecrit, et bascule toute seule quand un autre prend la main.
//! Quand plusieurs chantiers tournent dans plusieurs projets, une bande
//! d'onglets, un par projet, permet d'en choisir un ; sa page nomme ses
//! agents et pose toutes ses images, et l'on y choisit l'agent a suivre.
//! L'onglet « Direct » garde le comportement d'origine.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod catalogue;
mod chantier;
mod import;
mod dates;
mod formes;
mod images;
mod medias;
mod palette;
mod passage;
mod feuille;
mod mandataire;
mod navigateur;
mod ouverture;
mod reseau;
mod plan;
mod ressort;
mod suivi;
mod verre;
mod video;
mod visionneuse;
mod vues;

use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Instant;

use glyphon::{
    Attrs, Buffer, Cache, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache, TextArea,
    TextAtlas, TextBounds, TextRenderer, Viewport, Weight,
};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{CursorIcon, Window, WindowId};

use catalogue::Catalogue;
use plan::Cible;
use suivi::{Bloc, Choix, Nouvelle, Registre};
use vues::Vue;

const BARLOW_400: &[u8] = include_bytes!("../assets/fonts/Barlow-Regular.ttf");
const BARLOW_500: &[u8] = include_bytes!("../assets/fonts/Barlow-Medium.ttf");

/// Le corps du texte, et son interligne. 1,55 parce que ces blocs sont des
/// paragraphes lus en continu, pas des etiquettes.
const CORPS: f32 = 15.0;
const INTERLIGNE: f32 = 23.0;

/// La colonne du fil : les gestes, sur une ligne chacun, et les pieces
/// jointes. Elle peut aller jusqu'a 760 points, ou une commande se lit plus
/// loin avant d'etre coupee.
const COLONNE_MAX: f32 = 760.0;
/// La mesure de ce qui se lit en continu : la parole, la pensee. 45 a 75
/// signes par ligne (Bringhurst, §2.1.2) ; Barlow avance de 0,4123 a 0,4208 em
/// par signe (systeme.md §2), et 464 points font 75 signes de 15 points au
/// plus. Les 760 points de la colonne en faisaient 120, et masquer la barre
/// portait la parole de 59 a 89 signes en moitie d'ecran (juge-design.md, M3) :
/// la place que la barre rend va aux gestes et aux images, pas a la ligne.
const MESURE: f32 = 464.0;
/// Le bloc large : la bande, l'en-tete, les listes et la page d'un chantier.
/// La lecture continue garde sa colonne de 760 ; ce qui se parcourt (une
/// liste, une planche d'images) prend la place, comme dans Photos ou Mail,
/// jusqu'a cette largeur. Sur une fenetre agrandie de ce poste (1 535 px a
/// 1,25), la colonne seule laissait 570 px de vide (vu le 2026-10-27).
const BLOC_MAX: f32 = 1180.0;
/// La gouttiere des heures, a gauche de la colonne.
const GOUTTIERE: f32 = 76.0;
/// Ce qui separe la gouttiere de la colonne.
const ECART_GOUTTIERE: f32 = 20.0;
/// La marge exterieure.
const MARGE: f32 = 34.0;
/// La bande d'en-tete : celle des onglets (vues::ONGLETS_H, 40), puis
/// l'en-tete d'origine (82), qui dit qui parle et dans quel chantier.
const ENTETE_H: f32 = vues::ONGLETS_H + 82.0;

/// Le corps du lieu d'un fil dans l'en-tete (`poser_entete`).
const LIEU_CORPS: f32 = 20.0;
/// L'air entre deux blocs.
const AIR: f32 = 14.0;

/// Au-dela, les plus vieux blocs sortent. Mille paragraphes font deja plus de
/// texte qu'on n'en relit, et chaque bloc garde sa mise en page en memoire.
const BLOCS_MAX: usize = 1000;

/// Les images gardees avec leurs pixels. Au-dela, les plus anciennes rendent
/// leurs pixels et ne laissent qu'une ligne : trente captures de 1280 px font
/// deja plus de cent megaoctets.
const IMAGES_GARDEES: usize = 30;
/// Le rayon des coins d'une image, a echelle 1. Assez pour la detacher du fond,
/// pas assez pour rogner ce qui est dans ses coins.
const IMAGE_RAYON: f32 = 6.0;
/// La legende sous une image : un cran sous les gestes, qu'elle accompagne.
const LEGENDE_CORPS: f32 = 13.0;
const LEGENDE_H: f32 = 19.0;
/// Au-dela, l'arrivee d'un fil entier se repartit sur plusieurs trames : un
/// agent choisi se relit depuis son debut, et mettre en page six cents blocs
/// d'un coup gelerait la fenetre le temps d'un clin d'oeil de trop.
const BLOCS_PAR_TRAME: usize = 120;

/// Les touches de navigation. Debut revient au direct d'ou que l'on soit ;
/// Retour arriere remonte d'un niveau, du fil a la page du chantier et de la
/// page a la liste du projet ; les
/// fleches gauche et droite passent d'un onglet a l'autre, haut et bas d'une
/// ligne a l'autre ; Entree suit la ligne en evidence ; Ctrl + chiffre ouvre
/// l'onglet de ce rang. Le choix et ses raisons sont dans LISEZ-MOI.md.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Touche {
    Debut,
    Retour,
    Gauche,
    Droite,
    Haut,
    Bas,
    Entree,
    /// Ctrl + chiffre, de 1 a 9.
    Rang(usize),
    /// Ctrl + B : masque ou reaffiche la barre laterale.
    Barre,
    /// Echap : ferme la feuille du passage, puis la page ; jamais la vigie
    /// tant qu'une feuille est ouverte.
    Echap,
    /// Tab et Maj + Tab : d'un lien du fil au suivant, au precedent.
    Tab,
    MajTab,
}

impl Touche {
    /// Le nom d'une touche dans --touches=.
    fn nommee(nom: &str) -> Option<Touche> {
        Some(match nom.trim() {
            "debut" => Touche::Debut,
            "retour" => Touche::Retour,
            "gauche" => Touche::Gauche,
            "droite" => Touche::Droite,
            "haut" => Touche::Haut,
            "bas" => Touche::Bas,
            "entree" => Touche::Entree,
            "barre" => Touche::Barre,
            "echap" => Touche::Echap,
            "tab" => Touche::Tab,
            "maj-tab" => Touche::MajTab,
            c => match c.parse::<usize>() {
                Ok(r) if (1..=9).contains(&r) => Touche::Rang(r),
                _ => return None,
            },
        })
    }
}

/// Les mesures d'une trame : tout ce qui depend de la taille de la fenetre et
/// de l'echelle. Les constantes ci-dessus sont des mesures NOMINALES, a
/// echelle 1 ; rien dans le rendu ne doit les lire directement.
struct Mesures {
    corps: f32,
    interligne: f32,
    colonne: f32,
    marge: f32,
    entete_h: f32,
    air: f32,
    /// Abscisse de la gouttiere des heures. Le bloc entier est CENTRE : une
    /// colonne collee a gauche laisse un vide a droite qui n'a aucun sens.
    x_heure: f32,
    /// Abscisse de la colonne de lecture.
    x_corps: f32,
    /// Le bloc large, centre : son bord gauche et sa largeur. La bande,
    /// l'en-tete et les listes s'y alignent, et le fil se centre dedans.
    x_bloc: f32,
    large_bloc: f32,
    /// La colonne d'une liste ou d'une page, dans le bloc large.
    liste_corps: f32,
    liste_colonne: f32,
}

/// Ce que la visionneuse pose par-dessus le verre, pour une trame.
#[derive(Default)]
struct Dessus {
    t: f32,
    plaques: Vec<verre::Plaque>,
    aplats: Vec<formes::Forme>,
    image: Option<[f32; 4]>,
    /// Sur une image zoomee, qui passe sous les plaques : un voile sombre a
    /// leur forme, opaque, pose sur l'image : le melange se fait en lineaire,
    /// ou 4 % de blanc ressortent a 24 % de gris, et le texte d'une page blanche
    /// se lisait a travers l'aide sous un voile a 0,96.
    /// Le verre ne refracte que la scene, pas l'image posee par-dessus.
    voiles: Vec<formes::Forme>,
    pret: bool,
}

/// La console : un geste tient UNE ligne, au corps des reperes, sans air
/// entre deux gestes. Le verbe (l'outil) a sa colonne, le detail la suivante :
/// trois cents gestes se lisent alors comme une table, et l'oeil descend la
/// colonne des verbes sans relire les chemins.
const GESTE_CORPS: f32 = 13.0;
const GESTE_H: f32 = 20.0;
const VERBE_L: f32 = 78.0;
/// Une image du fil est une piece jointe : une vignette entiere dans une boite
/// de 128 x 72 points, son nom et sa legende a cote. Un clic sur la ligne
/// l'ouvre dans la visionneuse, qui grandit depuis la vignette.
const JOINTE_L: f32 = 128.0;
const JOINTE_H: f32 = 72.0;
const JOINTE_AIR: f32 = 6.0;
const JOINTE_TEXTE: f32 = 14.0;

/// Un bloc, avec sa mise en page deja faite.
struct Pose {
    bloc: Bloc,
    corps: Buffer,
    /// Pour un geste, son verbe ; pour une piece jointe, sa legende.
    second: Option<Buffer>,
    heure: Option<Buffer>,
    hauteur: f32,
    /// Sa position dans le flux, en pixels depuis le haut.
    y: f32,
    /// Pour une image : sa taille d'affichage, et sa texture une fois creee.
    image: Option<ImagePosee>,
    /// Les liens du bloc, avec leurs boites.
    liens: Vec<LienPose>,
}

/// Un lien d'un bloc : son adresse, et ses boites en pixels, relatives au coin
/// haut-gauche du texte du bloc. Coupe sur deux lignes, il en a deux et reste
/// une seule cible.
struct LienPose {
    url: String,
    boites: Vec<plan::Boite>,
    /// Ce qui se souligne : le libelle d'un lien Markdown, ou tout le lien.
    soulignees: Vec<plan::Boite>,
    /// Une ligne de geste se vise en entier, verbe compris.
    rangee: bool,
}

struct ImagePosee {
    /// La taille d'affichage de la vignette seule, dans sa boite.
    largeur: f32,
    hauteur: f32,
    /// Ce qui dit que deux images sont la meme : un `cp` la refait sous un
    /// autre nom, et le fil la montrait deux fois (cadre, constat 7).
    empreinte: u64,
    /// Creee a la premiere apparition a l'ecran, a la taille exacte
    /// d'affichage, et refaite si cette taille change.
    texture: Option<images::Texture>,
}

/// Un lien ouvert : l'adresse, ce que la feuille en dit, et ce qu'elle montre.
struct PassageOuvert {
    url: String,
    etat: passage::Etat,
    vue: feuille::Vue,
    depuis: Instant,
    local_confirme: bool,
    /// Ce que la page vivante a repondu la derniere fois (`Trouve` ou `NonRetrouve`),
    /// gardee quand la page se referme : l'extrait la dit.
    verdict: navigateur::Sonde,
    /// Elle se referme : le ressort revient a 0, puis elle disparait.
    ferme: bool,
    /// La page lue au clic (le reseau), quand il y en a une ; les captures
    /// lisent `Vigie::pages` a la place.
    page: Option<passage::Page>,
    /// Le numero de la lecture en cours : une reponse d'un clic precedent ne
    /// s'applique pas.
    jeton: u64,
}

/// Ce qu'une lecture rend au fil principal.
struct ArriveeLecture {
    jeton: u64,
    url: String,
    issue: reseau::Issue,
    duree: std::time::Duration,
}

struct Vigie {
    fenetre: std::sync::Arc<Window>,
    surface: wgpu::Surface<'static>,
    peripherique: wgpu::Device,
    file: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,

    systeme: FontSystem,
    swash: SwashCache,
    atlas: TextAtlas,
    rendu: TextRenderer,
    ecran: Viewport,

    formes: formes::Formes,
    images: images::Images,
    poses: Vec<Pose>,
    entete: Option<Buffer>,
    sous_entete: Option<Buffer>,
    /// Le mot de l'etat, pose a droite du titre, sa largeur et l'etat.
    etat_pose: Option<(Buffer, f32, catalogue::Etat, bool)>,
    /// Le temps ecoule depuis le demarrage, pour la pastille.
    temps: f32,
    role: String,
    chantier: String,

    /// Facteur d'affichage du systeme multiplie par le zoom de l'utilisateur.
    /// Tout ce qui se dessine passe par lui.
    echelle: f32,
    zoom: f32,
    defilement: ressort::Ressort,
    colle_en_bas: bool,
    hauteur_flux: f32,
    derniere_trame: Instant,

    recois: Receiver<Nouvelle>,
    /// En essai, le flux vivant est ecarte : un temoin noye ne prouve rien.
    essai_seul: bool,

    /// Le catalogue de tous les chantiers, refait toutes les trois secondes
    /// par son propre fil.
    catalogue: Arc<Catalogue>,
    vue: Vue,
    /// Ce que le fil de suivi suit. Distinct de la vue : on peut parcourir la
    /// liste d'un projet pendant que le fil continue d'en suivre un autre.
    choix: Choix,
    envoi_choix: Sender<(u64, Choix)>,
    /// Le numero du dernier choix envoye, et celui du dernier depart annonce
    /// par le fil. Tant qu'ils different, ce qui arrive appartient a un choix
    /// abandonne, et il est ecarte.
    generation: u64,
    generation_recue: u64,
    /// Le chantier choisi n'existe sous aucune session.
    introuvable: Option<String>,
    onglets: Vec<vues::OngletPose>,
    onglets_voulus: Vec<(Option<String>, bool)>,
    defilement_onglets: ressort::Ressort,
    liste: Option<vues::ListePosee>,
    defilement_liste: ressort::Ressort,
    /// La ligne en evidence : celle que survole la souris, ou que les fleches
    /// ont choisie. Une seule, pour que l'oeil ne cherche pas laquelle compte.
    focus: Option<usize>,
    /// La souris, en pixels de la fenetre, et ce qu'elle a enfonce.
    curseur: Option<(f32, f32)>,
    appui: Option<Cible>,
    /// Ce que la souris survolait a la derniere trame.
    survol: Cible,
    /// Le curseur pose : la fleche, la main sur ce qui se clique, la loupe sur
    /// une image qui se zoome, la main ouverte ou fermee sur une image zoomee.
    icone: CursorIcon,
    /// Le dernier clic sur l'image ouverte : un second, vite et au meme
    /// endroit, est un double-clic.
    dernier_clic: Option<(Instant, f32, f32)>,
    /// La main qui glisse une image zoomee : ou elle a appuye, le decalage
    /// d'alors, et si elle a bouge (un appui sans bouger reste un clic).
    glisse: Option<((f32, f32), (f32, f32), bool)>,
    entete_textes: (String, String),
    /// Le titre et la phrase d'un etat vide, et ce qui les a produits.
    vide: Option<(String, Buffer, Buffer)>,
    /// La barre laterale des projets. Masquee a la main, et gardee ainsi au
    /// lancement suivant ; rappelee par-dessus le contenu dans une fenetre
    /// etroite, ou elle se replie d'elle-meme. Son ressort porte la largeur
    /// qui s'en voit, en pixels : c'est le ressort du defilement.
    barre_masquee: bool,
    barre_rappelee: bool,
    /// La ligne en evidence vient du clavier : elle porte son anneau, et la
    /// souris immobile ne l'efface pas.
    focus_clavier: bool,
    barre: ressort::Ressort,
    /// « Projets », le titre du groupe de la barre, pose une fois par echelle.
    groupe_projets: Option<Buffer>,
    /// Ce que fait le bouton de la barre, dit a son survol avec sa touche :
    /// `Ctrl` + `B` ne s'ecrivait que dans le LISEZ-MOI (juge-design.md, m2).
    /// La cle est l'etat de la barre que le texte decrit.
    bulle: Option<(bool, Buffer, f32)>,
    /// Le contraste eleve de Windows, ou --contraste-eleve.
    contraste_eleve: bool,

    /// La fiche du chantier dont la page est ouverte, et la demande de fiche
    /// au fil qui les dresse.
    fiche: Option<Arc<chantier::Fiche>>,
    envoi_fiche: Sender<Option<String>>,
    /// Une vignette cliquee : quand son image arrive dans le fil de l'agent,
    /// le fil s'y place au lieu d'aller au bas.
    viser: Option<std::path::PathBuf>,
    /// Les blocs recus et pas encore mis en page : voir BLOCS_PAR_TRAME.
    en_attente: std::collections::VecDeque<Bloc>,

    /// L'image ou la video ouverte en grand, par-dessus le reste.
    visionneuse: Option<visionneuse::Visionneuse>,
    /// L'ecran d'ouverture, tant qu'il est la (ouverture.rs), sa texture, sa
    /// taille et la toile en train de se decoder dans un fil a part.
    ouverture: Option<ouverture::Ouverture>,
    ouverture_texture: Option<images::Texture>,
    ouverture_taille: (u32, u32),
    ouverture_recue: Option<std::sync::mpsc::Receiver<ouverture::Decodee>>,
    /// Le catalogue est arrive une premiere fois : la vigie sait quoi montrer.
    donnees_pretes: bool,
    /// Sa texture, et ce qu'elle montre : le rang et la source (0 la
    /// vignette, 1 l'image pleine, 2 la video).
    vis_texture: Option<(usize, u8, images::Texture)>,
    /// L'image pleine, lue sur le disque par un fil a part.
    vis_pleine: Option<std::sync::mpsc::Receiver<(usize, suivi::Image, Vec<visionneuse::Niveau>)>>,
    /// Ses textes, poses une fois par changement, et la cle qui les a faits.
    vis_textes: Option<(String, Vec<Buffer>)>,
    /// Le texte par-dessus la visionneuse : un second rendu, dessine apres
    /// elle, puisque celui du fil passe dessous.
    rendu_dessus: TextRenderer,
    /// La feuille du passage, ouverte par un clic sur un lien du fil.
    passage: Option<PassageOuvert>,
    /// Son ouverture, de 0 a 1, par le ressort du fil ; d'un coup sous le
    /// mouvement reduit.
    feuille_ressort: ressort::Ressort,
    /// Sa composition, gardee tant que l'entree et la taille ne changent pas.
    feuille: Option<(String, feuille::Composee)>,
    /// Les pages que la maquette a en main (VIGIE_PAGES), a la place du reseau.
    pages: std::collections::HashMap<String, passage::Page>,
    /// --reseau= : ce que le reseau repondrait, force pour une capture.
    reseau: Option<passage::Reseau>,
    /// Ce que les gestes WebFetch ont demande et cite, par adresse.
    lectures: std::collections::HashMap<String, suivi::LectureCommune>,
    lire_envoi: std::sync::mpsc::Sender<ArriveeLecture>,
    lire_recois: Receiver<ArriveeLecture>,
    lire_jeton: u64,
    /// --sonde=trouve|non|indisponible : ce que la page ouverte aurait repondu,
    /// force pour une capture (aucun moteur de page n'y tourne).
    sonde_forcee: Option<navigateur::Sonde>,
    /// --citation=texte : la citation d'un lien ouvert par --adresse, qu'aucune
    /// transcription ne porte (une page de contrôle locale, par exemple).
    citation_forcee: Option<String>,
    /// La page, tant qu'elle est ouverte. Jamais pendant une capture.
    navigateur: Option<navigateur::Navigateur>,
    page_permise: bool,
    sonde: navigateur::Sonde,
    /// --ecoule=s : les secondes affichees par un chargement, figees.
    ecoule_force: Option<f32>,
    /// Le lien que le clavier a atteint (Tab) : le bloc et son rang.
    lien_focus: Option<(usize, usize)>,
    /// Ce que dit un lien survole : l'hote reel, et si un passage est connu.
    bulle_lien: Option<(String, Buffer, Buffer)>,
    /// La derniere bulle dite au journal : une ligne par changement, pas par trame.
    bulle_journal: String,
    animations_coupees: bool,
    /// Le verre de la visionneuse : sa scene floutee et ses plaques.
    verre: verre::Verre,
    /// Le theme regle. Le theme effectif vit dans `palette`, lu a chaque
    /// trame.
    reglage: palette::Reglage,
    /// L'arrivee du dernier bloc. Une capture du fil attend une seconde de
    /// calme : le fil d'un agent relu depuis son debut arrive par morceaux, et
    /// deux captures du meme ecran comptaient 66 et 75 gestes.
    dernier_bloc: Instant,
    /// Le fil a fini de lire ce que le fichier contenait a l'ouverture
    /// (suivi::Nouvelle::Lu). Une seconde sans bloc ne le prouvait pas : sur
    /// une machine chargee, la capture partait sur un fil a moitie lu et
    /// `--defiler` butait sur son debut (juge-technique.md, §4).
    fil_lu: bool,
    /// Le haut voulu du fil apres un clic sur une vignette, tant qu'on n'a
    /// pas defile soi-meme.
    ancre: Option<f32>,
}

struct Demarrage {
    vigie: Option<Vigie>,
    recois: Option<Receiver<Nouvelle>>,
    /// --capture=chemin.png : rendre, ecrire, quitter. Sans lui, aucun
    /// relecteur ne peut juger sans ouvrir la fenetre a la main.
    capture: Option<String>,
    /// Ctrl enfonce : la molette zoome au lieu de defiler.
    ctrl: bool,
    /// Majuscule enfoncee : Ctrl + Maj + Tab recule d'un onglet.
    maj: bool,
    /// Alt enfonce : Alt + fleche recule ou avance dans la page ouverte.
    alt: bool,
    /// Une entree a change ce qui se voit : la trame suivante le dessine.
    redessiner: bool,
    /// --essai : pose un bloc de signes connus, pour prouver que le recours de
    /// police prend. Sans lui, il faudrait attendre qu'un agent ecrive du grec.
    essai: bool,
    depart: Instant,
    /// La vue de depart, posee par --onglet= ou --chantier= : sans elles,
    /// --capture ne rendrait que le direct, et aucun autre etat ne se jugerait
    /// sans clic.
    vue: Vue,
    choix: Choix,
    envoi_choix: Option<Sender<(u64, Choix)>>,
    envoi_fiche: Option<Sender<Option<String>>>,
    /// --attente=secondes : le temps laisse au fil apres un clic rejoue,
    /// avant la capture. Un agent relu depuis son debut, images et videos
    /// comprises, en demande plus que les deux secondes et demie d'origine.
    attente: f32,
    /// --clic=x,y et --touches=a,b,... : un clic et des touches rejoues, une
    /// fois le catalogue entier, par les memes fonctions que la souris et le
    /// clavier. --survol=x,y pose la souris pour la capture. Sans eux, le
    /// chemin du clic et celui du clavier ne se prouveraient qu'a la main.
    clic: Option<(f32, f32)>,
    touches: Vec<Touche>,
    survol: Option<(f32, f32)>,
    /// --defiler=px : fait descendre la vue de tant de pixels avant la
    /// capture, pour juger le bas d'une page ou d'un fil.
    defiler: f32,
    /// --lien=n : un clic sur le n-ieme lien visible du fil (en partant de 0),
    /// par la meme fonction que la souris. --survol-lien=n le survole.
    /// --adresse=url : le clic sur un lien qui porterait cette adresse, pour
    /// les classes de lien qu'aucun chantier fige ne contient (un schema
    /// refuse, une adresse locale).
    lien: Option<String>,
    survol_lien: Option<String>,
    adresse: Option<String>,
    /// Le moment ou le clic et les touches ont ete rejoues.
    rejoue: Option<Instant>,
    /// La fenetre de la derniere fois : sa taille, sa place et son zoom. Aucun
    /// en essai ou en capture, qui doivent partir d'un etat connu.
    souvenir: Option<app::Souvenir>,
    /// Garder l'etat a la fermeture : jamais en essai, en capture ni quand un
    /// drapeau a impose la vue, qui n'est pas celle qu'on a choisie.
    garder: bool,
    /// --echelle=f remplace le facteur d'affichage du systeme, et
    /// --taille=LxH la taille de la fenetre en points : un seul ecran suffit
    /// alors a juger chaque etat a 1, 1,25, 1,5 et 2, et en fenetre etroite.
    facteur_force: Option<f32>,
    taille: Option<(f64, f64)>,
    /// --instant=s : la capture part a s secondes, sans attendre le catalogue
    /// ni la fiche, pour juger un premier lancement ou une lecture en cours.
    instant: Option<f32>,
    /// --theme=sombre|clair|systeme : impose le theme, pour rendre chaque
    /// ecran dans les deux.
    theme: Option<palette::Reglage>,
    /// --viser=nom.png : avec --agent, place le fil sur cette image, comme un
    /// clic sur sa vignette. Une remontee en pixels dependait de la mise en
    /// page, donc de la direction et de l'echelle.
    viser: Option<std::path::PathBuf>,
    /// --ouvrir : avec --viser, ouvre l'image visee dans la visionneuse, comme
    /// un clic sur sa piece jointe.
    ouvrir: Option<std::path::PathBuf>,
    /// --reel=s : la vigie rejoue ses drapeaux comme pour une capture, mais
    /// reste ouverte s secondes au lieu de photographier, avec le moteur de
    /// page permis : de quoi voir une vraie page dans une fenetre a soi.
    reel: Option<f32>,
    reel_debut: Option<Instant>,
    /// --ouvrir-page : avec --reel, le second geste se fait des que la lecture est arrivee.
    ouvrir_page: bool,
    /// --loupe=z[,x,y] : avec --ouvrir, zoome l'image ouverte de z, autour du
    /// point (x, y) de la fenetre ou de son centre, comme la molette.
    loupe: Option<(f32, Option<(f32, f32)>)>,
    /// --ouverture=o : une capture montre l'ecran d'ouverture, a l'opacite o
    /// (0 a 1). Sans lui, une capture n'en montre pas.
    ouverture_capture: Option<f32>,
    /// --clic-vis=x,y : avec --ouvrir, un clic dans la visionneuse ouverte,
    /// par la meme fonction que la souris (le saut sur l'onde d'un son, par
    /// exemple). --clic, lui, tombe avant son ouverture.
    clic_vis: Option<(f32, f32)>,
    /// --barre=masquee|visible : impose le choix de la barre, que le souvenir
    /// donnerait sinon.
    barre: Option<bool>,
    /// --mouvement-reduit et --contraste-eleve : les deux reglages de Windows,
    /// imposes pour qu'une capture les montre sans changer ceux du poste.
    mouvement_reduit: bool,
    contraste_eleve: bool,
    /// VIGIE_PERDRE=s : le peripherique detruit a s secondes, temoin de la
    /// relance (`relancer`).
    perdre: Option<f32>,
}

fn attrs(registre: Registre) -> Attrs<'static> {
    let a = Attrs::new().family(Family::Name("Barlow"));
    // cosmic-text cherche dans les autres familles chargees quand un glyphe
    // manque : il suffit que le recours soit dans la base.
    match registre {
        // La parole porte : elle est en demi-gras, les autres en normal.
        Registre::Parole | Registre::Releve => a.weight(Weight::MEDIUM),
        _ => a.weight(Weight::NORMAL),
    }
}

fn couleur(registre: Registre) -> glyphon::Color {
    match registre {
        Registre::Pensee => palette::encre2(),
        Registre::Parole => palette::encre1(),
        Registre::Geste | Registre::Image => palette::encre3(),
        Registre::Releve => palette::encre1(),
    }
}

impl Vigie {
    fn mesures(&self) -> Mesures {
        let e = self.echelle;
        // La mise en page se fait pour la barre a sa place d'arrivee, et le
        // mouvement ne fait que la translater : remettre mille blocs en page a
        // chaque trame du ressort ne se paierait pas.
        let arrivee = self.decalage_arrivee();
        let glisse = self.decalage_barre() - arrivee;
        let l = self.config.width as f32 - arrivee;
        let marge = MARGE * e;
        let gouttiere = GOUTTIERE * e;
        let ecart = ECART_GOUTTIERE * e;
        let dispo = l - 2.0 * marge - gouttiere - ecart;
        let colonne = dispo.min(COLONNE_MAX * e).max(160.0 * e);
        let liste_colonne = dispo.min((BLOC_MAX - GOUTTIERE - ECART_GOUTTIERE) * e).max(colonne);
        let large_bloc = gouttiere + ecart + liste_colonne;
        // Centre, et jamais plus a gauche que la marge : sur une fenetre
        // etroite, centrer reviendrait a mordre le bord.
        let x_bloc = arrivee + glisse + ((l - large_bloc) * 0.5).max(marge);
        // Le fil, centre dans le bloc.
        let x_heure = x_bloc + ((large_bloc - (gouttiere + ecart + colonne)) * 0.5).max(0.0);
        Mesures {
            corps: CORPS * e,
            interligne: INTERLIGNE * e,
            colonne,
            marge,
            entete_h: ENTETE_H * e,
            air: AIR * e,
            x_heure,
            x_corps: x_heure + gouttiere + ecart,
            x_bloc,
            large_bloc,
            liste_corps: x_bloc + gouttiere + ecart,
            liste_colonne,
        }
    }

    /// Le theme effectif, d'apres le reglage et, pour « systeme », le theme
    /// des applications de Windows tel que winit le lit.
    fn appliquer_theme(&mut self) {
        let clair = match self.reglage {
            palette::Reglage::Sombre => false,
            palette::Reglage::Clair => true,
            palette::Reglage::Systeme => self.fenetre.theme() == Some(winit::window::Theme::Light),
        };
        palette::poser_clair(clair);
    }

    /// La fenetre est trop etroite pour la barre : elle se replie d'elle-meme
    /// et ne revient que par-dessus le contenu.
    fn barre_etroite(&self) -> bool {
        (self.config.width as f32) < vues::BARRE_SEUIL * self.echelle
    }

    /// La largeur de barre que le ressort doit atteindre, en pixels.
    fn barre_cible(&self) -> f32 {
        vues::cible_barre(self.barre_etroite(), self.barre_masquee, self.barre_rappelee) * self.echelle
    }

    /// Ce que la barre prend au contenu, a l'arrivee et en ce moment : rien
    /// dans une fenetre etroite, ou elle passe par-dessus.
    fn decalage_arrivee(&self) -> f32 {
        if self.barre_etroite() {
            0.0
        } else {
            self.barre_cible()
        }
    }

    fn decalage_barre(&self) -> f32 {
        if self.barre_etroite() {
            0.0
        } else {
            self.barre.position().clamp(0.0, vues::BARRE_L * self.echelle)
        }
    }

    /// La barre rappelee par-dessus le contenu d'une fenetre etroite.
    fn barre_par_dessus(&self) -> bool {
        self.barre_etroite() && self.barre.position() > 0.5
    }

    /// Le bouton ou Ctrl + B : masque ou reaffiche la barre. Dans une
    /// fenetre etroite, la rappelle ou la renvoie, sans toucher au choix
    /// garde pour une fenetre large.
    fn basculer_barre(&mut self) {
        if self.barre_etroite() {
            self.barre_rappelee = !self.barre_rappelee;
            self.viser_barre();
        } else {
            self.barre_masquee = !self.barre_masquee;
            self.barre_rappelee = false;
            self.viser_barre();
            // Le contenu se remet en page une fois, a sa largeur d'arrivee.
            self.remettre_en_page();
        }
    }

    /// Le ressort de la barre vers sa cible, d'un coup sous le mouvement
    /// reduit.
    fn viser_barre(&mut self) {
        let cible = self.barre_cible();
        if self.mouvement_reduit() {
            self.barre.poser(cible);
        } else {
            self.barre.viser(cible);
        }
    }

    /// Les effets d'animation de Windows coupes, ou `--mouvement-reduit` :
    /// tout ressort se pose d'un coup et rien ne respire.
    fn mouvement_reduit(&self) -> bool {
        self.animations_coupees
    }

    /// Met en page un bloc. Appele une fois par bloc, jamais par trame.
    fn poser(&mut self, bloc: Bloc) -> Pose {
        let m = self.mesures();
        let largeur = m.colonne.min(MESURE * self.echelle);
        let mut corps = Buffer::new(&mut self.systeme, Metrics::new(m.corps, m.interligne));
        corps.set_size(Some(largeur), None);

        let e = self.echelle;
        // Une image est une piece jointe : sa vignette entiere dans une boite
        // de JOINTE_L x JOINTE_H, jamais agrandie au-dela de sa taille a
        // l'echelle de l'ecran, et deux lignes a cote, son nom et ce qu'elle
        // est. Arrondie au pixel : sa texture sera faite a cette taille exacte.
        if let Some(img) = bloc.image.clone() {
            let (il, ih) = (img.largeur.max(1) as f32, img.hauteur.max(1) as f32);
            let r = (JOINTE_L * e / il).min(JOINTE_H * e / ih).min(e * img.source.0.max(1) as f32 / il);
            let (l, h) = ((il * r).round().max(1.0), (ih * r).round().max(1.0));
            let texte_l = (m.colonne - (JOINTE_L + JOINTE_TEXTE) * e).max(40.0 * e);
            let nom = img.nom().unwrap_or_else(|| "image reçue".to_string());
            let (nom_b, _) = vues::une_ligne(
                &mut self.systeme,
                Metrics::new(m.corps, m.interligne),
                &Attrs::new().family(Family::Name("Barlow")).weight(Weight::MEDIUM),
                &nom,
                texte_l,
            );
            let detail = suivi::legende_courte(&img);
            let (detail_b, _) = vues::une_ligne(
                &mut self.systeme,
                Metrics::new(LEGENDE_CORPS * e, LEGENDE_H * e),
                &Attrs::new().family(Family::Name("Barlow")),
                &detail,
                texte_l,
            );
            let heure = self.poser_heure(&bloc.heure, GESTE_H);
            let empreinte = img.empreinte();
            return Pose {
                bloc,
                corps: nom_b,
                second: Some(detail_b),
                heure,
                hauteur: (h.max((INTERLIGNE + LEGENDE_H) * e) + 2.0 * JOINTE_AIR * e).round(),
                y: 0.0,
                image: Some(ImagePosee { largeur: l, hauteur: h, empreinte, texture: None }),
                liens: Vec::new(),
            };
        }

        // Un geste : une ligne, le verbe dans sa colonne, le detail coupe
        // d'une ellipse a la largeur qui reste.
        if matches!(bloc.registre, Registre::Geste | Registre::Image) {
            // Une ligne qui est elle-meme un lien fait 24 pt : elle se vise en entier.
            let h_ligne = plan::hauteur_de_ligne(GESTE_H, bloc.registre == Registre::Geste && passage::geste_a_un_lien(&bloc.outil, &bloc.corps));
            let gm = Metrics::new(GESTE_CORPS * e, h_ligne * e);
            let verbe = if bloc.registre == Registre::Image { "Image" } else { bloc.outil.as_str() };
            let (verbe_b, _) = vues::une_ligne(
                &mut self.systeme,
                gm,
                &Attrs::new().family(Family::Name("Barlow")).weight(Weight::MEDIUM),
                verbe,
                (VERBE_L - 6.0) * e,
            );
            let detail = if bloc.registre == Registre::Image {
                format!("{}  ·  sortie de la mémoire, avec les plus anciennes", bloc.corps)
            } else {
                bloc.corps.clone()
            };
            let (corps_b, _) = vues::une_ligne(
                &mut self.systeme,
                gm,
                &Attrs::new().family(Family::Name("Barlow")),
                &detail,
                (m.colonne - VERBE_L * e).max(40.0 * e),
            );
            let heure = self.poser_heure(&bloc.heure, h_ligne);
            let liens = liens_du_geste(&bloc, &corps_b);
            return Pose { bloc, corps: corps_b, second: Some(verbe_b), heure, hauteur: h_ligne * e, y: 0.0, image: None, liens };
        }

        let texte = match bloc.registre {
            Registre::Releve => format!("— {} —", bloc.corps),
            Registre::Parole => suivi::sans_balisage(&bloc.corps),
            _ => bloc.corps.clone(),
        };
        // Un lien Markdown s'affiche par son titre ; l'adresse reste dans le lien.
        let (texte, liens_texte) = match bloc.registre {
            Registre::Parole | Registre::Pensee => passage::afficher_liens(&texte),
            _ => (texte, Vec::new()),
        };
        corps.set_text(&texte, &attrs(bloc.registre), Shaping::Advanced, None);
        corps.shape_until_scroll(&mut self.systeme, false);
        let lignes = corps.layout_runs().count().max(1) as f32;
        let hauteur = lignes * m.interligne;

        let heure = self.poser_heure(&bloc.heure, GESTE_H);
        let liens: Vec<LienPose> = liens_texte
            .into_iter()
            .filter(|l| passage::classer(&l.url) != passage::Classe::Gabarit)
            .map(|l| {
                let boites = feuille::boites(&corps, &texte, l.debut, l.fin);
                LienPose { soulignees: boites.clone(), boites, url: l.url, rangee: false }
            })
            .filter(|l| !l.boites.is_empty())
            .collect();
        Pose {
            bloc,
            corps,
            second: None,
            heure,
            hauteur,
            y: 0.0,
            image: None,
            liens,
        }
    }

    /// L'heure vit dans sa propre gouttiere : deux couleurs demandent deux
    /// zones, et cela donne en prime une colonne d'heures alignee.
    fn poser_heure(&mut self, heure: &str, h_ligne: f32) -> Option<Buffer> {
        if heure.is_empty() {
            return None;
        }
        let mut h = Buffer::new(&mut self.systeme, Metrics::new(GESTE_CORPS * self.echelle, h_ligne * self.echelle));
        h.set_size(Some(GOUTTIERE * self.echelle), None);
        h.set_text(heure, &Attrs::new().family(Family::Name("Barlow")), Shaping::Advanced, None);
        h.shape_until_scroll(&mut self.systeme, false);
        Some(h)
    }

    /// Les plus anciennes images au-dela de IMAGES_GARDEES rendent leurs
    /// pixels et redeviennent une ligne.
    fn borner_images(&mut self) {
        let avec_pixels: Vec<usize> = self
            .poses
            .iter()
            .enumerate()
            .filter(|(_, p)| p.bloc.image.is_some())
            .map(|(i, _)| i)
            .collect();
        if avec_pixels.len() <= IMAGES_GARDEES {
            return;
        }
        for &i in &avec_pixels[..avec_pixels.len() - IMAGES_GARDEES] {
            let mut b = self.poses[i].bloc.clone();
            b.image = None;
            let p = self.poser(b);
            self.poses[i] = p;
        }
    }

    /// Cree la texture d'une image a sa taille d'affichage, si elle manque ou
    /// si l'echelle a change. Appele seulement pour ce qui est a l'ecran.
    fn preparer_image(&mut self, i: usize) {
        let p = &mut self.poses[i];
        let (Some(ip), Some(img)) = (p.image.as_mut(), p.bloc.image.as_ref()) else {
            return;
        };
        let (l, h) = (ip.largeur as u32, ip.hauteur as u32);
        if ip.texture.as_ref().is_some_and(|t| t.largeur == l && t.hauteur == h) {
            return;
        }
        let texture = if img.largeur == l && img.hauteur == h {
            self.images.texture(&self.peripherique, &self.file, &img.pixels, l, h)
        } else {
            let Some(source) =
                image::ImageBuffer::<image::Rgba<u8>, &[u8]>::from_raw(img.largeur, img.hauteur, &img.pixels[..])
            else {
                return;
            };
            let reduite = image::imageops::resize(&source, l, h, image::imageops::FilterType::Triangle);
            self.images.texture(&self.peripherique, &self.file, reduite.as_raw(), l, h)
        };
        ip.texture = Some(texture);
    }

    /// Recalcule les ordonnees. Un seul parcours, apres tout ajout.
    /// Aucun air entre deux gestes : ils font une table. L'air du fil ne
    /// separe que ce qui change de registre, et la parole de tout le reste.
    fn replacer(&mut self) {
        let air = self.mesures().air;
        let mut y = 0.0;
        let mut avant: Option<Registre> = None;
        for p in self.poses.iter_mut() {
            let r = p.bloc.registre;
            let ligne = |r: Registre| matches!(r, Registre::Geste | Registre::Image);
            let jointe = p.image.is_some();
            if let Some(a) = avant {
                if !(ligne(a) && ligne(r) && !jointe) {
                    y += if jointe || a == Registre::Image { air * 0.5 } else { air };
                }
            }
            p.y = y;
            y += p.hauteur;
            avant = Some(if jointe { Registre::Image } else { r });
        }
        self.hauteur_flux = y + air;
    }

    fn visible(&self) -> f32 {
        let m = self.mesures();
        (self.config.height as f32 - m.entete_h - m.marge).max(1.0)
    }

    fn bas(&self) -> f32 {
        (self.hauteur_flux - self.visible()).max(0.0)
    }

    /// Ce que dit l'en-tete. Dans un fil, qui parle et dans quel chantier ;
    /// dans une liste, le projet et ce qu'il contient. L'etat a sa place a
    /// part, a droite du titre (`etat_entete`) : c'est ce que le coup d'oeil
    /// cherche, et il ne doit pas se perdre au milieu d'une ligne grise.
    fn textes_entete(&self) -> (String, String, String) {
        match &self.vue {
            Vue::Liste(p) => (p.clone(), String::new(), self.resume_projet(p)),
            Vue::Chantier(id) => (
                self.catalogue.chantier(id).map_or_else(|| id.clone(), |c| c.nom.clone()),
                String::new(),
                self.resume_chantier(id),
            ),
            Vue::Suivi(id) if self.introuvable.as_deref() == Some(id.as_str()) => {
                (id.clone(), String::new(), "introuvable sous toutes les sessions".to_string())
            }
            Vue::Suivi(id) if self.role.is_empty() => (
                self.catalogue.chantier(id).map_or_else(|| id.clone(), |c| c.nom.clone()),
                String::new(),
                "lecture du fil".to_string(),
            ),
            Vue::Direct if self.role.is_empty() && !self.catalogue.complet => (
                "Direct".to_string(),
                String::new(),
                format!("inventaire en cours  ·  {} chantiers lus", self.catalogue.chantiers.len()),
            ),
            Vue::Direct if self.role.is_empty() => ("Direct".to_string(), String::new(), "en attente".to_string()),
            _ => {
                let (lieu, reste) = self.libelle_fil();
                (self.role.clone(), lieu, reste)
            }
        }
    }

    /// L'etat que l'en-tete montre a droite du titre : celui du chantier de
    /// la vue. Aucun pour une liste, ni tant que le catalogue ne le connait pas.
    /// L'etat, son mot, et s'il est muet (`Chantier::muet`).
    fn etat_entete(&self) -> Option<(catalogue::Etat, String, bool)> {
        let maintenant = app::maintenant();
        match &self.vue {
            Vue::Liste(_) => None,
            Vue::Direct if self.role.is_empty() => None,
            _ => self
                .catalogue
                .chantier(&self.chantier_de_vue()?)
                .map(|c| (c.etat, c.mot(maintenant), c.muet(maintenant).is_some())),
        }
    }

    /// « design-de-l-application  ·  atelier-d », ou l'on est, puis « 372
    /// gestes, 13 messages, 29 images », ce que le fil contient, compte sur
    /// les blocs recus.
    fn libelle_fil(&self) -> (String, String) {
        let lieu = match self.catalogue.chantier(&self.chantier) {
            Some(c) => format!("{}  ·  {}", c.nom, c.projet),
            None => self.chantier.clone(),
        };
        let mut parties: Vec<String> = Vec::new();
        let compte = |r: Registre| self.poses.iter().filter(|p| p.bloc.registre == r).count();
        let (g, d) = (compte(Registre::Geste), compte(Registre::Parole));
        let sons = self
            .poses
            .iter()
            .filter(|p| p.bloc.image.as_ref().is_some_and(|x| x.est_audio()))
            .count();
        let i = self.poses.iter().filter(|p| p.image.is_some()).count().saturating_sub(sons);
        let pluriel = |n: usize, mot: &str| if n == 1 { format!("1 {mot}") } else { format!("{n} {mot}s") };
        if g + d + i + sons > 0 {
            let mut texte = format!("{}, {}, {}", pluriel(g, "geste"), pluriel(d, "message"), pluriel(i, "image"));
            if sons > 0 {
                texte.push_str(&format!(", {}", pluriel(sons, "son")));
            }
            parties.push(texte);
        }
        (lieu, parties.join("  ·  "))
    }

    /// « 7 agents · 29 images · lancé aujourd'hui à 09:49 · atelier-d ».
    /// Pendant la premiere lecture, aucun compte partiel n'est montre comme
    /// un vrai : « lecture des agents, 3 sur 7 » (cadre, C8).
    fn resume_chantier(&self, id: &str) -> String {
        let mut parties: Vec<String> = Vec::new();
        let c = self.catalogue.chantier(id);
        let total = c.map(|c| c.agents);
        match self.fiche.as_ref().filter(|f| f.id == id) {
            Some(f) if f.complete => {
                let n = f.agents.len();
                parties.push(if n == 1 { "1 agent".to_string() } else { format!("{n} agents") });
                match f.images {
                    0 => {}
                    1 => parties.push("1 image".to_string()),
                    k => parties.push(format!("{k} images")),
                }
            }
            Some(f) => parties.push(match total {
                Some(t) => format!("lecture des agents, {} sur {t}", f.agents.len().min(t)),
                None => format!("lecture des agents, {} lus", f.agents.len()),
            }),
            None => parties.push("lecture des agents".to_string()),
        }
        if let Some(c) = c {
            let (jour, minute) = dates::jour_et_minute(c.debut);
            let aujourdhui = dates::jour_et_minute(app::maintenant()).0;
            let j = dates::libelle_jour(jour, aujourdhui);
            parties.push(format!("lancé {} à {}", minuscule_jour(&j), dates::heure(minute)));
            parties.push(c.projet.clone());
        }
        parties.join("  ·  ")
    }

    /// « 16 chantiers · 1 en cours ».
    fn resume_projet(&self, p: &str) -> String {
        let n = self.catalogue.de(p).count();
        if n == 0 {
            return if self.catalogue.complet {
                "aucun chantier connu".to_string()
            } else {
                "inventaire en cours".to_string()
            };
        }
        let k = self.catalogue.de(p).filter(|c| c.etat == catalogue::Etat::EnCours).count();
        let mut s = if n == 1 { "1 chantier".to_string() } else { format!("{n} chantiers") };
        if k > 0 {
            s.push_str(&format!("  ·  {k} en cours"));
        }
        s
    }

    fn poser_entete(&mut self) {
        let textes = self.textes_entete();
        let etat = self.etat_entete();
        let e = self.echelle;
        // La ligne du lieu s'arrete au bout de sa colonne, pas au bord de la
        // fenetre.
        let (gauche, droite) = self.bords_entete(&self.mesures());
        let dispo = (droite - gauche).max(1.0);
        let cle = (textes.0.clone(), format!("{}\n{}\n{:?}\n{}", textes.1, textes.2, etat, dispo.round()));
        if self.entete.is_some() && cle == self.entete_textes {
            return;
        }
        self.entete_textes = cle;
        let (haut, lieu, bas) = textes;
        let l = self.config.width as f32;

        let mut a = Buffer::new(&mut self.systeme, Metrics::new(20.0 * e, 26.0 * e));
        a.set_size(Some(l), None);
        a.set_text(
            &haut,
            &Attrs::new()
                .family(Family::Name("Barlow"))
                .weight(Weight::MEDIUM),
            Shaping::Advanced,
            None,
        );
        a.shape_until_scroll(&mut self.systeme, false);
        self.entete = Some(a);

        // Le lieu d'un fil, le chantier et son projet, au corps du titre : a
        // 13 points, il ne se lisait pas sur une capture reduite au quart, le
        // seuil du coup d'oeil (cadre, C6 ; epreuve, tache 4), ou le titre en
        // 20 se lisait de justesse. Le titre le domine par sa graisse et son
        // encre. Les comptes qui suivent gardent le corps des reperes : ils ne
        // se lisent pas de loin.
        // Quand la ligne ne tient pas avant le bout du filet (une fenetre
        // etroite, un long nom de chantier), les comptes cedent : le lieu
        // reste entier.
        let mut b = Buffer::new(&mut self.systeme, Metrics::new(13.0 * e, 26.0 * e));
        b.set_size(None, None);
        let normal = Attrs::new().family(Family::Name("Barlow"));
        let grand = normal.clone().metrics(Metrics::new(LIEU_CORPS * e, 26.0 * e));
        let suite = if !lieu.is_empty() && !bas.is_empty() { format!("  ·  {bas}") } else { bas.clone() };
        for avec_comptes in [true, false] {
            let suite = if avec_comptes || lieu.is_empty() { suite.as_str() } else { "" };
            b.set_rich_text(
                [(lieu.as_str(), grand.clone()), (suite, normal.clone())],
                &normal,
                Shaping::Advanced,
                None,
            );
            b.shape_until_scroll(&mut self.systeme, false);
            let w = b.layout_runs().next().map_or(0.0, |r| r.line_w);
            if w <= dispo {
                break;
            }
        }
        self.sous_entete = Some(b);

        self.etat_pose = etat.map(|(et, mot, muet)| {
            let m = Metrics::new(15.0 * e, 26.0 * e);
            let attrs = Attrs::new().family(Family::Name("Barlow")).weight(Weight::MEDIUM);
            let mut t = Buffer::new(&mut self.systeme, m);
            t.set_size(None, None);
            t.set_text(&mot, &attrs, Shaping::Advanced, None);
            t.shape_until_scroll(&mut self.systeme, false);
            let w = t.layout_runs().next().map(|r| r.line_w).unwrap_or(0.0);
            (t, w, et, muet)
        });
    }

    fn recevoir(&mut self) -> bool {
        if self.essai_seul {
            while self.recois.try_recv().is_ok() {}
            return false;
        }
        let mut change = false;
        while let Ok(n) = self.recois.try_recv() {
            // Ce qui appartient a un choix abandonne est ecarte : voir
            // suivi::garder.
            if !suivi::garder(self.generation, &mut self.generation_recue, &n) {
                continue;
            }
            match n {
                Nouvelle::Depart(_) => {}
                Nouvelle::Lu => self.fil_lu = true,
                Nouvelle::Catalogue(c) => {
                    self.donnees_pretes = true;
                    self.catalogue = c;
                    self.catalogue_change();
                    change = true;
                }
                Nouvelle::Introuvable(id) => {
                    self.introuvable = Some(id);
                    self.poser_entete();
                    change = true;
                }
                Nouvelle::Agent { role, chantier } => {
                    self.role = role;
                    self.chantier = chantier;
                    self.introuvable = None;
                    self.poser_entete();
                    change = true;
                }
                Nouvelle::Fiche(f) => {
                    // Seule compte la fiche de la page ouverte : le fil peut
                    // finir d'en dresser une autre quand on vient de partir.
                    if matches!(&self.vue, Vue::Chantier(id) if *id == f.id) {
                        self.fiche = Some(f);
                        self.poser_liste();
                        self.poser_entete();
                        change = true;
                    }
                }
                Nouvelle::Bloc(b) => {
                    if let Some(l) = &b.lecture {
                        self.lectures.insert(b.corps.trim().to_string(), l.clone());
                    }
                    self.dernier_bloc = Instant::now();
                    self.en_attente.push_back(b);
                }
            }
        }
        let mut images = false;
        for _ in 0..BLOCS_PAR_TRAME {
            let Some(mut b) = self.en_attente.pop_front() else { break };
            // Le nom de l'agent en tete d'un fil vide redisait le titre de
            // l'en-tete, juste au-dessus (juge-design.md, m10). Il ne se montre
            // que quand le direct passe d'un agent a un autre.
            if b.registre == Registre::Releve && self.poses.is_empty() {
                continue;
            }
            // Une image deja dans le fil n'y revient pas : elle devient une
            // ligne qui dit de laquelle elle est la copie.
            if let Some(img) = b.image.clone() {
                let k = img.empreinte();
                let deja = self.poses.iter().find_map(|p| {
                    (p.image.as_ref()?.empreinte == k).then(|| p.bloc.image.as_ref().and_then(|i| i.nom()))
                });
                if let Some(autre) = deja {
                    b.image = None;
                    b.registre = Registre::Geste;
                    b.outil = "Image".to_string();
                    b.corps = format!(
                        "{}  ·  la même que {}",
                        img.nom().unwrap_or_else(|| "image reçue".to_string()),
                        autre.unwrap_or_else(|| "une image reçue plus haut".to_string())
                    );
                }
            }
            images |= b.image.is_some();
            let p = self.poser(b);
            self.poses.push(p);
            change = true;
        }
        if self.poses.len() > BLOCS_MAX {
            self.poses.drain(0..self.poses.len() - BLOCS_MAX);
        }
        if images {
            self.borner_images();
        }
        if change {
            self.replacer();
            self.poser_entete();
            // Une vignette cliquee : le fil se place sur son image des qu'elle
            // est posee, un peu d'air au-dessus pour la pensee qui la precede.
            // Un nom seul (--viser=) vaut pour le fichier de ce nom ; on
            // attend que le fil recu soit pose, sans quoi la cible se bornait
            // au bas d'un fil encore court.
            let vise = self.viser.as_ref().filter(|_| self.en_attente.is_empty()).and_then(|c| {
                let nom_seul = c.parent().is_none_or(|p| p.as_os_str().is_empty());
                self.poses
                    .iter()
                    .find(|p| {
                        let Some(ch) = p.bloc.image.as_ref().and_then(|i| i.chemin.as_ref()) else { return false };
                        ch == c || (nom_seul && ch.file_name() == c.file_name())
                    })
                    .map(|p| p.y)
            });
            if let Some(y) = vise {
                self.viser = None;
                self.colle_en_bas = false;
                self.ancre = Some(y - 3.0 * self.mesures().interligne);
            }
            // L'ancre tient tant qu'on ne defile pas : la suite du fil arrive
            // par morceaux, et une cible bornee au bas du premier morceau
            // placait l'image plus bas d'une capture a l'autre. Sous une image
            // ouverte, rien ne bouge : le fil recollera au bas a la fermeture.
            if let Some(a) = self.ancre {
                self.defilement.viser(a.clamp(0.0, self.bas()));
            } else if self.colle_en_bas && self.visionneuse.is_none() {
                self.defilement.viser(self.bas());
            }
        }
        change
    }

    /// La vue montre-t-elle une liste : celle d'un projet, ou la page d'un
    /// chantier ? Toutes deux defilent, se survolent et se parcourent au
    /// clavier de la meme facon.
    fn en_liste(&self) -> bool {
        matches!(self.vue, Vue::Liste(_) | Vue::Chantier(_))
    }

    /// Le bord gauche du titre et le bord droit de l'etat : ceux de la colonne
    /// que l'en-tete coiffe. Le titre pose sur le bloc tombait 96 points a
    /// gauche de la planche et des noms, et l'etat a droite du fil sur une
    /// page (vu le 2026-10-27 sur la page d'un chantier).
    fn bords_entete(&self, m: &Mesures) -> (f32, f32) {
        if self.en_liste() {
            (m.liste_corps, m.x_bloc + m.large_bloc)
        } else {
            (m.x_corps, m.x_corps + m.colonne)
        }
    }

    fn molette(&mut self, dy: f32) {
        if self.en_liste() {
            let cible = (self.defilement_liste.cible() - dy).clamp(0.0, self.bas_liste());
            self.defilement_liste.viser(cible);
            return;
        }
        self.ancre = None;
        let cible = (self.defilement.cible() - dy).clamp(0.0, self.bas());
        self.defilement.viser(cible);
        // On ne recolle en bas que si l'on y est vraiment revenu : sinon un
        // petit coup de molette vers le haut serait annule a la trame suivante
        // par l'arrivee d'un bloc.
        self.colle_en_bas = cible >= self.bas() - 2.0;
    }

    fn redimensionner(&mut self, l: u32, h: u32) {
        if l == 0 || h == 0 {
            return;
        }
        self.config.width = l;
        self.config.height = h;
        self.surface.configure(&self.peripherique, &self.config);
        self.ecran.update(
            &self.file,
            Resolution {
                width: l,
                height: h,
            },
        );
        self.recaler_barre();
        self.remettre_en_page();
    }

    /// Une largeur ou une echelle neuve : la barre se pose d'un coup a sa
    /// place, repliee si la fenetre est devenue etroite. Une barre rappelee
    /// ne survit pas au retour a une fenetre large.
    fn recaler_barre(&mut self) {
        if !self.barre_etroite() {
            self.barre_rappelee = false;
        }
        self.barre.poser(self.barre_cible());
    }

    /// Refait la mise en page de tout : appele quand la largeur ou l'echelle
    /// change. Un bloc garde son texte, mais pas ses lignes.
    fn remettre_en_page(&mut self) {
        let m = self.mesures();
        let anciens: Vec<Bloc> = self.poses.iter().map(|p| p.bloc.clone()).collect();
        self.poses.clear();
        for b in anciens {
            let p = self.poser(b);
            self.poses.push(p);
        }
        let _ = m;
        self.replacer();
        // Les onglets, la liste, l'en-tete et l'etat vide se refont a la
        // nouvelle echelle : on les oublie, et chacun se repose.
        self.entete = None;
        self.onglets_voulus.clear();
        self.liste = None;
        self.vide = None;
        self.groupe_projets = None;
        self.poser_entete();
        self.poser_onglets();
        self.montrer_onglet_actif();
        self.poser_liste();
        if self.colle_en_bas {
            self.defilement.poser(self.bas());
        }
    }

    // ----- Les onglets et la liste -----

    /// Le catalogue a change : les onglets, la liste et l'en-tete se refont
    /// s'ils le doivent, et seulement alors.
    fn catalogue_change(&mut self) {
        // --onglet=Atelier-e doit trouver atelier-e : la casse d'un nom tape a la
        // main n'est pas une raison de montrer un projet inconnu.
        if let Vue::Liste(p) = &self.vue {
            if let Some(q) = self.catalogue.projets().into_iter().find(|q| q.eq_ignore_ascii_case(p) && q != p) {
                self.vue = Vue::Liste(q);
            }
        }
        if self.poser_onglets() {
            self.montrer_onglet_actif();
        }
        // « Projets » dit qu'il lit tant que le premier parcours n'est pas
        // fini : la barre se remplissait sous les yeux sans le dire
        // (juge-design.md, m6).
        self.groupe_projets = None;
        self.poser_liste();
        self.poser_entete();
    }

    /// Le projet de la vue : celui de la liste, ou celui du chantier suivi.
    fn projet_de_vue(&self) -> Option<String> {
        match &self.vue {
            Vue::Direct => None,
            Vue::Liste(p) => Some(p.clone()),
            Vue::Suivi(id) | Vue::Chantier(id) => self.catalogue.chantier(id).map(|c| c.projet.clone()),
        }
    }

    /// Le chantier que montre la vue : sa page, son fil, ou celui du direct.
    fn chantier_de_vue(&self) -> Option<String> {
        match &self.vue {
            Vue::Chantier(id) | Vue::Suivi(id) => Some(id.clone()),
            Vue::Direct if !self.chantier.is_empty() => Some(self.chantier.clone()),
            _ => None,
        }
    }

    /// Repose les onglets s'ils ont change ; dit s'ils l'ont fait.
    fn poser_onglets(&mut self) -> bool {
        let projet = match &self.vue {
            Vue::Liste(p) => Some(p.clone()),
            _ => None,
        };
        let voulus = vues::onglets_voulus(&self.catalogue, projet.as_deref());
        if voulus == self.onglets_voulus && !self.onglets.is_empty() {
            return false;
        }
        self.onglets = vues::poser_onglets(&mut self.systeme, &voulus, self.echelle);
        self.onglets_voulus = voulus;
        let d = self.defilement_onglets.cible().min(self.max_onglets());
        self.defilement_onglets.viser(d);
        true
    }

    fn poser_liste(&mut self) {
        if let Vue::Chantier(id) = &self.vue {
            self.poser_page(id.clone());
            return;
        }
        let Vue::Liste(projet) = &self.vue else {
            self.liste = None;
            return;
        };
        let projet = projet.clone();
        let aujourdhui = dates::jour_et_minute(app::maintenant()).0;
        let signature = vues::signature(&self.catalogue, &projet, aujourdhui);
        if let Some(l) = &self.liste {
            if l.projet == projet && l.signature == signature {
                return;
            }
        }
        let colonne = self.mesures().liste_colonne;
        // Le focus reste sur le meme chantier s'il existe encore : une liste
        // qui se refait sous la souris ne doit pas deplacer la selection.
        let ancien = self
            .focus
            .and_then(|i| self.liste.as_ref()?.lignes.get(i).map(|l| l.id.clone()));
        let liste = vues::poser_liste(
            &mut self.systeme,
            &self.catalogue,
            &projet,
            colonne,
            self.echelle,
            aujourdhui,
            signature,
        );
        self.focus = ancien.and_then(|id| liste.lignes.iter().position(|l| l.id == id));
        self.liste = Some(liste);
        let d = self.defilement_liste.cible().min(self.bas_liste());
        self.defilement_liste.viser(d);
    }

    /// La page d'un chantier, refaite quand sa fiche change.
    fn poser_page(&mut self, id: String) {
        let fiche = self.fiche.clone().filter(|f| f.id == id);
        let en_cours = self.catalogue.chantier(&id).is_some_and(|c| c.etat == catalogue::Etat::EnCours);
        let signature = vues::signature_page(fiche.as_deref(), en_cours);
        if let Some(l) = &self.liste {
            if l.projet == id && l.signature == signature {
                return;
            }
        }
        let ancien = self
            .focus
            .and_then(|i| self.liste.as_ref()?.lignes.get(i).map(|l| l.id.clone()));
        let colonne = self.mesures().liste_colonne;
        let page = vues::poser_page(
            &mut self.systeme,
            fiche.as_deref(),
            &id,
            en_cours,
            colonne,
            self.echelle,
            signature,
        );
        self.focus = ancien.and_then(|a| page.lignes.iter().position(|l| l.id == a));
        self.liste = Some(page);
        let d = self.defilement_liste.cible().min(self.bas_liste());
        self.defilement_liste.viser(d);
    }

    /// Cree la texture d'une vignette a sa taille d'affichage, si elle manque.
    fn preparer_vignette(&mut self, i: usize) {
        let Some(fiche) = self.fiche.clone() else { return };
        let Some(v) = self.liste.as_mut().and_then(|l| l.vignettes.get_mut(i)) else { return };
        let Some(img) = fiche.vignettes.get(v.rang).map(|x| x.image.clone()) else { return };
        let (l, h) = (v.image.l as u32, v.image.h as u32);
        if v.texture.as_ref().is_some_and(|t| t.largeur == l && t.hauteur == h) {
            return;
        }
        let texture = if img.largeur == l && img.hauteur == h {
            self.images.texture(&self.peripherique, &self.file, &img.pixels, l, h)
        } else {
            let Some(source) =
                image::ImageBuffer::<image::Rgba<u8>, &[u8]>::from_raw(img.largeur, img.hauteur, &img.pixels[..])
            else {
                return;
            };
            let reduite = image::imageops::resize(&source, l, h, image::imageops::FilterType::Triangle);
            self.images.texture(&self.peripherique, &self.file, reduite.as_raw(), l, h)
        };
        v.texture = Some(texture);
    }

    fn bas_liste(&self) -> f32 {
        self.liste
            .as_ref()
            .map_or(0.0, |l| (l.hauteur - self.visible()).max(0.0))
    }

    fn geo_barre(&self) -> vues::GeoBarre {
        let e = self.echelle;
        vues::geo_barre(
            &self.onglets,
            self.barre.position() - vues::BARRE_L * e,
            self.config.height as f32,
            self.defilement_onglets.position(),
            e,
        )
    }

    fn max_onglets(&self) -> f32 {
        self.geo_barre().max()
    }

    fn geo_liste(&self) -> vues::GeoListe {
        let m = self.mesures();
        vues::GeoListe {
            haut: m.entete_h,
            bas: self.config.height as f32,
            defil: self.defilement_liste.position(),
            x_heure: m.x_bloc,
            x_corps: m.liste_corps,
            colonne: m.liste_colonne,
            large_bloc: m.large_bloc,
            droite: self.config.width as f32,
            e: self.echelle,
        }
    }

    /// Le rang de l'onglet de la vue. Aucun pour un chantier suivi dont le
    /// catalogue ne connait pas encore le projet.
    fn onglet_actif(&self) -> Option<usize> {
        match &self.vue {
            Vue::Direct => Some(0),
            _ => {
                let p = self.projet_de_vue()?;
                self.onglets
                    .iter()
                    .position(|o| o.projet.as_deref() == Some(p.as_str()))
            }
        }
    }

    /// Le plan de la trame : ce qui se clique, et ou, en pixels d'ecran.
    fn plan(&self) -> plan::Plan {
        let e = self.echelle;
        let gb = self.geo_barre();
        let mut fixes = vec![(vues::bouton_barre(e), Cible::Barre)];
        if gb.panneau.x + gb.panneau.l > 0.5 {
            fixes.push((gb.direct, Cible::Onglet(0)));
            let (haut, bas) = (gb.fenetre.y, gb.fenetre.y + gb.fenetre.h);
            for (i, o) in self.onglets.iter().enumerate().skip(1) {
                // Une rangee a demi sortie de sa zone ne se touche que dans
                // ce qui s'en voit.
                let r = gb.rangee(i, o);
                let (y0, y1) = (r.y.max(haut), (r.y + r.h).min(bas));
                if y1 > y0 {
                    fixes.push((plan::Boite { y: y0, h: y1 - y0, ..r }, Cible::Onglet(i)));
                }
            }
            // Le reste du panneau ne laisse rien passer au contenu dessous.
            fixes.push((gb.panneau, Cible::Rien));
            // Rappelee par-dessus une fenetre etroite : un clic a cote la renvoie.
            if self.barre_par_dessus() {
                let x = gb.panneau.x + gb.panneau.l;
                fixes.push((
                    plan::Boite { x, y: 0.0, l: self.config.width as f32 - x, h: self.config.height as f32 },
                    Cible::Barre,
                ));
            }
        }
        let mut p = plan::Plan {
            fixes,
            bande: None,
            onglets: Vec::new(),
            zone: Some(self.geo_liste().zone()),
            lignes: Vec::new(),
            vignettes: Vec::new(),
            images: Vec::new(),
            liens: Vec::new(),
        };
        if let Some(b) = self.boite_entete() {
            p.fixes.push((b, Cible::Entete));
        }
        if let (true, Some(l)) = (self.en_liste(), &self.liste) {
            let gl = self.geo_liste();
            p.lignes = l.lignes.iter().map(|x| gl.plaque(x)).collect();
            p.vignettes = l.vignettes.iter().map(|v| gl.vignette(v)).collect();
        } else if !self.en_liste() {
            // Les images du fil : un clic les ouvre dans la visionneuse.
            let m = self.mesures();
            let d = self.defilement.position();
            for (i, pose) in self.poses.iter().enumerate() {
                let (Some(_), Some(_)) = (&pose.image, &pose.bloc.image) else { continue };
                let y = m.entete_h + pose.y - d;
                if y + pose.hauteur < m.entete_h || y > self.config.height as f32 {
                    continue;
                }
                p.images.push((plan::Boite { x: m.x_corps, y, l: m.colonne, h: pose.hauteur }, i));
            }
            // Les liens : chaque boite de chaque lien, avec son bloc et son
            // rang ; un lien sur deux lignes a deux boites et une cible.
            let e = self.echelle;
            for (i, pose) in self.poses.iter().enumerate() {
                if pose.liens.is_empty() {
                    continue;
                }
                let y = m.entete_h + pose.y - d;
                if y + pose.hauteur < m.entete_h || y > self.config.height as f32 {
                    continue;
                }
                let x0 = self.origine_liens(pose, &m);
                for (k, l) in pose.liens.iter().enumerate() {
                    for b in &l.boites {
                        p.liens.push((plan::boite_du_lien(*b, x0, y, e, l.rangee.then_some(m.x_corps)), i, k));
                    }
                }
            }
        }
        p
    }

    /// Ou commence le texte d'un bloc, pour y poser ses liens.
    fn origine_liens(&self, pose: &Pose, m: &Mesures) -> f32 {
        if matches!(pose.bloc.registre, Registre::Geste | Registre::Image) {
            m.x_corps + VERBE_L * self.echelle
        } else {
            m.x_corps
        }
    }

    /// Le titre de l'en-tete, quand il mene quelque part : d'un fil a la page
    /// de son chantier, d'une page a la liste de son projet. Sa boite est
    /// celle du titre et de la ligne qui le suit, pas de toute la bande : on
    /// vise un texte, pas une surface.
    fn boite_entete(&self) -> Option<plan::Boite> {
        let mene = match &self.vue {
            Vue::Chantier(_) => self.projet_de_vue().is_some(),
            Vue::Suivi(_) | Vue::Direct => self.chantier_de_vue().is_some(),
            Vue::Liste(_) => false,
        };
        if !mene {
            return None;
        }
        let e = self.echelle;
        let m = self.mesures();
        let largeur = |b: &Option<Buffer>| {
            b.as_ref()
                .and_then(|b| b.layout_runs().next().map(|r| r.line_w))
                .unwrap_or(0.0)
        };
        let l = largeur(&self.entete).max(largeur(&self.sous_entete));
        Some(plan::Boite {
            x: self.bords_entete(&m).0 - 6.0 * e,
            y: (vues::ONGLETS_H + 16.0) * e,
            l: l + 12.0 * e,
            h: 60.0 * e,
        })
    }

    fn cible_sous_curseur(&self) -> Cible {
        match self.curseur {
            Some((x, y)) => plan::cible(&self.plan(), x, y),
            None => Cible::Rien,
        }
    }

    /// Change ce que suit le fil. Le fil affiche se vide et repart : garder
    /// les blocs d'un autre chantier sous le nom d'un nouveau serait mentir.
    fn choisir(&mut self, c: Choix) {
        if c == self.choix {
            return;
        }
        self.generation += 1;
        let _ = self.envoi_choix.send((self.generation, c.clone()));
        self.choix = c;
        self.fil_lu = false;
        self.poses.clear();
        self.en_attente.clear();
        self.viser = None;
        self.ancre = None;
        self.replacer();
        self.role.clear();
        self.chantier.clear();
        self.introuvable = None;
        self.colle_en_bas = true;
        self.defilement.poser(0.0);
    }

    fn aller_direct(&mut self) {
        self.vue = Vue::Direct;
        self.choisir(Choix::Direct);
        self.apres_navigation();
    }

    fn ouvrir_projet(&mut self, p: String) {
        if self.vue == Vue::Liste(p.clone()) {
            // Un second clic sur l'onglet actif remonte en haut de la liste,
            // comme partout ailleurs.
            self.defilement_liste.viser(0.0);
            return;
        }
        self.vue = Vue::Liste(p);
        self.focus = None;
        self.defilement_liste.poser(0.0);
        self.apres_navigation();
    }

    /// Le fil d'un chantier : celui de l'agent choisi, ou, sans agent, de
    /// celui qui ecrit.
    fn suivre(&mut self, id: String, agent: Option<String>) {
        self.vue = Vue::Suivi(id.clone());
        self.choisir(match agent {
            Some(agent) => Choix::Agent { chantier: id, agent },
            None => Choix::Chantier(id),
        });
        self.apres_navigation();
    }

    /// La page d'un chantier. Sa fiche se demande a son fil ; tant qu'elle
    /// n'est pas la, la page le dit.
    fn ouvrir_chantier(&mut self, id: String) {
        if self.vue == Vue::Chantier(id.clone()) {
            self.defilement_liste.viser(0.0);
            return;
        }
        if self.fiche.as_ref().is_none_or(|f| f.id != id) {
            self.fiche = None;
        }
        let _ = self.envoi_fiche.send(Some(id.clone()));
        self.vue = Vue::Chantier(id);
        self.focus = None;
        self.liste = None;
        self.defilement_liste.poser(0.0);
        self.apres_navigation();
    }

    /// On remonte d'un niveau, et l'on revient la d'ou l'on est parti, sa
    /// ligne en evidence, comme le Finder remonte d'un dossier en
    /// selectionnant celui qu'on quitte : du fil d'un agent a la page de son
    /// chantier, de la page a la liste du projet.
    fn retour(&mut self) {
        let (cible, ligne) = match self.vue.clone() {
            Vue::Suivi(_) | Vue::Direct => {
                let Some(id) = self.chantier_de_vue() else { return };
                let agent = match &self.choix {
                    Choix::Agent { agent, .. } => agent.clone(),
                    _ => String::new(),
                };
                self.ouvrir_chantier(id);
                (true, agent)
            }
            Vue::Chantier(id) => {
                let Some(p) = self.projet_de_vue() else { return };
                self.ouvrir_projet(p);
                (true, id)
            }
            Vue::Liste(_) => (false, String::new()),
        };
        if cible {
            self.focus = self.liste.as_ref().and_then(|l| l.lignes.iter().position(|x| x.id == ligne));
            if self.focus.is_some() {
                self.deplacer_focus(0);
            }
        }
    }

    fn activer_onglet(&mut self, i: usize) {
        let Some(o) = self.onglets.get(i) else { return };
        match o.projet.clone() {
            None => self.aller_direct(),
            Some(p) => self.ouvrir_projet(p),
        }
        // Rappelee dans une fenetre etroite, elle a servi : elle part, que le
        // projet vienne d'un clic ou d'une touche. Ctrl + 2 la laissait sur la
        // liste, voilee (epreuve.md, tache 1).
        if self.barre_etroite() && self.barre_rappelee {
            self.barre_rappelee = false;
            self.viser_barre();
        }
    }

    fn apres_navigation(&mut self) {
        self.vide = None;
        self.poser_onglets();
        self.poser_liste();
        self.poser_entete();
        self.montrer_onglet_actif();
    }

    /// L'onglet actif se voit toujours, meme au bout d'une bande trop longue :
    /// apres une navigation, et quand la bande se refait (au premier
    /// catalogue, par exemple, pour un --onglet= rejete en fin de bande).
    fn montrer_onglet_actif(&mut self) {
        if let Some(i) = self.onglet_actif().filter(|&i| i > 0) {
            let (debut, fin) = (self.onglets[i].x, self.onglets[i].x + self.onglets[i].l);
            let l = self.geo_barre().fenetre.h;
            let d = plan::garder_visible(
                debut,
                fin,
                l,
                self.defilement_onglets.cible(),
                8.0 * self.echelle,
                self.max_onglets(),
            );
            self.defilement_onglets.viser(d);
        }
    }

    /// L'identifiant de la ligne que le fil suit dans la liste affichee : un
    /// chantier dans la liste d'un projet, un agent dans la page d'un chantier
    /// (« » pour la ligne de celui qui ecrit). Elle porte le rail.
    fn ligne_suivie(&self) -> Option<&str> {
        match (&self.vue, &self.choix) {
            (Vue::Liste(_), Choix::Chantier(id) | Choix::Agent { chantier: id, .. }) => Some(id),
            (Vue::Chantier(p), Choix::Agent { chantier, agent }) if p == chantier => Some(agent),
            (Vue::Chantier(p), Choix::Chantier(id)) if p == id => Some(""),
            _ => None,
        }
    }

    /// Les fleches dans une liste : la ligne en evidence monte ou descend, et
    /// la liste defile juste assez pour qu'elle se voie.
    fn deplacer_focus(&mut self, pas: i32) {
        self.focus_clavier = true;
        let Some(l) = &self.liste else { return };
        if l.lignes.is_empty() {
            return;
        }
        let n = l.lignes.len() as i32;
        let i = match self.focus {
            Some(i) => (i as i32 + pas).clamp(0, n - 1),
            // Rien en evidence : on part de la ligne suivie si elle est la,
            // sinon du haut.
            None => l.lignes.iter().position(|x| Some(x.id.as_str()) == self.ligne_suivie()).unwrap_or(0) as i32,
        } as usize;
        self.focus = Some(i);
        let g = self.geo_liste();
        let b = g.plaque(&l.lignes[i]);
        let debut = b.y - g.haut + g.defil;
        let d = plan::garder_visible(
            debut,
            debut + b.h,
            self.visible(),
            self.defilement_liste.cible(),
            24.0 * self.echelle,
            self.bas_liste(),
        );
        self.defilement_liste.viser(d);
    }

    // ----- La feuille du passage -----

    /// Les secondes depuis le clic.
    fn feuille_ecoule(&self) -> f32 {
        self.ecoule_force.unwrap_or_else(|| self.passage.as_ref().map_or(0.0, |p| p.depuis.elapsed().as_secs_f32()))
    }

    /// Un clic, ou Entree, sur un lien : la feuille s'ouvre sur le passage.
    /// Rien d'autre ne bouge : le fil, la barre, l'agent suivi.
    fn ouvrir_passage(&mut self, url: String) {
        self.passage = Some(PassageOuvert {
            url: url.clone(),
            etat: passage::Etat::Chargement,
            vue: feuille::Vue::Passage,
            depuis: Instant::now(),
            local_confirme: false,
            verdict: navigateur::Sonde::Attente,
            ferme: false,
            page: None,
            jeton: 0,
        });
        self.navigateur = None;
        self.sonde = navigateur::Sonde::Attente;
        self.resoudre_passage(false);
        if let Some(p) = &self.passage {
            eprintln!("lien ouvert : {url} -> {}", nom_de_l_etat(&p.etat));
        }
        self.feuille = None;
        if self.mouvement_reduit() {
            self.feuille_ressort.poser(1.0);
        } else {
            self.feuille_ressort.poser(0.0);
            self.feuille_ressort.viser(1.0);
        }
        self.survoler();
    }

    /// Ce que la feuille montre de l'adresse ouverte : tout de suite quand
    /// aucune lecture n'est necessaire (schema refuse, adresse locale sans
    /// accord, page deja preparee pour une capture), sinon le chargement, et la
    /// page arrive par `recevoir_lecture`.
    fn resoudre_passage(&mut self, local_ok: bool) {
        let Some(p) = self.passage.as_mut() else { return };
        let url = p.url.clone();
        p.depuis = Instant::now();
        p.page = None;
        let classe = passage::classer(&url);
        let sans_reseau = self.reseau.is_some()
            || self.pages.contains_key(&url)
            || matches!(classe, passage::Classe::Refuse(_) | passage::Classe::Gabarit)
            || (classe == passage::Classe::Local && !local_ok);
        if sans_reseau {
            p.etat = passage::resoudre(&url, &self.pages, self.reseau, local_ok);
            return;
        }
        self.lire_jeton += 1;
        p.jeton = self.lire_jeton;
        p.etat = passage::Etat::Chargement;
        let (jeton, envoi) = (self.lire_jeton, self.lire_envoi.clone());
        std::thread::spawn(move || {
            let debut = Instant::now();
            let issue = reseau::lire(&url, local_ok);
            let _ = envoi.send(ArriveeLecture { jeton, url, issue, duree: debut.elapsed() });
        });
    }

    /// La lecture d'une page est arrivee : la feuille passe du chargement a ce
    /// que la page dit.
    fn recevoir_lecture(&mut self) {
        while let Ok(a) = self.lire_recois.try_recv() {
            let Some(p) = self.passage.as_mut() else { continue };
            if p.jeton != a.jeton || p.url != a.url || p.ferme {
                continue;
            }
            let lecture = self.lectures.get(&a.url).and_then(|c| c.lock().ok().map(|l| l.clone())).or_else(|| {
                self.citation_forcee.clone().map(|c| suivi::Lecture { invite: "citation donnée par --citation".into(), citations: vec![c], lu_le: String::new(), repondue: true })
            });
            let (jour, _) = dates::jour_et_minute(app::maintenant());
            let (an, mois, jj) = dates::civil(jour);
            let releve = format!("{an:04}-{mois:02}-{jj:02}");
            let (etat, page) = passage::etat_de_l_issue(&a.url, &a.issue, lecture.as_ref(), &releve);
            eprintln!("lecture : {} -> {} en {:.2} s", a.url, nom_de_l_etat(&etat), a.duree.as_secs_f32());
            p.etat = etat;
            p.page = page;
            p.depuis = Instant::now();
            self.feuille = None;
        }
    }

    /// Le passage est-il connu avant le clic : l'agent a cite quelque chose.
    fn passage_connu(&self, url: &str) -> bool {
        passage::passage_connu(url, &self.pages) || self.lectures.get(url).is_some_and(|c| c.lock().is_ok_and(|l| !l.citations.is_empty()))
    }

    /// L'agent a lu cette page et n'en a rien cite.
    fn aucun_passage_cite(&self, url: &str) -> bool {
        self.pages.get(url).is_some_and(|p| p.citation.is_none()) || self.lectures.get(url).is_some_and(|c| c.lock().is_ok_and(|l| l.repondue && l.citations.is_empty()))
    }

    /// Le moteur de page suit la feuille : il n'existe que pendant que la page
    /// est ouverte, dans le rectangle que la feuille lui reserve.
    fn synchroniser_navigateur(&mut self) {
        if let (false, Some(s)) = (self.page_permise, self.sonde_forcee) {
            let page = self.passage.as_ref().is_some_and(|p| p.vue == feuille::Vue::Page);
            let voulue = if page { s } else { navigateur::Sonde::Attente };
            if let Some(p) = self.passage.as_mut() {
                if p.verdict != s {
                    p.verdict = s;
                    self.feuille = None;
                }
            }
            if self.sonde != voulue {
                self.sonde = voulue;
                self.feuille = None;
            }
            return;
        }
        let voulu = self.page_permise
            && self.passage.as_ref().is_some_and(|p| {
                p.vue == feuille::Vue::Page && !p.ferme && self.feuille_ressort.au_repos() && matches!(passage::etat_de_fond(&p.etat), passage::Etat::Passage(_) | passage::Etat::Introuvable { .. } | passage::Etat::SansPassage { .. })
            });
        if !voulu {
            if self.navigateur.take().is_some() {
                eprintln!("page fermee");
            }
            if self.sonde != navigateur::Sonde::Indisponible || self.passage.is_none() {
                self.sonde = navigateur::Sonde::Attente;
            }
            return;
        }
        self.composer_feuille();
        let zone = self.feuille.as_ref().and_then(|(_, c)| c.zone_page);
        let Some(z) = zone else { return };
        let zone = [z.x, z.y, z.l, z.h];
        if self.navigateur.is_none() && self.sonde != navigateur::Sonde::Indisponible && !navigateur::page_activee() {
            eprintln!("page desactivee (VIGIE_SANS_PAGE) : rien n'est ouvert");
            self.sonde = navigateur::Sonde::Indisponible;
            self.feuille = None;
            return;
        }
        if self.navigateur.is_none() && self.sonde != navigateur::Sonde::Indisponible {
            let Some(p) = self.passage.as_ref() else { return };
            let (adresse, cherche) = passage::adresse_a_ouvrir(&p.url, &p.etat);
            match navigateur::Navigateur::ouvrir(&self.fenetre, zone, &adresse, cherche.as_deref(), p.local_confirme) {
                Ok(n) => {
                    eprintln!("page ouverte : {adresse}");
                    self.navigateur = Some(n);
                }
                Err(e) => {
                    eprintln!("la page ne s'ouvre pas : {e}");
                    self.sonde = navigateur::Sonde::Indisponible;
                    self.feuille = None;
                }
            }
            return;
        }
        let mut geste = None;
        if let Some(n) = self.navigateur.as_mut() {
            n.poser(zone);
            n.liberer();
            n.tic();
            let s = n.sonde();
            let echap = n.echap_demande();
            geste = n.geste_demande();
            if s != self.sonde {
                eprintln!("sonde : {s:?}");
                self.sonde = s;
                self.feuille = None;
                if let (Some(p), true) = (self.passage.as_mut(), matches!(s, navigateur::Sonde::Trouve | navigateur::Sonde::NonRetrouve)) {
                    p.verdict = s;
                }
            }
            if echap {
                eprintln!("echap dans la page");
                self.echap_feuille();
            }
        }
        match geste {
            Some(navigateur::Geste::Reculer) => self.agir_feuille(feuille::Action::Reculer),
            Some(navigateur::Geste::Avancer) => self.agir_feuille(feuille::Action::Avancer),
            None => {}
        }
    }

    fn fermer_passage(&mut self) {
        let Some(p) = self.passage.as_mut() else { return };
        if p.ferme {
            return;
        }
        p.ferme = true;
        if self.mouvement_reduit() {
            self.feuille_ressort.poser(0.0);
        } else {
            self.feuille_ressort.viser(0.0);
        }
        self.avancer_feuille(0.0);
    }

    /// Une trame de la feuille : son ressort avance ; refermee, elle disparait.
    /// L'ecran d'ouverture : la toile decodee arrive dans sa texture, le temps
    /// passe (il ne compte qu'une fois la toile montree), et elle commence a
    /// s'effacer quand les donnees sont la et qu'on l'a vue assez longtemps.
    fn avancer_ouverture(&mut self, dt: f32, dt_ressort: f32) {
        if self.ouverture.is_none() {
            return;
        }
        if let Some(r) = &self.ouverture_recue {
            match r.try_recv() {
                Ok(d) => {
                    self.ouverture_texture = Some(self.images.texture_reduite(
                        &self.peripherique,
                        &self.file,
                        &d.pixels,
                        d.largeur,
                        d.hauteur,
                        &d.niveaux,
                    ));
                    self.ouverture_taille = (d.largeur, d.hauteur);
                    self.ouverture_recue = None;
                }
                // Le fil est parti sans rien rendre : la toile ne se decode
                // pas, et la vigie s'ouvre sans elle.
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.ouverture_recue = None;
                    self.ouverture = None;
                    return;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        let reduit = self.mouvement_reduit();
        let montree = self.ouverture_texture.is_some();
        let pretes = self.donnees_pretes;
        let Some(o) = self.ouverture.as_mut() else { return };
        o.avancer(if montree { dt } else { 0.0 }, dt_ressort);
        // Si les donnees ne venaient jamais, elle ne retiendrait pas la vigie
        // plus de six secondes.
        if o.doit_sortir(pretes || o.age >= 6.0, reduit) {
            eprintln!("ouverture : elle s'efface apres {:.2} s (donnees pretes : {pretes})", o.age);
            o.sortir(reduit);
        }
        if o.finie() {
            eprintln!("ouverture : effacee");
            self.ouverture = None;
            self.ouverture_texture = None;
        }
    }

    /// Un clic ou une touche pendant qu'elle est pleine la passe, et ne fait
    /// rien d'autre. Dit si l'entree a ete prise.
    fn ouverture_passer(&mut self) -> bool {
        let reduit = self.mouvement_reduit();
        if let Some(o) = self.ouverture.as_mut() {
            if !o.sortie && o.opacite() > 0.5 {
                o.sortir(reduit);
                return true;
            }
        }
        false
    }

    fn avancer_feuille(&mut self, dt: f32) {
        self.recevoir_lecture();
        self.synchroniser_navigateur();
        let reduit = self.mouvement_reduit();
        self.feuille_ressort.avancer(vues::pas_des_ressorts(dt, reduit));
        if self.passage.as_ref().is_some_and(|p| p.ferme) && self.feuille_ressort.au_repos() {
            self.passage = None;
            self.feuille = None;
            self.survoler();
        }
    }

    /// Echap : la page revient au passage, le passage se ferme. La vigie, elle,
    /// ne se ferme jamais tant qu'une feuille est ouverte.
    fn echap_feuille(&mut self) {
        match self.passage.as_mut() {
            Some(p) if p.vue == feuille::Vue::Page && !p.ferme => {
                p.vue = feuille::Vue::Passage;
                self.feuille = None;
            }
            Some(_) => self.fermer_passage(),
            None => {}
        }
    }

    fn agir_feuille(&mut self, a: feuille::Action) {
        use feuille::Action;
        let Some(p) = self.passage.as_mut() else { return };
        if p.ferme {
            return;
        }
        match a {
            Action::Fermer => self.fermer_passage(),
            Action::OuvrirPage => {
                p.vue = feuille::Vue::Page;
                self.feuille = None;
            }
            Action::Reculer => match self.navigateur.as_ref() {
                // Dans la page, reculer est reculer dans la page ; au bout de son
                // historique, c'est revenir au passage.
                Some(n) if n.peut_reculer() => n.reculer(),
                _ => {
                    p.vue = feuille::Vue::Passage;
                    self.feuille = None;
                }
            },
            Action::Avancer => {
                if let Some(n) = self.navigateur.as_ref().filter(|n| n.peut_avancer()) {
                    n.avancer();
                    self.feuille = None;
                }
            }
            Action::RevoirPassage => {
                p.vue = feuille::Vue::Passage;
                self.feuille = None;
            }
            Action::Reessayer => {
                let ok = p.local_confirme;
                self.resoudre_passage(ok);
                self.feuille = None;
            }
            Action::ConfirmerLocal => {
                p.local_confirme = true;
                self.resoudre_passage(true);
                self.feuille = None;
            }
            Action::OuvrirAilleurs => {
                // Un geste explicite de Stone, et le seul chemin vers le
                // navigateur du systeme : sans interpreteur de commandes, donc
                // sans que l'adresse puisse y etre lue comme une commande.
                if matches!(passage::classer(&p.url), passage::Classe::Web | passage::Classe::NonChiffre) {
                    let _ = std::process::Command::new("rundll32.exe").args(["url.dll,FileProtocolHandler", p.url.as_str()]).spawn();
                }
            }
        }
    }

    /// La composition de la feuille, refaite seulement si quelque chose a change.
    fn composer_feuille(&mut self) -> bool {
        let Some(p) = self.passage.as_ref() else {
            self.feuille = None;
            return false;
        };
        let (l, h, e) = (self.config.width as f32, self.config.height as f32, self.echelle);
        let ecoule = self.feuille_ecoule();
        let cle = format!(
            "{}|{:?}|{:?}|{}x{}|{}|{}|{}|{}",
            p.url,
            p.etat,
            p.vue,
            l,
            h,
            e,
            palette::clair(),
            palette::contraste(),
            if matches!(p.etat, passage::Etat::Chargement) { ecoule.floor() } else { 0.0 },
        ) + &format!("|{:?}|{:?}|{}|{}", self.sonde, p.verdict, self.navigateur.as_ref().map_or(String::new(), |n| n.courante()), self.navigateur.as_ref().is_some_and(|n| n.peut_avancer()));
        if self.feuille.as_ref().is_some_and(|(k, _)| *k == cle) {
            return true;
        }
        let page = p.page.as_ref().or_else(|| self.pages.get(&p.url));
        let courante = self.navigateur.as_ref().map(|n| n.courante());
        let peut_avancer = self.navigateur.as_ref().is_some_and(|n| n.peut_avancer());
        let entree = feuille::Entree { url: &p.url, courante: courante.as_deref(), etat: &p.etat, page, vue: p.vue, ecoule, sonde: self.sonde, peut_avancer, verdict: p.verdict };
        let c = feuille::composer(&mut self.systeme, &entree, l, h, e);
        let lignes: Vec<String> = c.surbrillance.iter().map(|b| format!("[{:.0},{:.0},{:.0},{:.0}]", b.x, b.y, b.l, b.h)).collect();
        eprintln!(
            "feuille : carte [{:.0},{:.0},{:.0},{:.0}], surbrillance {} ligne(s) {}, page {}",
            c.carte.x,
            c.carte.y,
            c.carte.l,
            c.carte.h,
            lignes.len(),
            lignes.join(" "),
            c.zone_page.map_or("-".to_string(), |z| format!("[{:.0},{:.0},{:.0},{:.0}]", z.x, z.y, z.l, z.h))
        );
        if let Some(b) = c.bloc {
            eprintln!("feuille bloc : [{:.0},{:.0},{:.0},{:.0}]", b.x, b.y, b.l, b.h);
        }
        for t in &c.textes {
            let b = t.boite();
            eprintln!(
                "feuille texte : [{:.0},{:.0},{:.0},{:.0}] #{:02x}{:02x}{:02x} {:?}",
                b.x,
                b.y,
                b.l,
                b.h,
                t.couleur.r(),
                t.couleur.g(),
                t.couleur.b(),
                t.debut()
            );
        }
        for (b, a) in &c.cibles {
            eprintln!("feuille cible : [{:.0},{:.0},{:.0},{:.0}] {a:?}", b.x, b.y, b.l, b.h);
        }
        self.feuille = Some((cle, c));
        true
    }

    /// Ce qui, dans la feuille, est sous le point.
    fn action_sous(&self, x: f32, y: f32) -> Option<feuille::Action> {
        let (_, c) = self.feuille.as_ref()?;
        c.cibles.iter().find(|(b, _)| b.contient(x, y)).map(|(_, a)| *a)
    }

    fn clic_feuille(&mut self, x: f32, y: f32) {
        self.composer_feuille();
        if let Some(a) = self.action_sous(x, y) {
            self.agir_feuille(a);
            return;
        }
        // Hors de la carte, sur le voile : un clic ferme, comme partout.
        let dehors = self.feuille.as_ref().is_some_and(|(_, c)| c.zone_page.is_none() && !c.carte.contient(x, y));
        if dehors {
            self.fermer_passage();
        }
    }

    /// Ce que fait Entree dans la feuille : son action principale.
    fn entree_feuille(&mut self) {
        let Some(p) = self.passage.as_ref() else { return };
        if p.vue == feuille::Vue::Page {
            return;
        }
        if let Some(a) = feuille::action_principale(&p.etat, self.feuille_ecoule()) {
            self.agir_feuille(a);
        }
    }

    /// Les liens que le fil montre, dans l'ordre de lecture, un par lien.
    fn liens_visibles(&self) -> Vec<(usize, usize)> {
        let mut v: Vec<(f32, f32, usize, usize)> = Vec::new();
        for (b, i, k) in self.plan().liens {
            if !v.iter().any(|&(_, _, i2, k2)| i2 == i && k2 == k) {
                v.push((b.y, b.x, i, k));
            }
        }
        v.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
        v.into_iter().map(|(_, _, i, k)| (i, k)).collect()
    }

    /// Tab et Maj + Tab : d'un lien visible au suivant.
    fn parcourir_liens(&mut self, pas: i32) {
        let tous = self.liens_visibles();
        if tous.is_empty() {
            self.lien_focus = None;
            return;
        }
        let n = tous.len() as i32;
        let courant = self.lien_focus.and_then(|f| tous.iter().position(|&x| x == f)).map(|p| p as i32);
        let suivant = match (courant, pas) {
            (Some(c), p) => (c + p).rem_euclid(n),
            (None, p) if p > 0 => 0,
            (None, _) => n - 1,
        };
        self.lien_focus = Some(tous[suivant as usize]);
    }

    /// Les liens que la capture montre, avec ce qu'il faut pour les mesurer sur
    /// ses pixels : la boite de ce qui se souligne, la couleur du texte, le
    /// corps. Une ligne par boite.
    fn journal_des_liens(&self) {
        if self.passage.is_some() || self.en_liste() {
            return;
        }
        let m = self.mesures();
        let e = self.echelle;
        let d = self.defilement.position();
        for (i, pose) in self.poses.iter().enumerate() {
            let y = m.entete_h + pose.y - d;
            if pose.liens.is_empty() || y + pose.hauteur < m.entete_h || y > self.config.height as f32 {
                continue;
            }
            let x0 = self.origine_liens(pose, &m);
            let (c, corps) = if matches!(pose.bloc.registre, Registre::Geste) { (palette::encre3(), GESTE_CORPS) } else { (couleur(pose.bloc.registre), CORPS) };
            for (k, l) in pose.liens.iter().enumerate() {
                for b in &l.soulignees {
                    if y + b.y < m.entete_h {
                        continue;
                    }
                    eprintln!(
                        "lien visible : [{:.0},{:.0},{:.0},{:.0}] corps {:.1} #{:02x}{:02x}{:02x} bloc {i} rang {k} connu {} {}",
                        x0 + b.x,
                        y + b.y,
                        b.l,
                        b.h,
                        corps * e,
                        c.r(),
                        c.g(),
                        c.b(),
                        self.passage_connu(&l.url),
                        l.url
                    );
                }
            }
        }
    }

    /// Un lien d'apres --lien= : son rang parmi les visibles, ou un morceau de
    /// son adresse (le premier lien visible qui le contient).
    fn lien_nomme(&self, n: &str, tous: &[(usize, usize)]) -> Option<(usize, usize)> {
        match n.parse::<usize>() {
            Ok(r) => tous.get(r).copied(),
            Err(_) => tous.iter().copied().find(|&(b, k)| self.url_du_lien(b, k).is_some_and(|u| u.contains(n))),
        }
    }

    fn url_du_lien(&self, bloc: usize, rang: usize) -> Option<String> {
        self.poses.get(bloc)?.liens.get(rang).map(|l| l.url.clone())
    }

    fn cliquer(&mut self, c: Cible) {
        match c {
            Cible::Lien(b, k) => {
                if let Some(u) = self.url_du_lien(b, k) {
                    self.ouvrir_passage(u);
                }
            }
            Cible::Onglet(i) => self.activer_onglet(i),
            Cible::Barre => self.basculer_barre(),
            Cible::Ligne(i) => {
                let id = self
                    .liste
                    .as_ref()
                    .and_then(|l| l.lignes.get(i))
                    .map(|l| l.id.clone());
                match (self.vue.clone(), id) {
                    (Vue::Liste(_), Some(id)) => self.ouvrir_chantier(id),
                    // La ligne vide est celle de l'agent qui ecrit.
                    (Vue::Chantier(c), Some(a)) => self.suivre(c, Some(a).filter(|a| !a.is_empty())),
                    _ => {}
                }
            }
            Cible::Vignette(i) => self.ouvrir_visionneuse(visionneuse::Provenance::Planche(i)),
            Cible::Image(i) => self.ouvrir_visionneuse(visionneuse::Provenance::Fil(i)),
            Cible::Entete => match self.vue.clone() {
                Vue::Chantier(_) => {
                    if let Some(p) = self.projet_de_vue() {
                        self.ouvrir_projet(p);
                    }
                }
                _ => self.retour(),
            },
            Cible::Rien => {}
        }
    }

    // ----- La visionneuse -----

    /// Ou est, a l'ecran, l'image d'une provenance : sa vignette ou son bloc
    /// du fil, s'il est visible sous l'en-tete. Aucune sinon.
    fn source_de(&self, p: visionneuse::Provenance) -> Option<plan::Boite> {
        let m = self.mesures();
        let b = match p {
            visionneuse::Provenance::Fil(i) => {
                let pose = self.poses.get(i)?;
                let ip = pose.image.as_ref()?;
                let y = m.entete_h + pose.y - self.defilement.position();
                plan::Boite { x: m.x_corps.round(), y: (y + JOINTE_AIR * self.echelle).round(), l: ip.largeur, h: ip.hauteur }
            }
            visionneuse::Provenance::Planche(j) => {
                let v = self.liste.as_ref()?.vignettes.get(j)?;
                let cadre = self.geo_liste().vignette(v);
                plan::Boite { x: m.liste_corps + v.image.x, y: cadre.y + (v.image.y - v.cadre.y), ..v.image }
            }
        };
        (b.y + b.h > m.entete_h && b.y < self.config.height as f32).then_some(b)
    }

    /// Ouvre en grand l'image d'une provenance, parmi ses voisines : toutes
    /// les images du fil, ou toute la planche.
    fn ouvrir_visionneuse(&mut self, depuis: visionneuse::Provenance) {
        use visionneuse::{Element, Provenance};
        let heure_courte = |h: &str| h.get(..5).unwrap_or(h).to_string();
        let mut elements: Vec<Element> = Vec::new();
        match depuis {
            Provenance::Fil(_) => {
                for (i, p) in self.poses.iter().enumerate() {
                    let (Some(img), Some(_)) = (&p.bloc.image, &p.image) else { continue };
                    let mut d = vec![self.role.clone(), heure_courte(&p.bloc.heure)];
                    d.retain(|x| !x.is_empty());
                    elements.push(Element { image: img.clone(), provenance: Provenance::Fil(i), agent: None, details: d.join("  ·  ") });
                }
            }
            Provenance::Planche(_) => {
                let (Some(l), Some(f)) = (&self.liste, &self.fiche) else { return };
                for (j, v) in l.vignettes.iter().enumerate() {
                    let Some(x) = f.vignettes.get(v.rang) else { continue };
                    let mut d = vec![x.role.clone(), x.heure.clone()];
                    d.retain(|x| !x.is_empty());
                    elements.push(Element {
                        image: x.image.clone(),
                        provenance: Provenance::Planche(j),
                        agent: Some(x.agent.clone()),
                        details: d.join("  ·  "),
                    });
                }
            }
        }
        let Some(rang) = elements.iter().position(|e| e.provenance == depuis) else { return };
        let Some(depart) = self.source_de(depuis) else { return };
        self.visionneuse = Some(visionneuse::Visionneuse::ouvrir(elements, rang, depart, self.animations_coupees));
        self.charger_visionneuse();
        self.survoler();
    }

    /// L'element en cours a change : sa video se lance, ou son image pleine se
    /// lit dans un fil a part. La vignette se montre en attendant.
    fn charger_visionneuse(&mut self) {
        self.vis_texture = None;
        self.vis_pleine = None;
        self.vis_textes = None;
        let Some(v) = self.visionneuse.as_mut() else { return };
        let el = v.element().clone();
        let Some(chemin) = el.image.chemin.clone() else { return };
        if el.image.video.is_some() {
            // Comme Coup d'oeil, une video ouverte se lit tout de suite.
            v.lecture = video::Lecture::ouvrir(&chemin, 0.0, true);
            if !el.image.est_audio() {
                return;
            }
            // Un son : la vignette est son onde reduite ; l'onde pleine, a la
            // taille de la visionneuse, arrive dans un fil a part comme l'image
            // pleine d'une capture.
            let rang = v.rang;
            let (envoi, recois) = std::sync::mpsc::channel();
            self.vis_pleine = Some(recois);
            std::thread::spawn(move || {
                let Some((img, _)) = medias::onde(&chemin) else { return };
                let _ = envoi.send((
                    rang,
                    suivi::Image {
                        largeur: img.width(),
                        hauteur: img.height(),
                        pixels: img.into_raw(),
                        chemin: Some(chemin),
                        origine: el.image.origine,
                        video: el.image.video,
                        source: el.image.source,
                        nommee: el.image.nommee,
                    },
                    Vec::new(),
                ));
            });
            return;
        }
        let rang = v.rang;
        let (envoi, recois) = std::sync::mpsc::channel();
        self.vis_pleine = Some(recois);
        // L'image a sa vraie taille, pour qu'un zoom en montre chaque pixel :
        // bornee seulement par la plus grande texture de la carte.
        let max = self.peripherique.limits().max_texture_dimension_2d.min(16384);
        std::thread::spawn(move || {
            let Ok(img) = image::open(&chemin) else { return };
            let img = medias::reduire(img.to_rgba8(), (max, max));
            // Ses reductions, chacune la moitie de la precedente, jusqu'a 64
            // pixels : les tailles exactes que la carte attend d'une chaine.
            let mut niveaux: Vec<visionneuse::Niveau> = Vec::new();
            let mut prec = img.clone();
            while prec.width().max(prec.height()) > 64 {
                let (l, h) = ((prec.width() / 2).max(1), (prec.height() / 2).max(1));
                prec = image::imageops::resize(&prec, l, h, image::imageops::FilterType::Triangle);
                niveaux.push((l, h, prec.as_raw().clone()));
            }
            let _ = envoi.send((
                rang,
                suivi::Image {
                    largeur: img.width(),
                    hauteur: img.height(),
                    pixels: img.into_raw(),
                    chemin: Some(chemin),
                    origine: el.image.origine,
                    video: None,
                    source: el.image.source,
                    nommee: el.image.nommee,
                },
                niveaux,
            ));
        });
    }

    /// Se ferme vers sa vignette, ou, si elle n'est plus a l'ecran, vers un
    /// point au centre de la fenetre.
    fn fermer_visionneuse(&mut self) {
        let Some(p) = self.visionneuse.as_ref().map(|v| v.element().provenance) else { return };
        let (l, h) = (self.config.width as f32, self.config.height as f32);
        let retour = self
            .source_de(p)
            .unwrap_or(plan::Boite { x: l * 0.5 - 1.0, y: h * 0.5 - 1.0, l: 2.0, h: 2.0 });
        let coupees = self.animations_coupees;
        if let Some(v) = self.visionneuse.as_mut() {
            v.fermer(retour, coupees);
        }
        self.vis_textes = None;
    }

    fn voisine_visionneuse(&mut self, pas: i32) {
        if self.visionneuse.as_mut().is_some_and(|v| v.voisine(pas)) {
            self.charger_visionneuse();
        }
    }

    /// Lecture ou pause de la video ouverte.
    fn basculer_video(&mut self) {
        if let Some(v) = self.visionneuse.as_mut() {
            v.lecture = v.lecture.take().and_then(|l| l.basculer());
        }
    }

    /// Depuis la planche : l'image dans le fil de l'agent qui l'a faite, comme
    /// le faisait le clic avant la visionneuse. Depuis le fil, elle y est deja.
    fn voir_dans_le_fil(&mut self) {
        let Some(v) = self.visionneuse.as_ref() else { return };
        let el = v.element().clone();
        match (el.provenance, el.agent, self.vue.clone()) {
            (visionneuse::Provenance::Planche(_), Some(agent), Vue::Chantier(c)) => {
                self.visionneuse = None;
                self.vis_texture = None;
                self.suivre(c, Some(agent));
                self.viser = el.image.chemin.clone();
            }
            _ => self.fermer_visionneuse(),
        }
    }

    /// L'element ouvert ajuste a la fenetre, avant tout zoom.
    fn ajustee(&self) -> Option<plan::Boite> {
        let v = self.visionneuse.as_ref()?;
        let (l, h) = (self.config.width as f32, self.config.height as f32);
        let e = self.echelle;
        Some(visionneuse::ajuster(v.element().image.source, visionneuse::zone(l, h, e, v.video()), e))
    }

    /// Ou est l'image ouverte, zoom compris (une video ne se zoome pas).
    fn image_ouverte(&self) -> Option<plan::Boite> {
        let v = self.visionneuse.as_ref()?;
        let aj = self.ajustee()?;
        if v.video() {
            return Some(aj);
        }
        let (z, d) = v.zoom();
        Some(visionneuse::zoomee(aj, z, d))
    }

    /// Grossit l'image ouverte de f autour du point p (le centre de la
    /// fenetre par defaut), sur le ressort, a partir de ce qui est deja vise.
    fn zoomer_visionneuse(&mut self, f: f32, p: Option<(f32, f32)>) {
        let Some(aj) = self.ajustee() else { return };
        let fen = (self.config.width as f32, self.config.height as f32);
        let coupees = self.animations_coupees;
        let Some(v) = self.visionneuse.as_mut() else { return };
        if v.video() || v.ferme {
            return;
        }
        let p = p.unwrap_or((fen.0 * 0.5, fen.1 * 0.5));
        let source = v.element().image.source;
        let (z, d) = v.zoom_vise();
        let (z, d) = visionneuse::autour(aj, z, d, f, p, fen, visionneuse::loupe_max(aj, source));
        v.viser_zoom(z, d, coupees);
    }

    /// Le double-clic : de l'image ajustee a sa vraie taille autour du point
    /// vise, ou de retour a l'ajustee.
    fn basculer_zoom(&mut self, p: (f32, f32)) {
        let (Some(aj), Some(v)) = (self.ajustee(), self.visionneuse.as_ref()) else { return };
        if v.zoomee() {
            let coupees = self.animations_coupees;
            if let Some(v) = self.visionneuse.as_mut() {
                v.viser_zoom(1.0, (0.0, 0.0), coupees);
            }
            return;
        }
        let z = visionneuse::loupe_reelle(aj, v.element().image.source);
        self.zoomer_visionneuse(z / v.zoom_vise().0, Some(p));
    }

    /// Deplace l'image zoomee, d'un coup : elle suit la main ou le pave.
    fn deplacer_visionneuse(&mut self, d: (f32, f32)) {
        let Some(aj) = self.ajustee() else { return };
        let fen = (self.config.width as f32, self.config.height as f32);
        let Some(v) = self.visionneuse.as_mut() else { return };
        let z = v.zoom_vise().0;
        let d = visionneuse::borner(aj, z, d, fen);
        v.dx.poser(d.0);
        v.dy.poser(d.1);
    }

    /// La molette sur la visionneuse. Un cran de souris, ou le pincement du
    /// pave tactile (que Windows envoie avec Ctrl), zoome autour de la souris,
    /// comme dans Photos ; le glissement a deux doigts deplace une image
    /// zoomee, comme dans Aperçu.
    fn molette_visionneuse(&mut self, delta: MouseScrollDelta, ctrl: bool) {
        let Some(v) = self.visionneuse.as_ref() else { return };
        if v.video() || v.ferme {
            return;
        }
        let (_, d) = v.zoom_vise();
        let zoomee = v.zoomee();
        let e = self.echelle;
        match delta {
            MouseScrollDelta::LineDelta(x, y) => {
                let cran = x == 0.0 && y != 0.0 && y.fract() == 0.0;
                if ctrl || cran {
                    self.zoomer_visionneuse(visionneuse::LOUPE_PAS.powf(y), self.curseur);
                } else if zoomee {
                    self.deplacer_visionneuse((d.0 + x * 60.0 * e, d.1 + y * 60.0 * e));
                }
            }
            MouseScrollDelta::PixelDelta(q) => {
                if ctrl {
                    self.zoomer_visionneuse((q.y as f32 * 0.004).exp(), self.curseur);
                } else if zoomee {
                    self.deplacer_visionneuse((d.0 + q.x as f32, d.1 + q.y as f32));
                }
            }
        }
    }

    /// Ou tombe un clic, la visionneuse ouverte : la barre d'une video, la
    /// video elle-meme (lecture, pause), l'image (un double-clic la zoome), ou
    /// le fond, qui la ferme.
    fn clic_visionneuse(&mut self, x: f32, y: f32) {
        let Some(v) = self.visionneuse.as_ref() else { return };
        if v.ferme {
            return;
        }
        let e = self.echelle;
        let (l, h) = (self.config.width as f32, self.config.height as f32);
        let Some(cible) = self.image_ouverte() else { return };
        if v.video() {
            let (piste, zone_barre) = visionneuse::barre(l, h, e);
            if zone_barre.contient(x, y) {
                if let Some(v) = self.visionneuse.as_mut() {
                    if let Some(lec) = v.lecture.take() {
                        let t = visionneuse::position_sur(piste, x, lec.forme.duree);
                        let joue = lec.joue();
                        v.lecture = lec.aller(t, joue);
                    }
                }
                return;
            }
            if cible.contient(x, y) {
                if self.visionneuse.as_ref().is_some_and(|v| v.element().image.est_audio()) {
                    // Sur l'onde, un clic va a l'endroit du son qu'il touche.
                    if let Some(v) = self.visionneuse.as_mut() {
                        if let Some(lec) = v.lecture.take() {
                            let t = visionneuse::position_sur(cible, x, lec.forme.duree);
                            let joue = lec.joue();
                            v.lecture = lec.aller(t, joue);
                        }
                    }
                } else {
                    self.basculer_video();
                }
                return;
            }
        } else if cible.contient(x, y) {
            // Le delai et l'ecart d'un double-clic sont ceux de Windows par
            // defaut : 500 ms, quatre pixels.
            let double = self
                .dernier_clic
                .is_some_and(|(t, x0, y0)| t.elapsed().as_millis() < 500 && (x - x0).abs() <= 4.0 && (y - y0).abs() <= 4.0);
            if double {
                self.dernier_clic = None;
                self.basculer_zoom((x, y));
            } else {
                self.dernier_clic = Some((Instant::now(), x, y));
            }
            return;
        }
        self.fermer_visionneuse();
    }

    /// Une trame de la visionneuse : son ressort avance, l'image pleine arrive,
    /// la video donne l'image due. Dit s'il faut redessiner encore.
    fn avancer_visionneuse(&mut self, dt: f32) -> bool {
        let Some(v) = self.visionneuse.as_mut() else { return false };
        v.avancer(dt);
        if v.partie() {
            self.visionneuse = None;
            self.vis_texture = None;
            self.vis_pleine = None;
            self.vis_textes = None;
            self.survoler();
            return true;
        }
        if let Some(r) = &self.vis_pleine {
            if let Ok((rang, img, niveaux)) = r.try_recv() {
                if rang == v.rang {
                    v.pleine = Some(visionneuse::Pleine { rang, image: Arc::new(img), niveaux });
                }
            }
        }
        let rang = v.rang;
        if let Some(lec) = v.lecture.as_mut() {
            if let Some(pixels) = lec.image_due() {
                let (l, h) = (lec.largeur, lec.hauteur);
                let a_refaire = !matches!(&self.vis_texture, Some((r, 2, t)) if *r == rang && t.largeur == l && t.hauteur == h);
                if a_refaire {
                    let tex = self.images.texture(&self.peripherique, &self.file, &pixels, l, h);
                    self.vis_texture = Some((rang, 2, tex));
                } else if let Some((_, _, tex)) = &self.vis_texture {
                    tex.ecrire(&self.file, &pixels);
                }
            }
        }
        true
    }

    /// La texture de l'element en cours : la video si elle a donne une image,
    /// sinon l'image pleine, sinon la vignette.
    fn preparer_visionneuse(&mut self) {
        let Some(v) = self.visionneuse.as_ref() else { return };
        let rang = v.rang;
        if matches!(&self.vis_texture, Some((r, 2, _)) if *r == rang) {
            return;
        }
        let genre = if v.pleine.as_ref().is_some_and(|p| p.rang == rang) { 1u8 } else { 0u8 };
        if matches!(&self.vis_texture, Some((r, g, _)) if *r == rang && *g == genre) {
            return;
        }
        let tex = match &v.pleine {
            Some(p) if genre == 1 => {
                let img = &p.image;
                self.images
                    .texture_reduite(&self.peripherique, &self.file, &img.pixels, img.largeur, img.hauteur, &p.niveaux)
            }
            _ => {
                let img = v.element().image.clone();
                self.images.texture(&self.peripherique, &self.file, &img.pixels, img.largeur, img.hauteur)
            }
        };
        self.vis_texture = Some((rang, genre, tex));
    }

    /// Ce que survole la souris : la ligne passe en evidence, et la main
    /// remplace la fleche au-dessus de ce qui se clique. Dit si ce qu'elle
    /// survole a change, donc s'il faut redessiner : la fenetre ne se redessine
    /// pas d'elle-meme quand rien n'arrive.
    fn survoler(&mut self) -> bool {
        if self.passage.is_some() {
            // Sous la feuille, rien d'autre ne se survole.
            self.composer_feuille();
            let sur = self.curseur.and_then(|(x, y)| self.action_sous(x, y));
            self.survol = Cible::Rien;
            self.poser_icone(if sur.is_some() { CursorIcon::Pointer } else { CursorIcon::Default });
            return true;
        }
        if let Some(v) = self.visionneuse.as_ref() {
            // Sous la visionneuse, rien d'autre ne se survole. La main vient sur
            // une video et sa barre ; sur une image, la loupe dit qu'elle se
            // zoome, et la main ouverte qu'une image zoomee se glisse.
            let (l, h) = (self.config.width as f32, self.config.height as f32);
            let sur_image = self.curseur.is_some_and(|(x, y)| self.image_ouverte().is_some_and(|b| b.contient(x, y)));
            let icone = if v.ferme {
                CursorIcon::Default
            } else if v.video() {
                let sur_barre = self.curseur.is_some_and(|(x, y)| visionneuse::barre(l, h, self.echelle).1.contient(x, y));
                if sur_barre || sur_image { CursorIcon::Pointer } else { CursorIcon::Default }
            } else if self.glisse.is_some_and(|g| g.2) {
                CursorIcon::Grabbing
            } else if sur_image {
                if v.zoomee() { CursorIcon::Grab } else { CursorIcon::ZoomIn }
            } else {
                CursorIcon::Default
            };
            if self.survol != Cible::Rien {
                self.survol = Cible::Rien;
            }
            self.poser_icone(icone);
            return true;
        }
        let c = self.cible_sous_curseur();
        let change = c != self.survol;
        self.survol = c;
        if self.en_liste() && (change || !self.focus_clavier) {
            self.focus = match c {
                Cible::Ligne(i) => Some(i),
                _ => None,
            };
            self.focus_clavier = false;
        }
        self.poser_icone(if matches!(c, Cible::Rien) { CursorIcon::Default } else { CursorIcon::Pointer });
        change
    }

    fn poser_icone(&mut self, icone: CursorIcon) {
        if icone != self.icone {
            self.icone = icone;
            self.fenetre.set_cursor(icone);
        }
    }

    /// Une touche de navigation. Le clavier et --touches= passent tous deux
    /// par ici : le temoin rejoue exactement ce que fait la main.
    fn appliquer(&mut self, t: Touche) {
        let n = self.onglets.len();
        let i = self.onglet_actif().unwrap_or(0);
        match t {
            Touche::Echap => self.echap_feuille(),
            Touche::Tab => self.parcourir_liens(1),
            Touche::MajTab => self.parcourir_liens(-1),
            _ if self.passage.is_some() && t == Touche::Entree => self.entree_feuille(),
            Touche::Retour if self.passage.is_some() => self.echap_feuille(),
            Touche::Debut => self.aller_direct(),
            Touche::Barre => self.basculer_barre(),
            Touche::Retour => self.retour(),
            Touche::Gauche if i > 0 => self.activer_onglet(i - 1),
            Touche::Droite if i + 1 < n => self.activer_onglet(i + 1),
            Touche::Gauche | Touche::Droite => {}
            Touche::Haut => self.deplacer_focus(-1),
            Touche::Bas => self.deplacer_focus(1),
            Touche::Entree => {
                if let (true, Some(f)) = (self.en_liste(), self.focus) {
                    self.cliquer(Cible::Ligne(f));
                } else if let (false, Some((b, k))) = (self.en_liste(), self.lien_focus) {
                    self.cliquer(Cible::Lien(b, k));
                }
            }
            // Ctrl + 9 est le dernier onglet, comme dans les navigateurs.
            Touche::Rang(9) => self.activer_onglet(n.saturating_sub(1)),
            Touche::Rang(r) => self.activer_onglet(r.saturating_sub(1)),
        }
    }

    /// Le titre et la phrase d'un etat vide, s'il y en a un a montrer.
    fn etat_vide(&self) -> Option<(String, String)> {
        match &self.vue {
            Vue::Liste(p) if self.liste.as_ref().is_none_or(|l| l.lignes.is_empty()) => {
                Some(if self.catalogue.complet {
                    (
                        "Aucun chantier sous ce nom".to_string(),
                        format!("Aucun agent n’a travaillé dans « {p} ». La touche Début revient au direct."),
                    )
                } else {
                    (
                        "Inventaire des chantiers".to_string(),
                        "Le premier parcours lit chaque chantier, du plus récent au plus ancien."
                            .to_string(),
                    )
                })
            }
            Vue::Chantier(id) if self.liste.as_ref().is_none_or(|l| l.lignes.is_empty()) => Some(
                if self.fiche.as_ref().is_some_and(|f| &f.id == id) {
                    (
                        "Aucun agent".to_string(),
                        "Ce chantier n’a encore lancé personne. Retour arrière revient à la liste.".to_string(),
                    )
                } else {
                    (
                        "Lecture du chantier".to_string(),
                        "Chaque agent est lu une fois, images comprises ; ensuite, seul ce qui s’ajoute.".to_string(),
                    )
                },
            ),
            // Le direct sans agent : un etat, et ce qui va suivre (cadre, C8).
            Vue::Direct if self.role.is_empty() => Some(if self.catalogue.complet {
                (
                    "Aucun chantier ne tourne".to_string(),
                    // La suite promise doit exister : sans aucun projet, la
                    // barre n'a que le direct (juge-design.md, m4).
                    if self.onglets.len() > 1 {
                        "Le prochain qui démarre s’affichera ici de lui-même. ← et → parcourent les projets et leurs chantiers passés."
                            .to_string()
                    } else {
                        "Le prochain qui démarre s’affichera ici de lui-même.".to_string()
                    },
                )
            } else {
                (
                    "Inventaire des chantiers".to_string(),
                    "Ils arrivent du plus récent au plus ancien ; le direct s’attachera à l’agent qui écrit dès qu’il le trouve."
                        .to_string(),
                )
            }),
            Vue::Suivi(id) if self.introuvable.as_deref() == Some(id.as_str()) => Some((
                "Chantier introuvable".to_string(),
                format!("{id} n’existe sous aucune session. La touche Début revient au direct."),
            )),
            _ => None,
        }
    }

    /// Le zoom, borne. En dessous de 0,7 le texte devient illisible ; au-dela
    /// de 2,4 la colonne ne tient plus que quelques mots par ligne.
    fn zoomer(&mut self, pas: f32, facteur_systeme: f32) {
        let z = (self.zoom * pas).clamp(0.7, 2.4);
        if (z - self.zoom).abs() < 1e-3 {
            return;
        }
        self.zoom = z;
        self.echelle = facteur_systeme * z;
        self.recaler_barre();
        self.remettre_en_page();
    }

    fn dessiner(&mut self) {
        if perdu() {
            return;
        }
        let maintenant = Instant::now();
        let dt = (maintenant - self.derniere_trame).as_secs_f32().min(0.1);
        self.derniere_trame = maintenant;
        // Les TROIS ressorts : le fil, la liste et la bande. Jusqu'au
        // 2026-10-27, seul le fil avancait ; la liste et la bande recevaient
        // leur cible sans jamais bouger, et la fenetre se redessinait sans fin
        // a les attendre. Les captures le cachaient : elles posent chaque
        // ressort sur sa cible. Vu a la premiere vraie molette.
        // Sous le mouvement reduit, chaque ressort se pose d'un coup.
        let dt_ressorts = vues::pas_des_ressorts(dt, self.mouvement_reduit());
        self.defilement.avancer(dt_ressorts);
        self.defilement_liste.avancer(dt_ressorts);
        self.defilement_onglets.avancer(dt_ressorts);
        self.barre.avancer(dt_ressorts);
        self.avancer_visionneuse(dt);
        self.avancer_feuille(dt);
        self.avancer_ouverture(dt, dt_ressorts);
        self.temps += dt;

        use wgpu::CurrentSurfaceTexture as Etat;
        let image = match self.surface.get_current_texture() {
            Etat::Success(t) | Etat::Suboptimal(t) => t,
            // Perdue ou perimee, comme apres une fenetre reduite : on
            // reconfigure et l'on redemande une trame. Sans cette demande, la
            // fenetre restauree restait blanche jusqu'au bloc suivant (vu le
            // 2026-10-27).
            _ => {
                self.surface.configure(&self.peripherique, &self.config);
                self.fenetre.request_redraw();
                return;
            }
        };
        let vue = image
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.dessiner_dans(&vue);
        // La presentation passe par la file, pas par la texture.
        self.file.present(image);
    }

    /// Le dessin proprement dit. La capture hors-ecran emprunte le MEME
    /// chemin : c'est la seule facon qu'une image jugee soit celle qu'on voit.
    fn dessiner_dans(&mut self, vue: &wgpu::TextureView) {
        // Sur un peripherique perdu, le premier tampon cree naitrait invalide
        // et `create_buffer_init` paniquerait (objection 22).
        if perdu() {
            return;
        }
        let m = self.mesures();
        let e = self.echelle;
        let d = self.defilement.position();
        let haut_flux = m.entete_h;
        let bas_ecran = self.config.height as f32;
        let x_heure = m.x_heure;
        let x_corps = m.x_corps;
        let en_liste = self.en_liste();

        // « Projets », le titre du groupe de la barre, se pose hors du dessin.
        if self.groupe_projets.is_none() {
            let titre = if self.catalogue.complet { "Projets" } else { "Projets  ·  lecture" };
            let (b, _) = vues::une_ligne(
                &mut self.systeme,
                Metrics::new(12.0 * e, 16.0 * e),
                &Attrs::new().family(Family::Name("Barlow")).weight(Weight::MEDIUM),
                titre,
                vues::BARRE_L * e,
            );
            self.groupe_projets = Some(b);
        }
        let ouverte = self.barre.cible() > 0.5;
        if self.bulle.as_ref().is_none_or(|(o, _, _)| *o != ouverte) {
            let texte = if ouverte { "Masquer la barre  ·  Ctrl + B" } else { "Afficher la barre  ·  Ctrl + B" };
            let (b, l) = vues::une_ligne(
                &mut self.systeme,
                Metrics::new(13.0 * e, 18.0 * e),
                &Attrs::new().family(Family::Name("Barlow")).weight(Weight::MEDIUM),
                texte,
                400.0 * e,
            );
            self.bulle = Some((ouverte, b, l));
        }

        // L'etat vide se pose ici, hors du dessin, s'il a change.
        let voulu = self.etat_vide();
        match (&voulu, &self.vide) {
            (None, _) => self.vide = None,
            (Some((t, p)), Some((cle, _, _))) if *cle == format!("{t}\n{p}") => {}
            (Some((t, p)), _) => {
                let large = m.large_bloc;
                let (a, b) = vues::poser_vide(&mut self.systeme, t, p, large, e);
                self.vide = Some((format!("{t}\n{p}"), a, b));
            }
        }

        // Les images d'abord : leur texture se cree ici, avant que le texte
        // n'emprunte la mise en page. Seules celles a l'ecran sont preparees,
        // et aucune dans une liste, ou le fil n'est pas montre.
        let mut a_dessiner: Vec<(usize, [f32; 4])> = Vec::new();
        if !en_liste {
            for (i, p) in self.poses.iter().enumerate() {
                let Some(ip) = &p.image else { continue };
                let y = haut_flux + p.y - d;
                if y + p.hauteur < m.entete_h || y > bas_ecran {
                    continue;
                }
                a_dessiner.push((i, [x_corps.round(), (y + JOINTE_AIR * e).round(), ip.largeur, ip.hauteur]));
            }
        }
        for &(i, _) in &a_dessiner {
            self.preparer_image(i);
        }
        // Les vignettes de la planche, de meme : celles a l'ecran seulement.
        let visibles: Vec<usize> = match (&self.vue, &self.liste) {
            (Vue::Chantier(_), Some(l)) => {
                let g = self.geo_liste();
                l.vignettes
                    .iter()
                    .enumerate()
                    .filter(|(_, v)| {
                        let b = g.vignette(v);
                        b.y + b.h > g.haut && b.y < g.bas
                    })
                    .map(|(i, _)| i)
                    .collect()
            }
            _ => Vec::new(),
        };
        for &i in &visibles {
            self.preparer_vignette(i);
        }

        let mut aplats: Vec<formes::Forme> = Vec::new();
        // Le cadre d'une image : un filet d'un pixel, de la couleur du filet
        // d'en-tete, pour qu'une capture claire ne se fonde pas dans le fond.
        // Il n'est pose que sous l'en-tete, que les aplats ne savent pas rogner.
        for &(_, b) in &a_dessiner {
            if b[1] - e >= m.entete_h {
                aplats.push(formes::Forme {
                    boite: [b[0] - e, b[1] - e, b[2] + 2.0 * e, b[3] + 2.0 * e],
                    couleur: palette::lineaire(palette::filet(), 1.0),
                    rayon: [(IMAGE_RAYON + 1.0) * e, 0.0, 0.0, 0.0],
                });
            }
        }

        // La pastille ne dit qu'une chose : quelque chose travaille ici, en
        // ce moment (cadre, C7). Devant un chantier fini, interrompu, ou un
        // etat vide, elle n'est pas la. Elle respire, periode 2,6 s.
        let haut_entete = vues::ONGLETS_H * e;
        // Le titre sur sa colonne ; la pastille pend a sa gauche, dans la
        // gouttiere.
        let (x_titre, droite_entete) = self.bords_entete(&m);
        if self.etat_pose.as_ref().is_some_and(|(_, _, et, muet)| *et == catalogue::Etat::EnCours && !muet) {
            let souffle = if self.mouvement_reduit() {
                1.0
            } else {
                0.5 + 0.5 * (self.temps * 2.0 * std::f32::consts::PI / 2.6).sin()
            };
            aplats.push(formes::Forme {
                boite: [x_titre - 24.0 * e, haut_entete + 27.0 * e, 9.0 * e, 9.0 * e],
                couleur: {
                    // Le creux du souffle tient 3:1 sur le fond dans les deux
                    // themes : a 0,30, le clair tombait a 1,39:1 et restait
                    // sous 3:1 pendant 1,45 s sur 2,6 (juge-design.md, M2).
                    let bas = palette::plancher_souffle();
                    palette::lineaire(palette::encre1(), bas + (1.0 - bas) * souffle)
                },
                rayon: [4.5 * e, 0.0, 0.0, 0.0],
            });
        }
        // Le filet sous l'en-tete, aligne sur la colonne et non sur la fenetre.
        let large_bloc = m.large_bloc.max(1.0);
        aplats.push(formes::Forme {
            boite: [m.x_bloc, m.entete_h - 1.0 * e, large_bloc, (1.0 * e).max(1.0)],
            couleur: palette::lineaire(palette::filet(), 1.0),
            rayon: [0.0, 0.0, 0.0, 0.0],
        });

        let mut zones: Vec<TextArea> = Vec::new();
        let mut vignettes: Vec<(usize, plan::Boite)> = Vec::new();
        // Le titre de l'en-tete qui mene quelque part s'eclaire sous la souris,
        // comme un lien : c'est la seule chose qui dise qu'il se clique.
        let entete_survole = self.cible_sous_curseur() == Cible::Entete;

        let mut voiles: Vec<formes::Forme> = Vec::new();

        let bornes_entete = TextBounds {
            left: 0,
            top: haut_entete as i32,
            right: self.config.width as i32,
            bottom: m.entete_h as i32,
        };
        if let Some(tampon) = &self.entete {
            zones.push(TextArea {
                buffer: tampon,
                left: x_titre,
                top: haut_entete + 20.0 * e,
                scale: 1.0,
                bounds: bornes_entete,
                default_color: palette::encre1(),
                custom_glyphs: &[],
            });
        }
        if let Some(tampon) = &self.sous_entete {
            zones.push(TextArea {
                buffer: tampon,
                left: x_titre,
                top: haut_entete + 46.0 * e,
                scale: 1.0,
                bounds: bornes_entete,
                default_color: if entete_survole { palette::encre1() } else { palette::encre2() },
                custom_glyphs: &[],
            });
        }
        if let Some((t, w, et, muet)) = &self.etat_pose {
            zones.push(TextArea {
                buffer: t,
                left: droite_entete - w,
                top: haut_entete + 20.0 * e,
                scale: 1.0,
                bounds: bornes_entete,
                default_color: match et {
                    _ if *muet => palette::alerte(),
                    catalogue::Etat::EnCours => palette::encre1(),
                    catalogue::Etat::Fini => palette::encre2(),
                    catalogue::Etat::Interrompu => palette::alerte(),
                },
                custom_glyphs: &[],
            });
        }

        let bornes = TextBounds {
            left: 0,
            top: m.entete_h as i32,
            right: self.config.width as i32,
            bottom: self.config.height as i32,
        };

        if let (true, Some(liste)) = (en_liste, &self.liste) {
            let g = self.geo_liste();
            vues::dessiner_liste(liste, g, self.focus, self.focus_clavier, self.ligne_suivie(), &mut aplats, &mut zones);
            let survol = match self.cible_sous_curseur() {
                Cible::Vignette(i) => Some(i),
                _ => None,
            };
            vignettes = vues::dessiner_vignettes(liste, g, survol, &mut aplats, &mut zones);
        }

        if !en_liste {
            let connu = |u: &str| self.passage_connu(u);
            for (bi, p) in self.poses.iter().enumerate() {
                let y = haut_flux + p.y - d;
                // On ne prepare que ce qui se voit : mille blocs traites par
                // trame couteraient plus cher que tout le reste reuni.
                if y + p.hauteur < m.entete_h - m.interligne || y > bas_ecran {
                    continue;
                }
                // L'heure, dans la gouttiere, au corps des gestes. Calee sur
                // la premiere ligne de ce qu'elle date.
                let decalage = match (&p.image, p.bloc.registre) {
                    (Some(_), _) => JOINTE_AIR * e + (m.interligne - GESTE_H * e) * 0.5,
                    (None, Registre::Geste | Registre::Image) => 0.0,
                    _ => (m.interligne - GESTE_H * e) * 0.5,
                };
                if let Some(h) = &p.heure {
                    zones.push(TextArea {
                        buffer: h,
                        left: x_heure,
                        top: y + decalage,
                        scale: 1.0,
                        bounds: bornes,
                        default_color: palette::encre3(),
                        custom_glyphs: &[],
                    });
                }
                if p.image.is_some() {
                    // La vignette se dessine plus bas ; ici, son nom et sa
                    // legende, dans la colonne qui suit sa boite.
                    let x = x_corps + (JOINTE_L + JOINTE_TEXTE) * e;
                    zones.push(TextArea {
                        buffer: &p.corps,
                        left: x,
                        top: y + JOINTE_AIR * e,
                        scale: 1.0,
                        bounds: bornes,
                        default_color: palette::encre1(),
                        custom_glyphs: &[],
                    });
                    if let Some(s) = &p.second {
                        zones.push(TextArea {
                            buffer: s,
                            left: x,
                            top: y + JOINTE_AIR * e + m.interligne,
                            scale: 1.0,
                            bounds: bornes,
                            default_color: palette::encre3(),
                            custom_glyphs: &[],
                        });
                    }
                    continue;
                }
                if matches!(p.bloc.registre, Registre::Geste | Registre::Image) {
                    // Le verbe en encre-2, le detail en encre-3 : la colonne des
                    // verbes se lit d'abord.
                    if let Some(s) = &p.second {
                        zones.push(TextArea {
                            buffer: s,
                            left: x_corps,
                            top: y,
                            scale: 1.0,
                            bounds: bornes,
                            default_color: palette::encre2(),
                            custom_glyphs: &[],
                        });
                    }
                    zones.push(TextArea {
                        buffer: &p.corps,
                        left: x_corps + VERBE_L * e,
                        top: y,
                        scale: 1.0,
                        bounds: bornes,
                        default_color: palette::encre3(),
                        custom_glyphs: &[],
                    });
                    dessiner_liens(p, bi, x_corps + VERBE_L * e, y, (m.entete_h, bas_ecran), e, GESTE_CORPS, palette::encre3(), &connu, self.lien_focus, &mut aplats);
                    continue;
                }
                // Un rail ne marque QUE la parole. En marquer trois rendrait la
                // page rayee et ne dirait plus rien ; en marquer un fait que
                // l'oeil trouve d'emblee ce qui finira dans le rapport.
                if p.bloc.registre == Registre::Parole && y >= m.entete_h {
                    aplats.push(formes::Forme {
                        boite: [
                            x_corps - 16.0 * e,
                            y + 4.0 * e,
                            (2.0 * e).max(1.5),
                            (p.hauteur - 8.0 * e).max(2.0),
                        ],
                        couleur: palette::lineaire(palette::encre1(), 0.55),
                        rayon: [1.0 * e, 0.0, 0.0, 0.0],
                    });
                }
                zones.push(TextArea {
                    buffer: &p.corps,
                    left: x_corps,
                    top: y,
                    scale: 1.0,
                    bounds: bornes,
                    default_color: couleur(p.bloc.registre),
                    custom_glyphs: &[],
                });
                dessiner_liens(p, bi, x_corps, y, (m.entete_h, bas_ecran), e, CORPS, couleur(p.bloc.registre), &connu, self.lien_focus, &mut aplats);
            }
        }

        // Un etat vide se pose au tiers de la hauteur visible, ou l'oeil
        // tombe en arrivant, et non au milieu, ou il paraitrait perdu.
        if let Some((_, titre, phrase)) = &self.vide {
            let y = m.entete_h + self.visible() * 0.30;
            zones.push(TextArea {
                buffer: titre,
                left: m.x_bloc,
                top: y,
                scale: 1.0,
                bounds: bornes,
                default_color: palette::encre1(),
                custom_glyphs: &[],
            });
            zones.push(TextArea {
                buffer: phrase,
                left: m.x_bloc,
                top: y + m.interligne + 4.0 * e,
                scale: 1.0,
                bounds: bornes,
                default_color: palette::encre2(),
                custom_glyphs: &[],
            });
        }

        // La barre en dernier des aplats, et ses textes en dernier : elle passe
        // sur ce qui glisse dessous pendant son mouvement. Rappelee par-dessus
        // une fenetre etroite, le contenu est rogne a son bord et voile.
        let gb = self.geo_barre();
        let bord = (gb.panneau.x + gb.panneau.l).max(0.0);
        let par_dessus = self.barre_par_dessus();
        if par_dessus {
            for z in zones.iter_mut() {
                z.bounds.left = z.bounds.left.max(bord.ceil() as i32);
            }
            let noir = if palette::clair() { 0.18 } else { 0.45 };
            voiles.push(formes::Forme {
                boite: [bord, 0.0, self.config.width as f32 - bord, self.config.height as f32],
                couleur: [0.0, 0.0, 0.0, noir],
                rayon: [0.0, 0.0, 0.0, 0.0],
            });
        }
        let sous_curseur = self.cible_sous_curseur();
        if let Some(groupe) = &self.groupe_projets {
            let survol = match sous_curseur {
                Cible::Onglet(i) => Some(i),
                _ => None,
            };
            vues::dessiner_barre(
                &self.onglets,
                &gb,
                self.onglet_actif(),
                survol,
                groupe,
                self.contraste_eleve,
                par_dessus,
                &mut aplats,
                &mut zones,
            );
        }
        let bouton = vues::bouton_barre(e);
        let bouton_survole = sous_curseur == Cible::Barre && self.curseur.is_some_and(|(x, y)| bouton.contient(x, y));
        vues::dessiner_bouton_barre(bouton, self.barre.cible() > 0.5, bouton_survole, &mut aplats, e);
        // Survole, le bouton dit ce qu'il fait et sa touche, a sa droite, dans
        // la bande du haut ou rien d'autre ne s'ecrit.
        if let (true, Some((_, texte, l))) = (bouton_survole, &self.bulle) {
            let (h, air) = ((22.0 * e).round(), (8.0 * e).round());
            let x = (bouton.x + bouton.l + 6.0 * e).round();
            let y = (bouton.y + (bouton.h - h) * 0.5).round();
            aplats.push(formes::Forme {
                boite: [x, y, (l + 2.0 * air).round(), h],
                couleur: palette::lineaire(palette::plaque(), 1.0),
                rayon: [6.0 * e, 0.0, 0.0, 0.0],
            });
            zones.push(TextArea {
                buffer: texte,
                left: x + air,
                top: y + (h - 18.0 * e) * 0.5,
                scale: 1.0,
                bounds: TextBounds { left: 0, top: 0, right: self.config.width as i32, bottom: self.config.height as i32 },
                default_color: palette::encre1(),
                custom_glyphs: &[],
            });
        }

        let pret = if zones.is_empty() {
            false
        } else {
            self.rendu
                .prepare(
                    &self.peripherique,
                    &self.file,
                    &mut self.systeme,
                    &mut self.atlas,
                    &self.ecran,
                    zones,
                    &mut self.swash,
                )
                .is_ok()
        };

        // La visionneuse, par-dessus tout. Ouverte, la vue se dessine dans la
        // scene du verre, qui la floute ; sinon, directement a l'ecran.
        let dessus = self.dessus_visionneuse();
        let dessus_feuille = self.dessus_feuille();
        let verre_actif = self.visionneuse.is_some();
        if verre_actif {
            self.verre.scene(&self.peripherique, self.config.width, self.config.height);
        }
        let cible_vue: &wgpu::TextureView = match (verre_actif, self.verre.vue_scene()) {
            (true, Some(s)) => s,
            _ => vue,
        };

        let mut enc = self
            .peripherique
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut passe = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("vigie"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: cible_vue,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(palette::fond()),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                multiview_mask: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            let taille = (self.config.width as f32, self.config.height as f32);
            self.formes.dessiner(&self.peripherique, &mut passe, taille, &aplats);
            if pret {
                let _ = self.rendu.render(&self.atlas, &self.ecran, &mut passe);
            }
            // Les voiles APRES le texte, puisqu'ils doivent passer dessus.
            self.formes.dessiner(&self.peripherique, &mut passe, taille, &voiles);
            // Les images en dernier, rognees sous l'en-tete : le ciseau reste
            // pose pour la fin de la passe, donc rien ne doit venir apres.
            let haut = (m.entete_h.max(0.0) as u32).min(self.config.height);
            let reste = self.config.height - haut;
            if (!a_dessiner.is_empty() || !vignettes.is_empty()) && reste > 0 {
                let gauche = if par_dessus { (bord.ceil() as u32).min(self.config.width - 1) } else { 0 };
                passe.set_scissor_rect(gauche, haut, self.config.width - gauche, reste);
                let mut liste: Vec<(&images::Texture, [f32; 4], f32)> = a_dessiner
                    .iter()
                    .filter_map(|&(i, b)| {
                        self.poses[i]
                            .image
                            .as_ref()?
                            .texture
                            .as_ref()
                            .map(|t| (t, b, IMAGE_RAYON * e))
                    })
                    .collect();
                if let Some(l) = &self.liste {
                    liste.extend(vignettes.iter().filter_map(|&(i, b)| {
                        let t = l.vignettes.get(i)?.texture.as_ref()?;
                        Some((t, [b.x.round(), b.y.round(), b.l, b.h], IMAGE_RAYON * e))
                    }));
                }
                self.images.dessiner(&self.peripherique, &mut passe, taille, &liste);
            }
            // Le voile d'une barre rappelee passe aussi sur les images.
            if par_dessus {
                passe.set_scissor_rect(0, 0, self.config.width, self.config.height);
                self.formes.dessiner(&self.peripherique, &mut passe, taille, &voiles);
            }
        }
        // Le verre : le flou de la scene, le fond, les plaques ; puis, dans une
        // derniere passe, la barre d'une video, l'image et les textes.
        if verre_actif {
            self.verre.rendre(&self.peripherique, &mut enc, vue, dessus.t, e, &dessus.plaques);
            let mut passe = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("visionneuse"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: vue,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                multiview_mask: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            let taille = (self.config.width as f32, self.config.height as f32);
            self.formes.dessiner(&self.peripherique, &mut passe, taille, &dessus.aplats);
            if let (Some(b), Some((_, _, tex))) = (dessus.image, &self.vis_texture) {
                self.images.dessiner(&self.peripherique, &mut passe, taille, &[(tex, b, IMAGE_RAYON * e)]);
            }
            self.formes.dessiner(&self.peripherique, &mut passe, taille, &dessus.voiles);
            if dessus.pret {
                let _ = self.rendu_dessus.render(&self.atlas, &self.ecran, &mut passe);
            }
        }
        // L'ecran d'ouverture, par-dessus tout, jusqu'a ce qu'il soit efface.
        if let (Some(o), Some(tex)) = (&self.ouverture, &self.ouverture_texture) {
            let taille = (self.config.width as f32, self.config.height as f32);
            let b = ouverture::couvrir(self.ouverture_taille, taille);
            let mut passe = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ouverture"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: vue,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                multiview_mask: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            self.images.dessiner_avec_opacite(
                &self.peripherique,
                &mut passe,
                taille,
                &[(tex, [b.x, b.y, b.l, b.h], 0.0, o.opacite())],
            );
        }
        // La feuille du passage, et la bulle d'un lien survole : par-dessus le
        // fil et son texte, qui sont deja dessines.
        if let Some((aplats, pret)) = &dessus_feuille {
            let mut passe = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("feuille"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: vue,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                multiview_mask: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            let taille = (self.config.width as f32, self.config.height as f32);
            self.formes.dessiner(&self.peripherique, &mut passe, taille, aplats);
            if *pret {
                let _ = self.rendu_dessus.render(&self.atlas, &self.ecran, &mut passe);
            }
        }
        self.file.submit(Some(enc.finish()));
        self.atlas.trim();
    }

    /// Ce que la feuille du passage pose par-dessus le fil, pour une trame, ou
    /// la bulle d'un lien survole. Les textes se preparent ici, dans le second
    /// rendu, avec les aplats qui doivent passer dessous.
    fn dessus_feuille(&mut self) -> Option<(Vec<formes::Forme>, bool)> {
        let e = self.echelle;
        let (l, h) = (self.config.width as f32, self.config.height as f32);
        let bornes = TextBounds { left: 0, top: 0, right: l as i32, bottom: h as i32 };
        if self.passage.is_some() {
            self.composer_feuille();
            let p = self.feuille_ressort.position().clamp(0.0, 1.0);
            let dy = (1.0 - p) * 10.0 * e;
            let (_, c) = self.feuille.as_ref()?;
            let aplats: Vec<formes::Forme> = c
                .aplats
                .iter()
                .map(|a| {
                    let mut a = *a;
                    if a.boite[3] < h - 1.0 {
                        a.boite[1] += dy;
                    }
                    a.couleur[3] *= p;
                    a
                })
                .collect();
            let zones: Vec<TextArea> = c
                .textes
                .iter()
                .map(|t| TextArea {
                    buffer: &t.tampon,
                    left: t.x.round(),
                    top: (t.y + dy).round(),
                    scale: 1.0,
                    bounds: bornes,
                    default_color: glyphon::Color::rgba(t.couleur.r(), t.couleur.g(), t.couleur.b(), (t.couleur.a() as f32 * p) as u8),
                    custom_glyphs: &[],
                })
                .collect();
            let pret = self
                .rendu_dessus
                .prepare(&self.peripherique, &self.file, &mut self.systeme, &mut self.atlas, &self.ecran, zones, &mut self.swash)
                .is_ok();
            return Some((aplats, pret));
        }
        // Un lien survole : l'hote reel et ce qu'on sait du passage, sans reseau.
        let Cible::Lien(b, k) = self.survol else { return None };
        let url = self.url_du_lien(b, k)?;
        let boite = self.plan().liens.iter().find(|(_, i, j)| *i == b && *j == k)?.0;
        let a = passage::adresse(&url);
        let (mot, alerte) = match passage::classer(&url) {
            passage::Classe::Refuse(_) => ("lien refusé", true),
            passage::Classe::Local => ("cet ordinateur", true),
            passage::Classe::NonChiffre => ("non chiffré", true),
            _ if self.passage_connu(&url) => ("passage cité", false),
            _ if self.aucun_passage_cite(&url) => ("aucun passage cité", false),
            _ => ("", false),
        };
        let cle = format!("{url}|{mot}|{e}|{}", palette::clair());
        if self.bulle_lien.as_ref().is_none_or(|(k2, _, _)| *k2 != cle) {
            let mut poser = |t: &str, poids: Weight| {
                let mut tb = Buffer::new(&mut self.systeme, Metrics::new(13.0 * e, 18.0 * e));
                tb.set_size(None, None);
                tb.set_text(t, &Attrs::new().family(Family::Name("Barlow")).weight(poids), Shaping::Advanced, None);
                tb.shape_until_scroll(&mut self.systeme, false);
                tb
            };
            let hote = poser(&a.hote, Weight::MEDIUM);
            let m = poser(&if mot.is_empty() { String::new() } else { format!("·   {mot}") }, Weight::NORMAL);
            self.bulle_lien = Some((cle, hote, m));
        }
        let (_, hote, m) = self.bulle_lien.as_ref()?;
        let largeur = |t: &Buffer| t.layout_runs().map(|r| r.line_w).fold(0.0, f32::max);
        let (lh, lm) = (largeur(hote), largeur(m));
        let air = 10.0 * e;
        let pl = lh + if lm > 0.0 { 12.0 * e + lm } else { 0.0 } + 2.0 * air;
        let ph = 26.0 * e;
        let x = boite.x.min(l - pl - 8.0 * e).max(8.0 * e);
        let sous = boite.y + boite.h + 4.0 * e;
        let y = if sous + ph > h - 4.0 * e { boite.y - ph - 4.0 * e } else { sous };
        let aplats = vec![
            formes::Forme { boite: [(x - e).round(), (y - e).round(), (pl + 2.0 * e).round(), (ph + 2.0 * e).round()], couleur: palette::lineaire(palette::filet(), 1.0), rayon: [8.0 * e, 0.0, 0.0, 0.0] },
            formes::Forme { boite: [x.round(), y.round(), pl.round(), ph.round()], couleur: palette::lineaire(palette::plaque(), 1.0), rayon: [7.0 * e, 0.0, 0.0, 0.0] },
        ];
        let ty = y + (ph - 18.0 * e) * 0.5;
        let dit = format!(
            "bulle : plaque [{:.0},{:.0},{:.0},{:.0}] feuille texte : [{:.0},{:.0},{:.0},18] #{} {:?} feuille texte : [{:.0},{:.0},{:.0},18] #{} {:?}",
            x,
            y,
            pl,
            ph,
            x + air,
            ty,
            lh,
            hex_couleur(palette::encre1()),
            a.hote,
            x + air + lh + 12.0 * e,
            ty,
            lm,
            hex_couleur(if alerte { palette::alerte() } else { palette::encre2() }),
            mot
        );
        if self.bulle_journal != dit {
            for ligne in dit.replace(" feuille texte", "\nfeuille texte").lines() {
                eprintln!("{ligne}");
            }
            self.bulle_journal = dit;
        }
        let mut zones = vec![TextArea { buffer: hote, left: (x + air).round(), top: ty.round(), scale: 1.0, bounds: bornes, default_color: palette::encre1(), custom_glyphs: &[] }];
        if lm > 0.0 {
            zones.push(TextArea {
                buffer: m,
                left: (x + air + lh + 12.0 * e).round(),
                top: ty.round(),
                scale: 1.0,
                bounds: bornes,
                default_color: if alerte { palette::alerte() } else { palette::encre2() },
                custom_glyphs: &[],
            });
        }
        let pret = self
            .rendu_dessus
            .prepare(&self.peripherique, &self.file, &mut self.systeme, &mut self.atlas, &self.ecran, zones, &mut self.swash)
            .is_ok();
        Some((aplats, pret))
    }

    /// Ce que la visionneuse pose par-dessus le verre, pour une trame : les
    /// plaques de verre, les aplats de la barre d'une video, la boite de
    /// l'image et ses textes. Les textes se preparent ici, dans le second rendu.
    fn dessus_visionneuse(&mut self) -> Dessus {
        self.preparer_visionneuse();
        let Some(v) = self.visionneuse.as_ref() else { return Dessus::default() };
        let e = self.echelle;
        let (l, h) = (self.config.width as f32, self.config.height as f32);
        let t = v.t();
        let el = v.element().clone();
        let video = v.video();
        let zoomee = !video && v.zoomee();
        let Some(cible) = self.image_ouverte() else { return Dessus::default() };
        let b = visionneuse::interpoler(v.depart, cible, t);
        // Les plaques et les textes viennent avec l'ouverture, un peu apres
        // l'image : on voit d'abord l'image arriver, puis ce qui la nomme.
        let chrome = ((t - 0.4) / 0.6).clamp(0.0, 1.0);
        let (n, total) = (v.rang + 1, v.elements.len());
        let lecture = v.lecture.as_ref().map(|lec| (lec.position(), lec.forme.duree, lec.joue()));

        // Les textes : le nom et ce qui le suit, le rang, l'aide, et pour une
        // video son temps et sa duree. Poses seulement quand ils changent.
        let nom = el.image.nom().unwrap_or_else(|| "image reçue".to_string());
        let audio = el.image.est_audio();
        let mut details = vec![el.details.clone()];
        if !audio {
            details.push(format!("{} × {}", el.image.source.0, el.image.source.1));
        }
        if let Some(d) = el.image.video {
            details.push(el.image.mention_duree(d));
        }
        // Zoomee, sa taille par rapport a ses vrais pixels : 100 %, chacun
        // de ses pixels sur un pixel de l'ecran.
        if zoomee {
            let cible_vise = visionneuse::zoomee(
                visionneuse::ajuster(el.image.source, visionneuse::zone(l, h, e, false), e),
                v.zoom_vise().0,
                (0.0, 0.0),
            );
            details.push(format!("{} %", (cible_vise.l / el.image.source.0.max(1) as f32 * 100.0).round()));
        }
        details.retain(|x| !x.is_empty());
        let compteur = if total > 1 { format!("{n} / {total}") } else { String::new() };
        let mut aide = vec!["Échap  fermer"];
        if total > 1 {
            aide.push("← →  parcourir");
        }
        if audio {
            aide.push("Espace  lire ou suspendre");
            aide.push("Clic sur l'onde  aller à");
        } else if video {
            aide.push("Espace  lire ou suspendre");
        } else if zoomee {
            aide.push("Glisser  déplacer");
            aide.push("0  ajuster");
        } else {
            aide.push("Molette ou double-clic  zoomer");
        }
        if el.agent.is_some() {
            aide.push("Entrée  voir dans le fil");
        }
        let aide = aide.join("      ");
        let (ecoule, duree) = lecture.map_or((String::new(), String::new()), |(p, d, joue)| {
            (format!("{}   {}", if joue { "❚❚" } else { "▶" }, video::temps(p)), video::temps(d))
        });
        let cle = format!("{nom}|{}|{compteur}|{aide}|{ecoule}|{duree}|{e}", details.join("|"));
        if self.vis_textes.as_ref().is_none_or(|(k, _)| *k != cle) {
            let barlow = Attrs::new().family(Family::Name("Barlow"));
            let mut poser = |texte: &str, corps: f32, haut: f32, gras: bool| {
                let mut b = Buffer::new(&mut self.systeme, Metrics::new(corps * e, haut * e));
                b.set_size(Some(l), None);
                let a = if gras { barlow.clone().weight(Weight::MEDIUM) } else { barlow.clone() };
                b.set_text(texte, &a, Shaping::Advanced, None);
                b.shape_until_scroll(&mut self.systeme, false);
                b
            };
            let tampons = vec![
                poser(&nom, 17.0, 22.0, true),
                poser(&details.join("  ·  "), 13.0, 18.0, false),
                poser(&compteur, 13.0, 18.0, false),
                poser(&aide, 12.0, 16.0, false),
                poser(&ecoule, 13.0, 18.0, true),
                poser(&duree, 13.0, 18.0, false),
            ];
            self.vis_textes = Some((cle, tampons));
        }
        let mut d = Dessus { image: Some([b.x.round(), b.y.round(), b.l.round().max(1.0), b.h.round().max(1.0)]), t, ..Dessus::default() };
        let Some((_, tampons)) = &self.vis_textes else { return d };
        if chrome <= 0.0 {
            return d;
        }
        let largeur = |b: &Buffer| b.layout_runs().next().map_or(0.0, |r| r.line_w);
        let bornes = TextBounds { left: 0, top: 0, right: l as i32, bottom: h as i32 };
        let couleur = |c: glyphon::Color| glyphon::Color::rgba(c.r(), c.g(), c.b(), (c.a() as f32 * chrome) as u8);
        let plaque = |x: f32, y: f32, pl: f32, ph: f32| verre::Plaque {
            boite: [x.round(), y.round(), pl.round(), ph.round()],
            rayon: (ph * 0.5).min(22.0 * e),
            opacite: chrome,
        };
        let mut zones: Vec<TextArea> = Vec::new();
        let mut zone = |buffer, left: f32, top: f32, c: glyphon::Color| {
            zones.push(TextArea { buffer, left: left.round(), top: top.round(), scale: 1.0, bounds: bornes, default_color: couleur(c), custom_glyphs: &[] });
        };

        // Le nom et ses details, dans une capsule en haut a gauche.
        let marge = 28.0 * e;
        let air = 18.0 * e;
        let titre_l = largeur(&tampons[0]).max(largeur(&tampons[1]));
        d.plaques.push(plaque(marge, 14.0 * e, titre_l + 2.0 * air, 60.0 * e));
        zone(&tampons[0], marge + air, 22.0 * e, palette::PAROLE);
        zone(&tampons[1], marge + air, 46.0 * e, palette::PENSEE);
        // Le rang, dans une capsule en haut a droite.
        if total > 1 {
            let cl = largeur(&tampons[2]) + 2.0 * 14.0 * e;
            let x = l - marge - cl;
            d.plaques.push(plaque(x, 28.0 * e, cl, 32.0 * e));
            zone(&tampons[2], x + 14.0 * e, 35.0 * e, palette::PAROLE);
        }
        // L'aide, dans une capsule en bas, au centre.
        let al = largeur(&tampons[3]) + 2.0 * 16.0 * e;
        d.plaques.push(plaque((l - al) * 0.5, h - 44.0 * e, al, 30.0 * e));
        zone(&tampons[3], (l - al) * 0.5 + 16.0 * e, h - 37.0 * e, palette::PENSEE);
        // Les commandes d'une video, comme celles de QuickTime : la lecture et
        // le temps ecoule, la barre, la duree, dans une capsule a elles.
        if video {
            let hud = visionneuse::commandes(l, h, e);
            d.plaques.push(plaque(hud.x, hud.y, hud.l, hud.h));
            let (piste, _) = visionneuse::barre(l, h, e);
            let fraction = lecture.map_or(0.0, |(p, du, _)| if du > 0.0 { (p / du).clamp(0.0, 1.0) } else { 0.0 });
            d.aplats.push(formes::Forme {
                boite: [piste.x, piste.y, piste.l, piste.h],
                couleur: palette::lineaire(palette::PAROLE, 0.22 * chrome),
                rayon: [piste.h * 0.5, 0.0, 0.0, 0.0],
            });
            d.aplats.push(formes::Forme {
                boite: [piste.x, piste.y, (piste.l * fraction).max(piste.h), piste.h],
                couleur: palette::lineaire(palette::PAROLE, 0.9 * chrome),
                rayon: [piste.h * 0.5, 0.0, 0.0, 0.0],
            });
            let r = 6.0 * e;
            d.aplats.push(formes::Forme {
                boite: [piste.x + piste.l * fraction - r, piste.y + piste.h * 0.5 - r, 2.0 * r, 2.0 * r],
                couleur: palette::lineaire(palette::PAROLE, chrome),
                rayon: [r, 0.0, 0.0, 0.0],
            });
            let milieu = hud.y + hud.h * 0.5 - 9.0 * e;
            zone(&tampons[4], hud.x + 18.0 * e, milieu, palette::PAROLE);
            zone(&tampons[5], piste.x + piste.l + 14.0 * e, milieu, palette::PENSEE);
        }
        // Un son : la part qu'on n'a pas encore entendue s'assombrit, et un
        // trait fin marque l'instant. Poses sur l'image, sous les textes.
        if audio {
            let fraction = lecture.map_or(0.0, |(p, du, _)| if du > 0.0 { (p / du).clamp(0.0, 1.0) } else { 0.0 });
            let (bx, by, bl, bh) = (b.x.round(), b.y.round(), b.l.round(), b.h.round());
            let x = (bx + bl * fraction).round();
            d.voiles.push(formes::Forme {
                boite: [x, by, (bx + bl - x).max(0.0), bh],
                couleur: palette::lineaire(glyphon::Color::rgb(0, 0, 0), 0.42 * chrome),
                rayon: [0.0; 4],
            });
            d.voiles.push(formes::Forme {
                boite: [x - 1.0 * e, by, 2.0 * e, bh],
                couleur: palette::lineaire(palette::PAROLE, 0.95 * chrome),
                rayon: [e, 0.0, 0.0, 0.0],
            });
        }
        if zoomee {
            let sombre = glyphon::Color::rgb(0x14, 0x15, 0x18);
            d.voiles = d
                .plaques
                .iter()
                .map(|p| formes::Forme { boite: p.boite, couleur: palette::lineaire(sombre, chrome), rayon: [p.rayon, 0.0, 0.0, 0.0] })
                .collect();
        }
        d.pret = self
            .rendu_dessus
            .prepare(&self.peripherique, &self.file, &mut self.systeme, &mut self.atlas, &self.ecran, zones, &mut self.swash)
            .is_ok();
        d
    }

    /// Rend une image hors-ecran et l'ecrit en PNG, puis rend la main.
    fn capturer(&mut self, chemin: &str) {
        let (l, h) = (self.config.width, self.config.height);
        let cible = self.peripherique.create_texture(&wgpu::TextureDescriptor {
            label: Some("capture"),
            size: wgpu::Extent3d { width: l, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let vue = cible.create_view(&wgpu::TextureViewDescriptor::default());
        self.dessiner_dans(&vue);

        // copy_texture_to_buffer exige des lignes multiples de 256 octets.
        let ligne = l * 4;
        let alignee = ligne.div_ceil(256) * 256;
        let lecture = self.peripherique.create_buffer(&wgpu::BufferDescriptor {
            label: Some("relecture"),
            size: (alignee * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = self
            .peripherique
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &cible,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &lecture,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(alignee),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d { width: l, height: h, depth_or_array_layers: 1 },
        );
        self.file.submit(Some(enc.finish()));

        let tranche = lecture.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        tranche.map_async(wgpu::MapMode::Read, move |r| { let _ = tx.send(r); });
        let _ = self.peripherique.poll(wgpu::PollType::wait_indefinitely());
        let _ = rx.recv();
        // Perdu pendant la trame, le peripherique laisse une image vide ou
        // partielle : on n'ecrit rien, et la relance refait la capture.
        let Ok(brut) = tranche.get_mapped_range() else { return };
        if perdu() {
            return;
        }
        let mut pixels = Vec::with_capacity((ligne * h) as usize);
        // La surface est en BGRA ; le PNG veut du RGBA.
        let bgra = self.config.format == wgpu::TextureFormat::Bgra8UnormSrgb
            || self.config.format == wgpu::TextureFormat::Bgra8Unorm;
        for y in 0..h {
            let debut = (y * alignee) as usize;
            let fin = debut + ligne as usize;
            if bgra {
                for p in brut[debut..fin].chunks_exact(4) {
                    pixels.extend_from_slice(&[p[2], p[1], p[0], 255]);
                }
            } else {
                pixels.extend_from_slice(&brut[debut..fin]);
            }
        }
        drop(brut);
        lecture.unmap();

        if let Some(parent) = std::path::Path::new(chemin).parent() {
            if !parent.as_os_str().is_empty() {
                let _ = std::fs::create_dir_all(parent);
            }
        }
        let Ok(f) = std::fs::File::create(chemin) else { return };
        let mut e = png::Encoder::new(std::io::BufWriter::new(f), l, h);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        if let Ok(mut w) = e.write_header() {
            let _ = w.write_image_data(&pixels);
        }
        eprintln!("-> {chemin}");
    }
}

impl ApplicationHandler for Demarrage {
    fn resumed(&mut self, boucle: &ActiveEventLoop) {
        if self.vigie.is_some() {
            return;
        }
        let (l, h) = self.taille.unwrap_or((1000.0, 760.0));
        let mut attributs = Window::default_attributes()
            .with_title("Vigie")
            .with_inner_size(winit::dpi::LogicalSize::new(l, h))
            .with_min_inner_size(winit::dpi::LogicalSize::new(420.0, 320.0));
        if let Some(f) = self.facteur_force {
            let f = f as f64;
            attributs = attributs
                .with_inner_size(winit::dpi::PhysicalSize::new((l * f).round() as u32, (h * f).round() as u32))
                .with_min_inner_size(winit::dpi::PhysicalSize::new((420.0 * f) as u32, (320.0 * f) as u32));
        }
        #[cfg(windows)]
        {
            use winit::platform::windows::{IconExtWindows, WindowAttributesExtWindows};
            attributs = attributs.with_class_name(app::CLASSE);
            // L'icone de la chambre noire, a la taille que Windows lui demande :
            // la petite pour la barre de titre, la grande pour la barre des
            // taches. Chacune est chargee a sa taille dans le .ico embarque
            // (build.rs) : si winit n'en recevait qu'une, il la reduirait pour
            // l'autre, et le dessin fait a part pour 16 a 32 px serait perdu.
            let echelle_ecran = self
                .facteur_force
                .map(|f| f as f64)
                .or_else(|| boucle.primary_monitor().map(|m| m.scale_factor()))
                .unwrap_or(1.0);
            let (petite, grande) = taille_des_icones(echelle_ecran);
            let charger = |t: u32| winit::window::Icon::from_resource(1, Some(winit::dpi::PhysicalSize::new(t, t))).ok();
            let (ip, ig) = (charger(petite), charger(grande));
            eprintln!(
                "icones : petite {petite} px {}, grande {grande} px {}",
                if ip.is_some() { "chargee" } else { "ABSENTE" },
                if ig.is_some() { "chargee" } else { "ABSENTE" }
            );
            attributs = attributs.with_window_icon(ip).with_taskbar_icon(ig);
        }
        // Une capture ne prend pas le premier plan : le clavier reste a qui
        // ecrivait.
        if self.capture.is_some() {
            attributs = attributs.with_active(false);
        }
        if let Some(s) = &self.souvenir {
            attributs = attributs.with_inner_size(winit::dpi::PhysicalSize::new(s.taille.0, s.taille.1));
            // Une place hors de tout ecran (un second ecran debranche depuis)
            // ferait une fenetre invisible : on ne la reprend que si son coin
            // haut-gauche, un peu rentre, tombe sur un ecran present.
            if let Some((x, y)) = s.position {
                let visible = boucle.available_monitors().any(|m| {
                    let (p, t) = (m.position(), m.size());
                    let (cx, cy) = (x + 40, y + 20);
                    cx >= p.x && cy >= p.y && cx < p.x + t.width as i32 && cy < p.y + t.height as i32
                });
                if visible {
                    attributs = attributs.with_position(winit::dpi::PhysicalPosition::new(x, y));
                }
            }
        }
        let fenetre = std::sync::Arc::new(boucle.create_window(attributs).unwrap());

        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(fenetre.clone()).unwrap();
        let adaptateur = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
            apply_limit_buckets: false,
        }))
        .expect("aucun adaptateur graphique");
        let (peripherique, file) = pollster::block_on(adaptateur.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("vigie"),
                required_features: wgpu::Features::empty(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                required_limits: wgpu::Limits::downlevel_defaults()
                    .using_resolution(adaptateur.limits()),
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::Off,
            },
        ))
        .expect("peripherique refuse");
        let carte = adaptateur.get_info();
        eprintln!("carte : {} ({:?}, pilote {})", carte.name, carte.backend, carte.driver_info);
        peripherique.set_device_lost_callback(|raison, message| {
            eprintln!("peripherique perdu : {raison:?} : {message}");
            PERDU.store(true, std::sync::atomic::Ordering::SeqCst);
        });
        // Une fois le peripherique perdu, ce qui suit n'est que l'echo de la
        // perte : on le tait, la vigie se relance (`relancer`). Toute autre
        // erreur reste une panique, comme sans ce gestionnaire.
        peripherique.on_uncaptured_error(std::sync::Arc::new(|erreur: wgpu::Error| {
            if !perdu() {
                panic!("erreur wgpu : {erreur}");
            }
        }));

        let taille = fenetre.inner_size();
        let capacites = surface.get_capabilities(&adaptateur);
        let format = capacites
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(capacites.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: taille.width.max(1),
            height: taille.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&peripherique, &config);

        // Base VIDE, puis DEUX chargements choisis. Aucune police n'est
        // scrutee au hasard, donc aucune ne peut se substituer a Barlow sans
        // qu'on l'ait voulu.
        let mut base = glyphon::fontdb::Database::new();
        base.load_font_data(BARLOW_400.to_vec());
        base.load_font_data(BARLOW_500.to_vec());
        // UN recours, nomme. Barlow n'a pas le grec, et la pensee d'un agent
        // qui fait de l'optique est pleine de theta et de phi : sans lui, ces
        // lettres rendent un carre vide, ce qui est pire qu'une police qui
        // depareille. Segoe UI est sur toute machine Windows depuis Vista.
        // Son absence n'est pas une panne : on perd le grec, pas le texte.
        for chemin in [
            // Le grec, les fleches et la ponctuation savante.
            r"C:\Windows\Fonts\segoeui.ttf",
            // Les signes mathematiques que Segoe n'a pas : le temoin de
            // --essai montrait un carre vide a la place du nabla.
            r"C:\Windows\Fonts\cambria.ttc",
            r"C:\Windows\Fonts\seguisym.ttf",
        ] {
            if let Ok(d) = std::fs::read(chemin) {
                base.load_font_data(d);
            }
        }
        let systeme = FontSystem::new_with_locale_and_db("fr-CA".to_string(), base);
        let cache = Cache::new(&peripherique);
        let mut atlas = TextAtlas::new(&peripherique, &file, &cache, format);
        let rendu = TextRenderer::new(
            &mut atlas,
            &peripherique,
            wgpu::MultisampleState::default(),
            None,
        );
        let rendu_dessus = TextRenderer::new(&mut atlas, &peripherique, wgpu::MultisampleState::default(), None);
        let mut ecran = Viewport::new(&peripherique, &cache);
        ecran.update(
            &file,
            Resolution {
                width: config.width,
                height: config.height,
            },
        );

        let aplats = formes::Formes::nouveau(&peripherique, format);
        let dessin_images = images::Images::nouveau(&peripherique, format);
        let verre_visionneuse = verre::Verre::nouveau(&peripherique, format);
        // L'echelle du systeme : sans elle, tout est minuscule sur un ecran
        // dense, et l'utilisateur croit que l'application est mal faite.
        let facteur = self.facteur_force.unwrap_or(fenetre.scale_factor() as f32);
        let zoom = self.souvenir.as_ref().map_or(1.0, |s| s.zoom);

        let (lire_envoi, lire_recois) = channel();
        let mut vigie = Vigie {
            fenetre,
            surface,
            peripherique,
            file,
            config,
            systeme,
            swash: SwashCache::new(),
            atlas,
            rendu,
            ecran,
            formes: aplats,
            images: dessin_images,
            poses: Vec::new(),
            entete: None,
            sous_entete: None,
            etat_pose: None,
            temps: 0.0,
            role: String::new(),
            chantier: String::new(),
            echelle: facteur * zoom,
            zoom,
            defilement: ressort::Ressort::pose(0.0),
            colle_en_bas: true,
            hauteur_flux: 0.0,
            derniere_trame: Instant::now(),
            recois: self.recois.take().expect("un seul demarrage"),
            essai_seul: false,
            catalogue: Arc::new(Catalogue::default()),
            vue: self.vue.clone(),
            choix: self.choix.clone(),
            envoi_choix: self.envoi_choix.take().expect("un seul demarrage"),
            generation: 0,
            generation_recue: 0,
            introuvable: None,
            onglets: Vec::new(),
            onglets_voulus: Vec::new(),
            defilement_onglets: ressort::Ressort::pose(0.0),
            liste: None,
            defilement_liste: ressort::Ressort::pose(0.0),
            focus: None,
            curseur: None,
            appui: None,
            survol: Cible::Rien,
            icone: CursorIcon::Default,
            dernier_clic: None,
            glisse: None,
            entete_textes: (String::new(), String::new()),
            vide: None,
            barre_masquee: self.barre.or(self.souvenir.as_ref().map(|s| s.barre_masquee)).unwrap_or(false),
            barre_rappelee: false,
            focus_clavier: false,
            barre: ressort::Ressort::pose(0.0),
            groupe_projets: None,
            bulle: None,
            contraste_eleve: self.contraste_eleve || palette::contraste_du_systeme(),
            fiche: None,
            envoi_fiche: self.envoi_fiche.take().expect("un seul demarrage"),
            viser: None,
            en_attente: std::collections::VecDeque::new(),
            visionneuse: None,
            ouverture: match (self.capture.is_some(), self.ouverture_capture) {
                _ if std::env::var_os("VIGIE_SANS_OUVERTURE").is_some() => None,
                (false, _) => Some(ouverture::Ouverture::nouvelle()),
                (true, Some(o)) => Some(ouverture::Ouverture::fixee(o)),
                (true, None) => None,
            },
            ouverture_texture: None,
            ouverture_taille: (3840, 2400),
            ouverture_recue: None,
            donnees_pretes: false,
            vis_texture: None,
            vis_pleine: None,
            vis_textes: None,
            rendu_dessus,
            passage: None,
            feuille_ressort: ressort::Ressort::pose(0.0),
            feuille: None,
            pages: std::env::var_os("VIGIE_PAGES")
                .filter(|v| !v.is_empty())
                .map(|d| passage::charger_pages(std::path::Path::new(&d)))
                .unwrap_or_default(),
            reseau: argument("--reseau=").and_then(|a| passage::Reseau::nomme(&a)),
            lectures: std::collections::HashMap::new(),
            lire_envoi: lire_envoi,
            lire_recois: lire_recois,
            lire_jeton: 0,
            sonde_forcee: argument("--sonde=").and_then(|a| match a.as_str() {
                "trouve" => Some(navigateur::Sonde::Trouve),
                "non" => Some(navigateur::Sonde::NonRetrouve),
                "indisponible" => Some(navigateur::Sonde::Indisponible),
                _ => None,
            }),
            citation_forcee: argument("--citation="),
            navigateur: None,
            page_permise: self.capture.is_none() || self.reel.is_some(),
            sonde: navigateur::Sonde::Attente,
            ecoule_force: argument("--ecoule=").and_then(|a| a.replace(',', ".").parse().ok()),
            lien_focus: None,
            bulle_lien: None,
            bulle_journal: String::new(),
            animations_coupees: self.mouvement_reduit || visionneuse::animations_coupees(),
            verre: verre_visionneuse,
            dernier_bloc: Instant::now(),
            fil_lu: false,
            ancre: None,
            reglage: self
                .theme
                .or(self.souvenir.as_ref().map(|s| s.theme))
                .unwrap_or(palette::Reglage::Sombre),
        };
        vigie.appliquer_theme();
        vigie.barre.poser(vigie.barre_cible());
        palette::poser_contraste(vigie.contraste_eleve);
        vigie.viser = self.viser.take();
        vigie.dernier_bloc = Instant::now();
        {
            // Ce que la machine annonce vraiment. Sans ce relevé, un ecran a
            // 100 % et un correctif d'echelle sans effet sont indiscernables.
            let m = vigie.mesures();
            eprintln!(
                "fenetre {}x{}  facteur systeme {:.2}  zoom {:.2}  echelle {:.2}",
                vigie.config.width, vigie.config.height, facteur, vigie.zoom, vigie.echelle
            );
            eprintln!(
                "corps {:.1} px  colonne {:.0} px  gouttiere a x={:.0}  colonne a x={:.0}",
                m.corps, m.colonne, m.x_heure, m.x_corps
            );
        }
        vigie.poser_entete();
        vigie.poser_onglets();
        vigie.montrer_onglet_actif();
        // La toile d'ouverture se decode dans un fil a part : la fenetre ne
        // l'attend pas.
        if vigie.ouverture.is_some() {
            let (envoi, recois) = std::sync::mpsc::channel();
            vigie.ouverture_recue = Some(recois);
            std::thread::spawn(move || {
                if let Some(d) = ouverture::decoder() {
                    let _ = envoi.send(d);
                }
            });
        }
        self.vigie = Some(vigie);
    }

    fn window_event(&mut self, boucle: &ActiveEventLoop, _id: WindowId, ev: WindowEvent) {
        let Some(v) = self.vigie.as_mut() else {
            return;
        };
        // Une capture ne juge que ce que ses drapeaux posent. Sa fenetre est
        // visible le temps d'attendre, et une touche tapee ailleurs (Retour
        // arriere), un clic ou la souris qui passe y agissaient sans laisser de
        // trace : la page au lieu du fil, 2 fois sur 14 (juge-technique.md,
        // §4). Les clics et touches rejoues ne passent pas par ici.
        if self.capture.is_some()
            && matches!(
                ev,
                WindowEvent::KeyboardInput { .. }
                    | WindowEvent::MouseInput { .. }
                    | WindowEvent::MouseWheel { .. }
                    | WindowEvent::CursorMoved { .. }
                    | WindowEvent::CursorLeft { .. }
                    | WindowEvent::ModifiersChanged(_)
            )
        {
            if matches!(ev, WindowEvent::KeyboardInput { .. } | WindowEvent::MouseInput { .. } | WindowEvent::MouseWheel { .. }) {
                eprintln!("entree reelle ignoree pendant la capture : {}", match ev {
                    WindowEvent::KeyboardInput { .. } => "touche",
                    WindowEvent::MouseInput { .. } => "clic",
                    _ => "molette",
                });
            }
            return;
        }
        // Une touche, un clic ou la molette changent presque toujours ce qui
        // se voit, souvent sans faire bouger de ressort : changer d'onglet ne
        // defile rien. Sans ce drapeau, l'ecran attendrait le bloc suivant.
        if matches!(
            ev,
            WindowEvent::KeyboardInput { .. } | WindowEvent::MouseInput { .. } | WindowEvent::MouseWheel { .. }
        ) {
            self.redessiner = true;
        }
        match ev {
            WindowEvent::CloseRequested => boucle.exit(),
            // Une fenetre restauree ou redimensionnee se redessine tout de
            // suite, sans attendre qu'un bloc arrive.
            WindowEvent::Resized(t) => {
                v.redimensionner(t.width, t.height);
                self.redessiner = true;
            }
            WindowEvent::Occluded(false) => self.redessiner = true,
            WindowEvent::ThemeChanged(_) => {
                v.appliquer_theme();
                self.redessiner = true;
            }
            WindowEvent::ModifiersChanged(m) => {
                self.ctrl = m.state().control_key();
                self.maj = m.state().shift_key();
                self.alt = m.state().alt_key();
            }
            WindowEvent::CursorMoved { position, .. } => {
                // Les positions de winit sont en pixels physiques, comme tout
                // ce que la vigie dessine : aucune conversion.
                v.curseur = Some((position.x as f32, position.y as f32));
                // Une image zoomee suit la main qui la glisse.
                if let Some((p0, d0, bouge)) = v.glisse {
                    let (x, y) = (position.x as f32, position.y as f32);
                    let bouge = bouge || (x - p0.0).abs() > 3.0 || (y - p0.1).abs() > 3.0;
                    v.glisse = Some((p0, d0, bouge));
                    if bouge {
                        v.deplacer_visionneuse((d0.0 + x - p0.0, d0.1 + y - p0.1));
                    }
                }
                self.redessiner |= v.survoler();
            }
            WindowEvent::CursorLeft { .. } => {
                v.curseur = None;
                self.redessiner |= v.survoler();
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, .. } if v.ouverture_passer() => {}
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                // Un clic agit au RELACHEMENT, et seulement sur ce qui etait
                // sous la souris a l'appui : on peut encore se raviser en
                // glissant hors de la cible, comme partout sur un Mac.
                if v.visionneuse.is_some() {
                    // Ouverte, la visionneuse prend tous les clics : rien ne
                    // passe a ce qu'elle couvre.
                    match state {
                        // L'appui sur une image zoomee la saisit : elle se
                        // glisse, et ne devient un clic que si elle n'a pas bouge.
                        ElementState::Pressed => {
                            let zoomee = v.visionneuse.as_ref().is_some_and(|x| x.zoomee() && !x.ferme);
                            let sur = v.curseur.filter(|&(x, y)| v.image_ouverte().is_some_and(|b| b.contient(x, y)));
                            if let (true, Some(p)) = (zoomee, sur) {
                                let d = v.visionneuse.as_ref().map_or((0.0, 0.0), |x| x.zoom_vise().1);
                                v.glisse = Some((p, d, false));
                            }
                        }
                        ElementState::Released => {
                            let glissee = v.glisse.take().is_some_and(|g| g.2);
                            if let (false, Some((x, y))) = (glissee, v.curseur) {
                                v.clic_visionneuse(x, y);
                            }
                            v.survoler();
                        }
                    }
                    v.appui = None;
                    self.redessiner = true;
                    return;
                }
                if v.passage.is_some() {
                    // Ouverte, la feuille prend tous les clics, au relachement.
                    if let (ElementState::Released, Some((x, y))) = (state, v.curseur) {
                        v.clic_feuille(x, y);
                        v.survoler();
                    }
                    v.appui = None;
                    self.redessiner = true;
                    return;
                }
                let c = v.cible_sous_curseur();
                match state {
                    ElementState::Pressed => v.appui = Some(c),
                    ElementState::Released => {
                        if v.appui.take() == Some(c) {
                            v.cliquer(c);
                            v.survoler();
                        }
                    }
                }
            }
            // La feuille du passage prend la molette : le fil dessous ne bouge pas.
            WindowEvent::MouseWheel { .. } if v.passage.is_some() => {}
            WindowEvent::MouseWheel { delta, .. } if v.visionneuse.is_some() => {
                v.molette_visionneuse(delta, self.ctrl);
                v.survoler();
                self.redessiner = true;
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let brut = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                let sur_bande = v.curseur.is_some_and(|(x, y)| {
                    let g = v.geo_barre();
                    g.panneau.x + g.panneau.l > 0.5 && g.panneau.contient(x, y)
                });
                if self.ctrl {
                    let f = self.facteur_force.unwrap_or(v.fenetre.scale_factor() as f32);
                    v.zoomer(if brut > 0.0 { 1.1 } else { 1.0 / 1.1 }, f);
                } else if sur_bande && v.max_onglets() > 0.0 {
                    // Sur une barre trop longue, la molette fait glisser ses
                    // projets, et seulement eux.
                    let cible = (v.defilement_onglets.cible() - brut * 60.0 * v.echelle)
                        .clamp(0.0, v.max_onglets());
                    v.defilement_onglets.viser(cible);
                } else if !sur_bande {
                    v.molette(brut * v.mesures().interligne * 3.0);
                }
                v.survoler();
            }
            WindowEvent::KeyboardInput { event, .. } if event.state.is_pressed() && v.ouverture_passer() => {}
            WindowEvent::KeyboardInput { event, .. } => {
                use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};
                if let Some(ferme) = v.visionneuse.as_ref().map(|x| x.ferme) {
                    // La visionneuse prend le clavier tant qu'elle existe :
                    // Echap la ferme au lieu de quitter la vigie, et un second
                    // Echap pendant qu'elle se ferme ne quitte rien non plus.
                    if !event.state.is_pressed() || ferme {
                        return;
                    }
                    match event.logical_key {
                        Key::Named(NamedKey::Escape) => v.fermer_visionneuse(),
                        Key::Named(NamedKey::ArrowLeft) => v.voisine_visionneuse(-1),
                        Key::Named(NamedKey::ArrowRight) => v.voisine_visionneuse(1),
                        Key::Named(NamedKey::Space) => v.basculer_video(),
                        Key::Named(NamedKey::Enter) => v.voir_dans_le_fil(),
                        // Le zoom au clavier, avec ou sans Ctrl, comme Aperçu
                        // et Photos.
                        Key::Character(ref c) if c == "+" || c == "=" => v.zoomer_visionneuse(1.5, None),
                        Key::Character(ref c) if c == "-" => v.zoomer_visionneuse(1.0 / 1.5, None),
                        Key::Character(ref c) if c == "0" || c == "à" => {
                            let coupees = v.animations_coupees;
                            if let Some(x) = v.visionneuse.as_mut() {
                                x.viser_zoom(1.0, (0.0, 0.0), coupees);
                            }
                        }
                        _ => {}
                    }
                    return;
                }
                if v.passage.is_some() {
                    // La feuille prend le clavier tant qu'elle existe : Echap la
                    // ferme au lieu de quitter la vigie.
                    if !event.state.is_pressed() {
                        return;
                    }
                    let dans_la_page = v.passage.as_ref().is_some_and(|p| p.vue == feuille::Vue::Page);
                    match event.logical_key {
                        Key::Named(NamedKey::ArrowLeft) if self.alt && dans_la_page => v.agir_feuille(feuille::Action::Reculer),
                        Key::Named(NamedKey::ArrowRight) if self.alt && dans_la_page => v.agir_feuille(feuille::Action::Avancer),
                        Key::Named(NamedKey::Escape) | Key::Named(NamedKey::Backspace) => v.echap_feuille(),
                        Key::Named(NamedKey::Enter) => v.entree_feuille(),
                        Key::Character(ref c) if c.eq_ignore_ascii_case("r") => {
                            let r = v.passage.as_ref().and_then(|p| feuille::reessai(&p.etat, v.feuille_ecoule()));
                            if let Some(a) = r {
                                v.agir_feuille(a);
                            }
                        }
                        _ => {}
                    }
                    return;
                }
                if event.state.is_pressed() {
                    // Ctrl + chiffre : l'onglet de ce rang, par la touche
                    // PHYSIQUE. Sur un clavier AZERTY la rangee du haut donne
                    // « & é " » sans majuscule ; la position, elle, ne change
                    // pas.
                    if self.ctrl {
                        if let PhysicalKey::Code(code) = event.physical_key {
                            let t = match code {
                                KeyCode::Digit1 | KeyCode::Numpad1 => Some(Touche::Rang(1)),
                                KeyCode::Digit2 | KeyCode::Numpad2 => Some(Touche::Rang(2)),
                                KeyCode::Digit3 | KeyCode::Numpad3 => Some(Touche::Rang(3)),
                                KeyCode::Digit4 | KeyCode::Numpad4 => Some(Touche::Rang(4)),
                                KeyCode::Digit5 | KeyCode::Numpad5 => Some(Touche::Rang(5)),
                                KeyCode::Digit6 | KeyCode::Numpad6 => Some(Touche::Rang(6)),
                                KeyCode::Digit7 | KeyCode::Numpad7 => Some(Touche::Rang(7)),
                                KeyCode::Digit8 | KeyCode::Numpad8 => Some(Touche::Rang(8)),
                                KeyCode::Digit9 | KeyCode::Numpad9 => Some(Touche::Rang(9)),
                                KeyCode::KeyB => Some(Touche::Barre),
                                KeyCode::Tab if self.maj => Some(Touche::Gauche),
                                KeyCode::Tab => Some(Touche::Droite),
                                _ => None,
                            };
                            if let Some(t) = t {
                                v.appliquer(t);
                                return;
                            }
                        }
                    }
                    let t = match event.logical_key {
                        Key::Named(NamedKey::Home) => Some(Touche::Debut),
                        Key::Named(NamedKey::Backspace) => Some(Touche::Retour),
                        Key::Named(NamedKey::ArrowLeft) => Some(Touche::Gauche),
                        Key::Named(NamedKey::ArrowRight) => Some(Touche::Droite),
                        Key::Named(NamedKey::ArrowUp) => Some(Touche::Haut),
                        Key::Named(NamedKey::ArrowDown) => Some(Touche::Bas),
                        Key::Named(NamedKey::Enter) => Some(Touche::Entree),
                        Key::Named(NamedKey::Tab) if self.maj => Some(Touche::MajTab),
                        Key::Named(NamedKey::Tab) => Some(Touche::Tab),
                        _ => None,
                    };
                    if let Some(t) = t {
                        v.appliquer(t);
                        return;
                    }
                    match event.logical_key {
                        Key::Named(NamedKey::Escape) if v.barre_par_dessus() => v.basculer_barre(),
                        Key::Named(NamedKey::Escape) => boucle.exit(),
                        Key::Named(NamedKey::End) => {
                            v.colle_en_bas = true;
                            v.ancre = None;
                            v.defilement.viser(v.bas());
                        }
                        Key::Named(NamedKey::PageUp) => v.molette(v.visible() * 0.85),
                        Key::Named(NamedKey::PageDown) => v.molette(-v.visible() * 0.85),
                        Key::Character(ref c) if c == "+" || c == "=" => {
                            let f = self.facteur_force.unwrap_or(v.fenetre.scale_factor() as f32);
                            v.zoomer(1.1, f);
                        }
                        Key::Character(ref c) if c == "-" => {
                            let f = self.facteur_force.unwrap_or(v.fenetre.scale_factor() as f32);
                            v.zoomer(1.0 / 1.1, f);
                        }
                        // T fait tourner le theme : sombre, clair, systeme.
                        Key::Character(ref c) if c.eq_ignore_ascii_case("t") => {
                            v.reglage = v.reglage.suivant();
                            v.appliquer_theme();
                        }
                        Key::Character(ref c) if c == "0" => {
                            let f = self.facteur_force.unwrap_or(v.fenetre.scale_factor() as f32);
                            v.zoom = 1.0;
                            v.echelle = f;
                            v.remettre_en_page();
                        }
                        _ => {}
                    }
                }
            }
            WindowEvent::RedrawRequested => v.dessiner(),
            _ => {}
        }
    }

    /// La fermeture, d'ou qu'elle vienne (la croix, Echap, la fin de session) :
    /// la vigie garde ce qu'on retrouvera au lancement suivant.
    fn exiting(&mut self, _boucle: &ActiveEventLoop) {
        let (true, Some(v)) = (self.garder, self.vigie.as_ref()) else { return };
        let t = v.fenetre.inner_size();
        // Reduite, la fenetre n'a plus de taille : on garde alors l'ancienne.
        let ancien = app::Souvenir::relire();
        let reduite = v.fenetre.is_minimized().unwrap_or(false) || t.width < 200 || t.height < 200;
        let taille = if reduite { ancien.as_ref().map_or((0, 0), |s| s.taille) } else { (t.width, t.height) };
        let position = if reduite {
            ancien.as_ref().and_then(|s| s.position)
        } else {
            v.fenetre.outer_position().ok().map(|p| (p.x, p.y))
        };
        app::Souvenir {
            taille,
            position,
            zoom: v.zoom,
            vue: v.vue.clone(),
            choix: v.choix.clone(),
            theme: v.reglage,
            barre_masquee: v.barre_masquee,
        }
        .garder();
    }

    fn about_to_wait(&mut self, boucle: &ActiveEventLoop) {
        let Some(v) = self.vigie.as_mut() else {
            return;
        };
        // Le temoin de la relance : VIGIE_PERDRE=s detruit le peripherique a
        // s secondes, une fois, dans la premiere vigie seulement.
        if let Some(s) = self.perdre.filter(|&s| self.depart.elapsed().as_secs_f32() >= s) {
            self.perdre = None;
            eprintln!("peripherique detruit a {s:.1} s (VIGIE_PERDRE)");
            v.peripherique.destroy();
            // Une vraie perte (`lose` dans wgpu-core) appelle le rappel tout
            // de suite ; une destruction l'attend au tour suivant du
            // peripherique, qu'on fait ici.
            let _ = v.peripherique.poll(wgpu::PollType::Poll);
        }
        if perdu() {
            boucle.exit();
            return;
        }
        if self.essai {
            self.essai = false;
            v.essai_seul = true;
            v.poses.clear();
            // Le TEMOIN du recours de police. Barlow n'a ni grec ni fleches ;
            // si ces signes rendent un carre vide, le recours n'a pas pris, et
            // aucune capture de contenu reel ne le dirait de facon fiable —
            // il faudrait attendre qu'un agent ecrive du grec.
            for (r, s) in [
                (Registre::Releve, "temoin du recours de police"),
                (Registre::Parole, "Latin : les naifs deja cooperent, ou l'ouvrier."),
                (Registre::Pensee, "Grec : theta θ, phi φ, sigma σ, mu μ, lambda λ, pi π, Δ Ω."),
                (Registre::Pensee, "Signes : ≤ ≥ ≈ ≠ ± × ÷ ∇ ∫ √ ∞ → ← ↔ ° ′ ″ § ¶ † ‡ … — –"),
                (Registre::Geste, "Bash"),
            ] {
                let bloc = Bloc {
                    heure: "00:00:00".to_string(),
                    registre: r,
                    outil: if r == Registre::Geste { "Bash".into() } else { String::new() },
                    lecture: None,
                    corps: if r == Registre::Geste { "echo θ ≤ 45".into() } else { s.to_string() },
                    image: None,
                };
                let pose = v.poser(bloc);
                v.poses.push(pose);
            }
            // Le TEMOIN des images : une mire dont chaque pixel est connu. Deux
            // rampes, rouge puis verte, et un damier. Au milieu d'une rampe,
            // la capture doit lire 127 ou 128 : une conversion sRGB faite deux
            // fois, ou oubliee, l'eloigne de plus de cinquante.
            let (l, h) = (640u32, 360u32);
            let mut px = Vec::with_capacity((l * h * 4) as usize);
            for y in 0..h {
                for x in 0..l {
                    let rampe = (x * 255 / (l - 1)) as u8;
                    let (r, g, b) = if y < 120 {
                        (rampe, 0, 0)
                    } else if y < 240 {
                        (0, rampe, 0)
                    } else if ((x / 40) + (y / 40)) % 2 == 0 {
                        (235, 235, 235)
                    } else {
                        (28, 28, 28)
                    };
                    px.extend_from_slice(&[r, g, b, 255]);
                }
            }
            let bloc = Bloc {
                heure: "00:00:00".to_string(),
                registre: Registre::Image,
                outil: String::new(),
                lecture: None,
                corps: format!("{l} x {h}"),
                image: Some(std::sync::Arc::new(suivi::Image {
                    largeur: l,
                    hauteur: h,
                    pixels: px,
                    chemin: None,
                    origine: suivi::Origine::Produite,
                    video: None,
                    source: (l, h),
                    nommee: true,
                })),
            };
            let pose = v.poser(bloc);
            v.poses.push(pose);
            v.replacer();
            v.defilement.poser(v.bas());
        }
        let neuf = v.recevoir();
        if let Some(chemin) = self.capture.clone() {
            // On laisse le fil de suivi remplir avant de juger : une fenetre
            // vide ne prouve rien. Et l'on attend le catalogue entier, sans
            // quoi un onglet de projet se jugerait sur une liste partielle ;
            // une minute et demie au plus, pour qu'un disque lent ne bloque
            // pas un temoin pour toujours.
            let ecoule = self.depart.elapsed().as_secs_f32();
            // Une page de chantier attend aussi sa fiche : un clic sur une
            // vignette ne se rejoue pas sur une planche vide.
            let fiche_prete = match &v.vue {
                Vue::Chantier(id) => v.fiche.as_ref().is_some_and(|f| &f.id == id && f.complete),
                _ => true,
            };
            // --instant=s capture a s secondes sans rien attendre : c'est le
            // seul moyen de juger ce qu'on voit avant que tout soit lu.
            let pret = match self.instant {
                Some(s) => ecoule >= s,
                None => ecoule > 3.0 && ((v.catalogue.complet && fiche_prete) || ecoule > 90.0),
            };
            let a_rejouer = self.clic.is_some() || !self.touches.is_empty() || self.lien.is_some() || self.adresse.is_some();
            if pret && a_rejouer && self.rejoue.is_none() {
                // Le defilement demande vient avant le clic, comme la main fait
                // defiler puis clique ; il ne se refait pas a la capture.
                if self.defiler != 0.0 {
                    v.molette(-std::mem::take(&mut self.defiler));
                    v.defilement.poser(v.defilement.cible());
                }
                // Les ressorts se posent d'abord : un clic se vise sur ce qui
                // est affiche, pas sur un glissement en cours.
                v.defilement_onglets.poser(v.defilement_onglets.cible());
                v.defilement_liste.poser(v.defilement_liste.cible());
                if let Some((x, y)) = self.clic {
                    v.curseur = Some((x, y));
                    let c = v.cible_sous_curseur();
                    v.cliquer(c);
                    eprintln!("clic en ({x:.0}, {y:.0}) : {c:?}, vue {:?}", v.vue);
                }
                if let Some(n) = self.lien.clone() {
                    let tous = v.liens_visibles();
                    match v.lien_nomme(&n, &tous) {
                        Some((b, k)) => {
                            eprintln!("lien {n} sur {} visibles : bloc {b}, rang {k}", tous.len());
                            v.cliquer(Cible::Lien(b, k));
                        }
                        None => eprintln!("lien {n} absent : {} visibles", tous.len()),
                    }
                }
                if let Some(u) = self.adresse.clone() {
                    v.ouvrir_passage(u);
                }
                for &t in &self.touches {
                    v.barre.poser(v.barre.cible());
                    v.defilement_onglets.poser(v.defilement_onglets.cible());
                    v.defilement_liste.poser(v.defilement_liste.cible());
                    v.appliquer(t);
                    eprintln!("touche {t:?} : vue {:?}, ligne {:?}", v.vue, v.focus);
                }
                v.curseur = None;
                self.rejoue = Some(Instant::now());
            }
            // Apres un clic, le fil a --attente secondes pour se remplir.
            let apres = self.rejoue.is_none_or(|t| t.elapsed().as_secs_f32() > self.attente);
            // Une image visee (--viser) attend d'etre dans le fil, trente secondes
            // au plus : sans elle, la capture prenait le fil ailleurs.
            // Un fil attend d'etre lu jusqu'au bout de ce qui existait a son
            // ouverture, puis une seconde de calme pour que ses blocs soient
            // poses.
            let lu = !matches!(v.vue, Vue::Suivi(_)) || v.fil_lu;
            let calme = lu && v.dernier_bloc.elapsed().as_secs_f32() > 1.0;

            // Trente secondes pour l'image visee, une fois le fil lu ; une
            // minute et demie pour le lire, comme le catalogue. Une capture
            // partie a trente secondes sur un fil a moitie lu montrait 34
            // blocs sur 89, sur une machine chargee.
            let vise = (v.viser.is_none() && calme) || (lu && ecoule > 30.0) || ecoule > 90.0 || self.instant.is_some();

            if pret && vise && (!a_rejouer || self.rejoue.is_some()) && apres {
                while !v.en_attente.is_empty() {
                    v.recevoir();
                }
                v.recevoir();
                if self.defiler != 0.0 {
                    v.molette(-self.defiler);
                }
                if let Some((x, y)) = self.survol {
                    v.curseur = Some((x, y));
                    v.survoler();
                }
                // Les ressorts se posent sur leur cible : une capture prise
                // au milieu d'un glissement ne montrerait pas l'etat voulu.
                v.defilement.poser(v.defilement.cible());
                v.defilement_liste.poser(v.defilement_liste.cible());
                v.defilement_onglets.poser(v.defilement_onglets.cible());
                v.barre.poser(v.barre.cible());
                v.feuille_ressort.poser(v.feuille_ressort.cible());
                if v.passage.as_ref().is_some_and(|p| p.ferme) {
                    v.passage = None;
                    v.feuille = None;
                }
                if let Some(n) = self.survol_lien.clone() {
                    let tous = v.liens_visibles();
                    let boite = v.lien_nomme(&n, &tous).and_then(|(b, k)| v.plan().liens.iter().find(|(_, i, j)| *i == b && *j == k).map(|x| x.0));
                    match boite {
                        Some(b) => {
                            eprintln!("survol du lien {n} sur {} visibles", tous.len());
                            v.curseur = Some((b.x + b.l * 0.5, b.y + b.h * 0.5));
                            v.survoler();
                        }
                        None => eprintln!("survol du lien {n} absent : {} visibles", tous.len()),
                    }
                }
                // --ouvrir : l'image visee s'ouvre dans la visionneuse, par la
                // fonction du clic, puis on attend son image pleine (trois
                // secondes au plus) : sinon la capture montrerait la vignette.
                if let Some(nom) = self.ouvrir.take() {
                    let i = v.poses.iter().position(|p| {
                        p.image.is_some()
                            && p.bloc.image.as_ref().and_then(|x| x.chemin.as_ref()).and_then(|c| c.file_name())
                                == nom.file_name()
                    });
                    if let Some(i) = i {
                        v.ouvrir_visionneuse(visionneuse::Provenance::Fil(i));
                        let debut = Instant::now();
                        while v.visionneuse.as_ref().is_some_and(|x| x.pleine.is_none())
                            && debut.elapsed().as_secs_f32() < 3.0
                        {
                            std::thread::sleep(std::time::Duration::from_millis(20));
                            v.avancer_visionneuse(0.0);
                        }
                    }
                    eprintln!("visionneuse : {}", if v.visionneuse.is_some() { "ouverte" } else { "fermee" });
                }
                if let Some((x, y)) = self.clic_vis.take() {
                    v.clic_visionneuse(x, y);
                    if let Some(lec) = v.visionneuse.as_ref().and_then(|x| x.lecture.as_ref()) {
                        eprintln!("clic dans la visionneuse : position {:.2} s sur {:.2} s", lec.position(), lec.forme.duree);
                    }
                }
                // --ouverture= : la toile se decode dans son fil ; la capture
                // l'attend (huit secondes au plus), sans faire avancer le temps.
                if v.ouverture.is_some() {
                    let debut = Instant::now();
                    while v.ouverture_texture.is_none() && v.ouverture.is_some() && debut.elapsed().as_secs_f32() < 8.0 {
                        std::thread::sleep(std::time::Duration::from_millis(20));
                        v.avancer_ouverture(0.0, 0.0);
                    }
                    eprintln!("ouverture : {}", if v.ouverture_texture.is_some() { "montree" } else { "absente" });
                }
                if let Some((z, p)) = self.loupe.take() {
                    v.zoomer_visionneuse(z, p);
                    if let Some(x) = v.visionneuse.as_mut() {
                        let (z, d) = x.zoom_vise();
                        x.poser_zoom(z, d);
                    }
                    eprintln!("loupe : {:?}", v.visionneuse.as_ref().map(|x| x.zoom()));
                }
                // La visionneuse aussi, ouverte en entier, son image pleine ou
                // sa video recue.
                if let Some(x) = v.visionneuse.as_mut() {
                    x.ouverture.poser(x.ouverture.cible());
                }
                v.avancer_visionneuse(0.0);
                // Ce que la capture montre, pour que celui qui la refait sache si
                // c'est l'etat voulu sans la regarder.
                if self.reel.is_none() || self.reel_debut.is_none() {
                eprintln!(
                    "etat capture : vue {:?}, {} blocs, image visee {}, theme {}, fil {}, a {:.1} s",
                    v.vue,
                    v.poses.len(),
                    if v.viser.is_none() { "atteinte" } else { "en attente" },
                    if palette::clair() { "clair" } else { "sombre" },
                    if v.fil_lu { "lu" } else { "non lu" },
                    ecoule
                );
                }

                v.journal_des_liens();
                if self.reel.is_some() && self.ouvrir_page && v.passage.as_ref().is_some_and(|p| p.vue == feuille::Vue::Passage && !matches!(p.etat, passage::Etat::Chargement)) {
                    // Le second geste, une fois la lecture arrivee : l'action
                    // principale de la feuille (« Ouvrir la page », ou « Ouvrir
                    // quand meme » pour une adresse locale, puis la page).
                    if v.passage.as_ref().is_some_and(|p| feuille::action_principale(&p.etat, v.feuille_ecoule()).is_some()) {
                        v.entree_feuille();
                        eprintln!("geste de Stone : action principale de la feuille");
                    }
                    if v.passage.as_ref().is_some_and(|p| p.vue == feuille::Vue::Page) {
                        self.ouvrir_page = false;
                    }
                }
                if let Some(s) = self.reel {
                    // Pas de capture : la page vit ses s secondes, qu'une
                    // photographie de la fenetre entiere vient prendre de l'exterieur.
                    let debut = *self.reel_debut.get_or_insert_with(Instant::now);
                    if debut.elapsed().as_secs_f32() >= s {
                        self.capture = None;
                        boucle.exit();
                        return;
                    }
                } else {
                    v.capturer(&chemin);
                    // Une seule capture : la boucle peut encore passer ici avant
                    // que la sortie ne prenne effet.
                    self.capture = None;
                    boucle.exit();
                    return;
                }
            }
        }
        if neuf
            || std::mem::take(&mut self.redessiner)
            || !v.defilement.au_repos()
            || !v.defilement_liste.au_repos()
            || !v.defilement_onglets.au_repos()
            || !v.barre.au_repos()
            || v.visionneuse.is_some()
            || v.passage.is_some()
            || v.ouverture.is_some()
        {
            v.fenetre.request_redraw();
        }
        // Un reveil toutes les 40 ms suffit : le fil de suivi relit le disque
        // toutes les 400 ms, et rien d'autre ne bouge entre deux blocs. Une
        // visionneuse ouverte en demande un toutes les 8 ms : une video a 30
        // images par seconde n'en montrerait sinon que 25, et l'ouverture
        // sauterait des pas.
        let reveil = if v.visionneuse.is_some() || v.passage.is_some() || v.ouverture.is_some() { 8 } else { 40 };
        boucle.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + std::time::Duration::from_millis(reveil),
        ));
    }
}

/// Les tailles d'icone que Windows demande : la petite (barre de titre) et la
/// grande (barre des taches), selon l'echelle de l'ecran. Ce sont des tailles
/// du .ico (16 a 24 et 32 a 64) : chacune a son dessin.
fn taille_des_icones(echelle: f64) -> (u32, u32) {
    let petite = if echelle < 1.25 {
        16
    } else if echelle < 1.5 {
        20
    } else if echelle < 2.0 {
        24
    } else {
        32
    };
    let grande = if echelle < 1.25 {
        32
    } else if echelle < 1.5 {
        40
    } else if echelle < 2.0 {
        48
    } else {
        64
    };
    (petite, grande)
}

/// Un titre de jour repris au milieu d'une phrase : « lancé aujourd’hui »,
/// « lancé hier », « lancé le vendredi 25 septembre ».
/// Le signe d'un lien au repos : un soulignement d'un point, de la couleur de
/// son texte, qui ne depend pas de cette couleur pour se voir. Un lien dont
/// l'agent a cite un passage porte en plus un point d'ambre apres lui. Le lien
/// que le clavier a atteint prend un cadre de deux points.
#[allow(clippy::too_many_arguments)]
fn dessiner_liens(
    p: &Pose,
    bi: usize,
    x0: f32,
    y: f32,
    (haut, bas): (f32, f32),
    e: f32,
    corps: f32,
    c: glyphon::Color,
    connu: &dyn Fn(&str) -> bool,
    focus: Option<(usize, usize)>,
    aplats: &mut Vec<formes::Forme>,
) {
    let rect = |b: [f32; 4], couleur: glyphon::Color, rayon: f32| formes::Forme {
        boite: [b[0].round(), b[1].round(), b[2].round(), b[3].round()],
        couleur: palette::lineaire(couleur, 1.0),
        rayon: [rayon, 0.0, 0.0, 0.0],
    };
    for (k, l) in p.liens.iter().enumerate() {
        for b in &l.soulignees {
            let haut_b = y + b.y;
            if haut_b < haut || haut_b + b.h > bas {
                continue;
            }
            let epaisseur = (1.0 * e).round().max(1.0);
            let uy = haut_b + b.h * 0.5 + corps * e * 0.5 - 0.5 * e;
            aplats.push(rect([x0 + b.x, uy, b.l, epaisseur], c, 0.0));
        }
        for b in &l.boites {
            let haut_b = y + b.y;
            if haut_b < haut || haut_b + b.h > bas {
                continue;
            }
            if focus == Some((bi, k)) {
                let t = (2.0 * e).round().max(2.0);
                let (fx, fy, fl, fh) = (x0 + b.x - plan::LIEN_DEBORD * e, haut_b, b.l + 2.0 * plan::LIEN_DEBORD * e, b.h);
                let c1 = palette::encre1();
                aplats.push(rect([fx, fy, fl, t], c1, 0.0));
                aplats.push(rect([fx, fy + fh - t, fl, t], c1, 0.0));
                aplats.push(rect([fx, fy, t, fh], c1, 0.0));
                aplats.push(rect([fx + fl - t, fy, t, fh], c1, 0.0));
            }
        }
        if let Some(b) = l.boites.last().filter(|_| connu(&l.url)) {
            let haut_b = y + b.y;
            if haut_b >= haut && haut_b + b.h <= bas {
                let d = 6.0 * e;
                aplats.push(rect([x0 + b.x + b.l + 6.0 * e, haut_b + (b.h - d) * 0.5, d, d], glyphon::Color::rgb(0xFF, 0xC8, 0x57), d * 0.5));
            }
        }
    }
}

/// Les liens d'une ligne de geste. `WebFetch` : la ligne entiere est le lien.
/// `Bash` et `PowerShell` : seulement les adresses que la commande porte, et
/// que la ligne montre encore apres sa coupe. Les autres outils (un chemin, un
/// motif) n'ont pas de lien.
fn liens_du_geste(bloc: &Bloc, corps: &Buffer) -> Vec<LienPose> {
    let montre: String = corps.lines.iter().map(|l| l.text()).collect::<Vec<_>>().join("\n");
    match bloc.outil.as_str() {
        "WebFetch" => {
            let url = bloc.corps.trim();
            if montre.is_empty() || passage::classer(url) == passage::Classe::Gabarit {
                return Vec::new();
            }
            let boites = feuille::boites(corps, &montre, 0, montre.len());
            if boites.is_empty() {
                return Vec::new();
            }
            vec![LienPose { url: url.to_string(), soulignees: boites.clone(), boites, rangee: true }]
        }
        "Bash" | "PowerShell" => {
            let vraies = passage::liens(&bloc.corps);
            passage::liens(&montre)
                .into_iter()
                .zip(vraies)
                .filter(|(_, v)| passage::classer(&v.url) != passage::Classe::Gabarit)
                .map(|(m, v)| {
                    let boites = feuille::boites(corps, &montre, m.debut, m.fin);
                    LienPose { soulignees: boites.clone(), boites, url: v.url, rangee: false }
                })
                .filter(|l| !l.boites.is_empty())
                .collect()
        }
        _ => Vec::new(),
    }
}

fn hex_couleur(c: glyphon::Color) -> String {
    format!("{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

fn nom_de_l_etat(e: &passage::Etat) -> String {
    match e {
        passage::Etat::Redirection { vers, contenu } => format!("redirection vers {vers} ({})", nom_de_l_etat(contenu)),
        passage::Etat::Passage(x) => format!("passage ({:?})", x.trouve),
        passage::Etat::Introuvable { .. } => "passage introuvable".to_string(),
        passage::Etat::SansPassage { .. } => "aucun passage cite".to_string(),
        passage::Etat::Refuse(s) => format!("refuse ({s})"),
        passage::Etat::Echec(x) => format!("echec ({x:?})"),
        autre => format!("{autre:?}").to_lowercase(),
    }
}

fn minuscule_jour(titre: &str) -> String {
    let mut c = titre.chars();
    let bas = match c.next() {
        Some(p) => p.to_lowercase().chain(c).collect::<String>(),
        None => String::new(),
    };
    if bas.starts_with("aujourd") || bas == "hier" {
        bas
    } else {
        format!("le {bas}")
    }
}

fn argument(nom: &str) -> Option<String> {
    std::env::args()
        .find(|a| a.starts_with(nom))
        .map(|a| a[nom.len()..].to_string())
        .filter(|a| !a.is_empty())
}

/// « 640,212 » : un point en pixels de la fenetre, ceux des captures.
fn point(a: &str) -> Option<(f32, f32)> {
    let (x, y) = a.split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

fn main() {
    // Sans drapeau, c'est l'application qu'on ouvre : une seule fenetre, qui
    // reprend la ou on l'a laissee. Un drapeau, lui, impose un etat a juger,
    // et une capture doit pouvoir tourner pendant que la vigie est ouverte.
    let libre = std::env::args().len() == 1;
    let balayes = navigateur::balayer_orphelins();
    if balayes > 0 {
        eprintln!("{balayes} profil(s) de page laisse(s) par un arret force, supprime(s)");
    }
    if libre && app::deja_ouverte() {
        return;
    }
    let souvenir = if libre { app::Souvenir::relire() } else { None };
    let (envoi, recois) = channel();
    let (envoi_choix, recois_choix) = channel();
    let (envoi_fiche, recois_fiche) = channel();
    // --chantier=wf_<id> ouvre la page de ce chantier ; avec --agent=<role>,
    // le fil de cet agent ; --onglet=<projet> ouvre la liste de ce projet. Le
    // chantier l'emporte : on ne peut pas a la fois lire une liste et un fil.
    let chantier_voulu = argument("--chantier=");
    let onglet = argument("--onglet=");
    let agent = match (&chantier_voulu, argument("--agent=")) {
        (Some(c), Some(a)) => {
            let trouve = chantier::agent_nomme(c, &a);
            if trouve.is_none() {
                eprintln!("aucun agent « {a} » dans {c}");
            }
            trouve
        }
        _ => None,
    };
    let (vue, choix) = match (chantier_voulu, onglet, agent) {
        (Some(id), _, Some(a)) => (Vue::Suivi(id.clone()), Choix::Agent { chantier: id, agent: a }),
        (Some(id), _, None) => {
            let _ = envoi_fiche.send(Some(id.clone()));
            (Vue::Chantier(id), Choix::Direct)
        }
        (None, Some(p), _) => (Vue::Liste(p), Choix::Direct),
        (None, None, _) => match &souvenir {
            Some(s) => {
                if let Vue::Chantier(id) = &s.vue {
                    let _ = envoi_fiche.send(Some(id.clone()));
                }
                (s.vue.clone(), s.choix.clone())
            }
            None => (Vue::Direct, Choix::Direct),
        },
    };
    // Vingt entrees en arriere : assez pour raccrocher le fil, pas assez pour
    // noyer l'arrivee sous la consigne, qui fait des milliers de lignes.
    suivi::lancer(envoi.clone(), recois_choix, choix.clone(), 20);
    import::lancer();
    chantier::lancer(envoi.clone(), recois_fiche);
    catalogue::lancer(envoi);

    let boucle = EventLoop::new().expect("boucle d'evenements impossible");
    boucle.set_control_flow(ControlFlow::Wait);
    let capture = std::env::args()
        .find(|a| a.starts_with("--capture="))
        .map(|a| a["--capture=".len()..].to_string());
    let mut demarrage = Demarrage {
        vigie: None,
        recois: Some(recois),
        capture,
        ctrl: false,
        essai: std::env::args().any(|a| a == "--essai"),
        depart: Instant::now(),
        vue,
        choix,
        envoi_choix: Some(envoi_choix),
        envoi_fiche: Some(envoi_fiche),
        attente: argument("--attente=").and_then(|a| a.parse().ok()).unwrap_or(2.5),
        maj: false,
        alt: false,
        redessiner: false,
        clic: argument("--clic=").and_then(|a| point(&a)),
        touches: argument("--touches=")
            .map(|a| a.split(',').filter_map(Touche::nommee).collect())
            .unwrap_or_default(),
        survol: argument("--survol=").and_then(|a| point(&a)),
        lien: argument("--lien="),
        survol_lien: argument("--survol-lien="),
        adresse: argument("--adresse="),
        defiler: argument("--defiler=").and_then(|a| a.parse().ok()).unwrap_or(0.0),
        rejoue: None,
        souvenir,
        garder: libre,
        facteur_force: argument("--echelle=")
            .and_then(|a| a.replace(',', ".").parse::<f32>().ok())
            .filter(|f| (0.5..=4.0).contains(f)),
        taille: argument("--taille=").and_then(|a| {
            let (l, h) = a.split_once('x')?;
            Some((l.trim().parse().ok()?, h.trim().parse().ok()?))
        }),
        instant: argument("--instant=").and_then(|a| a.replace(',', ".").parse().ok()),
        theme: argument("--theme=").and_then(|a| palette::Reglage::nomme(&a)),
        viser: argument("--viser=").map(std::path::PathBuf::from),
        reel: argument("--reel=").and_then(|a| a.replace(',', ".").parse().ok()),
        reel_debut: None,
        ouvrir_page: std::env::args().any(|a| a == "--ouvrir-page"),
        ouvrir: argument("--viser=")
            .filter(|_| std::env::args().any(|a| a == "--ouvrir"))
            .map(std::path::PathBuf::from),
        ouverture_capture: argument("--ouverture=").and_then(|a| a.replace(',', ".").parse::<f32>().ok()),
        loupe: argument("--loupe=").and_then(|a| {
            let n: Vec<f32> = a.split(',').filter_map(|x| x.trim().parse().ok()).collect();
            match n[..] {
                [z] => Some((z, None)),
                [z, x, y] => Some((z, Some((x, y)))),
                _ => None,
            }
        }),
        clic_vis: argument("--clic-vis=").and_then(|a| {
            let (x, y) = a.split_once(',')?;
            Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
        }),
        barre: match argument("--barre=").as_deref() {
            Some("masquee") | Some("masquée") => Some(true),
            Some("visible") => Some(false),
            _ => None,
        },
        mouvement_reduit: std::env::args().any(|a| a == "--mouvement-reduit"),
        contraste_eleve: std::env::args().any(|a| a == "--contraste-eleve"),
        perdre: std::env::var("VIGIE_PERDRE")
            .ok()
            .filter(|_| std::env::var_os("VIGIE_RELANCE").is_none())
            .and_then(|v| v.replace(',', ".").parse().ok()),
    };
    let capture = demarrage.capture.is_some();
    boucle.run_app(&mut demarrage).expect("la vigie s'est arretee");
    // La fenetre detruite avant la relance : sans cela, la nouvelle vigie
    // trouverait l'ancienne ouverte et rendrait la main.
    drop(demarrage);
    navigateur::attendre_les_suppressions();
    if perdu() {
        relancer(capture);
    }
}

/// Le peripherique graphique perdu, dit par wgpu (`set_device_lost_callback`).
static PERDU: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn perdu() -> bool {
    PERDU.load(std::sync::atomic::Ordering::SeqCst)
}

/// Relance la vigie, avec les memes drapeaux, quand son peripherique graphique
/// est perdu : une carte reinitialisee par le pilote, ou prise par un autre
/// programme. Rien ne peut etre redessine sur un peripherique perdu, et tout
/// refaire dans le meme processus demanderait de recreer chaque ressource ; un
/// nouveau processus les recree toutes, et le souvenir le ramene ou l'on etait.
/// Une capture attend la sienne et rend son code, pour que le script qui l'a
/// lancee trouve l'image. Trois relances au plus, comptees dans
/// l'environnement, pour qu'une carte qui se perd a chaque lancement ne fasse
/// pas une boucle.
fn relancer(capture: bool) {
    const MAX: u32 = 3;
    let n: u32 = std::env::var("VIGIE_RELANCE").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    if n >= MAX {
        eprintln!("peripherique perdu {MAX} fois de suite : la vigie s'arrete");
        std::process::exit(3);
    }
    let Ok(moi) = std::env::current_exe() else { std::process::exit(3) };
    eprintln!("relance {} sur {MAX}", n + 1);
    let mut c = std::process::Command::new(moi);
    c.args(std::env::args().skip(1)).env("VIGIE_RELANCE", (n + 1).to_string());
    match c.spawn() {
        Ok(mut enfant) if capture => {
            let code = enfant.wait().ok().and_then(|s| s.code()).unwrap_or(3);
            std::process::exit(code);
        }
        Ok(_) => {}
        Err(_) => std::process::exit(3),
    }
}
