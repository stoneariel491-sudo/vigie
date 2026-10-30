//! La langue de l'interface : l'anglais par defaut, le francais sur demande.
//!
//! `VIGIE_LANGUE=fr` (ou `en`) tranche ; sinon la langue du systeme, lue dans
//! `LC_ALL`, `LC_MESSAGES`, `LANG` (ou, sous Windows, la langue d'affichage), et
//! l'anglais pour tout ce qui n'est pas le francais. Un texte se dit par
//! `tr("en francais", "in English")`, ou par `tf!` quand il porte des valeurs.

use std::sync::OnceLock;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Langue {
    Fr,
    En,
}

fn depuis_texte(t: &str) -> Option<Langue> {
    let t = t.trim().to_ascii_lowercase();
    if t.is_empty() || t == "c" || t == "posix" {
        None
    } else if t.starts_with("fr") {
        Some(Langue::Fr)
    } else {
        Some(Langue::En)
    }
}

fn detecter() -> Langue {
    if let Some(l) = std::env::var("VIGIE_LANGUE").ok().and_then(|v| depuis_texte(&v)) {
        return l;
    }
    for cle in ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"] {
        if let Some(l) = std::env::var(cle).ok().and_then(|v| depuis_texte(&v)) {
            return l;
        }
    }
    #[cfg(windows)]
    {
        if let Some(l) = windows_langue() {
            return l;
        }
    }
    Langue::En
}

/// La langue d'affichage de Windows, par `GetUserDefaultLocaleName` (« fr-CA »).
#[cfg(windows)]
fn windows_langue() -> Option<Langue> {
    extern "system" {
        fn GetUserDefaultLocaleName(nom: *mut u16, taille: i32) -> i32;
    }
    let mut tampon = [0u16; 85];
    let n = unsafe { GetUserDefaultLocaleName(tampon.as_mut_ptr(), tampon.len() as i32) };
    if n <= 1 {
        return None;
    }
    depuis_texte(&String::from_utf16_lossy(&tampon[..(n as usize - 1)]))
}

static CHOIX: OnceLock<Langue> = OnceLock::new();

#[cfg(test)]
thread_local! {
    /// Un essai peut imposer sa langue sans toucher a celle des autres.
    static IMPOSEE: std::cell::Cell<Option<Langue>> = const { std::cell::Cell::new(None) };
}

pub fn courante() -> Langue {
    #[cfg(test)]
    if let Some(l) = IMPOSEE.with(|c| c.get()) {
        return l;
    }
    // Les essais attendent du francais, sauf s'ils imposent une langue.
    #[cfg(test)]
    return Langue::Fr;
    #[cfg(not(test))]
    *CHOIX.get_or_init(detecter)
}

pub fn fr() -> bool {
    courante() == Langue::Fr
}

/// Exécute `f` dans la langue donnée (essais seulement).
#[cfg(test)]
pub fn avec<T>(l: Langue, f: impl FnOnce() -> T) -> T {
    let avant = IMPOSEE.with(|c| c.replace(Some(l)));
    let r = f();
    IMPOSEE.with(|c| c.set(avant));
    r
}

/// Un texte fixe, dans la langue courante.
pub fn tr(francais: &'static str, anglais: &'static str) -> &'static str {
    if fr() {
        francais
    } else {
        anglais
    }
}

/// Un texte a valeurs, dans la langue courante :
/// `tf!("{} gestes", "{} steps", n)`.
#[macro_export]
macro_rules! tf {
    ($fr:literal, $en:literal $(, $a:expr)* $(,)?) => {
        if $crate::langue::fr() { format!($fr $(, $a)*) } else { format!($en $(, $a)*) }
    };
}

#[cfg(test)]
mod essais {
    use super::*;

    #[test]
    fn la_langue_se_lit_d_un_texte() {
        assert_eq!(depuis_texte("fr_CA.UTF-8"), Some(Langue::Fr));
        assert_eq!(depuis_texte("fr"), Some(Langue::Fr));
        assert_eq!(depuis_texte("en_US.UTF-8"), Some(Langue::En));
        assert_eq!(depuis_texte("de_DE"), Some(Langue::En));
        assert_eq!(depuis_texte("C"), None);
        assert_eq!(depuis_texte(""), None);
    }

    #[test]
    fn un_texte_suit_la_langue_imposee() {
        assert_eq!(avec(Langue::Fr, || tr("bonjour", "hello")), "bonjour");
        assert_eq!(avec(Langue::En, || tr("bonjour", "hello")), "hello");
        assert_eq!(avec(Langue::En, || tf!("{} gestes", "{} steps", 3)), "3 steps");
        assert_eq!(avec(Langue::Fr, || tf!("{} gestes", "{} steps", 3)), "3 gestes");
    }
}
