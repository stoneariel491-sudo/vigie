//! Les couleurs, et le sens de chacune, dans les deux themes.
//!
//! Les valeurs sont celles du systeme du tour 00 (`design/tours/tour-00/
//! outils/palette.json`) : trois encres et pas quatre, chacune a son seuil sur
//! son fond, mesure sur les pixels. Aucune teinte dans l'interface : les seules
//! couleurs de la fenetre sont celles des images des agents, et l'alerte, qui
//! n'est jamais seule (un mot dit l'etat).
//!
//! Le theme est global et se lit a chaque trame : le changer redessine tout
//! sans rien remettre en page, puisque la couleur ne change aucune mesure.

use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};

use glyphon::Color;

/// Le theme reglé : sombre, clair, ou celui du systeme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reglage {
    Sombre,
    Clair,
    Systeme,
}

impl Reglage {
    pub fn nomme(n: &str) -> Option<Reglage> {
        Some(match n.trim() {
            "sombre" => Reglage::Sombre,
            "clair" => Reglage::Clair,
            "systeme" | "système" => Reglage::Systeme,
            _ => return None,
        })
    }

    pub fn nom(self) -> &'static str {
        match self {
            Reglage::Sombre => "sombre",
            Reglage::Clair => "clair",
            Reglage::Systeme => "systeme",
        }
    }

    /// Le suivant, pour la touche qui fait tourner le reglage.
    pub fn suivant(self) -> Reglage {
        match self {
            Reglage::Sombre => Reglage::Clair,
            Reglage::Clair => Reglage::Systeme,
            Reglage::Systeme => Reglage::Sombre,
        }
    }
}

static CLAIR: AtomicU8 = AtomicU8::new(0);

/// Pose le theme effectif.
pub fn poser_clair(clair: bool) {
    CLAIR.store(clair as u8, Ordering::Relaxed);
}

pub fn clair() -> bool {
    CLAIR.load(Ordering::Relaxed) == 1
}

/// Les encres posees sur le verre de la visionneuse. Le verre est sombre dans
/// les deux themes (une teinte noire de 0,80 sur la vue floutee, comme le
/// givre de veilleuse) : ses textes gardent les encres du theme sombre.
pub const PAROLE: Color = rgb(0xF2EFE9);
pub const PENSEE: Color = rgb(0xA3A09B);

