//! La visionneuse : une image ou une video ouverte en grand, dans la vigie,
//! par-dessus ce qu'on regardait.
//!
//! Elle fait ce que font Coup d'oeil et Photos sur un Mac. L'image grandit
//! depuis sa vignette jusqu'a sa place, sur un fond qui s'assombrit ; elle s'y
//! rend en se fermant ; dessous, rien ne bouge : ni le defilement, ni la page,
//! ni le fil, qui ne recolle pas au bas tant qu'elle est ouverte. On passe a
//! l'image voisine par les fleches, et une video se lit sur place, son
//! compris.
//!
//! Le mouvement est un ressort a amortissement critique (`ressort.rs`), le
//! meme que le defilement : un clic pendant l'ouverture change la cible et
//! rien d'autre, donc fermer une image qui s'ouvre la ramene sans saut. Quand
//! Windows demande de reduire les animations, elle s'ouvre et se ferme d'un
//! coup.
//!
//! Une image se zoome comme dans Aperçu : la molette, le pincement du pave
//! tactile ou un double-clic grossissent autour du point vise, qui reste sous
//! la souris ; on la deplace en la glissant. Le zoom va jusqu'a huit pixels
//! d'ecran par pixel de l'image : une capture ne devient pas vectorielle, mais
//! chacun de ses pixels se voit net, et l'image pleine est lue a sa vraie
//! taille, avec ses reductions, pour rester fine en petit aussi.

use std::sync::Arc;

use crate::plan::Boite;
use crate::ressort::Ressort;
use crate::suivi::Image;

/// L'ouverture va de 0 a ECHELLE : le ressort compte en pixels, et son seuil de
/// repos aussi ; sur un trajet de 0 a 1, il se croirait arrive des le depart.
pub const ECHELLE: f32 = 1000.0;
/// L'air autour de l'image ouverte, en points : de quoi poser le titre au-dessus
/// et, pour une video, sa barre en dessous.
pub const MARGE_COTES: f32 = 48.0;
pub const MARGE_HAUT: f32 = 84.0;
pub const MARGE_BAS: f32 = 64.0;
pub const MARGE_BAS_VIDEO: f32 = 112.0;

/// Un cran de molette grossit d'un quart, comme Aperçu.
pub const LOUPE_PAS: f32 = 1.25;

/// Ce qu'on peut grossir au plus : huit pixels d'ecran par pixel de l'image,
/// et au moins quatre fois l'image ajustee, pour une petite image deja a sa
/// taille.
pub fn loupe_max(ajustee: Boite, source: (u32, u32)) -> f32 {
    let r = ajustee.l / source.0.max(1) as f32;
    (8.0 / r.max(1e-6)).max(4.0)
}

/// Le grossissement qui montre l'image a sa vraie taille, un pixel d'ecran par
/// pixel d'image ; deux fois l'ajustee au moins, pour qu'un double-clic sur une
/// petite image fasse quelque chose.
pub fn loupe_reelle(ajustee: Boite, source: (u32, u32)) -> f32 {
    (source.0.max(1) as f32 / ajustee.l.max(1.0)).max(2.0)
}

/// L'image ajustee grossie de z autour de son centre, puis decalee de d.
pub fn zoomee(ajustee: Boite, z: f32, d: (f32, f32)) -> Boite {
    let (l, h) = (ajustee.l * z, ajustee.h * z);
    let (cx, cy) = (ajustee.x + ajustee.l * 0.5 + d.0, ajustee.y + ajustee.h * 0.5 + d.1);
    Boite { x: cx - l * 0.5, y: cy - h * 0.5, l, h }
}

/// Borne le decalage. Plus etroite que la fenetre, l'image reste centree sur sa
/// place ; plus large, ses bords ne se decollent pas de ceux de la fenetre,
/// comme dans Aperçu : on ne la pousse pas dans le vide.
pub fn borner(ajustee: Boite, z: f32, d: (f32, f32), fenetre: (f32, f32)) -> (f32, f32) {
    let axe = |debut: f32, taille: f32, d: f32, f: f32| {
        let s = taille * z;
        if s <= f {
            return 0.0;
        }
        let c0 = debut + taille * 0.5;
        (c0 + d).clamp(f - s * 0.5, s * 0.5) - c0
    };
    (axe(ajustee.x, ajustee.l, d.0, fenetre.0), axe(ajustee.y, ajustee.h, d.1, fenetre.1))
}

