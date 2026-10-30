//! Les dates du catalogue, en heure locale, sans caisse de dates.
//!
//! Une liste de cent chantiers ne se lit que rangee par jour : c'est ainsi que
//! Mail ou Photos rendent lisible ce qui s'accumule. Il faut donc le jour
//! civil d'un instant, son nom, et l'heure courte. Une caisse entiere pour
//! trente lignes n'en vaut pas la peine ; le decalage local est celui que le
//! fil de suivi lit deja pour ses heures.

use std::time::{SystemTime, UNIX_EPOCH};

const JOURS: [&str; 7] = ["lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche"];
const JOURS_EN: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
const MOIS_EN: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July", "August", "September", "October",
    "November", "December",
];
const MOIS: [&str; 12] = [
    "janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre",
    "novembre", "décembre",
];

/// L'annee, le mois et le jour d'un nombre de jours depuis le 1er janvier
/// 1970. C'est l'algorithme de Howard Hinnant (« chrono-Compatible Low-Level
/// Date Algorithms ») : exact sur tout le calendrier gregorien, sans table.
pub fn civil(jours: i64) -> (i64, u32, u32) {
    let z = jours + 719_468;
    let ere = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let jde = z - ere * 146_097;
    let ade = (jde - jde / 1460 + jde / 36_524 - jde / 146_096) / 365;
    let jda = jde - (365 * ade + ade / 4 - ade / 100);
    let mp = (5 * jda + 2) / 153;
    let j = (jda - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let a = ade + ere * 400 + i64::from(m <= 2);
    (a, m, j)
}

/// L'inverse de `civil` : le nombre de jours depuis le 1er janvier 1970 d'une
/// date du calendrier gregorien. Meme source, meme exactitude.
pub fn jours_depuis_1970(a: i64, m: u32, j: u32) -> i64 {
    let a = a - i64::from(m <= 2);
    let ere = (if a >= 0 { a } else { a - 399 }) / 400;
    let ade = a - ere * 400;
    let mp = i64::from(if m > 2 { m - 3 } else { m + 9 });
    let jda = (153 * mp + 2) / 5 + i64::from(j) - 1;
    let jde = ade * 365 + ade / 4 - ade / 100 + jda;
    ere * 146_097 + jde - 719_468
}

/// L'instant d'un horodatage de transcription, « 2026-10-27T13:52:07.250Z ».
/// Il sert a dire si un fichier a ete ecrit pendant qu'un agent travaillait.
pub fn instant_iso(ts: &str) -> Option<SystemTime> {
    let n = |a: usize, b: usize| ts.get(a..b)?.parse::<i64>().ok();
    let jours = jours_depuis_1970(n(0, 4)?, n(5, 7)? as u32, n(8, 10)? as u32);
    let s = jours * 86_400 + n(11, 13)? * 3600 + n(14, 16)? * 60 + n(17, 19)?;
    // Les millisecondes, quand elles y sont : « .250 ».
    let ms = if ts.get(19..20) == Some(".") { n(20, 23).unwrap_or(0) } else { 0 };
    let total = std::time::Duration::from_millis((s * 1000 + ms).max(0) as u64);
    Some(UNIX_EPOCH + total)
}

/// Le jour local (en jours depuis 1970) et la minute du jour d'un instant.
pub fn jour_et_minute(t: SystemTime) -> (i64, u32) {
    let s = match t.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs() as i64),
    };
    let locale = s + i64::from(crate::suivi::decalage_horaire_heures()) * 3600;
    (locale.div_euclid(86_400), (locale.rem_euclid(86_400) / 60) as u32)
}

/// « 01:09 ».
pub fn heure(minute: u32) -> String {
    format!("{:02}:{:02}", minute / 60, minute % 60)
}

