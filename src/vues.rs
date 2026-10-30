//! La barre des projets, la liste des chantiers d'un projet, et la page d'un
//! chantier : ses agents sur la grille de la liste, puis la planche de ses
//! images.
//!
//! Une rangee par projet dans la barre laterale, « Direct » en premier, puis les projets du plus
//! recemment actif au plus ancien. Dans un onglet de projet, la liste de ses
//! chantiers, rangee par jour comme Mail range son courrier : cent chantiers
//! en vrac ne se lisent pas, cent chantiers sous « Aujourd'hui », « Hier »,
//! « jeudi 1er octobre » se parcourent d'un coup d'oeil.
//!
//! Rien ici n'invente de langue visuelle. La liste reprend la grille du fil :
//! l'heure dans la gouttiere, le texte dans la colonne, le rail pour ce qu'on
//! suit, un point blanc pour ce qui vit. Le projet ouvert prend la plaque du
//! gris des filets, la seule surface que la vigie connaissait deja.
//!
//! Les mesures sont NOMINALES, a echelle 1, comme celles de main.rs ; rien ne
//! les lit sans les multiplier par l'echelle.

use std::sync::Arc;

use glyphon::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, TextArea, TextBounds, Weight};

use crate::catalogue::Catalogue;
use crate::chantier::{Fiche, FicheAgent};
use crate::formes::Forme;
use crate::plan::Boite;
use crate::{dates, palette};

/// La bande du haut, au-dessus de l'en-tete : elle portait les onglets, elle
/// porte le bouton de la barre laterale, et la barre commence sous elle.
pub const ONGLETS_H: f32 = 40.0;
/// La hauteur de ligne du nom d'un projet, centree dans sa rangee.
const PILULE_H: f32 = 26.0;
/// Un cran sous le texte lu (15) : les onglets se lisent, mais ne doivent pas
/// appeler l'oeil avant le fil.
const ONGLET_CORPS: f32 = 13.0;
/// Le point d'un projet ou quelque chose tourne, et l'air qui le precede.
/// Il ne respire pas : l'animer obligerait a redessiner vingt-cinq fois par
/// seconde une fenetre qui, sinon, ne travaille qu'a l'arrivee d'un bloc. Le
/// blanc du dit, un peu baisse, suffit a le distinguer de tout le reste.
const POINT_ALPHA: f32 = 0.85;
const POINT: f32 = 5.0;
const AVANT_POINT: f32 = 6.0;
/// La barre laterale (tour 00, l'emprunt a la direction materiau). Sa largeur
/// est fixe, sans proportion qui la ferait varier avec la fenetre. Elle est
/// tranchee par la planche (passage-b/artisan/cout_barre.py) : a la moitie
/// d'ecran, 188 points ne laissaient que 2 vignettes de front contre 4 barre
/// masquee ; a 180,4 points au plus il en tient 3. A 176, le nom d'un projet garde
/// 129 points, une vingtaine de signes : « dossier personnel » y tient.
pub const BARRE_L: f32 = 176.0;
/// En dessous de cette largeur de fenetre, en points, elle se replie d'elle
/// meme et ne revient que par-dessus le contenu (HIG *Sidebars* : « Consider
/// automatically hiding and revealing a sidebar when its container window
/// resizes » ; 640 pt, le seuil de la direction materiau).
pub const BARRE_SEUIL: f32 = 640.0;
/// Le bouton : 28 points, la cible du systeme (§6), dans la bande du haut.
pub const BOUTON: f32 = 28.0;
const BOUTON_X: f32 = 8.0;
/// Une rangee : 28 points de haut, la meme cible, et 2 d'air entre deux.
const RANG_H: f32 = 28.0;
const RANG_ENTRE: f32 = 2.0;
const RANG_AIR: f32 = 8.0;
const RANG_TEXTE: f32 = 10.0;
const RANG_RAYON: f32 = 7.0;
const BARRE_HAUT: f32 = 4.0;
/// « Projets », le titre du groupe : l'air qui le precede et sa hauteur.
const GROUPE_AVANT: f32 = 14.0;
const GROUPE_H: f32 = 22.0;

/// L'air entre le filet de l'en-tete et le premier titre de jour.
const LISTE_HAUT: f32 = 18.0;
/// Un titre de jour : l'air qui le precede (sauf le premier), sa hauteur, et
/// l'air qui le suit. Plus d'air avant qu'apres : un titre appartient a ce
/// qu'il annonce.
const SECTION_AVANT: f32 = 22.0;
const SECTION_H: f32 = 16.0;
const SECTION_APRES: f32 = 8.0;
const SECTION_CORPS: f32 = 12.0;
/// Une ligne de chantier : le nom sur l'interligne du fil, la description
/// en dessous, plus petite, puis l'air du fil entre deux blocs.
const DESC_CORPS: f32 = 13.0;
const DESC_H: f32 = 19.0;
/// La plaque de survol deborde la grille de chaque cote, et ses coins sont ceux
/// d'une cellule de liste chez Apple.
const PLAQUE_DEBORD: f32 = 12.0;
const PLAQUE_RAYON: f32 = 8.0;
/// Le point d'un chantier en cours, apres son nom.
const POINT_LIGNE: f32 = 6.0;
const AVANT_POINT_LIGNE: f32 = 8.0;

/// Ce que montre la fenetre.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Vue {
    /// Le fil du chantier le plus recent : le comportement d'origine.
    Direct,
    /// La liste des chantiers d'un projet.
    Liste(String),
    /// La page d'un chantier : ses agents et la planche de ses images.
    Chantier(String),
    /// Le fil d'un chantier choisi, par son identifiant : celui de l'agent
    /// choisi dans sa page, ou de celui qui ecrit.
    Suivi(String),
}

pub struct OngletPose {
    /// Aucun pour « Direct ».
    pub projet: Option<String>,
    pub tampon: Buffer,
    /// Le haut de la rangee dans la zone des projets de la barre, avant
    /// defilement (sans objet pour « Direct », qui est fixe), et sa hauteur.
    /// Les noms viennent de la bande d'onglets, ou c'etaient une abscisse et
    /// une largeur.
    pub x: f32,
    pub l: f32,
    pub vivant: bool,
}

pub struct LignePosee {
    pub id: String,
    pub heure: Buffer,
    pub nom: Buffer,
    pub nom_l: f32,
    pub meta: Buffer,
    pub meta_l: f32,
    pub desc: Buffer,
    /// Ou se pose la description : sur la ligne du nom, apres lui, quand la
    /// colonne est large ; dessous sinon. Relatif au nom.
    pub desc_x: f32,
    pub desc_dy: f32,
    /// Le haut de la ligne, dans la liste, avant defilement, et sa hauteur.
    pub y: f32,
    pub h: f32,
    pub en_cours: bool,
}

/// La chronologie d'un chantier : un axe des heures, et pour chaque agent une
/// barre de son debut a sa derniere ecriture, une marque par entree de sa
/// transcription et une marque haute par image. Plusieurs agents se lisent
/// alors d'un coup, et ce qui s'est fait en meme temps se voit en meme temps.
pub struct Chrono {
    /// Les graduations : leur texte, leur abscisse relative a la colonne.
    pub axe: Vec<(Buffer, f32, f32)>,
    /// Le haut de la ligne des graduations, dans la page.
    pub y_axe: f32,
    pub barres: Vec<Barre>,
    /// La demi-hauteur du trait d'une graduation autour de chaque barre. Le
    /// trait ne court plus de l'axe a la derniere barre : il traversait les
    /// noms et les bilans, et sous le contraste eleve, devenu noir, il barrait
    /// « C:/Us|ers » (juge-design.md, m1). Il ne se pose que dans la bande de
    /// la barre, ou il n'y a pas de texte.
    pub demi_trame: f32,
}

pub struct Barre {
    /// Le milieu de la barre, dans la page.
    pub y: f32,
    /// Debut et fin, relatifs a la colonne.
    pub x0: f32,
    pub x1: f32,
    pub vivant: bool,
    pub activite: Vec<f32>,
    pub images: Vec<f32>,
}