const fn rgb(v: u32) -> Color {
    Color::rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

fn choisir(sombre: u32, clair_: u32) -> Color {
    rgb(if clair() { clair_ } else { sombre })
}

/// Le contraste eleve : les couleurs que Windows impose, la fenetre, son
/// texte et la surbrillance. Pose, il remplace le theme : les trois encres
/// deviennent le texte du systeme, le filet aussi, et la plaque un melange de
/// la fenetre et de la surbrillance. Toutes les encres se valant, ce qui
/// distinguait par l'encre se dit alors par un signe (le rail).
static CONTRASTE: AtomicU8 = AtomicU8::new(0);
static SYS_FOND: AtomicU32 = AtomicU32::new(0x000000);
static SYS_TEXTE: AtomicU32 = AtomicU32::new(0xFFFFFF);
static SYS_SURBRILLANCE: AtomicU32 = AtomicU32::new(0x1AEBFF);

pub fn poser_contraste(actif: bool) {
    if actif {
        let (fond, texte, surbrillance) = couleurs_du_systeme();
        SYS_FOND.store(fond, Ordering::Relaxed);
        SYS_TEXTE.store(texte, Ordering::Relaxed);
        SYS_SURBRILLANCE.store(surbrillance, Ordering::Relaxed);
    }
    CONTRASTE.store(actif as u8, Ordering::Relaxed);
}

pub fn contraste() -> bool {
    CONTRASTE.load(Ordering::Relaxed) == 1
}

fn sys(c: &AtomicU32) -> Color {
    rgb(c.load(Ordering::Relaxed))
}

/// Le reglage « Contraste eleve » de Windows, lu par SPI_GETHIGHCONTRAST.
#[cfg(windows)]
pub fn contraste_du_systeme() -> bool {
    #[repr(C)]
    struct HighContrastW {
        taille: u32,
        drapeaux: u32,
        schema: *mut u16,
    }
    extern "system" {
        fn SystemParametersInfoW(action: u32, param: u32, valeur: *mut core::ffi::c_void, maj: u32) -> i32;
    }
    const SPI_GETHIGHCONTRAST: u32 = 0x0042;
    const HCF_HIGHCONTRASTON: u32 = 1;
    let mut hc = HighContrastW { taille: std::mem::size_of::<HighContrastW>() as u32, drapeaux: 0, schema: std::ptr::null_mut() };
    let ok = unsafe { SystemParametersInfoW(SPI_GETHIGHCONTRAST, hc.taille, &mut hc as *mut HighContrastW as *mut _, 0) };
    ok != 0 && hc.drapeaux & HCF_HIGHCONTRASTON != 0
}

#[cfg(not(windows))]
pub fn contraste_du_systeme() -> bool {
    false
}

/// La fenetre, son texte et la surbrillance, par GetSysColor, en 0xRRGGBB.
#[cfg(windows)]
fn couleurs_du_systeme() -> (u32, u32, u32) {
    extern "system" {
        fn GetSysColor(index: i32) -> u32;
    }
    const COLOR_WINDOW: i32 = 5;
    const COLOR_WINDOWTEXT: i32 = 8;
    const COLOR_HIGHLIGHT: i32 = 13;
    // GetSysColor rend 0x00BBGGRR.
    let rvb = |c: u32| ((c & 0xFF) << 16) | (c & 0xFF00) | ((c >> 16) & 0xFF);
    unsafe { (rvb(GetSysColor(COLOR_WINDOW)), rvb(GetSysColor(COLOR_WINDOWTEXT)), rvb(GetSysColor(COLOR_HIGHLIGHT))) }
}

#[cfg(not(windows))]
fn couleurs_du_systeme() -> (u32, u32, u32) {
    (0x000000, 0xFFFFFF, 0x1AEBFF)
}

/// La page.
pub fn fond_octets() -> Color {
    if contraste() {
        return sys(&SYS_FOND);
    }
    choisir(0x0B0B0A, 0xF6F4F0)
}

/// Ce qui s'adresse : la parole, les noms, les titres. 7:1 au moins.
pub fn encre1() -> Color {
    if contraste() {
        return sys(&SYS_TEXTE);
    }
    choisir(0xF2EFE9, 0x211E19)
}

/// Ce qui se lit ensuite : bilans, descriptions, pensee. 4,5:1, 7:1 vise.
pub fn encre2() -> Color {
    if contraste() {
        return sys(&SYS_TEXTE);
    }
    choisir(0xA3A09B, 0x514E49)
}

/// Ce qui se repere : gestes, heures, meta, legendes, onglets. 4,5:1.
pub fn encre3() -> Color {
    if contraste() {
        return sys(&SYS_TEXTE);
    }
    // Le clair s'ecarte du systeme (#6B6863) : a l'echelle 1, le lissage
    // pali le coeur des glyphes de 13 points jusqu'a 4,12:1 sur ses pixels
    // (partisan-console.md, mesures). #63605B en garde 5,70 ecrits.
    choisir(0x83807B, 0x63605B)
}

/// Ce qui est choisi ou survole. Ne dit jamais seul un etat.
pub fn plaque() -> Color {
    if contraste() {
        // Deux cinquiemes de surbrillance sur la fenetre, melanges octet par
        // octet : assez pour se voir, assez peu pour que le texte s'y lise.
        let (f, b) = (SYS_FOND.load(Ordering::Relaxed), SYS_SURBRILLANCE.load(Ordering::Relaxed));
        let m = |k: u32| {
            let (x, y) = ((f >> k) & 0xFF, (b >> k) & 0xFF);
            ((x * 3 + y * 2) / 5) << k
        };
        return rgb(m(16) | m(8) | m(0));
    }
    choisir(0x33312F, 0xE7E4DE)
}

/// Ce qui separe, sans rien dire.
pub fn filet() -> Color {
    if contraste() {
        return sys(&SYS_TEXTE);
    }
    choisir(0x33312F, 0xD9D5CE)
}

/// Demande quelqu'un : interrompu, en erreur. Toujours avec un mot.
pub fn alerte() -> Color {
    if contraste() {
        return sys(&SYS_TEXTE);
    }
    choisir(0xFF8F80, 0xB42318)
}

/// La luminance relative d'un octet sRGB (WCAG 2.x).
fn luminance(c: Color) -> f32 {
    let [r, g, b, _] = lineaire(c, 1.0);
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// Le contraste que doit tenir un signe d'etat, au plus pale de son souffle :
/// 3:1, ce que WCAG 1.4.11 demande a un element graphique, et un dixieme de
/// marge pour le lissage du bord d'un disque de 9 points.
pub const SOUFFLE_CONTRASTE: f32 = 3.3;

/// L'opacite la plus basse de la pastille qui respire. Elle se melange au fond
/// en lumiere lineaire (la surface est sRGB), donc sa luminance est
/// a·Le + (1 − a)·Lf, et l'on resout pour SOUFFLE_CONTRASTE. Le sombre garde
/// son ancien creux, 0,30, qui tient deja 5,85:1 ; le clair monte a 0,75 : a
/// 0,30 il tombait a 1,39:1 (juge-design.md, M2).
pub fn plancher_souffle() -> f32 {
    let (le, lf) = (luminance(encre1()), luminance(fond_octets()));
    let voulu = if le > lf {
        SOUFFLE_CONTRASTE * (lf + 0.05) - 0.05
    } else {
        (lf + 0.05) / SOUFFLE_CONTRASTE - 0.05
    };
    let a = if (le - lf).abs() < 1e-6 { 1.0 } else { (voulu - lf) / (le - lf) };
    a.clamp(0.30, 1.0)
}

/// Le fond, en lineaire, pour l'effacement de la passe.
pub fn fond() -> wgpu::Color {
    let [r, g, b, a] = lineaire(fond_octets(), 1.0);
    wgpu::Color { r: r as f64, g: g as f64, b: b as f64, a: a as f64 }
}

/// Une couleur de la charte, convertie en LINEAIRE pour les aplats.
///
/// Le texte passe par glyphon, qui fait cette conversion lui-meme ; les formes
/// ecrivent directement dans une surface sRGB, donc c'est a nous. Poser un
/// octet tel quel donnerait un filet beaucoup trop clair, et c'est l'erreur
/// qu'on ne voit pas parce qu'elle a l'air d'un choix.
pub fn lineaire(c: Color, alpha: f32) -> [f32; 4] {
    let d = |v: u8| {
        let s = v as f32 / 255.0;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    [d(c.r()), d(c.g()), d(c.b()), alpha]
}

#[cfg(test)]
mod essais {
    use super::*;

    /// Le contraste WCAG 2.x de deux octets sRGB, refait ici par la formule
    /// du W3C, sans passer par `lineaire` : un second chemin.
    fn contraste(a: Color, b: Color) -> f64 {
        let l = |c: Color| {
            let v = |x: u8| {
                let s = x as f64 / 255.0;
                if s <= 0.03928 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
            };
            0.2126 * v(c.r()) + 0.7152 * v(c.g()) + 0.0722 * v(c.b())
        };
        let (x, y) = (l(a), l(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    /// Chaque encre tient son seuil sur son fond, dans les deux themes : 7
    /// pour encre-1, 4,5 pour les autres. Le temoin : l'ancien gris des
    /// gestes, `#5E5B57`, doit tomber sous 4,5 sur le fond sombre.
    /// Le theme est un etat global : les essais qui le changent passent l'un
    /// apres l'autre.
    static THEME: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn les_encres_tiennent_leur_seuil() {
        let _g = THEME.lock().unwrap_or_else(|e| e.into_inner());
        for c in [false, true] {
            poser_clair(c);
            let f = fond_octets();
            assert!(contraste(encre1(), f) >= 7.0, "encre-1, clair={c}");
            assert!(contraste(encre2(), f) >= 4.5, "encre-2, clair={c}");
            assert!(contraste(encre3(), f) >= 4.5, "encre-3, clair={c}");
            assert!(contraste(alerte(), f) >= 4.5, "alerte, clair={c}");
        }
        poser_clair(false);
        assert!(contraste(Color::rgb(0x5E, 0x5B, 0x57), fond_octets()) < 4.5);
    }

    /// Au creux de son souffle, la pastille tient 3:1 sur le fond dans les
    /// deux themes. Le melange est refait ici octet par octet, sur la formule
    /// du W3C, et non par `luminance`. Le temoin : l'ancien creux de 0,30 doit
    /// tomber sous 3:1 au clair.
    #[test]
    fn la_pastille_reste_visible_au_creux() {
        let _g = THEME.lock().unwrap_or_else(|e| e.into_inner());

        let melange = |a: f32| {
            let (e, f) = (lineaire(encre1(), 1.0), lineaire(fond_octets(), 1.0));
            let s = |x: f32| {
                let v = if x <= 0.0031308 { 12.92 * x } else { 1.055 * x.powf(1.0 / 2.4) - 0.055 };
                (v * 255.0).round().clamp(0.0, 255.0) as u8
            };
            let m = |i: usize| s(a * e[i] + (1.0 - a) * f[i]);
            Color::rgb(m(0), m(1), m(2))
        };
        for c in [false, true] {
            poser_clair(c);
            let bas = plancher_souffle();
            assert!(contraste(melange(bas), fond_octets()) >= 3.0, "creux {bas}, clair={c}");
        }
        poser_clair(true);
        assert!(contraste(melange(0.30), fond_octets()) < 3.0);
        poser_clair(false);
    }
}
