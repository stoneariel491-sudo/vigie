//! La page d'un lien, hors de Windows.
//!
//! Le moteur de pages integre (WebView2, son mandataire et ses gardiens) n'existe
//! que sous Windows. Ailleurs, la vigie n'ouvre rien elle-meme : « Ouvrir la
//! page » le dit sur la feuille, et le lien se lit toujours (l'extrait, lu sans
//! script par le client HTTP de la vigie, qui refuse les adresses privees) ou
//! s'ouvre dans le navigateur du systeme. Ce module garde la forme de
//! `navigateur.rs` pour que le reste de la vigie ne le sache pas.

use std::time::Instant;

/// Ce que la vigie sait du passage dans la page ouverte.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Sonde {
    #[default]
    Attente,
    Trouve,
    NonRetrouve,
    Sans,
    /// Le moteur de pages n'existe pas sur ce systeme.
    Indisponible,
}

/// Un geste de l'historique de la page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Geste {
    Reculer,
    Avancer,
}

/// Jamais construit hors de Windows : `ouvrir` refuse toujours.
pub struct Navigateur {
    _x: Instant,
}

/// Hors de Windows, la page n'est jamais ouverte par la vigie.
pub fn page_activee() -> bool {
    false
}

impl Navigateur {
    pub fn ouvrir(_fenetre: &winit::window::Window, _zone: [f32; 4], _adresse: &str, _passage: Option<&str>, _local_ok: bool) -> Result<Navigateur, String> {
        Err("le moteur de pages n'existe que sous Windows".to_string())
    }
    pub fn poser(&mut self, _zone: [f32; 4]) {}
    pub fn tic(&mut self) {}
    pub fn sonde(&self) -> Sonde {
        Sonde::Indisponible
    }
    pub fn echap_demande(&self) -> bool {
        false
    }
    pub fn geste_demande(&self) -> Option<Geste> {
        None
    }
    pub fn peut_reculer(&self) -> bool {
        false
    }
    pub fn peut_avancer(&self) -> bool {
        false
    }
    pub fn reculer(&self) {}
    pub fn avancer(&self) {}
    pub fn liberer(&mut self) {}
    pub fn courante(&self) -> String {
        String::new()
    }
}

pub fn balayer_orphelins() -> usize {
    0
}

pub fn attendre_les_suppressions() {}