pub struct SectionPosee {
    pub titre: Buffer,
    pub y: f32,
}

pub struct ListePosee {
    /// Le projet d'une liste, ou le chantier d'une page.
    pub projet: String,
    /// Ce qui a servi a la poser : si rien n'a change, on ne refait rien.
    pub signature: Vec<String>,
    pub lignes: Vec<LignePosee>,
    pub sections: Vec<SectionPosee>,
    /// La planche d'une page de chantier ; vide dans une liste de projet.
    pub vignettes: Vec<VignettePosee>,
    pub chrono: Option<Chrono>,
    pub hauteur: f32,
}

/// Une vignette de la planche. Ses boites sont relatives : x depuis le bord de
/// la colonne, y depuis le haut de la page, avant defilement.
pub struct VignettePosee {
    /// Son rang dans la fiche.
    pub rang: usize,
    /// Le cadre ou l'image tient entiere, sans etre rognee : couper une capture
    /// pour la faire entrer dans un carre cacherait ce qu'on veut juger.
    pub cadre: Boite,
    /// L'image dans son cadre, a sa taille d'affichage.
    pub image: Boite,
    pub nom: Buffer,
    pub legende: Buffer,
    /// Creee a la premiere apparition, a la taille exacte d'affichage.
    pub texture: Option<crate::images::Texture>,
}

/// La barre laterale des projets, pour une trame. Elle remplace la bande
/// d'onglets (decision de l'auteur, tour 00 : « La console + la barre de C ») :
/// « Direct » en haut, puis le groupe « Projets », une rangee par projet.
pub struct GeoBarre {
    /// Le panneau, du haut au bas de la fenetre ; son bord droit porte le
    /// filet qui le separe du contenu.
    pub panneau: Boite,
    /// La rangee de « Direct », fixe.
    pub direct: Boite,
    /// Le haut du titre « Projets ».
    pub groupe_y: f32,
    /// La zone ou defilent les rangees des projets : ce qui en sort n'est ni
    /// vu ni cliquable.
    pub fenetre: Boite,
    pub defil: f32,
    pub longueur: f32,
    pub e: f32,
}

impl GeoBarre {
    /// La rangee de l'onglet de rang i, en pixels d'ecran.
    pub fn rangee(&self, i: usize, o: &OngletPose) -> Boite {
        if i == 0 {
            return self.direct;
        }
        Boite { x: self.direct.x, y: self.fenetre.y + o.x - self.defil, l: self.direct.l, h: RANG_H * self.e }
    }

    pub fn max(&self) -> f32 {
        (self.longueur - self.fenetre.h).max(0.0)
    }
}

/// La geometrie de la barre. `x` est le bord gauche du panneau : 0 quand
/// elle se voit en entier, moins sa largeur quand elle est rentree, entre les
/// deux pendant son mouvement.
pub fn geo_barre(onglets: &[OngletPose], x: f32, hauteur: f32, defil: f32, e: f32) -> GeoBarre {
    let panneau = Boite { x, y: 0.0, l: BARRE_L * e, h: hauteur };
    let direct = Boite {
        x: x + RANG_AIR * e,
        y: (ONGLETS_H + BARRE_HAUT) * e,
        l: (BARRE_L - 2.0 * RANG_AIR) * e,
        h: RANG_H * e,
    };
    let groupe_y = direct.y + direct.h + GROUPE_AVANT * e;
    let haut = groupe_y + GROUPE_H * e;
    let fenetre = Boite { x, y: haut, l: panneau.l, h: (hauteur - haut - RANG_AIR * e).max(0.0) };
    let longueur = if onglets.len() > 1 {
        onglets.last().map_or(0.0, |o| o.x + RANG_H * e)
    } else {
        0.0
    };
    GeoBarre {
        panneau,
        direct,
        groupe_y,
        fenetre,
        defil: defil.clamp(0.0, (longueur - fenetre.h).max(0.0)),
        longueur,
        e,
    }
}

/// Le bouton qui masque et reaffiche la barre, toujours a la meme place :
/// en haut a gauche de la fenetre, comme celui de l'application Claude.
pub fn bouton_barre(e: f32) -> Boite {
    Boite { x: BOUTON_X * e, y: (ONGLETS_H - BOUTON) * 0.5 * e, l: BOUTON * e, h: BOUTON * e }
}

/// La geometrie de la liste, pour une trame.
#[derive(Clone, Copy)]
pub struct GeoListe {
    /// Le haut de la zone visible : le bas de l'en-tete.
    pub haut: f32,
    pub bas: f32,
    pub defil: f32,
    pub x_heure: f32,
    pub x_corps: f32,
    pub colonne: f32,
    pub large_bloc: f32,
    /// Le bord droit de la fenetre, pour borner le texte.
    pub droite: f32,
    pub e: f32,
}

impl GeoListe {
    pub fn plaque(&self, l: &LignePosee) -> Boite {
        let e = self.e;
        Boite {
            x: self.x_heure - PLAQUE_DEBORD * e,
            y: self.haut + l.y - self.defil - LIGNE_AIR * 0.5 * e,
            l: self.large_bloc + 2.0 * PLAQUE_DEBORD * e,
            h: l.h + LIGNE_AIR * e,
        }
    }

    /// Le cadre d'une vignette, en pixels d'ecran.
    pub fn vignette(&self, v: &VignettePosee) -> Boite {
        Boite { x: self.x_corps + v.cadre.x, y: self.haut + v.cadre.y - self.defil, ..v.cadre }
    }

    pub fn zone(&self) -> Boite {
        Boite {
            x: 0.0,
            y: self.haut,
            l: f32::MAX,
            h: (self.bas - self.haut).max(0.0),
        }
    }
}

fn barlow() -> Attrs<'static> {
    Attrs::new().family(Family::Name("Barlow"))
}