/// Le titre d'une section de la liste. Les deux jours qu'on vit se nomment ;
/// les autres se datent, a la francaise (« Jeudi 1er octobre »), et l'annee
/// ne s'ecrit que si ce n'est pas celle d'aujourd'hui.
pub fn libelle_jour(jour: i64, aujourdhui: i64) -> String {
    match aujourdhui - jour {
        0 => return crate::langue::tr("Aujourd’hui", "Today").to_string(),
        1 => return crate::langue::tr("Hier", "Yesterday").to_string(),
        _ => {}
    }
    let (a, m, j) = civil(jour);
    if !crate::langue::fr() {
        // « Thursday, October 1 », puis l'annee si ce n'est pas celle d'aujourd'hui.
        let mut s = format!("{}, {} {j}", JOURS_EN[(jour + 3).rem_euclid(7) as usize], MOIS_EN[(m - 1) as usize]);
        if a != civil(aujourdhui).0 {
            s.push_str(&format!(", {a}"));
        }
        return s;
    }
    // Le 1er janvier 1970 etait un jeudi.
    let nom = JOURS[(jour + 3).rem_euclid(7) as usize];
    let quantieme = if j == 1 { "1er".to_string() } else { j.to_string() };
    // Un titre commence par une capitale, comme « Aujourd'hui » et « Hier »
    // au-dessus de lui ; le reste suit l'usage francais, en minuscules.
    let mut s = format!("{}{} {quantieme} {}", nom[..1].to_uppercase(), &nom[1..], MOIS[(m - 1) as usize]);
    if a != civil(aujourdhui).0 {
        s.push_str(&format!(" {a}"));
    }
    s
}

#[cfg(test)]
mod essais {
    use super::*;

    /// Les reponses viennent d'ailleurs que du code teste : le module datetime
    /// de Python, interroge le 2026-10-03.
    #[test]
    fn le_calendrier_tombe_juste() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(11_016), (2000, 2, 29));
        assert_eq!(civil(20_453), (2025, 12, 31));
        assert_eq!(civil(20_454), (2026, 1, 1));
        assert_eq!(civil(20_729), (2026, 10, 3));
        assert_eq!(civil(-1), (1969, 12, 31));
    }

    #[test]
    fn les_jours_se_nomment_a_la_francaise() {
        let aujourdhui = 20_729; // samedi 3 octobre 2026
        assert_eq!(libelle_jour(20_729, aujourdhui), "Aujourd’hui");
        assert_eq!(libelle_jour(20_728, aujourdhui), "Hier");
        assert_eq!(libelle_jour(20_727, aujourdhui), "Jeudi 1er octobre");
        assert_eq!(libelle_jour(20_454, aujourdhui), "Jeudi 1er janvier");
        assert_eq!(libelle_jour(20_453, aujourdhui), "Mercredi 31 décembre 2025");
    }

    /// Les secondes depuis 1970 viennent de datetime.fromisoformat de Python,
    /// interroge le 2026-10-27.
    #[test]
    fn un_horodatage_devient_un_instant() {
        let s = |ts: &str| instant_iso(ts).unwrap().duration_since(UNIX_EPOCH).unwrap().as_millis();
        assert_eq!(s("2026-10-27T13:52:07.250Z"), 1_793_109_127_250);
        assert_eq!(s("1970-01-01T00:00:00Z"), 0);
        assert_eq!(s("2000-02-29T23:59:59.999Z"), 951_868_799_999);
        assert!(instant_iso("hier").is_none());
        for jours in [-1, 0, 11_016, 20_453, 20_729] {
            let (a, m, j) = civil(jours);
            assert_eq!(jours_depuis_1970(a, m, j), jours);
        }
    }

    #[test]
    fn l_heure_courte() {
        assert_eq!(heure(69), "01:09");
        assert_eq!(heure(23 * 60 + 59), "23:59");
    }

    #[test]
    fn les_titres_de_jour_se_disent_dans_la_langue_courante() {
        use crate::langue::{avec, Langue};
        let aujourdhui = jours_depuis_1970(2026, 10, 3);
        let jeudi_1er = jours_depuis_1970(2026, 10, 1);
        assert_eq!(avec(Langue::En, || libelle_jour(aujourdhui, aujourdhui)), "Today");
        assert_eq!(avec(Langue::En, || libelle_jour(aujourdhui - 1, aujourdhui)), "Yesterday");
        assert_eq!(avec(Langue::En, || libelle_jour(jeudi_1er, aujourdhui)), "Thursday, October 1");
        assert_eq!(avec(Langue::En, || libelle_jour(jours_depuis_1970(2025, 12, 31), aujourdhui)), "Wednesday, December 31, 2025");
        assert_eq!(avec(Langue::Fr, || libelle_jour(jeudi_1er, aujourdhui)), "Jeudi 1er octobre");
    }
}