/// Grossit de f autour du point p : le point de l'image sous p y reste, sauf
/// ou la borne l'en empeche.
pub fn autour(ajustee: Boite, z: f32, d: (f32, f32), f: f32, p: (f32, f32), fenetre: (f32, f32), zmax: f32) -> (f32, (f32, f32)) {
    let z2 = (z * f).clamp(1.0, zmax);
    let c = (ajustee.x + ajustee.l * 0.5 + d.0, ajustee.y + ajustee.h * 0.5 + d.1);
    let u = ((p.0 - c.0) / z, (p.1 - c.1) / z);
    let c2 = (p.0 - u.0 * z2, p.1 - u.1 * z2);
    let d2 = (c2.0 - (ajustee.x + ajustee.l * 0.5), c2.1 - (ajustee.y + ajustee.h * 0.5));
    (z2, borner(ajustee, z2, d2, fenetre))
}

/// Une reduction de l'image pleine : largeur, hauteur, pixels. La carte y lit
/// l'image quand elle est montree plus petite que sa vraie taille, sans le
/// crenelage d'une seule grande texture reduite a la volee.
pub type Niveau = (u32, u32, Vec<u8>);

/// L'image pleine de l'element en cours et ses reductions, lues dans un fil a
/// part.
pub struct Pleine {
    pub rang: usize,
    pub image: Arc<Image>,
    pub niveaux: Vec<Niveau>,
}

/// D'ou vient une image : il faut le savoir pour la rendre a sa place en se
/// fermant, et pour « voir dans le fil ».
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provenance {
    /// Le rang de son bloc dans le fil.
    Fil(usize),
    /// Son rang dans la planche de la page d'un chantier.
    Planche(usize),
}

#[derive(Clone)]
pub struct Element {
    pub image: Arc<Image>,
    pub provenance: Provenance,
    /// L'agent qui l'a faite, pour « voir dans le fil » depuis la planche.
    pub agent: Option<String>,
    /// « chercheur-forme · 10:05 », sous le nom.
    pub details: String,
}

/// L'image entiere dans la zone, centree, jamais agrandie au-dela de sa taille
/// naturelle a l'echelle de l'ecran : une capture de 400 px etiree a 1 500
/// montrerait son flou, pas son contenu.
pub fn ajuster(source: (u32, u32), zone: Boite, echelle: f32) -> Boite {
    let (l, h) = (source.0.max(1) as f32, source.1.max(1) as f32);
    let r = (zone.l / l).min(zone.h / h).min(echelle).max(0.0);
    let (dl, dh) = ((l * r).round(), (h * r).round());
    Boite {
        x: (zone.x + (zone.l - dl) * 0.5).round(),
        y: (zone.y + (zone.h - dh) * 0.5).round(),
        l: dl,
        h: dh,
    }
}

/// La boite a mi-chemin : t = 0 la vignette, t = 1 l'image ouverte. Le ressort
/// ne depasse pas sa cible, mais t est borne quand meme.
pub fn interpoler(a: Boite, b: Boite, t: f32) -> Boite {
    let t = t.clamp(0.0, 1.0);
    let m = |x: f32, y: f32| x + (y - x) * t;
    Boite { x: m(a.x, b.x), y: m(a.y, b.y), l: m(a.l, b.l), h: m(a.h, b.h) }
}

/// La zone ou l'image ouverte peut tenir, dans une fenetre de l × h pixels.
pub fn zone(l: f32, h: f32, e: f32, video: bool) -> Boite {
    let bas = if video { MARGE_BAS_VIDEO } else { MARGE_BAS };
    Boite {
        x: MARGE_COTES * e,
        y: MARGE_HAUT * e,
        l: (l - 2.0 * MARGE_COTES * e).max(1.0),
        h: (h - (MARGE_HAUT + bas) * e).max(1.0),
    }
}

/// La capsule des commandes d'une video, sous l'image et au-dessus de l'aide,
/// comme celle de QuickTime : 720 points de large au plus.
pub fn commandes(l: f32, h: f32, e: f32) -> Boite {
    let largeur = (720.0 * e).min(l - 2.0 * MARGE_COTES * e).max(1.0);
    Boite { x: ((l - largeur) * 0.5).round(), y: (h - 98.0 * e).round(), l: largeur.round(), h: (44.0 * e).round() }
}