/// Les chemins longs d'un texte ramenes a leurs deux derniers elements :
/// « C:/Users/alex/dev/active/atelier-d/recherche/design/forme.md » devient
/// « …/design/forme.md ». Ce qui distingue un chemin d'un autre est sa fin ;
/// son debut est commun a tout un chantier (juge-design.md, M4 : six bilans
/// sur sept coupes avant le nom du fichier).
pub fn abreger_chemins(texte: &str) -> String {
    texte
        .split(' ')
        .map(|mot| {
            let debut = mot.len() - mot.trim_start_matches(['"', '\'', '(', '`']).len();
            let fin = mot.trim_end_matches(['"', '\'', ')', ',', ';', ':', '.', '`']).len();
            if fin <= debut {
                return mot.to_string();
            }
            let coeur = &mot[debut..fin];
            let parts: Vec<&str> = coeur.split(['/', '\\']).filter(|p| !p.is_empty()).collect();
            if parts.len() < 4 || coeur.contains("://") || !(coeur.contains('/') || coeur.contains('\\')) {
                return mot.to_string();
            }
            let sep = if coeur.contains('/') { "/" } else { "\\" };
            let queue = &parts[parts.len() - 2..];
            format!("{}…{sep}{}{}", &mot[..debut], queue.join(sep), &mot[fin..])
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Le dernier mot d'un texte coupe en son milieu pour que le tout tienne, s'il
/// est assez long pour que debut et fin disent encore quelque chose (douze
/// signes au moins, trois de chaque cote). Aucun sinon.
fn couper_au_milieu(systeme: &mut FontSystem, metrics: Metrics, attrs: &Attrs, texte: &str, largeur: f32) -> Option<(Buffer, f32)> {
    let (tete, mot) = match texte.rfind(' ') {
        Some(i) => (&texte[..=i], &texte[i + 1..]),
        None => ("", texte),
    };
    let signes: Vec<char> = mot.chars().collect();
    if signes.len() < 12 {
        return None;
    }
    // Un nom de fichier se distingue par son debut et garde son extension :
    // moitie, moitie. Un nom d'agent se distingue par sa fin, son debut
    // (« chercheur- ») etant commun a tout un chantier : un tiers, deux tiers.
    let fichier = mot.contains('.');
    let forme = |k: usize| {
        let a = if fichier { k.div_ceil(2) } else { (k / 3).max(3) };
        let z = k - a;

        let debut: String = signes[..a].iter().collect();
        let fin: String = signes[signes.len() - z..].iter().collect();
        format!("{tete}{}…{}", debut.trim_end_matches(['-', '_', '.', ' ']), fin.trim_start_matches(['-', '_', '.']))
    };
    // Le plus grand nombre de signes gardes qui tienne, par dichotomie.
    let (mut bas, mut haut) = (6usize, signes.len() - 1);
    if !essayer(systeme, metrics, attrs, &forme(bas), largeur).2 {
        return None;
    }
    while bas < haut {
        let k = (bas + haut).div_ceil(2);
        if essayer(systeme, metrics, attrs, &forme(k), largeur).2 {
            bas = k;
        } else {
            haut = k - 1;
        }
    }
    let (b, l, _) = essayer(systeme, metrics, attrs, &forme(bas), largeur);
    Some((b, l))
}

/// Le premier tampon d'un texte a cette largeur, et s'il tient sur une ligne.
fn essayer(systeme: &mut FontSystem, metrics: Metrics, attrs: &Attrs, texte: &str, largeur: f32) -> (Buffer, f32, bool) {
    let mut b = Buffer::new(systeme, metrics);
    b.set_size(Some(largeur), None);
    b.set_text(texte, attrs, Shaping::Advanced, None);
    b.shape_until_scroll(systeme, false);
    let (lignes, l0) = {
        let mut runs = b.layout_runs();
        let l0 = runs.next().map(|r| r.line_w).unwrap_or(0.0);
        (1 + runs.count(), l0)
    };
    (b, l0, lignes <= 1)
}

/// Un texte sur UNE ligne d'au plus `largeur` px. S'il deborde, ses chemins
/// sont d'abord ramenes a leur fin ; puis, si son dernier mot est un long nom
/// (un fichier, un agent), il est coupe en son milieu, comme le Finder coupe
/// un nom de fichier : « chercheur-direction-artistique » garde son debut et
/// sa fin, ce qui le distingue de « chercheur-forme ». Sinon, une ellipse a la
/// fin, de preference apres un mot. Rend le tampon et sa largeur.
pub fn une_ligne(systeme: &mut FontSystem, metrics: Metrics, attrs: &Attrs, texte: &str, largeur: f32) -> (Buffer, f32) {
    let brut = texte.split_whitespace().collect::<Vec<_>>().join(" ");
    let largeur = largeur.max(1.0);
    let (b, l0, tient) = essayer(systeme, metrics, attrs, &brut, largeur);
    if tient {
        return (b, l0);
    }
    let propre = abreger_chemins(&brut);
    let (mut b, l0, tient) = essayer(systeme, metrics, attrs, &propre, largeur);
    if tient {
        return (b, l0);
    }
    if let Some(r) = couper_au_milieu(systeme, metrics, attrs, &propre, largeur) {
        return r;
    }

    let ellipse = {
        let mut m = Buffer::new(systeme, metrics);
        m.set_size(None, None);
        m.set_text("…", attrs, Shaping::Advanced, None);
        m.shape_until_scroll(systeme, false);
        m.layout_runs().next().map(|r| r.line_w).unwrap_or(metrics.font_size)
    };
    let coupe = b
        .layout_runs()
        .next()
        .map(|r| {
            let mut c = 0;
            for g in r.glyphs {
                if g.x + g.w <= largeur - ellipse {
                    c = g.end;
                } else {
                    break;
                }
            }
            c
        })
        .unwrap_or(0);
    let mut t = propre.get(..coupe).unwrap_or("").trim_end().to_string();
    // Une ellipse apres une virgule ou un deux-points se lit comme une faute.
    while t.ends_with([',', ';', ':', '·', '-', '—', '(']) {
        t.pop();
        t = t.trim_end().to_string();
    }
    t.push('…');
    b.set_text(&t, attrs, Shaping::Advanced, None);
    b.shape_until_scroll(systeme, false);
    let l = b.layout_runs().next().map(|r| r.line_w).unwrap_or(0.0);
    (b, l)
}

/// La largeur d'un texte pose sans limite.
fn largeur_libre(systeme: &mut FontSystem, metrics: Metrics, attrs: &Attrs, texte: &str) -> (Buffer, f32) {
    let mut b = Buffer::new(systeme, metrics);
    b.set_size(None, None);
    b.set_text(texte, attrs, Shaping::Advanced, None);
    b.shape_until_scroll(systeme, false);
    let l = b.layout_runs().next().map(|r| r.line_w).unwrap_or(0.0);
    (b, l)
}

/// Les onglets a montrer : Direct, puis les projets par lancement recent, et
/// le projet demande s'il n'est pas (ou pas encore) au catalogue, pour que la
/// selection se voie toujours. Avec, pour chacun, s'il a un chantier en cours.
pub fn onglets_voulus(catalogue: &Catalogue, vue_projet: Option<&str>) -> Vec<(Option<String>, bool)> {
    let mut v: Vec<(Option<String>, bool)> = vec![(None, false)];
    for p in catalogue.projets() {
        let vivant = catalogue.de(&p).any(|c| c.vivant(crate::app::maintenant()));
        v.push((Some(p), vivant));
    }
    if let Some(p) = vue_projet {
        if !v.iter().any(|(q, _)| q.as_deref() == Some(p)) {
            v.push((Some(p.to_string()), false));
        }
    }
    v
}

pub fn poser_onglets(systeme: &mut FontSystem, voulus: &[(Option<String>, bool)], e: f32) -> Vec<OngletPose> {
    let metrics = Metrics::new(ONGLET_CORPS * e, PILULE_H * e);
    let attrs = barlow().weight(Weight::MEDIUM);
    // Le nom tient sa rangee : coupe d'une ellipse plutot que de passer sous
    // le point d'un projet vivant ou hors de la barre.
    let place = (BARRE_L - 2.0 * RANG_AIR - 2.0 * RANG_TEXTE - POINT - AVANT_POINT) * e;
    let mut y = 0.0;
    let mut sortie = Vec::with_capacity(voulus.len());
    for (rang, (projet, vivant)) in voulus.iter().enumerate() {
        let libelle = projet.as_deref().unwrap_or(crate::langue::tr("Direct", "Live"));
        let (tampon, _) = une_ligne(systeme, metrics, &attrs, libelle, place);
        sortie.push(OngletPose {
            projet: projet.clone(),
            tampon,
            x: if rang == 0 { 0.0 } else { y },
            l: RANG_H * e,
            vivant: *vivant,
        });
        if rang > 0 {
            y += (RANG_H + RANG_ENTRE) * e;
        }
    }
    sortie
}

/// La signature d'une liste : tout ce qui, en changeant, change ce qu'on lit.
pub fn signature(catalogue: &Catalogue, projet: &str, aujourdhui: i64) -> Vec<String> {
    let mut s: Vec<String> = catalogue
        .de(projet)
        .map(|c| {
            let (j, m) = dates::jour_et_minute(c.debut);
            format!("{}|{}|{}|{:?}|{}|{}|{}", c.id, c.nom, c.description, c.etat, c.agents, j, m)
        })
        .collect();
    s.push(format!("aujourd'hui {aujourdhui}"));
    s
}

/// Met en page la liste d'un projet. Appele quand sa signature change, jamais
/// par trame.
pub fn poser_liste(
    systeme: &mut FontSystem,
    catalogue: &Catalogue,
    projet: &str,
    colonne: f32,
    e: f32,
    aujourdhui: i64,
    signature: Vec<String>,
) -> ListePosee {
    let nom_m = Metrics::new(crate::CORPS * e, crate::INTERLIGNE * e);
    let petit_m = Metrics::new(DESC_CORPS * e, crate::INTERLIGNE * e);
    let desc_m = Metrics::new(DESC_CORPS * e, crate::INTERLIGNE * e);
    let section_m = Metrics::new(SECTION_CORPS * e, SECTION_H * e);
    let normal = barlow();
    let demi_gras = barlow().weight(Weight::MEDIUM);
    let large = colonne >= LARGE * e;

    let mut lignes = Vec::new();
    let mut sections = Vec::new();
    let mut y = LISTE_HAUT * e;
    let mut jour_courant: Option<i64> = None;
    // La colonne des descriptions est alignee, comme dans une table : elle
    // commence apres le plus long nom (et son point), sans passer 45 % de la
    // colonne ; un nom plus long s'y coupe.
    let point_l = (AVANT_POINT_LIGNE + POINT_LIGNE) * e;
    let maintenant = crate::app::maintenant();
    let col_desc = catalogue
        .de(projet)
        .map(|c| largeur_libre(systeme, nom_m, &demi_gras, &c.nom).1 + if c.vivant(maintenant) { point_l } else { 0.0 })
        .fold(0.0f32, f32::max)
        .min(colonne * 0.45)
        + 20.0 * e;
    for c in catalogue.de(projet) {
        let (jour, minute) = dates::jour_et_minute(c.debut);
        if jour_courant != Some(jour) {
            if jour_courant.is_some() {
                y += SECTION_AVANT * e;
            }
            let (titre, _) = largeur_libre(systeme, section_m, &demi_gras, &dates::libelle_jour(jour, aujourdhui));
            sections.push(SectionPosee { titre, y });
            y += (SECTION_H + SECTION_APRES) * e;
            jour_courant = Some(jour);
        }
        y += LIGNE_AIR * 0.5 * e;
        // Muet, un chantier en cours perd sa pastille et dit depuis quand
        // (`Chantier::muet`).
        let en_cours = c.vivant(maintenant);
        let agents = if c.agents == 1 { "1 agent".to_string() } else { format!("{} agents", c.agents) };
        let (meta, meta_l) = largeur_libre(systeme, petit_m, &normal, &format!("{}  ·  {}", c.mot(maintenant), agents));
        let reserve = meta_l + 24.0 * e;
        let point = if en_cours { point_l } else { 0.0 };
        // Une ligne par chantier : la description dans sa colonne, coupee a
        // ce qui reste avant l'etat. En fenetre etroite, elle passe dessous
        // plutot que de couper le nom.
        let reste = colonne - col_desc - reserve;
        let en_ligne = large && reste > 120.0 * e;
        let nom_max = if en_ligne { col_desc - 20.0 * e - point } else { colonne - reserve - point };
        let (nom, nom_l) = une_ligne(systeme, nom_m, &demi_gras, &c.nom, nom_max);
        // Sans description, l'identifiant, s'il n'est pas deja le nom.
        let texte_desc = match (c.description.is_empty(), c.nom == c.id) {
            (false, _) => c.description.as_str(),
            (true, false) => c.id.as_str(),
            (true, true) => "",
        };
        let desc_x = col_desc;
        let (desc_x, desc_dy, h, desc) = if en_ligne {
            let (d, _) = une_ligne(systeme, desc_m, &normal, texte_desc, reste);
            (desc_x, 0.0, LIGNE_H * e, d)
        } else {
            let (d, _) = une_ligne(systeme, desc_m, &normal, texte_desc, colonne);
            let h = if texte_desc.is_empty() { LIGNE_H } else { LIGNE_H + DESC_H };
            (0.0, crate::INTERLIGNE * e, h * e, d)
        };
        let (heure, _) = largeur_libre(systeme, petit_m, &normal, &dates::heure(minute));
        lignes.push(LignePosee {
            id: c.id.clone(),
            heure,
            nom,
            nom_l,
            meta,
            meta_l,
            desc,
            desc_x,
            desc_dy,
            y,
            h,
            en_cours,
        });
        y += h + LIGNE_AIR * 0.5 * e;
    }
    ListePosee {
        projet: projet.to_string(),
        signature,
        lignes,
        sections,
        vignettes: Vec::new(),
        chrono: None,
        hauteur: y + LISTE_HAUT * e,
    }
}

/// Une colonne plus etroite que cela passe a la mise en page empilee : la
/// description sous le nom, la barre sous le nom. 520 points, c'est ce qu'il
/// faut pour qu'un nom d'agent de trente signes, sa barre et son etat tiennent
/// sur une ligne.
const LARGE: f32 = 520.0;
/// Une ligne de liste : l'interligne du fil, et l'air entre deux lignes.
/// 23 + 7 = 30 points de pas, au-dessus des 28 d'une cible chez Apple.
const LIGNE_H: f32 = 23.0;
const LIGNE_AIR: f32 = 7.0;
/// La chronologie : la place du nom, et celle de l'etat a droite.
const NOM_CHRONO: f32 = 0.40;
const BARRE_H: f32 = 3.0;
const ACTIVITE_H: f32 = 9.0;
const IMAGE_MARQUE_H: f32 = 15.0;
const AXE_H: f32 = 20.0;

/// L'air entre deux vignettes, et sous la legende d'une rangee.
const VIGNETTE_AIR: f32 = 16.0;
const VIGNETTE_APRES: f32 = 22.0;
/// Le cadre d'une vignette, en 16:10 : celui de la plupart des captures
/// d'ecran, qui y tiennent donc presque sans bandes.
const VIGNETTE_RAPPORT: f32 = 0.625;
pub const VIGNETTE_RAYON: f32 = 8.0;
const LEGENDE_CORPS: f32 = 12.0;
const LEGENDE_H: f32 = 17.0;

/// La ligne d'un agent : « phase · etat · 3 images ».
fn meta_agent(a: &FicheAgent) -> String {
    let etat = if a.vivant {
        crate::langue::tr("en cours", "running")
    } else if a.fini {
        crate::langue::tr("fini", "finished")
    } else {
        crate::langue::tr("interrompu", "interrupted")
    };
    let mut parties: Vec<String> = Vec::new();
    parties.push(etat.to_string());
    match a.images {
        0 => {}
        1 => parties.push("1 image".to_string()),
        n => parties.push(format!("{n} images")),
    }
    parties.join("  ·  ")
}

/// La signature d'une page : ce qui, en changeant, change ce qu'on lit.
pub fn signature_page(fiche: Option<&Fiche>, en_cours: bool) -> Vec<String> {
    let Some(f) = fiche else { return vec!["vide".to_string()] };
    let mut s: Vec<String> = f
        .agents
        .iter()
        .map(|a| format!("{}|{}|{}|{}|{}", a.id, a.role, meta_agent(a), a.resume, a.activite.len()))
        .collect();
    s.extend(f.vignettes.iter().map(|v| format!("{:p}", Arc::as_ptr(&v.image))));
    s.push(format!("{en_cours} {}", f.complete));
    s
}

fn secondes(t: std::time::SystemTime) -> f64 {
    t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

/// Le pas des graduations, en minutes : le plus petit qui en laisse au plus
/// `n` sur la duree.
pub fn pas_axe(duree_s: f64, n: usize) -> u32 {
    const PAS: [u32; 10] = [1, 2, 5, 10, 15, 30, 60, 120, 240, 720];
    *PAS.iter().find(|p| duree_s / (**p as f64 * 60.0) <= n as f64).unwrap_or(&1440)
}

/// Met en page la page d'un chantier, en console : ses agents rangés par
/// phase, chacun sur sa ligne de la chronologie, puis les pieces jointes.
/// `en_cours` ajoute la ligne qui suit l'agent qui ecrit.
pub fn poser_page(
    systeme: &mut FontSystem,
    fiche: Option<&Fiche>,
    id: &str,
    en_cours: bool,
    colonne: f32,
    e: f32,
    signature: Vec<String>,
) -> ListePosee {
    let nom_m = Metrics::new(crate::CORPS * e, crate::INTERLIGNE * e);
    let petit_m = Metrics::new(DESC_CORPS * e, crate::INTERLIGNE * e);
    let desc_m = Metrics::new(DESC_CORPS * e, DESC_H * e);
    let section_m = Metrics::new(SECTION_CORPS * e, SECTION_H * e);
    let legende_m = Metrics::new(LEGENDE_CORPS * e, LEGENDE_H * e);
    let normal = barlow();
    let demi_gras = barlow().weight(Weight::MEDIUM);
    let large = colonne >= LARGE * e;

    let mut lignes = Vec::new();
    let mut sections = Vec::new();
    let mut vignettes = Vec::new();
    let mut y = LISTE_HAUT * e;
    let Some(fiche) = fiche else {
        return ListePosee { projet: id.to_string(), signature, lignes, sections, vignettes, chrono: None, hauteur: 0.0 };
    };

    // L'etendue du chantier, et la place de la chronologie dans la colonne.
    let t0 = fiche.agents.iter().map(|a| secondes(a.debut)).fold(f64::MAX, f64::min);
    let t1 = fiche.agents.iter().map(|a| secondes(a.fin).max(secondes(a.debut))).fold(f64::MIN, f64::max);
    let duree = (t1 - t0).max(60.0);
    let metas: Vec<String> = fiche.agents.iter().map(meta_agent).collect();
    let meta_max = metas
        .iter()
        .map(|m| largeur_libre(systeme, petit_m, &normal, m).1)
        .fold(0.0f32, f32::max);
    let (cx0, cx1) = if large {
        let x0 = (colonne * NOM_CHRONO).round();
        (x0, colonne - meta_max - 20.0 * e)
    } else {
        (0.0, colonne)
    };
    let cl = (cx1 - cx0).max(40.0 * e);
    let x_de = |t: f64| cx0 + (((t - t0) / duree) as f32).clamp(0.0, 1.0) * cl;

    // Les graduations, en heure locale, sur des minutes rondes.
    let mut axe = Vec::new();
    let pas = pas_axe(duree, if large { 4 } else { 3 }) as i64 * 60;
    let decalage = i64::from(crate::suivi::decalage_horaire_heures()) * 3600;
    let mut g = ((t0 as i64 + decalage + pas - 1) / pas) * pas - decalage;
    while (g as f64) <= t1 {
        let minute = ((g + decalage).rem_euclid(86_400) / 60) as u32;
        let (b, l) = largeur_libre(systeme, petit_m, &normal, &dates::heure(minute));
        axe.push((b, x_de(g as f64), l));
        g += pas;
    }
    let y_axe = y;
    y += AXE_H * e;
    let mut barres = Vec::new();

    let mut ligne = |systeme: &mut FontSystem, id: &str, heure: &str, nom: &str, meta: &str, desc: &str, vivant: bool, y: &mut f32, barre: Option<(f64, f64, Vec<f64>, Vec<f64>)>| {
        *y += LIGNE_AIR * 0.5 * e;
        let (meta_b, meta_l) = largeur_libre(systeme, petit_m, &normal, meta);
        // Le nom tient dans sa colonne quand la chronologie est a cote ; en
        // mise en page empilee, il a toute la ligne moins l'etat.
        let nom_max = if large && barre.is_some() { cx0 - 12.0 * e } else { colonne - meta_l - 24.0 * e };
        let (nom_b, nom_l) = une_ligne(systeme, nom_m, &demi_gras, nom, nom_max);
        let (desc_b, _) = une_ligne(systeme, desc_m, &normal, desc, colonne);
        let (heure_b, _) = largeur_libre(systeme, petit_m, &normal, heure);
        let empilee = !large && barre.is_some();
        let desc_dy = if empilee { (crate::INTERLIGNE + 12.0) * e } else { crate::INTERLIGNE * e };
        let h = desc_dy + DESC_H * e;
        if let Some((d, f, act, img)) = barre {
            let milieu = if empilee { *y + (crate::INTERLIGNE + 6.0) * e } else { *y + crate::INTERLIGNE * 0.5 * e };
            barres.push(Barre {
                y: milieu,
                x0: x_de(d),
                x1: x_de(f),
                vivant,
                activite: act.into_iter().map(x_de).collect(),
                images: img.into_iter().map(x_de).collect(),
            });
        }
        lignes.push(LignePosee {
            id: id.to_string(),
            heure: heure_b,
            nom: nom_b,
            nom_l,
            meta: meta_b,
            meta_l,
            desc: desc_b,
            desc_x: 0.0,
            desc_dy,
            y: *y,
            h,
            en_cours: vivant,
        });
        *y += h + LIGNE_AIR * 0.5 * e;
    };
    // Suivre celui qui ecrit, comme le direct, mais dans CE chantier.
    if en_cours {
        ligne(
            systeme,
            "",
            "",
            crate::langue::tr("Celui qui écrit", "The one writing"),
            crate::langue::tr("direct", "live"),
            crate::langue::tr("Suit l’agent qui écrit, et passe à celui qui prend la main.", "Follows the agent that is writing, and switches to the one that takes over."),
            false,
            &mut y,
            None,
        );
    }
    let mut phase: Option<&str> = None;
    for (a, meta) in fiche.agents.iter().zip(&metas) {
        // Les agents se rangent par phase, dans l'ordre ou ils ont commence :
        // le titre de phase remplace la phase repetee sur chaque ligne.
        if phase != Some(a.phase.as_str()) && !a.phase.is_empty() {
            y += 6.0 * e;
            let (titre, _) = largeur_libre(systeme, section_m, &demi_gras, &a.phase);
            sections.push(SectionPosee { titre, y });
            y += (SECTION_H + 2.0) * e;
            phase = Some(a.phase.as_str());
        }
        let heure = dates::heure(dates::jour_et_minute(a.debut).1);
        let desc = if a.resume.is_empty() { a.id.as_str() } else { a.resume.as_str() };
        let images: Vec<f64> = fiche
            .vignettes
            .iter()
            .filter(|v| v.agent == a.id)
            .filter_map(|v| v.quand.map(secondes))
            .collect();
        let activite = a.activite.iter().map(|t| secondes(*t)).collect();
        ligne(
            systeme,
            &a.id,
            &heure,
            &a.role,
            meta,
            desc,
            a.vivant,
            &mut y,
            Some((secondes(a.debut), secondes(a.fin).max(secondes(a.debut)), activite, images)),
        );
    }

    if !fiche.vignettes.is_empty() {
        let titre = if fiche.complete && fiche.images > fiche.vignettes.len() {
            crate::tf!("Pièces jointes, les {} premières sur {}", "Attachments, the first {} of {}", fiche.vignettes.len(), fiche.images)
        } else {
            if fiche.complete { crate::tf!("Pièces jointes  ·  {}", "Attachments  ·  {}", fiche.images) } else { crate::langue::tr("Pièces jointes  ·  lecture en cours", "Attachments  ·  reading").to_string() }
        };
        y += SECTION_AVANT * e;
        let (t, _) = largeur_libre(systeme, section_m, &demi_gras, &titre);
        sections.push(SectionPosee { titre: t, y });
        y += (SECTION_H + SECTION_APRES + 4.0) * e;
        // Denses : autant de colonnes qu'il en tient a 130 points au moins, et
        // six au plus : au-dela, une capture ne se reconnait plus.
        let air = VIGNETTE_AIR * e;
        let n = (((colonne + air) / (130.0 * e + air)).floor() as usize).clamp(2, 6);
        let l = ((colonne - air * (n - 1) as f32) / n as f32).floor();
        let h = (l * VIGNETTE_RAPPORT).round();
        for (rang, v) in fiche.vignettes.iter().enumerate() {
            let (col, rangee) = (rang % n, rang / n);
            let rangee_h = h + 6.0 * e + 2.0 * LEGENDE_H * e + VIGNETTE_APRES * e;
            let cadre = Boite { x: col as f32 * (l + air), y: y + rangee as f32 * rangee_h, l, h };
            let (il, ih) = (v.image.largeur.max(1) as f32, v.image.hauteur.max(1) as f32);
            let r = (l / il).min(h / ih).min(e * v.image.source.0.max(1) as f32 / il);
            let (dl, dh) = ((il * r).round().max(1.0), (ih * r).round().max(1.0));
            let image = Boite {
                x: cadre.x + ((l - dl) * 0.5).round(),
                y: cadre.y + ((h - dh) * 0.5).round(),
                l: dl,
                h: dh,
            };
            let nom = v.image.nom().unwrap_or_else(|| crate::langue::tr("image reçue", "image received").to_string());
            let (nom_b, _) = une_ligne(systeme, legende_m, &demi_gras, &nom, l);
            let mut details = Vec::new();
            if let Some(d) = v.image.video {
                details.push(crate::suivi::duree(d));
            }
            if !v.heure.is_empty() {
                details.push(v.heure.clone());
            }
            details.push(v.role.clone());
            let (legende_b, _) = une_ligne(systeme, legende_m, &normal, &details.join("  ·  "), l);
            vignettes.push(VignettePosee { rang, cadre, image, nom: nom_b, legende: legende_b, texture: None });
        }
        let rangees = fiche.vignettes.len().div_ceil(n);
        y += rangees as f32 * (h + 6.0 * e + 2.0 * LEGENDE_H * e + VIGNETTE_APRES * e);
    }
    ListePosee {
        projet: id.to_string(),
        signature,
        lignes,
        sections,
        vignettes,
        chrono: Some(Chrono {
            axe,
            y_axe,
            barres,
            demi_trame: if large { crate::INTERLIGNE * 0.5 * e } else { 6.0 * e },
        }),
        hauteur: y + LISTE_HAUT * e,
    }
}

/// Les cadres, les noms et les legendes de la planche. Rend les images a
/// dessiner, en pixels d'ecran, avec leur rang dans la planche : leurs
/// textures se font dans main.rs, qui tient la carte.
pub fn dessiner_vignettes<'a>(
    liste: &'a ListePosee,
    g: GeoListe,
    survol: Option<usize>,
    aplats: &mut Vec<Forme>,
    zones: &mut Vec<TextArea<'a>>,
) -> Vec<(usize, Boite)> {
    let e = g.e;
    let bornes = TextBounds {
        left: 0,
        top: g.haut as i32,
        right: g.droite.ceil() as i32,
        bottom: g.bas as i32,
    };
    let mut images = Vec::new();
    for (i, v) in liste.vignettes.iter().enumerate() {
        let cadre = g.vignette(v);
        if cadre.y + cadre.h + 2.0 * LEGENDE_H * e + 8.0 * e < g.haut || cadre.y > g.bas {
            continue;
        }
        // Le fond du cadre, du gris des filets : une capture claire ne se
        // fond pas dans la page, et une image etroite garde sa place. Il
        // s'eclaire sous la souris, comme la plaque d'une ligne.
        let alpha = if survol == Some(i) { 1.0 } else { 0.45 };
        if let Some(b) = rogner(cadre, g.haut, g.bas) {
            aplats.push(forme(b, palette::lineaire(palette::filet(), alpha), VIGNETTE_RAYON * e));
        }
        let im = Boite { x: g.x_corps + v.image.x, y: cadre.y + (v.image.y - v.cadre.y), ..v.image };
        if im.y + im.h > g.haut && im.y < g.bas {
            images.push((i, im));
        }
        zones.push(TextArea {
            buffer: &v.nom,
            left: cadre.x,
            top: cadre.y + cadre.h + 6.0 * e,
            scale: 1.0,
            bounds: bornes,
            default_color: if survol == Some(i) { palette::encre1() } else { palette::encre2() },
            custom_glyphs: &[],
        });
        zones.push(TextArea {
            buffer: &v.legende,
            left: cadre.x,
            top: cadre.y + cadre.h + 6.0 * e + LEGENDE_H * e,
            scale: 1.0,
            bounds: bornes,
            default_color: palette::encre3(),
            custom_glyphs: &[],
        });
    }
    images
}

/// La plaque, intersectee avec la zone visible : les aplats ne savent pas se
/// rogner, et une plaque qui glisse sous l'en-tete doit y disparaitre.
fn rogner(b: Boite, haut: f32, bas: f32) -> Option<Boite> {
    let y0 = b.y.max(haut);
    let y1 = (b.y + b.h).min(bas);
    (y1 > y0).then_some(Boite { y: y0, h: y1 - y0, ..b })
}

fn forme(b: Boite, couleur: [f32; 4], rayon: f32) -> Forme {
    Forme {
        boite: [b.x, b.y, b.l, b.h],
        couleur,
        rayon: [rayon, 0.0, 0.0, 0.0],
    }
}

/// La barre : son filet, ses rangees, ses points. `rail` met un rail a la
/// rangee active : sous le contraste eleve, ou toutes les encres se valent,
/// c'est lui qui dit le projet ouvert.
#[allow(clippy::too_many_arguments)]
pub fn dessiner_barre<'a>(
    onglets: &'a [OngletPose],
    g: &GeoBarre,
    actif: Option<usize>,
    survol: Option<usize>,
    groupe: &'a Buffer,
    rail: bool,
    par_dessus: bool,
    aplats: &mut Vec<Forme>,
    zones: &mut Vec<TextArea<'a>>,
) {
    let e = g.e;
    let p = g.panneau;
    if p.x + p.l <= 0.0 {
        return;
    }
    // Le panneau couvre ce qui passerait dessous pendant son mouvement, et un
    // filet d'un pixel le separe du contenu : pas de surface de plus. Rappelee
    // par-dessus le contenu voile d'une fenetre etroite, son bord prend
    // l'encre des reperes : au sombre, le filet seul (1,52:1) etait tout ce qui
    // separait les deux plans (juge-design.md, m9).
    let trait_ = (1.0 * e).max(1.0).round();
    let bord = if par_dessus { palette::encre3() } else { palette::filet() };
    aplats.push(forme(p, palette::lineaire(palette::fond_octets(), 1.0), 0.0));
    aplats.push(forme(
        Boite { x: (p.x + p.l).round() - trait_, y: 0.0, l: trait_, h: p.h },
        palette::lineaire(bord, 1.0),
        0.0,
    ));

    let bornes = |haut: f32, bas: f32| TextBounds {
        left: p.x.max(0.0) as i32,
        top: haut as i32,
        right: (p.x + p.l - trait_).max(0.0) as i32,
        bottom: bas as i32,
    };
    if onglets.len() > 1 {
        zones.push(TextArea {
            buffer: groupe,
            left: g.direct.x + RANG_TEXTE * e,
            top: g.groupe_y,
            scale: 1.0,
            bounds: bornes(g.groupe_y, g.fenetre.y),
            default_color: palette::encre3(),
            custom_glyphs: &[],
        });
    }
    for (i, o) in onglets.iter().enumerate() {
        let r = g.rangee(i, o);
        let (haut, bas) = if i == 0 { (r.y, r.y + r.h) } else { (g.fenetre.y, g.fenetre.y + g.fenetre.h) };
        if r.y + r.h <= haut || r.y >= bas {
            continue;
        }
        let est_actif = actif == Some(i);
        let est_survole = survol == Some(i) && !est_actif;
        if est_actif || est_survole {
            // Rognee a la zone des projets, a la main : les aplats ne savent
            // pas rogner.
            let y0 = r.y.max(haut);
            let y1 = (r.y + r.h).min(bas);
            aplats.push(forme(
                Boite { y: y0, h: y1 - y0, ..r },
                palette::lineaire(palette::plaque(), if est_actif { 1.0 } else { 0.5 }),
                RANG_RAYON * e,
            ));
            if est_actif && rail {
                aplats.push(forme(
                    Boite { x: r.x, y: y0, l: (3.0 * e).round(), h: y1 - y0 },
                    palette::lineaire(palette::encre1(), 1.0),
                    0.0,
                ));
            }
        }
        if o.vivant {
            let d = POINT * e;
            let y = r.y + (r.h - d) * 0.5;
            if y >= haut && y + d <= bas {
                aplats.push(forme(
                    Boite { x: r.x + r.l - RANG_TEXTE * e - d, y, l: d, h: d },
                    palette::lineaire(palette::encre1(), POINT_ALPHA),
                    d * 0.5,
                ));
            }
        }
        let couleur = if est_actif {
            palette::encre1()
        } else if est_survole {
            palette::encre2()
        } else {
            palette::encre3()
        };
        zones.push(TextArea {
            buffer: &o.tampon,
            left: r.x + RANG_TEXTE * e,
            top: r.y + (RANG_H - PILULE_H) * 0.5 * e,
            scale: 1.0,
            bounds: bornes(haut, bas),
            default_color: couleur,
            custom_glyphs: &[],
        });
    }
}

/// Le signe du bouton de la barre : un cadre et, a sa gauche, le panneau, le
/// dessin que Claude, le Finder et Mail donnent a ce bouton. Trace en formes,
/// sans police de symboles : rien d'autre de la direction materiau n'est pris.
/// Le panneau est plein quand la barre se voit, un simple trait sinon.
pub fn dessiner_bouton_barre(b: Boite, ouverte: bool, survole: bool, aplats: &mut Vec<Forme>, e: f32) {
    if survole {
        aplats.push(forme(b, palette::lineaire(palette::plaque(), 1.0), RANG_RAYON * e));
    }
    let encre = palette::lineaire(if survole { palette::encre1() } else { palette::encre2() }, 1.0);
    let fond = palette::lineaire(if survole { palette::plaque() } else { palette::fond_octets() }, 1.0);
    let (l, h) = ((16.0 * e).round(), (13.0 * e).round());
    let x = (b.x + (b.l - l) * 0.5).round();
    let y = (b.y + (b.h - h) * 0.5).round();
    let trait_ = (1.5 * e).max(1.0).round();
    aplats.push(forme(Boite { x, y, l, h }, encre, 3.0 * e));
    aplats.push(forme(
        Boite { x: x + trait_, y: y + trait_, l: l - 2.0 * trait_, h: h - 2.0 * trait_ },
        fond,
        (3.0 * e - trait_).max(0.0),
    ));
    let panneau_l = (5.0 * e).round();
    if ouverte {
        aplats.push(forme(Boite { x, y, l: panneau_l + trait_, h }, encre, 0.0));
    } else {
        aplats.push(forme(Boite { x: x + panneau_l, y, l: trait_, h }, encre, 0.0));
    }
}

/// Les plaques, rails, points et textes de la liste.
#[allow(clippy::too_many_arguments)]
pub fn dessiner_liste<'a>(
    liste: &'a ListePosee,
    g: GeoListe,
    focus: Option<usize>,
    clavier: bool,
    suivi: Option<&str>,
    aplats: &mut Vec<Forme>,
    zones: &mut Vec<TextArea<'a>>,
) {
    let e = g.e;
    let bornes = TextBounds {
        left: 0,
        top: g.haut as i32,
        right: g.droite.ceil() as i32,
        bottom: g.bas as i32,
    };
    let ecran = |y: f32| g.haut + y - g.defil;
    for s in &liste.sections {
        let y = ecran(s.y);
        if y + SECTION_H * e < g.haut || y > g.bas {
            continue;
        }
        zones.push(TextArea {
            buffer: &s.titre,
            left: g.x_heure,
            top: y,
            scale: 1.0,
            bounds: bornes,
            default_color: palette::encre3(),
            custom_glyphs: &[],
        });
    }
    if let Some(c) = &liste.chrono {
        dessiner_chrono(c, g, aplats, zones, bornes);
    }
    for (i, l) in liste.lignes.iter().enumerate() {
        let plaque = g.plaque(l);
        if plaque.y + plaque.h < g.haut || plaque.y > g.bas {
            continue;
        }
        let y = ecran(l.y);
        if focus == Some(i) {
            if let Some(b) = rogner(plaque, g.haut, g.bas) {
                // Venue du clavier, la ligne porte aussi un anneau d'encre-2 :
                // la plaque seule fait 1,52:1 sur le fond, sous les 3:1 qu'un
                // etat demande (systeme.md §4, regle 4, et §7). Sous la souris,
                // la plaque suffit : la main est deja dessus.
                if clavier {
                    let t = (1.5 * e).max(1.0).round();
                    aplats.push(forme(b, palette::lineaire(palette::encre2(), 1.0), PLAQUE_RAYON * e));
                    aplats.push(forme(
                        Boite { x: b.x + t, y: b.y + t, l: b.l - 2.0 * t, h: b.h - 2.0 * t },
                        palette::lineaire(palette::plaque(), 1.0),
                        (PLAQUE_RAYON * e - t).max(0.0),
                    ));
                } else {
                    aplats.push(forme(b, palette::lineaire(palette::plaque(), 0.75), PLAQUE_RAYON * e));
                }
            }
        }
        // Le rail, comme dans le fil : il marque ce qu'on suit, et lui seul.
        if suivi == Some(l.id.as_str()) {
            let rail = Boite {
                x: g.x_corps - 16.0 * e,
                y: y + 4.0 * e,
                l: (2.0 * e).max(1.5),
                h: l.h - 8.0 * e,
            };
            if let Some(b) = rogner(rail, g.haut, g.bas) {
                aplats.push(forme(b, palette::lineaire(palette::encre1(), 0.55), 1.0 * e));
            }
        }
        if l.en_cours {
            let p = Boite {
                x: g.x_corps + l.nom_l + AVANT_POINT_LIGNE * e,
                y: y + (crate::INTERLIGNE - POINT_LIGNE) * 0.5 * e,
                l: POINT_LIGNE * e,
                h: POINT_LIGNE * e,
            };
            if p.y >= g.haut {
                aplats.push(forme(p, palette::lineaire(palette::encre1(), POINT_ALPHA), POINT_LIGNE * 0.5 * e));
            }
        }
        zones.push(TextArea {
            buffer: &l.heure,
            left: g.x_heure,
            top: y + 1.0 * e,
            scale: 1.0,
            bounds: bornes,
            default_color: palette::encre3(),
            custom_glyphs: &[],
        });
        zones.push(TextArea {
            buffer: &l.nom,
            left: g.x_corps,
            top: y,
            scale: 1.0,
            bounds: bornes,
            default_color: palette::encre1(),
            custom_glyphs: &[],
        });
        zones.push(TextArea {
            buffer: &l.meta,
            left: g.x_corps + g.colonne - l.meta_l,
            top: y + 1.0 * e,
            scale: 1.0,
            bounds: bornes,
            default_color: if l.en_cours { palette::encre2() } else { palette::encre3() },
            custom_glyphs: &[],
        });
        zones.push(TextArea {
            buffer: &l.desc,
            left: g.x_corps + l.desc_x,
            top: y + l.desc_dy + if l.desc_dy == 0.0 { 1.0 * e } else { 0.0 },
            scale: 1.0,
            bounds: bornes,
            default_color: palette::encre2(),
            custom_glyphs: &[],
        });
    }
}

/// L'axe, ses graduations, et les barres des agents.
fn dessiner_chrono<'a>(c: &'a Chrono, g: GeoListe, aplats: &mut Vec<Forme>, zones: &mut Vec<TextArea<'a>>, bornes: TextBounds) {
    let e = g.e;
    let ecran = |y: f32| g.haut + y - g.defil;
    let fin = |b: Boite| rogner(b, g.haut, g.bas);
    let un = (1.0 * e).max(1.0).round();
    for (b, x, l) in &c.axe {
        let x = g.x_corps + x;
        let y = ecran(c.y_axe);
        let trait_ = |haut: f32, h: f32| Boite { x: x.round(), y: haut, l: un, h };
        // Sous l'heure, une coche ; puis un trait dans la bande de chaque barre.
        if let Some(r) = fin(trait_(y + AXE_H * e - 2.0 * e, 6.0 * e)) {
            aplats.push(forme(r, palette::lineaire(palette::filet(), 1.0), 0.0));
        }
        for barre in &c.barres {
            let yb = ecran(barre.y);
            if let Some(r) = fin(trait_((yb - c.demi_trame).round(), (2.0 * c.demi_trame).round())) {
                aplats.push(forme(r, palette::lineaire(palette::filet(), 1.0), 0.0));
            }
        }

        if y + AXE_H * e > g.haut && y < g.bas {
            zones.push(TextArea {
                buffer: b,
                left: (x - l * 0.5).round(),
                top: y,
                scale: 1.0,
                bounds: bornes,
                default_color: palette::encre3(),
                custom_glyphs: &[],
            });
        }
    }
    for b in &c.barres {
        let y = ecran(b.y);
        if y + IMAGE_MARQUE_H * e < g.haut || y - IMAGE_MARQUE_H * e > g.bas {
            continue;
        }
        let encre = if b.vivant { palette::encre1() } else { palette::encre3() };
        let barre = Boite { x: g.x_corps + b.x0, y: y - BARRE_H * 0.5 * e, l: (b.x1 - b.x0).max(2.0 * e), h: BARRE_H * e };
        if let Some(r) = fin(barre) {
            aplats.push(forme(r, palette::lineaire(encre, 1.0), BARRE_H * 0.5 * e));
        }
        // Une marque par entree : la densite du travail. Deux entrees qui
        // tombent sur le meme pixel n'en font qu'une.
        let mut dernier = f32::MIN;
        for x in &b.activite {
            let x = (g.x_corps + x).round();
            if x - dernier < un {
                continue;
            }
            dernier = x;
            if let Some(r) = fin(Boite { x, y: y - ACTIVITE_H * 0.5 * e, l: un, h: ACTIVITE_H * e }) {
                aplats.push(forme(r, palette::lineaire(encre, 1.0), 0.0));
            }
        }
        for x in &b.images {
            let x = (g.x_corps + x - e).round();
            if let Some(r) = fin(Boite { x, y: y - IMAGE_MARQUE_H * 0.5 * e, l: 2.0 * un, h: IMAGE_MARQUE_H * e }) {
                aplats.push(forme(r, palette::lineaire(palette::encre1(), 1.0), 0.0));
            }
        }
    }
}

