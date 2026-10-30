//! L'ecran d'ouverture : une toile peinte qui couvre toute la fenetre pendant
//! que la vigie lit ses chantiers, puis s'efface.
//!
//! La toile est celle que Stone a choisie le 2026-10-29 (« L'etat 5 », la
//! fusion celeste, `design/ouverture/toile-01/DECISION.md`) : 3840 x 2400, en
//! JPEG dans l'executable. Elle se recadre pour couvrir la fenetre, quelle que
//! soit sa forme : le soleil et l'horizon sont au centre, c'est ce que le
//! recadrage de la fusion a ete juge tenir.
//!
//! Les regles, dites avant le code :
//! - elle ne retarde jamais la vigie : elle se montre pendant la lecture
//!   reelle, et au plus une seconde et demie pour qu'on la voie ;
//! - un clic ou une touche la passe, et ne fait rien d'autre ;
//! - elle s'efface sur le ressort de la vigie, d'un coup quand Windows
//!   demande de reduire les animations (et alors sans attente de plus) ;
//! - une capture (`--capture`) n'en montre pas, sauf `--ouverture=`.

use std::time::Duration;

use crate::plan::Boite;
use crate::ressort::Ressort;

/// La toile, en JPEG, 3840 x 2400.
pub const TOILE: &[u8] = include_bytes!("../assets/ouverture.jpg");

/// Ce qu'on la garde au moins, pour qu'on la voie : une lecture qui finit en
/// deux cents millisecondes la ferait clignoter.
pub const MINIMUM: Duration = Duration::from_millis(1500);

/// Le ressort va de 0 (effacee) a ECHELLE (pleine), comme celui de la
/// visionneuse : le seuil de repos du ressort compte en pixels.
pub const ECHELLE: f32 = 1000.0;

pub struct Ouverture {
    /// Son opacite, de 0 a ECHELLE.
    pub ressort: Ressort,
    /// Elle a commence a s'effacer.
    pub sortie: bool,
    /// Le temps qu'elle est la, en secondes.
    pub age: f32,
    /// Fixee pour une capture : elle ne sort jamais d'elle-meme.
    pub fixe: bool,
}

impl Ouverture {
    pub fn nouvelle() -> Ouverture {
        Ouverture { ressort: Ressort::pose(ECHELLE), sortie: false, age: 0.0, fixe: false }
    }

    /// Fixee a une opacite donnee : pour une capture.
    pub fn fixee(opacite: f32) -> Ouverture {
        Ouverture {
            ressort: Ressort::pose(opacite.clamp(0.0, 1.0) * ECHELLE),
            sortie: false,
            age: MINIMUM.as_secs_f32(),
            fixe: true,
        }
    }

    pub fn opacite(&self) -> f32 {
        (self.ressort.position() / ECHELLE).clamp(0.0, 1.0)
    }

    /// Doit-elle commencer a s'effacer ? Quand les donnees sont la et qu'elle
    /// a ete montree assez longtemps (sous le mouvement reduit : tout de suite).
    pub fn doit_sortir(&self, donnees_pretes: bool, mouvement_reduit: bool) -> bool {
        if self.sortie || self.fixe || !donnees_pretes {
            return false;
        }
        mouvement_reduit || self.age >= MINIMUM.as_secs_f32()
    }

    /// Commence a s'effacer (ou disparait d'un coup sous le mouvement reduit).
    pub fn sortir(&mut self, mouvement_reduit: bool) {
        if self.sortie {
            return;
        }
        self.sortie = true;
        self.ressort.viser(0.0);
        if mouvement_reduit {
            self.ressort.poser(0.0);
        }
    }

    /// Fait avancer le temps et le ressort.
    pub fn avancer(&mut self, dt: f32, dt_ressort: f32) {
        self.age += dt;
        self.ressort.avancer(dt_ressort);
    }

    /// Effacee et au repos : elle peut disparaitre.
    pub fn finie(&self) -> bool {
        self.sortie && self.ressort.au_repos() && self.ressort.position() < 1.0
    }
}

/// Le rectangle qui couvre toute la fenetre avec une image de l x h, centre,
/// jamais deforme : l'image est agrandie jusqu'a couvrir les deux cotes, et
/// ce qui depasse est coupe par la fenetre.
pub fn couvrir(image: (u32, u32), fenetre: (f32, f32)) -> Boite {
    let (il, ih) = (image.0.max(1) as f32, image.1.max(1) as f32);
    let r = (fenetre.0 / il).max(fenetre.1 / ih);
    let (l, h) = (il * r, ih * r);
    Boite { x: (fenetre.0 - l) * 0.5, y: (fenetre.1 - h) * 0.5, l, h }
}

/// La toile decodee et ses reductions, de la moitie jusqu'a 64 px : la carte y
/// lit l'image quand elle est montree plus petite que sa taille (une fenetre
/// de 1 400 px de large la reduit de moitie et plus), sans crenelage.
pub struct Decodee {
    pub largeur: u32,
    pub hauteur: u32,
    pub pixels: Vec<u8>,
    pub niveaux: Vec<(u32, u32, Vec<u8>)>,
}