/// La barre de lecture, dans la capsule : de la place a gauche pour le signe
/// et le temps ecoule, a droite pour la duree. Et la zone ou un clic se vise,
/// plus haute que la piste, qui ne fait que quatre points.
pub fn barre(l: f32, h: f32, e: f32) -> (Boite, Boite) {
    let c = commandes(l, h, e);
    let piste = Boite {
        x: c.x + 104.0 * e,
        y: c.y + c.h * 0.5 - 2.0 * e,
        l: (c.l - 104.0 * e - 68.0 * e).max(1.0),
        h: 4.0 * e,
    };
    let cible = Boite { y: piste.y - 12.0 * e, h: piste.h + 24.0 * e, ..piste };
    (piste, cible)
}

/// La position visee par un clic sur la barre, en secondes.
pub fn position_sur(barre: Boite, x: f32, duree: f32) -> f32 {
    ((x - barre.x) / barre.l.max(1.0)).clamp(0.0, 1.0) * duree.max(0.0)
}

pub struct Visionneuse {
    pub elements: Vec<Element>,
    pub rang: usize,
    /// L'ouverture, de 0 (fermee, sur sa vignette) a ECHELLE (ouverte).
    pub ouverture: Ressort,
    /// D'ou elle part en s'ouvrant, et ou elle retourne en se fermant.
    pub depart: Boite,
    pub ferme: bool,
    /// Les pixels pleins de l'image en cours, lus sur le disque apres
    /// l'ouverture : on montre d'abord la vignette, deja la.
    pub pleine: Option<Pleine>,
    pub lecture: Option<crate::video::Lecture>,
    /// Le zoom : le grossissement (de ECHELLE, l'image ajustee, a
    /// `loupe_max`) et le decalage de son centre, en pixels.
    pub loupe: Ressort,
    pub dx: Ressort,
    pub dy: Ressort,
}

impl Visionneuse {
    pub fn ouvrir(elements: Vec<Element>, rang: usize, depart: Boite, reduire: bool) -> Visionneuse {
        let mut ouverture = Ressort::pose(0.0);
        ouverture.viser(ECHELLE);
        if reduire {
            ouverture.poser(ECHELLE);
        }
        Visionneuse {
            elements,
            rang,
            ouverture,
            depart,
            ferme: false,
            pleine: None,
            lecture: None,
            loupe: Ressort::pose(ECHELLE),
            dx: Ressort::pose(0.0),
            dy: Ressort::pose(0.0),
        }
    }

    /// Le grossissement et le decalage en ce moment.
    pub fn zoom(&self) -> (f32, (f32, f32)) {
        (self.loupe.position() / ECHELLE, (self.dx.position(), self.dy.position()))
    }

    /// Ceux qu'on vise : deux crans de molette rapides s'ajoutent a la cible,
    /// pas a la position que le ressort n'a pas encore atteinte.
    pub fn zoom_vise(&self) -> (f32, (f32, f32)) {
        (self.loupe.cible() / ECHELLE, (self.dx.cible(), self.dy.cible()))
    }

    pub fn zoomee(&self) -> bool {
        self.loupe.cible() > ECHELLE * 1.001
    }

    /// Vise un grossissement et un decalage ; d'un coup sous le mouvement
    /// reduit.
    pub fn viser_zoom(&mut self, z: f32, d: (f32, f32), reduire: bool) {
        self.loupe.viser(z * ECHELLE);
        self.dx.viser(d.0);
        self.dy.viser(d.1);
        if reduire {
            self.poser_zoom(z, d);
        }
    }

    /// Pose le zoom sans mouvement : l'image suit la main qui la glisse.
    pub fn poser_zoom(&mut self, z: f32, d: (f32, f32)) {
        self.loupe.poser(z * ECHELLE);
        self.dx.poser(d.0);
        self.dy.poser(d.1);
    }

    pub fn avancer(&mut self, dt: f32) {
        self.ouverture.avancer(dt);
        self.loupe.avancer(dt);
        self.dx.avancer(dt);
        self.dy.avancer(dt);
    }

    pub fn element(&self) -> &Element {
        &self.elements[self.rang.min(self.elements.len().saturating_sub(1))]
    }

    pub fn video(&self) -> bool {
        self.element().image.video.is_some()
    }