/// Un etat vide : un titre et une phrase, centres dans la colonne.
pub fn poser_vide(systeme: &mut FontSystem, titre: &str, phrase: &str, largeur: f32, e: f32) -> (Buffer, Buffer) {
    let centre = Some(glyphon::cosmic_text::Align::Center);
    let mut a = Buffer::new(systeme, Metrics::new(crate::CORPS * e, crate::INTERLIGNE * e));
    a.set_size(Some(largeur), None);
    a.set_text(titre, &barlow().weight(Weight::MEDIUM), Shaping::Advanced, centre);
    a.shape_until_scroll(systeme, false);
    let mut b = Buffer::new(systeme, Metrics::new(DESC_CORPS * e, DESC_H * e));
    b.set_size(Some(largeur), None);
    b.set_text(phrase, &barlow(), Shaping::Advanced, centre);
    b.shape_until_scroll(systeme, false);
    (a, b)
}

/// La largeur de barre a atteindre, en points. Dans une fenetre etroite, seul
/// le rappel compte : le choix fait a la main pour une fenetre large n'y
/// change rien, et ne s'y perd pas non plus.
pub fn cible_barre(etroite: bool, masquee: bool, rappelee: bool) -> f32 {
    let ouverte = if etroite { rappelee } else { !masquee };
    if ouverte {
        BARRE_L
    } else {
        0.0
    }
}