pub fn decoder() -> Option<Decodee> {
    let img = image::load_from_memory_with_format(TOILE, image::ImageFormat::Jpeg).ok()?.to_rgba8();
    let mut niveaux = Vec::new();
    let mut prec = img.clone();
    while prec.width().max(prec.height()) > 64 {
        let (l, h) = ((prec.width() / 2).max(1), (prec.height() / 2).max(1));
        prec = image::imageops::resize(&prec, l, h, image::imageops::FilterType::Triangle);
        niveaux.push((l, h, prec.as_raw().clone()));
    }
    Some(Decodee { largeur: img.width(), hauteur: img.height(), pixels: img.into_raw(), niveaux })
}

#[cfg(test)]
mod essais {
    use super::*;

    /// Une fenetre plus large que la toile coupe le haut et le bas ; plus
    /// haute, les cotes ; carree, les cotes. Jamais de bande vide, jamais
    /// deformee, toujours centree.
    #[test]
    fn la_toile_couvre_toute_fenetre_sans_se_deformer() {
        let toile = (3840, 2400);
        for f in [(1920.0, 1200.0), (766.0, 785.0), (480.0, 760.0), (2560.0, 1080.0), (1000.0, 1000.0)] {
            let b = couvrir(toile, f);
            assert!(b.x <= 0.001 && b.y <= 0.001, "{f:?} : bande vide au bord haut-gauche");
            assert!(b.x + b.l >= f.0 - 0.001 && b.y + b.h >= f.1 - 0.001, "{f:?} : bande vide au bord bas-droit");
            assert!((b.l / b.h - 1.6).abs() < 1e-4, "{f:?} : deformee");
            assert!(
                ((b.x + b.l * 0.5) - f.0 * 0.5).abs() < 0.01 && ((b.y + b.h * 0.5) - f.1 * 0.5).abs() < 0.01,
                "{f:?} : pas centree"
            );
        }
        // Exactement a la taille : rien ne depasse.
        let b = couvrir(toile, (3840.0, 2400.0));
        assert_eq!((b.x, b.y, b.l, b.h), (0.0, 0.0, 3840.0, 2400.0));
    }

    /// Elle reste le minimum, jamais moins, meme si les donnees sont la ; elle
    /// ne sort pas sans elles ; sous le mouvement reduit, elle sort des qu'elles
    /// sont la.
    #[test]
    fn elle_se_montre_au_moins_le_minimum_et_attend_les_donnees() {
        let mut o = Ouverture::nouvelle();
        o.age = 0.2;
        assert!(!o.doit_sortir(true, false), "trop tot : on ne l'aurait pas vue");
        o.age = MINIMUM.as_secs_f32();
        assert!(!o.doit_sortir(false, false), "les donnees ne sont pas la : elle reste");
        assert!(o.doit_sortir(true, false));
        let mut r = Ouverture::nouvelle();
        r.age = 0.0;
        assert!(r.doit_sortir(true, true), "mouvement reduit : sans attente de plus");
        assert!(!r.doit_sortir(false, true));
    }

    /// Une ouverture fixee pour une capture ne sort jamais d'elle-meme, meme
    /// quand les donnees sont la et qu'elle a passe le minimum.
    #[test]
    fn une_ouverture_fixee_ne_sort_jamais() {
        let mut o = Ouverture::fixee(1.0);
        o.age = 100.0;
        assert!(!o.doit_sortir(true, false) && !o.doit_sortir(true, true));
        assert_eq!(o.opacite(), 1.0);
        assert_eq!(Ouverture::fixee(0.5).opacite(), 0.5);
        // Et le contre-temoin : la vraie ouverture sort dans les memes conditions.
        let mut v = Ouverture::nouvelle();
        v.age = 100.0;
        assert!(v.doit_sortir(true, false));
    }

    /// Elle s'efface sur le ressort, finit au repos et ne revient pas ; sous le
    /// mouvement reduit, d'un coup.
    #[test]
    fn elle_s_efface_et_ne_revient_pas() {
        let mut o = Ouverture::nouvelle();
        assert_eq!(o.opacite(), 1.0);
        assert!(!o.finie());
        o.sortir(false);
        assert!(!o.finie(), "elle commence a peine");
        for _ in 0..240 {
            o.avancer(1.0 / 60.0, 1.0 / 60.0);
        }
        assert!(o.finie() && o.opacite() == 0.0);
        let mut d = Ouverture::nouvelle();
        d.sortir(true);
        assert!(d.finie(), "mouvement reduit : d'un coup");
        // Un second appel ne la relance pas.
        let mut e = Ouverture::nouvelle();
        e.sortir(false);
        let apres = e.ressort.cible();
        e.sortir(false);
        assert_eq!(e.ressort.cible(), apres);
    }

    /// La toile embarquee est bien celle de 3 840 x 2 400, et ses reductions
    /// vont de moitie en moitie.
    #[test]
    fn la_toile_embarquee_se_decode() {
        let d = decoder().expect("la toile ne se decode pas");
        assert_eq!((d.largeur, d.hauteur), (3840, 2400));
        assert_eq!(d.pixels.len(), 4 * 3840 * 2400);
        assert_eq!((d.niveaux[0].0, d.niveaux[0].1), (1920, 1200));
        let dernier = d.niveaux.last().unwrap();
        assert!(dernier.0.max(dernier.1) <= 64);
        // Le centre est clair (le soleil), le coin haut est sombre : ce n'est
        // pas une image vide.
        let px = |x: usize, y: usize| d.pixels[4 * (y * 3840 + x)] as u32 + d.pixels[4 * (y * 3840 + x) + 1] as u32;
        assert!(px(1920, 1250) > px(40, 40) + 150, "le centre devrait etre bien plus clair que le coin");
    }
}
