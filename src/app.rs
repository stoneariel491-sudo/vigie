//! Ce qui fait de la vigie une application, et non un executable qu'on lance :
//! une seule fenetre, un dossier a elle, et le souvenir de la ou on l'a
//! laissee.
//!
//! - Un second lancement ne rouvre pas une seconde fenetre : il ramene la
//!   premiere au premier plan, comme le Finder ou Mail.
//! - La taille, la position, le zoom et ce qu'on regardait se retrouvent au
//!   lancement suivant.
//! - Ce qui se garde vit dans `%APPDATA%\Vigie`, jamais a cote de l'executable,
//!   qui peut etre installe dans un dossier ou l'on n'ecrit pas.

use std::path::PathBuf;

use crate::suivi::Choix;
use crate::vues::Vue;

/// Le nom de classe de la fenetre : c'est par lui qu'un second lancement la
/// retrouve. Le titre « Vigie » ne suffirait pas, un dossier ouvert dans
/// l'Explorateur porte le meme.
pub const CLASSE: &str = "Stone.Vigie.Fenetre";

/// Une copie figee des donnees, pour que deux rendus du meme ecran soient
/// comparables : `VIGIE_DONNEES=<dossier>` fait lire les chantiers sous
/// `<dossier>\projects` et garder la memoire sous `<dossier>\vigie`, jamais
/// dans `%APPDATA%\Vigie`, qui est celui de la vigie installee.
pub fn figee() -> Option<PathBuf> {
    std::env::var_os("VIGIE_DONNEES").filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// L'heure de la vigie. `VIGIE_MAINTENANT=<secondes depuis 1970>` l'arrete a
/// cet instant : l'etat d'un chantier (« en cours » trente minutes apres sa
/// derniere ecriture) et les titres de jour (« Aujourd'hui ») dependent de
/// l'heure, et deux rendus d'une copie figee faits a une heure d'ecart, ou un
/// autre jour, ne montraient plus le meme ecran (cadre du tour 00, §8.2).
pub fn maintenant() -> std::time::SystemTime {
    match std::env::var("VIGIE_MAINTENANT").ok().and_then(|v| v.trim().parse::<u64>().ok()) {
        Some(s) => std::time::UNIX_EPOCH + std::time::Duration::from_secs(s),
        None => std::time::SystemTime::now(),
    }
}

/// Le dossier des donnees de la vigie, cree au besoin.
pub fn donnees() -> Option<PathBuf> {
    let d = match figee() {
        Some(f) => f.join("vigie"),
        None => PathBuf::from(std::env::var("APPDATA").ok()?).join("Vigie"),
    };
    std::fs::create_dir_all(&d).ok()?;
    Some(d)
}

/// Ecrit un fichier d'un seul coup : dans un fichier voisin, puis renomme.
/// Une vigie fermee au milieu d'une ecriture ne laisse pas un etat a moitie
/// ecrit, qui ne se relirait plus.
pub fn ecrire(nom: &str, contenu: &[u8]) {
    let Some(d) = donnees() else { return };
    let provisoire = d.join(format!("{nom}.provisoire"));
    if std::fs::write(&provisoire, contenu).is_ok() {
        let _ = std::fs::rename(&provisoire, d.join(nom));
    }
}

pub fn lire(nom: &str) -> Option<Vec<u8>> {
    std::fs::read(donnees()?.join(nom)).ok()
}

/// Une autre vigie est-elle deja ouverte ? Si oui, elle est ramenee au premier
/// plan, restauree si elle etait reduite, et celle-ci n'a plus qu'a partir.
///
/// Un mutex nomme dit qu'elle existe, et le nom de classe de sa fenetre la
/// retrouve. Le mutex n'est jamais relache : il meurt avec le processus.
#[cfg(windows)]
pub fn deja_ouverte() -> bool {
    type Poignee = *mut core::ffi::c_void;
    extern "system" {
        fn CreateMutexW(attributs: *const core::ffi::c_void, proprietaire: i32, nom: *const u16) -> Poignee;
        fn GetLastError() -> u32;
        fn FindWindowW(classe: *const u16, titre: *const u16) -> Poignee;
        fn IsIconic(fenetre: Poignee) -> i32;
        fn ShowWindow(fenetre: Poignee, commande: i32) -> i32;
        fn SetForegroundWindow(fenetre: Poignee) -> i32;
    }
    const DEJA_LA: u32 = 183;
    const RESTAURER: i32 = 9;
    let large = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let nom = large("Local\\Stone.Vigie.Instance");
    let classe = large(CLASSE);
    unsafe {
        let m = CreateMutexW(std::ptr::null(), 0, nom.as_ptr());
        if m.is_null() || GetLastError() != DEJA_LA {
            return false;
        }
        // La premiere vigie peut etre en train de creer sa fenetre : on lui
        // laisse deux secondes pour qu'elle existe.
        for _ in 0..20 {
            let f = FindWindowW(classe.as_ptr(), std::ptr::null());
            if !f.is_null() {
                if IsIconic(f) != 0 {
                    ShowWindow(f, RESTAURER);
                }
                SetForegroundWindow(f);
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
    // Un mutex sans fenetre : une vigie qui se ferme. On ouvre la sienne.
    false
}

#[cfg(not(windows))]
pub fn deja_ouverte() -> bool {
    false
}

/// Ce qui se retrouve au lancement suivant.
#[derive(Clone, Debug, PartialEq)]
pub struct Souvenir {
    /// La taille de la zone de dessin et la position de la fenetre, en pixels
    /// physiques.
    pub taille: (u32, u32),
    pub position: Option<(i32, i32)>,
    pub zoom: f32,
    pub vue: Vue,
    pub choix: Choix,
    /// Le theme regle : sombre, clair, ou celui du systeme.
    pub theme: crate::palette::Reglage,
    /// La barre laterale masquee a la main. Un souvenir d'avant la barre
    /// n'en dit rien : elle se montre.
    pub barre_masquee: bool,
}

fn vue_en_texte(v: &Vue) -> String {
    match v {
        Vue::Direct => "direct".to_string(),
        Vue::Liste(p) => format!("liste:{p}"),
        Vue::Chantier(id) => format!("chantier:{id}"),
        Vue::Suivi(id) => format!("suivi:{id}"),
    }
}

fn vue_de_texte(s: &str) -> Option<Vue> {
    let (genre, reste) = s.split_once(':').unwrap_or((s, ""));
    Some(match genre {
        "direct" => Vue::Direct,
        "liste" if !reste.is_empty() => Vue::Liste(reste.to_string()),
        "chantier" if !reste.is_empty() => Vue::Chantier(reste.to_string()),
        "suivi" if !reste.is_empty() => Vue::Suivi(reste.to_string()),
        _ => return None,
    })
}

impl Souvenir {
    pub fn en_json(&self) -> String {
        let (chantier, agent) = match &self.choix {
            Choix::Direct => (None, None),
            Choix::Chantier(c) => (Some(c.clone()), None),
            Choix::Agent { chantier, agent } => (Some(chantier.clone()), Some(agent.clone())),
        };
        serde_json::json!({
            "largeur": self.taille.0,
            "hauteur": self.taille.1,
            "position": self.position.map(|(x, y)| [x, y]),
            "zoom": self.zoom,
            "vue": vue_en_texte(&self.vue),
            "chantier": chantier,
            "agent": agent,
            "theme": self.theme.nom(),
            "barre_masquee": self.barre_masquee,
        })
        .to_string()
    }

    /// Un souvenir illisible, ou incoherent, n'est pas un souvenir : la vigie
    /// part alors de son etat neuf, plutot que d'une fenetre de zero pixel.
    pub fn de_json(texte: &str) -> Option<Souvenir> {
        let v: serde_json::Value = serde_json::from_str(texte).ok()?;
        let n = |c: &str| v.get(c).and_then(|x| x.as_u64());
        let (l, h) = (n("largeur")? as u32, n("hauteur")? as u32);
        if !(200..=16_000).contains(&l) || !(200..=16_000).contains(&h) {
            return None;
        }
        let position = v.get("position").and_then(|p| {
            let p = p.as_array()?;
            Some((p.first()?.as_i64()? as i32, p.get(1)?.as_i64()? as i32))
        });
        let zoom = v.get("zoom").and_then(|x| x.as_f64()).unwrap_or(1.0) as f32;
        let vue = vue_de_texte(v.get("vue").and_then(|x| x.as_str()).unwrap_or("direct"))?;
        let texte_de = |c: &str| v.get(c).and_then(|x| x.as_str()).map(str::to_string);
        let choix = match (texte_de("chantier"), texte_de("agent")) {
            (Some(chantier), Some(agent)) => Choix::Agent { chantier, agent },
            (Some(c), None) => Choix::Chantier(c),
            _ => Choix::Direct,
        };
        let theme = v
            .get("theme")
            .and_then(|x| x.as_str())
            .and_then(crate::palette::Reglage::nomme)
            .unwrap_or(crate::palette::Reglage::Sombre);
        Some(Souvenir {
            taille: (l, h),
            position,
            zoom: zoom.clamp(0.7, 2.4),
            vue,
            choix,
            theme,
            barre_masquee: v.get("barre_masquee").and_then(|x| x.as_bool()).unwrap_or(false),
        })
    }

    pub fn relire() -> Option<Souvenir> {
        Souvenir::de_json(&String::from_utf8_lossy(&lire("fenetre.json")?))
    }

    pub fn garder(&self) {
        ecrire("fenetre.json", self.en_json().as_bytes());
    }
}

#[cfg(test)]
mod essais {
    use super::*;

    /// Un souvenir relu est celui qu'on a garde, pour chaque sorte de vue.
    #[test]
    fn un_souvenir_se_relit_tel_quel() {
        for (vue, choix) in [
            (Vue::Direct, Choix::Direct),
            (Vue::Liste("atelier-d".into()), Choix::Chantier("wf_a".into())),
            (Vue::Chantier("wf_b".into()), Choix::Direct),
            (Vue::Suivi("wf_c".into()), Choix::Agent { chantier: "wf_c".into(), agent: "a1".into() }),
        ] {
            let s = Souvenir {
                taille: (1250, 950),
                position: Some((-1200, 40)),
                zoom: 1.1,
                vue,
                choix,
                theme: crate::palette::Reglage::Clair,
                barre_masquee: true,
            };
            assert_eq!(Souvenir::de_json(&s.en_json()), Some(s));
        }
    }

    /// Ce qui ne doit pas devenir une fenetre : une taille nulle, un texte
    /// tronque, une vue inconnue. Et un zoom hors bornes est ramene dedans.
    #[test]
    fn un_souvenir_incoherent_est_ecarte() {
        assert!(Souvenir::de_json(r#"{"largeur":0,"hauteur":950,"vue":"direct"}"#).is_none());
        assert!(Souvenir::de_json(r#"{"largeur":1250,"hauteur":9"#).is_none());
        assert!(Souvenir::de_json(r#"{"largeur":1250,"hauteur":950,"vue":"ailleurs:x"}"#).is_none());
        let s = Souvenir::de_json(r#"{"largeur":1250,"hauteur":950,"vue":"direct","zoom":9}"#).unwrap();
        assert_eq!(s.zoom, 2.4);
        assert_eq!(s.position, None);
        assert_eq!(s.theme, crate::palette::Reglage::Sombre);
        assert!(!s.barre_masquee);
    }

    /// La barre masquee a la main se retrouve masquee, visible se retrouve
    /// visible : les deux valeurs, pour qu'un souvenir qui ignorerait le champ
    /// (et rendrait toujours l'une) tombe sur l'autre.
    #[test]
    fn la_barre_se_retrouve_comme_on_l_a_laissee() {
        for masquee in [true, false] {
            let s = Souvenir {
                taille: (958, 981),
                position: None,
                zoom: 1.0,
                vue: Vue::Direct,
                choix: Choix::Direct,
                theme: crate::palette::Reglage::Sombre,
                barre_masquee: masquee,
            };
            assert_eq!(Souvenir::de_json(&s.en_json()).map(|x| x.barre_masquee), Some(masquee));
        }
    }
}