/// Le pas de temps donne aux ressorts. Sous le mouvement reduit, un pas si
/// long que la solution exacte du ressort est deja sur sa cible : tout se
/// pose d'un coup, par le meme chemin que le mouvement.
pub fn pas_des_ressorts(dt: f32, reduit: bool) -> f32 {
    if reduit {
        1.0e3
    } else {
        dt
    }
}

#[cfg(test)]
mod essais_coupe {
    use super::abreger_chemins;

    /// Ce qui distingue reste : le nom du fichier et son dossier. Un mot sans
    /// chemin, une adresse web et un chemin court ne bougent pas.
    #[test]
    fn un_chemin_garde_sa_fin() {
        assert_eq!(
            abreger_chemins("Fichier produit : C:/Users/alex/dev/active/atelier-d/recherche/design/forme.md, commité"),
            "Fichier produit : …/design/forme.md, commité"
        );
        assert_eq!(
            abreger_chemins("cd /c/Users/alex/dev/active/atelier-d/recherche/design/essais && ffmpeg"),
            "cd …/design/essais && ffmpeg"
        );
        assert_eq!(abreger_chemins("« recherche/forme.md » https://a.b/c/d/e/f"), "« recherche/forme.md » https://a.b/c/d/e/f");
        assert_eq!(abreger_chemins("Waiting for the monitor event."), "Waiting for the monitor event.");
    }
}

