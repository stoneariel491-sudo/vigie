//! Le ressort du defilement, integre par sa SOLUTION EXACTE.
//!
//! Deux raisons, et la seconde compte plus que la premiere :
//!
//! 1. un pas d'Euler a cette pulsation est stable a 60 Hz mais deja mou, et
//!    sur une trame perdue il se met a depasser ;
//! 2. la solution exacte ne depend que de la position et de la vitesse au
//!    debut du pas. C'est la definition meme de l'interruptibilite : la molette
//!    change la cible, l'etat physique reste, et le mouvement continue depuis
//!    ou il en etait — sans reprise a zero, sans saut.
//!
//! L'amortissement est critique : il se calcule, il ne se regle pas. Pour une
//! raideur k et une masse 1, il vaut 2*sqrt(k). Il n'y a donc aucun rebond a
//! doser, puisqu'il n'y en a pas.

/// Pulsation propre, en radians par seconde. A 14, le defilement se pose en
/// un peu plus de 300 ms : assez vif pour suivre la molette, assez lent pour
/// que l'oeil raccroche la ligne qu'il lisait.
const PULSATION: f32 = 14.0;

/// En dessous, on se pose exactement sur la cible et on cesse de demander des
/// trames. Le seuil est en PIXELS, comme la course — une unite de position
/// normalisee ferait perdre le sens du nombre.
const REPOS_POSITION: f32 = 0.15;
/// Il est immobile s'il ne parcourra pas un cinquieme de pixel a la trame
/// suivante, soit 12 px/s a 60 Hz.
const REPOS_VITESSE: f32 = 12.0;

pub struct Ressort {
    position: f32,
    vitesse: f32,
    cible: f32,
}

impl Ressort {
    pub fn pose(x: f32) -> Ressort {
        Ressort {
            position: x,
            vitesse: 0.0,
            cible: x,
        }
    }

    pub fn position(&self) -> f32 {
        self.position
    }

    pub fn cible(&self) -> f32 {
        self.cible
    }

    /// Change la cible, et RIEN d'autre. La position et la vitesse restent.
    pub fn viser(&mut self, cible: f32) {
        self.cible = cible;
    }

    /// Se pose d'un coup, sans mouvement. Pour un changement d'agent, ou quand
    /// la reduction de mouvement est demandee.
    pub fn poser(&mut self, x: f32) {
        self.position = x;
        self.cible = x;
        self.vitesse = 0.0;
    }

    pub fn au_repos(&self) -> bool {
        (self.position - self.cible).abs() < REPOS_POSITION
            && self.vitesse.abs() < REPOS_VITESSE
    }

    /// Avance du temps REEL ecoule. `dt` n'est pas ecrete : c'est a l'appelant
    /// de ne pas lui donner une seconde entiere apres un reveil.
    pub fn avancer(&mut self, dt: f32) {
        if self.au_repos() {
            self.position = self.cible;
            self.vitesse = 0.0;
            return;
        }
        let w = PULSATION;
        let x = self.position - self.cible;
        let v = self.vitesse;
        // Cas critique : x(t) = (x0 + (v0 + w*x0) t) e^(-w t)
        let e = (-w * dt).exp();
        let nx = (x + (v + w * x) * dt) * e;
        let nv = (v - w * (v + w * x) * dt) * e;
        self.position = self.cible + nx;
        self.vitesse = nv;
    }
}