    /// Ou en est l'ouverture, de 0 a 1.
    pub fn t(&self) -> f32 {
        (self.ouverture.position() / ECHELLE).clamp(0.0, 1.0)
    }

    /// Se ferme vers `retour` : la vignette ou l'image du fil si elle est a
    /// l'ecran, sinon un point au centre, ou l'image s'eteint en retrecissant.
    pub fn fermer(&mut self, retour: Boite, reduire: bool) {
        self.ferme = true;
        self.depart = retour;
        self.lecture = None;
        self.ouverture.viser(0.0);
        if reduire {
            self.ouverture.poser(0.0);
        }
    }

    /// Fermee et revenue a sa place : elle peut disparaitre.
    pub fn partie(&self) -> bool {
        self.ferme && self.ouverture.au_repos() && self.ouverture.position() < 1.0
    }

    /// L'image voisine, dans un sens ou dans l'autre ; rien au bout.
    pub fn voisine(&mut self, pas: i32) -> bool {
        let n = self.elements.len() as i32;
        let r = self.rang as i32 + pas;
        if self.ferme || r < 0 || r >= n {
            return false;
        }
        self.rang = r as usize;
        self.pleine = None;
        self.lecture = None;
        self.poser_zoom(1.0, (0.0, 0.0));
        true
    }
}

/// Le reglage de Windows « Effets d'animation » : sans lui, la visionneuse
/// s'ouvre et se ferme d'un coup, comme le demande la personne qui l'a coupe.
#[cfg(windows)]
pub fn animations_coupees() -> bool {
    extern "system" {
        fn SystemParametersInfoW(action: u32, param: u32, valeur: *mut core::ffi::c_void, maj: u32) -> i32;
    }
    const SPI_GETCLIENTAREAANIMATION: u32 = 0x1042;
    let mut actif: i32 = 1;
    let ok = unsafe { SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, &mut actif as *mut i32 as *mut _, 0) };
    ok != 0 && actif == 0
}

#[cfg(not(windows))]
pub fn animations_coupees() -> bool {
    false
}

#[cfg(test)]
mod essais {
    use super::*;

    fn b(x: f32, y: f32, l: f32, h: f32) -> Boite {
        Boite { x, y, l, h }
    }

    /// Une capture large se cale sur la largeur, une haute sur la hauteur, et
    /// une petite garde sa taille naturelle a l'echelle de l'ecran.
    #[test]
    fn une_image_s_ajuste_sans_s_etirer() {
        let z = b(60.0, 105.0, 1130.0, 765.0);
        assert_eq!(ajuster((2000, 640), z, 1.25), b(60.0, 307.0, 1130.0, 362.0));
        assert_eq!(ajuster((400, 2000), z, 1.25), b(549.0, 105.0, 153.0, 765.0));
        assert_eq!(ajuster((400, 300), z, 1.25), b(375.0, 300.0, 500.0, 375.0));
    }

    #[test]
    fn la_boite_glisse_de_la_vignette_a_sa_place() {
        let (a, c) = (b(200.0, 600.0, 300.0, 200.0), b(60.0, 100.0, 1100.0, 700.0));
        assert_eq!(interpoler(a, c, 0.0), a);
        assert_eq!(interpoler(a, c, 1.0), c);
        assert_eq!(interpoler(a, c, 0.5), b(130.0, 350.0, 700.0, 450.0));
        assert_eq!(interpoler(a, c, 7.0), c);
    }

    #[test]
    fn un_clic_sur_la_barre_vise_sa_position() {
        let (piste, cible) = barre(1250.0, 950.0, 1.25);
        assert!(cible.contient(piste.x + 1.0, piste.y - 10.0));
        assert_eq!(position_sur(piste, piste.x, 11.0), 0.0);
        assert_eq!(position_sur(piste, piste.x + piste.l * 0.5, 11.0), 5.5);
        assert_eq!(position_sur(piste, piste.x + piste.l * 2.0, 11.0), 11.0);
    }