#[cfg(test)]
mod essais_barre {
    use super::*;
    use crate::ressort::Ressort;

    /// Les quatre cas : large et visible, large et masquee, etroite (repliee
    /// d'elle-meme, meme si on ne l'a pas masquee), etroite et rappelee (meme
    /// si on l'a masquee pour une fenetre large).
    #[test]
    fn la_barre_vise_selon_la_fenetre_et_le_choix() {
        assert_eq!(cible_barre(false, false, false), BARRE_L);
        assert_eq!(cible_barre(false, true, false), 0.0);
        assert_eq!(cible_barre(true, false, false), 0.0);
        assert_eq!(cible_barre(true, true, true), BARRE_L);
        // Une barre rappelee ne compte pas dans une fenetre large.
        assert_eq!(cible_barre(false, true, true), 0.0);
    }

    /// Une trame a 60 Hz : sans mouvement reduit, la barre n'a fait qu'une
    /// partie du chemin ; avec, elle est arrivee, et au repos.
    #[test]
    fn le_mouvement_reduit_pose_la_barre_d_un_coup() {
        let mut libre = Ressort::pose(0.0);
        libre.viser(235.0);
        libre.avancer(pas_des_ressorts(1.0 / 60.0, false));
        assert!(libre.position() > 0.0 && libre.position() < 235.0 * 0.5);
        let mut reduit = Ressort::pose(0.0);
        reduit.viser(235.0);
        reduit.avancer(pas_des_ressorts(1.0 / 60.0, true));
        assert_eq!(reduit.position(), 235.0);
        assert!(reduit.au_repos());
    }

