//! Le plan de l'ecran : ce qui se trouve sous un point.
//!
//! Le rendu dresse a chaque trame la liste des boites cliquables, en pixels
//! d'ecran, defilement deja applique. Savoir ce qu'il y a sous la souris est
//! alors une fonction pure, sans fenetre ni carte graphique, et c'est ce qui
//! permet de la verifier par des essais.
//!
//! Deux pieges qu'elle doit eviter, et que ses essais gardent :
//! - un onglet a demi sorti de sa bande, voile par le fondu, ne doit pas se
//!   cliquer hors de la bande, ni a travers « Direct » ou le compte des
//!   onglets caches, qui restent fixes pendant que la bande glisse ;
//! - une ligne de liste qui a defile SOUS l'en-tete est cachee par lui, et un
//!   clic sur l'en-tete ne doit jamais la choisir.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Boite {
    pub x: f32,
    pub y: f32,
    pub l: f32,
    pub h: f32,
}

impl Boite {
    pub fn contient(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.l && y >= self.y && y < self.y + self.h
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cible {
    /// Un onglet, par son rang dans la bande (0 = Direct).
    Onglet(usize),
    /// Le bouton de la barre laterale, qui la masque et la reaffiche ; et,
    /// dans une fenetre etroite, ce que voile la barre rappelee, qui la renvoie.
    Barre,
    /// Une ligne de la liste, ou d'un agent dans la page d'un chantier.
    Ligne(usize),
    /// Une vignette de la planche d'un chantier, par son rang.
    Vignette(usize),
    /// Une image du fil, par le rang de son bloc : un clic l'ouvre.
    Image(usize),
    /// Un lien du fil : le rang du bloc, puis le rang du lien dans le bloc.
    /// Un lien coupe sur deux lignes a plusieurs boites et une seule cible.
    Lien(usize, usize),
    /// Le titre de l'en-tete : il remonte d'un niveau, comme le titre d'une
    /// fenetre du Finder.
    Entete,
    Rien,
}

#[derive(Clone, Debug, Default)]
pub struct Plan {
    /// Ce qui ne defile pas : « Direct », et le compte des onglets caches.
    /// Ils passent avant la bande, qui glisse sous eux.
    pub fixes: Vec<(Boite, Cible)>,
    /// La fenetre ou defilent les onglets de projet : ce qui en sort n'est ni
    /// vu ni cliquable.
    pub bande: Option<Boite>,
    /// Chaque onglet de projet, et son rang dans la bande.
    pub onglets: Vec<(Boite, usize)>,
    /// La zone visible sous l'en-tete, celle d'une liste ou du fil : ce qui a
    /// glisse dessous n'est pas cliquable.
    pub zone: Option<Boite>,
    pub lignes: Vec<Boite>,
    pub vignettes: Vec<Boite>,
    pub images: Vec<(Boite, usize)>,
    /// Les boites des liens du fil, chacune avec son bloc et son rang.
    pub liens: Vec<(Boite, usize, usize)>,
}

/// Ce qui se trouve sous le point (x, y).
pub fn cible(plan: &Plan, x: f32, y: f32) -> Cible {
    if let Some((_, c)) = plan.fixes.iter().find(|(b, _)| b.contient(x, y)) {
        return *c;
    }
    if let Some(bande) = plan.bande {
        if bande.contient(x, y) {
            return plan
                .onglets
                .iter()
                .find(|(b, _)| b.contient(x, y))
                .map_or(Cible::Rien, |(_, i)| Cible::Onglet(*i));
        }
    }
    if let Some(zone) = plan.zone {
        if zone.contient(x, y) {
            if let Some(i) = plan.lignes.iter().position(|b| b.contient(x, y)) {
                return Cible::Ligne(i);
            }
            if let Some((_, bloc, rang)) = plan.liens.iter().find(|(b, _, _)| b.contient(x, y)) {
                return Cible::Lien(*bloc, *rang);
            }
            if let Some(i) = plan.vignettes.iter().position(|b| b.contient(x, y)) {
                return Cible::Vignette(i);
            }
            if let Some((_, i)) = plan.images.iter().find(|(b, _)| b.contient(x, y)) {
                return Cible::Image(*i);
            }
        }
    }
    Cible::Rien
}

/// La hauteur minimale d'une cible de clic, en points (WCAG 2.5.8, niveau AA).
/// Un lien dans une phrase en est dispense, sa taille tenant a l'interligne du
/// texte qui l'entoure ; une ligne qui est elle-meme le lien ne l'est pas.
pub const CIBLE_MIN: f32 = 24.0;

/// La hauteur d'une ligne de geste : celle du fil, ou `CIBLE_MIN` si la ligne
/// est un lien (la ligne entiere se vise, sans recouvrir sa voisine).
pub fn hauteur_de_ligne(base: f32, est_un_lien: bool) -> f32 {
    if est_un_lien {
        base.max(CIBLE_MIN)
    } else {
        base
    }
}

/// Un lien deborde de sa boite de trois points au plus : un clic plus loin ne
/// l'ouvre pas.
pub const LIEN_DEBORD: f32 = 3.0;

/// La boite cliquable d'un lien, en pixels d'ecran, d'apres sa boite dans le
/// texte du bloc. `x0` est le bord gauche du texte, `y` le haut du bloc. Une
/// ligne de geste se vise en entier : `rangee` donne alors le bord gauche de la
/// rangee, verbe compris, et la boite n'a pas de debord.
pub fn boite_du_lien(b: Boite, x0: f32, y: f32, e: f32, rangee: Option<f32>) -> Boite {
    match rangee {
        Some(xr) => Boite { x: xr, y: y + b.y, l: x0 - xr + b.x + b.l, h: b.h },
        None => Boite { x: x0 + b.x - LIEN_DEBORD * e, y: y + b.y, l: b.l + 2.0 * LIEN_DEBORD * e, h: b.h },
    }
}

/// Le defilement qui montre l'intervalle [debut, fin) dans une fenetre de
/// longueur `vue`, en bougeant le moins possible : rien si l'intervalle se
/// voit deja, sinon juste assez pour qu'il se voie avec sa `marge`. Sert aux
/// onglets (en x) et a la ligne choisie au clavier (en y).
pub fn garder_visible(debut: f32, fin: f32, vue: f32, actuel: f32, marge: f32, max: f32) -> f32 {
    let mut d = actuel;
    if fin + marge > d + vue {
        d = fin + marge - vue;
    }
    if debut - marge < d {
        d = debut - marge;
    }
    d.clamp(0.0, max.max(0.0))
}

#[cfg(test)]
mod essais {
    use super::*;

    /// Objection C12 de la correction : les lignes de lien faisaient 20 pt.
    #[test]
    fn une_ligne_de_lien_fait_vingt_quatre_points_et_les_autres_gardent_la_leur() {
        assert_eq!(hauteur_de_ligne(20.0, true), 24.0);
        assert_eq!(hauteur_de_ligne(20.0, false), 20.0);
        assert_eq!(hauteur_de_ligne(30.0, true), 30.0);
        for e in [1.0f32, 1.25, 1.5, 2.0] {
            assert!(hauteur_de_ligne(20.0, true) * e >= 24.0 * e);
        }
    }

    fn b(x: f32, y: f32, l: f32, h: f32) -> Boite {
        Boite { x, y, l, h }
    }

    /// « Direct » fixe a gauche ; une bande de 300 px qui a glisse de 30 px,
    /// si bien que le premier onglet de projet passe sous « Direct » et que le
    /// troisieme deborde a droite, sous le compte des caches ; et une liste
    /// dont la premiere ligne a glisse sous l'en-tete.
    fn plan() -> Plan {
        Plan {
            fixes: vec![(b(20.0, 12.0, 70.0, 26.0), Cible::Onglet(0)), (b(410.0, 12.0, 40.0, 26.0), Cible::Barre)],
            bande: Some(b(100.0, 0.0, 300.0, 40.0)),
            onglets: vec![(b(70.0, 12.0, 80.0, 26.0), 1), (b(154.0, 12.0, 120.0, 26.0), 2), (b(278.0, 12.0, 150.0, 26.0), 3)],
            zone: Some(b(0.0, 120.0, 1000.0, 600.0)),
            lignes: vec![b(90.0, 80.0, 800.0, 56.0), b(90.0, 136.0, 800.0, 56.0), b(90.0, 192.0, 800.0, 56.0)],
            vignettes: vec![b(90.0, 300.0, 300.0, 200.0)],
            images: vec![(b(90.0, 100.0, 400.0, 300.0), 7)],
            liens: vec![(b(100.0, 500.0, 200.0, 23.0), 4, 0), (b(100.0, 523.0, 80.0, 23.0), 4, 0), (b(100.0, 600.0, 200.0, 20.0), 6, 1)],
        }
    }

    #[test]
    fn un_onglet_se_touche() {
        let p = plan();
        assert_eq!(cible(&p, 30.0, 20.0), Cible::Onglet(0));
        assert_eq!(cible(&p, 120.0, 20.0), Cible::Onglet(1));
        assert_eq!(cible(&p, 250.0, 30.0), Cible::Onglet(2));
        assert_eq!(cible(&p, 390.0, 20.0), Cible::Onglet(3));
        assert_eq!(cible(&p, 430.0, 20.0), Cible::Barre);
    }

    #[test]
    fn entre_deux_onglets_il_n_y_a_rien() {
        assert_eq!(cible(&plan(), 152.0, 20.0), Cible::Rien);
    }

    #[test]
    fn un_onglet_hors_de_sa_bande_ne_se_touche_pas() {
        // x = 405 est dans la boite du troisieme onglet de projet, mais hors
        // de la bande, et pas encore sur le compte des caches.
        assert_eq!(cible(&plan(), 405.0, 20.0), Cible::Rien);
        // x = 80 est dans la boite du premier, qui a glisse sous « Direct » :
        // c'est « Direct » qu'on touche.
        assert_eq!(cible(&plan(), 80.0, 20.0), Cible::Onglet(0));
        // x = 95 est dans sa boite, entre « Direct » et la bande : rien.
        assert_eq!(cible(&plan(), 95.0, 20.0), Cible::Rien);
    }

    #[test]
    fn une_ligne_se_touche() {
        assert_eq!(cible(&plan(), 500.0, 150.0), Cible::Ligne(1));
        assert_eq!(cible(&plan(), 500.0, 247.0), Cible::Ligne(2));
    }

    #[test]
    fn une_ligne_sous_l_en_tete_ne_se_touche_pas() {
        // y = 100 est dans la premiere ligne, mais au-dessus de la zone : sous
        // l'en-tete, qui la cache.
        assert_eq!(cible(&plan(), 500.0, 100.0), Cible::Rien);
        // Et sa partie encore visible, elle, se touche.
        assert_eq!(cible(&plan(), 500.0, 125.0), Cible::Ligne(0));
    }

    /// Une vignette et une image du fil se touchent sous l'en-tete, et pas
    /// au-dessus : l'image du fil commence a 100, l'en-tete finit a 120.
    #[test]
    fn une_vignette_et_une_image_se_touchent() {
        assert_eq!(cible(&plan(), 200.0, 400.0), Cible::Vignette(0));
        assert_eq!(cible(&plan(), 950.0, 400.0), Cible::Rien);
        let mut p = plan();
        p.lignes.clear();
        assert_eq!(cible(&p, 300.0, 130.0), Cible::Image(7));
        assert_eq!(cible(&p, 300.0, 110.0), Cible::Rien);
    }

    #[test]
    fn hors_de_tout() {
        assert_eq!(cible(&plan(), 950.0, 700.0), Cible::Rien);
        assert_eq!(cible(&Plan::default(), 10.0, 10.0), Cible::Rien);
    }

    #[test]
    fn garder_visible_bouge_le_moins_possible() {
        // Deja visible : rien ne bouge.
        assert_eq!(garder_visible(100.0, 200.0, 300.0, 50.0, 10.0, 1000.0), 50.0);
        // Deborde a droite : juste assez pour voir sa fin et la marge.
        assert_eq!(garder_visible(400.0, 500.0, 300.0, 0.0, 10.0, 1000.0), 210.0);
        // Deborde a gauche : juste assez pour voir son debut et la marge.
        assert_eq!(garder_visible(100.0, 150.0, 300.0, 400.0, 10.0, 1000.0), 90.0);
        // Jamais hors des bornes.
        assert_eq!(garder_visible(0.0, 50.0, 300.0, 40.0, 10.0, 1000.0), 0.0);
        assert_eq!(garder_visible(900.0, 990.0, 300.0, 0.0, 30.0, 600.0), 600.0);
    }
}

#[cfg(test)]
mod essais_liens {
    use super::*;

    fn p() -> Plan {
        let b = |x, y, l, h| Boite { x, y, l, h };
        Plan {
            zone: Some(b(0.0, 120.0, 1000.0, 600.0)),
            liens: vec![(b(100.0, 500.0, 200.0, 23.0), 4, 0), (b(100.0, 523.0, 80.0, 23.0), 4, 0), (b(100.0, 600.0, 200.0, 20.0), 6, 1)],
            ..Plan::default()
        }
    }

    #[test]
    fn un_lien_coupe_sur_deux_lignes_est_une_seule_cible() {
        let p = p();
        assert_eq!(cible(&p, 150.0, 510.0), Cible::Lien(4, 0));
        assert_eq!(cible(&p, 120.0, 530.0), Cible::Lien(4, 0));
        // Les bords : la cible est la boite, pas une boite decalee.
        assert_eq!(cible(&p, 100.5, 510.0), Cible::Lien(4, 0));
        assert_eq!(cible(&p, 299.5, 510.0), Cible::Lien(4, 0));
        assert_eq!(cible(&p, 99.5, 510.0), Cible::Rien);
        assert_eq!(cible(&p, 300.5, 510.0), Cible::Rien);
        // Dans l'angle que la seconde ligne ne couvre pas : rien.
        assert_eq!(cible(&p, 250.0, 530.0), Cible::Rien);
    }

    /// Un clic a plus de trois points d'un lien ne l'ouvre pas, a chaque
    /// echelle ; a moins de trois, il l'ouvre. Le temoin : une boite decalee de
    /// dix points rate un clic pris en plein sur le lien.
    #[test]
    fn le_debord_d_un_lien_est_de_trois_points_a_chaque_echelle() {
        for e in [1.0f32, 1.25, 1.5, 2.0] {
            let b = Boite { x: 40.0, y: 0.0, l: 100.0, h: 23.0 * e };
            let (x0, y) = (300.0, 500.0);
            let c = boite_du_lien(b, x0, y, e, None);
            let plan = Plan { zone: Some(Boite { x: 0.0, y: 0.0, l: 2000.0, h: 2000.0 }), liens: vec![(c, 7, 0)], ..Plan::default() };
            let (gauche, droite) = (x0 + b.x, x0 + b.x + b.l);
            let milieu = y + b.h * 0.5;
            assert_eq!(cible(&plan, gauche - 2.9 * e, milieu), Cible::Lien(7, 0), "echelle {e}");
            assert_eq!(cible(&plan, droite + 2.9 * e, milieu), Cible::Lien(7, 0), "echelle {e}");
            assert_eq!(cible(&plan, gauche - 3.1 * e, milieu), Cible::Rien, "echelle {e}");
            assert_eq!(cible(&plan, droite + 3.1 * e, milieu), Cible::Rien, "echelle {e}");
            // La hauteur est celle de l'interligne, sans debord.
            assert_eq!(cible(&plan, gauche + 10.0, y - 0.5), Cible::Rien);
            assert_eq!(cible(&plan, gauche + 10.0, y + b.h + 0.5), Cible::Rien);
            // Temoin : une boite decalee de dix points rate le clic en plein lien.
            let decalee = Plan { liens: vec![(Boite { x: c.x + 10.0, ..c }, 7, 0)], ..plan.clone() };
            assert_eq!(cible(&decalee, gauche + 2.0, milieu), Cible::Rien);
        }
    }

    #[test]
    fn la_rangee_d_un_geste_se_vise_en_entier_verbe_compris() {
        let b = Boite { x: 0.0, y: 0.0, l: 300.0, h: 20.0 };
        let c = boite_du_lien(b, 478.0, 600.0, 1.0, Some(400.0));
        assert_eq!((c.x, c.l), (400.0, 378.0));
        let plan = Plan { zone: Some(Boite { x: 0.0, y: 0.0, l: 2000.0, h: 2000.0 }), liens: vec![(c, 3, 0)], ..Plan::default() };
        assert_eq!(cible(&plan, 410.0, 610.0), Cible::Lien(3, 0));
        assert_eq!(cible(&plan, 399.0, 610.0), Cible::Rien);
        assert_eq!(cible(&plan, 777.0, 610.0), Cible::Lien(3, 0));
        assert_eq!(cible(&plan, 779.0, 610.0), Cible::Rien);
        assert_eq!(cible(&plan, 779.0, 590.0), Cible::Rien);
    }

    #[test]
    fn un_lien_a_cote_ou_sous_l_en_tete_ne_se_touche_pas() {
        let p = p();
        assert_eq!(cible(&p, 99.0, 510.0), Cible::Rien);
        assert_eq!(cible(&p, 150.0, 622.0), Cible::Rien);
        // Un lien qui a defile sous l'en-tete n'est pas cliquable.
        let mut q = p;
        q.zone = Some(Boite { x: 0.0, y: 560.0, l: 1000.0, h: 100.0 });
        assert_eq!(cible(&q, 150.0, 510.0), Cible::Rien);
        assert_eq!(cible(&q, 150.0, 610.0), Cible::Lien(6, 1));
    }
}