    /// Le point vise reste sous la souris ; le zoom se borne a l'ajustee et a
    /// son maximum ; l'image grossie ne se decolle pas des bords de la
    /// fenetre, et plus petite qu'elle, elle reste a sa place.
    #[test]
    fn le_zoom_garde_le_point_vise_et_ses_bornes() {
        let aj = b(100.0, 100.0, 800.0, 400.0);
        let fen = (1000.0, 600.0);
        let zmax = loupe_max(aj, (3200, 1600));
        assert_eq!(zmax, 32.0);
        // Le point (300, 250) de l'ecran, grossi deux fois, reste en place.
        let (z, d) = autour(aj, 1.0, (0.0, 0.0), 2.0, (300.0, 250.0), fen, zmax);
        let g = zoomee(aj, z, d);
        assert_eq!(z, 2.0);
        let avant = ((300.0 - aj.x) / aj.l, (250.0 - aj.y) / aj.h);
        let apres = ((300.0 - g.x) / g.l, (250.0 - g.y) / g.h);
        assert!((avant.0 - apres.0).abs() < 1e-5 && (avant.1 - apres.1).abs() < 1e-5);
        // Au coin, la borne l'emporte : le bord gauche reste sur celui de la
        // fenetre.
        let (z, d) = autour(aj, 1.0, (0.0, 0.0), 2.0, (100.0, 100.0), fen, zmax);
        assert_eq!(zoomee(aj, z, d).x, 0.0);
        // Ni sous l'ajustee, ni au-dela du maximum.
        assert_eq!(autour(aj, 1.0, (0.0, 0.0), 0.5, (500.0, 300.0), fen, zmax).0, 1.0);
        assert_eq!(autour(aj, 30.0, (0.0, 0.0), 2.0, (500.0, 300.0), fen, zmax).0, zmax);
        // Plus petite que la fenetre, aucun decalage ne tient.
        assert_eq!(borner(aj, 1.1, (300.0, -200.0), fen), (0.0, 0.0));
        // Glissee trop loin, elle s'arrete au bord.
        let d = borner(aj, 3.0, (5000.0, 0.0), fen);
        assert_eq!(zoomee(aj, 3.0, d).x, 0.0);
        // Un double-clic sur une petite image grossit quand meme.
        assert_eq!(loupe_reelle(aj, (400, 200)), 2.0);
        assert_eq!(loupe_reelle(aj, (3200, 1600)), 4.0);
    }

    /// Passer a la voisine remet le zoom a l'ajustee.
    #[test]
    fn la_voisine_repart_ajustee() {
        let mut v = Visionneuse::ouvrir((0..2).map(element).collect(), 0, b(0.0, 0.0, 10.0, 10.0), true);
        v.viser_zoom(3.0, (40.0, 0.0), true);
        assert!(v.zoomee());
        assert!(v.voisine(1));
        assert!(!v.zoomee() && v.zoom() == (1.0, (0.0, 0.0)));
    }

    fn element(n: usize) -> Element {
        Element {
            image: Arc::new(Image {
                largeur: 1,
                hauteur: 1,
                pixels: vec![0; 4],
                chemin: None,
                origine: crate::suivi::Origine::Produite,
                video: None,
                source: (1, 1),
                nommee: true,
            }),
            provenance: Provenance::Planche(n),
            agent: None,
            details: String::new(),
        }
    }

    /// Ouverte, elle va de la vignette a sa place ; fermee, elle revient, et
    /// disparait une fois arrivee. Les fleches s'arretent aux bouts.
    #[test]
    fn elle_s_ouvre_se_parcourt_et_se_ferme() {
        let mut v = Visionneuse::ouvrir((0..3).map(element).collect(), 1, b(0.0, 0.0, 10.0, 10.0), false);
        assert_eq!(v.t(), 0.0);
        for _ in 0..120 {
            v.ouverture.avancer(1.0 / 60.0);
        }
        assert!(v.t() > 0.999 && !v.partie());
        assert!(v.voisine(1) && v.rang == 2);
        assert!(!v.voisine(1) && v.rang == 2);
        assert!(v.voisine(-2) && v.rang == 0);
        v.fermer(b(0.0, 0.0, 10.0, 10.0), false);
        assert!(!v.voisine(1));
        for _ in 0..120 {
            v.ouverture.avancer(1.0 / 60.0);
        }
        assert!(v.partie());
        // Animations coupees : ouverte et fermee d'un coup.
        let mut w = Visionneuse::ouvrir(vec![element(0)], 0, b(0.0, 0.0, 10.0, 10.0), true);
        assert_eq!(w.t(), 1.0);
        w.fermer(b(0.0, 0.0, 10.0, 10.0), true);
        assert!(w.partie());
    }
}