    /// Les rangees : 28 points de haut, la cible du systeme ; « Direct » sous
    /// le bouton, les projets sous le titre du groupe ; et la barre rentree
    /// n'a plus rien a l'ecran.
    #[test]
    fn les_rangees_tiennent_leur_place() {
        let e = 1.25;
        let o = |y: f32| OngletPose { projet: None, tampon: dummy(), x: y, l: RANG_H * e, vivant: false };
        let onglets = vec![o(0.0), o(0.0), o((RANG_H + RANG_ENTRE) * e)];
        let g = geo_barre(&onglets, 0.0, 981.0, 0.0, e);
        let bouton = bouton_barre(e);
        assert!(g.direct.y >= bouton.y + bouton.h);
        assert!(g.direct.h >= 28.0 * e && bouton.l >= 28.0 * e && bouton.h >= 28.0 * e);
        let r1 = g.rangee(1, &onglets[1]);
        let r2 = g.rangee(2, &onglets[2]);
        assert!(r1.y >= g.groupe_y + GROUPE_H * e - 0.01);
        assert_eq!(r2.y - r1.y, (RANG_H + RANG_ENTRE) * e);
        assert!(r1.x + r1.l <= g.panneau.x + g.panneau.l);
        let rentree = geo_barre(&onglets, -BARRE_L * e, 981.0, 0.0, e);
        assert!(rentree.direct.x + rentree.direct.l <= 0.0);
    }

    fn dummy() -> Buffer {
        Buffer::new(&mut FontSystem::new(), Metrics::new(13.0, 20.0))
    }
}
