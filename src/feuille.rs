//! La feuille du passage : ce qui s'ouvre au clic sur un lien, avant la page.
//!
//! Direction « le passage d'abord, la page ensuite ». Le clic ouvre une feuille
//! posee sur le fil : l'hote reel, le passage que l'agent a cite en grand dans
//! la typographie de la vigie, ce qui l'entoure dans la page, et ce que l'agent
//! demandait. La page entiere ne vient que sur un second geste, et la feuille
//! devient alors le cadre autour d'elle.
//!
//! Ce module ne dessine rien : il compose (textes mis en page, aplats, zones
//! cliquables) et `main.rs` pose le resultat dans la trame. Tout est en pixels
//! d'ecran, l'echelle `e` deja appliquee. La composition ne depend que de
//! l'entree et de la taille ; elle se garde tant qu'elles ne changent pas.

use glyphon::{Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, Weight};

use crate::formes::Forme;
use crate::palette;
use crate::passage::{self, Classe, Echec, Etat, Extrait, Page, Trouve};
use crate::plan::Boite;
use crate::vues;

/// Ce que la feuille montre : le passage, ou la page ouverte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Vue {
    Passage,
    Page,
}

/// Ce que chaque zone cliquable fait.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Fermer,
    OuvrirPage,
    Reessayer,
    Reculer,
    Avancer,
    RevoirPassage,
    ConfirmerLocal,
    OuvrirAilleurs,
}

pub use crate::navigateur::Sonde;

pub struct Entree<'a> {
    pub url: &'a str,
    /// L'adresse que la page affiche vraiment, quand Stone a suivi un lien : le
    /// bandeau d'adresse dit toujours l'hote reel, jamais celui du clic.
    pub courante: Option<&'a str>,
    pub etat: &'a Etat,
    pub page: Option<&'a Page>,
    pub vue: Vue,
    /// Les secondes depuis le clic : le chargement les dit, il ne tourne pas.
    pub ecoule: f32,
    /// Ce que la page ouverte a repondu a la recherche du passage.
    pub sonde: Sonde,
    /// Le moteur garde-t-il une page apres celle-ci (on a recule) : « Avancer »
    /// n'est une cible que dans ce cas, sinon il est grise.
    pub peut_avancer: bool,
    /// Ce que la page vivante a repondu la derniere fois qu'elle a ete ouverte
    /// sur ce lien (`Trouve` ou `NonRetrouve`), gardee quand la page se referme :
    /// l'extrait, lu sans script, ne dit pas « cite » sans ce qu'elle a repondu.
    pub verdict: Sonde,
}

pub struct Texte {
    pub tampon: Buffer,
    pub x: f32,
    pub y: f32,
    pub couleur: Color,
}

impl Texte {
    /// La boite d'encre du texte : de quoi mesurer son contraste sur la capture.
    pub fn boite(&self) -> Boite {
        let mut l = 0.0f32;
        let mut h = 0.0f32;
        for r in self.tampon.layout_runs() {
            l = l.max(r.line_w);
            h = h.max(r.line_top + r.line_height);
        }
        Boite { x: self.x, y: self.y, l, h }
    }

    /// Les premiers signes, pour le journal.
    pub fn debut(&self) -> String {
        self.tampon.lines.iter().map(|l| l.text()).collect::<Vec<_>>().join(" ").chars().take(28).collect()
    }
}

pub struct Composee {
    /// Sous les textes : voile, carte, surbrillance, filets, boutons.
    pub aplats: Vec<Forme>,
    pub textes: Vec<Texte>,
    pub cibles: Vec<(Boite, Action)>,
    pub carte: Boite,
    /// Le rectangle de la page, quand la page est ouverte.
    pub zone_page: Option<Boite>,
    /// La surbrillance : les lignes du passage, pour la mesure.
    pub surbrillance: Vec<Boite>,
    /// Le bloc du passage, bord compris.
    pub bloc: Option<Boite>,
}

// Les mesures nominales, en points a l'echelle 1. Aucun corps nouveau : ceux
// du systeme (20, 15, 13, 12).
const CARTE_MAX: f32 = 620.0;
/// En dessous de cette largeur (en points), la citation du bandeau de la page
/// passe sur sa propre rangee.
const LARGEUR_CITATION_MIN: f32 = 240.0;
const GOUTTIERE: f32 = 28.0;
const GOUTTIERE_ETROITE: f32 = 16.0;
const MARGE: f32 = 24.0;
const MARGE_ETROITE: f32 = 20.0;
const BOUTON: f32 = 28.0;
const BOUTON_ACTION_H: f32 = 36.0;
const GRAND: f32 = 20.0;
const GRAND_H: f32 = 30.0;
const LECTURE: f32 = 15.0;
const LECTURE_H: f32 = 23.0;
const REPERE: f32 = 13.0;
const REPERE_H: f32 = 18.0;
const PETIT: f32 = 12.0;
const PETIT_H: f32 = 16.0;
pub const SECONDE_LENTE: f32 = 10.0;

/// L'ambre du passage cite : la seule teinte que le navigateur ajoute.
const AMBRE: Color = Color::rgb(0xFF, 0xC8, 0x57);

fn barlow(poids: Weight) -> Attrs<'static> {
    Attrs::new().family(Family::Name("Barlow")).weight(poids)
}

fn poser_bloc(systeme: &mut FontSystem, texte: &str, corps: f32, haut: f32, poids: Weight, largeur: f32) -> (Buffer, f32) {
    let mut b = Buffer::new(systeme, Metrics::new(corps, haut));
    b.set_size(Some(largeur.max(1.0)), None);
    b.set_text(texte, &barlow(poids), Shaping::Advanced, None);
    b.shape_until_scroll(systeme, false);
    let lignes = b.layout_runs().count().max(1) as f32;
    (b, lignes * haut)
}

/// Une ligne qui ne se replie pas : sa largeur d'encre.
fn mesurer(systeme: &mut FontSystem, texte: &str, corps: f32, haut: f32, poids: Weight) -> (Buffer, f32) {
    let mut b = Buffer::new(systeme, Metrics::new(corps, haut));
    b.set_size(None, None);
    b.set_text(texte, &barlow(poids), Shaping::Advanced, None);
    b.shape_until_scroll(systeme, false);
    let l = b.layout_runs().map(|r| r.line_w).fold(0.0, f32::max);
    (b, l)
}

/// Les boites, en pixels depuis le coin haut-gauche du tampon, de ce qu'occupe
/// l'intervalle d'octets [debut, fin) du texte : une par ligne que l'intervalle
/// traverse. Un lien coupe sur deux lignes en a deux.
pub fn boites(tampon: &Buffer, texte: &str, debut: usize, fin: usize) -> Vec<Boite> {
    let mut departs: Vec<usize> = Vec::new();
    let mut o = 0;
    for segment in texte.split('\n') {
        departs.push(o);
        o += segment.len() + 1;
    }
    let mut sortie = Vec::new();
    for run in tampon.layout_runs() {
        let base = departs.get(run.line_i).copied().unwrap_or(0);
        let (mut x0, mut x1) = (f32::MAX, f32::MIN);
        for g in run.glyphs {
            if base + g.start < fin && base + g.end > debut {
                x0 = x0.min(g.x);
                x1 = x1.max(g.x + g.w);
            }
        }
        if x1 > x0 {
            sortie.push(Boite { x: x0, y: run.line_top, l: x1 - x0, h: run.line_height });
        }
    }
    sortie
}

fn forme(boite: [f32; 4], c: Color, alpha: f32, rayon: f32) -> Forme {
    Forme { boite: [boite[0].round(), boite[1].round(), boite[2].round(), boite[3].round()], couleur: palette::lineaire(c, alpha), rayon: [rayon, 0.0, 0.0, 0.0] }
}

/// Le bord du passage : celui qui contraste le plus avec le fond lu. La carte
/// est sombre en theme sombre, claire en theme clair.
fn bord_du_passage() -> Color {
    if palette::contraste() {
        return palette::encre1();
    }
    if palette::clair() { Color::rgb(0x1A, 0x1A, 0x1A) } else { Color::rgb(0xF2, 0xEF, 0xE9) }
}

/// La transparence de l'ambre. Les aplats se melangent en lineaire, ou 0,40
/// eclaircit bien plus qu'en sRGB : le sombre est cale pour que le texte
/// garde 4,5:1 sur la surbrillance (mesure sur la capture, voir le suivi).
fn alpha_ambre() -> f32 {
    if palette::contraste() {
        0.0
    } else if palette::clair() {
        0.60
    } else {
        0.17
    }
}

struct Corps<'a> {
    systeme: &'a mut FontSystem,
    e: f32,
    x: f32,
    l: f32,
    textes: Vec<Texte>,
    aplats: Vec<Forme>,
    cibles: Vec<(Boite, Action)>,
}

impl<'a> Corps<'a> {
    fn texte(&mut self, t: &str, corps: f32, haut: f32, poids: Weight, c: Color, y: f32) -> f32 {
        let (b, h) = poser_bloc(self.systeme, t, corps * self.e, haut * self.e, poids, self.l);
        self.textes.push(Texte { tampon: b, x: self.x, y, couleur: c });
        h
    }
}

/// Coupe pour tenir sur `n` lignes : le texte est deja borne en signes par
/// l'extraction ; ceci ne sert qu'aux phrases de l'agent, d'une longueur libre.
/// Deux traits d'union de suite se fondent en un seul trait long dans la police
/// de la vigie : `xn--pple` se lirait `xn—pple`. Une espace de largeur nulle
/// entre eux garde chaque trait.
fn sans_trait_fondu(s: &str) -> String {
    s.replace("--", "-\u{200c}-")
}

fn borner(s: &str, n: usize) -> String {
    let c: Vec<char> = s.chars().collect();
    if c.len() <= n {
        return s.to_string();
    }
    let mut f = n;
    while f > 0 && !c[f].is_whitespace() {
        f -= 1;
    }
    format!("{}…", c[..f].iter().collect::<String>().trim_end())
}

fn kicker(etat: &Etat, verdict: Sonde) -> (&'static str, bool) {
    match (etat, verdict) {
        (Etat::Passage(_) | Etat::Introuvable { .. }, Sonde::NonRetrouve) => return (crate::langue::tr("Passage non retrouvé dans la page", "Passage not found in the page"), false),
        (Etat::Introuvable { .. }, Sonde::Trouve) => return (crate::langue::tr("Passage retrouvé dans la page", "Passage found in the page"), false),
        _ => {}
    }
    match etat {
        Etat::Chargement => (crate::langue::tr("Lecture de la page", "Reading the page"), false),
        Etat::Passage(_) => (crate::langue::tr("Passage cité", "Quoted passage"), false),
        Etat::Introuvable { .. } => (crate::langue::tr("Passage introuvable sur cette page", "Passage not found on this page"), false),
        Etat::SansPassage { .. } => (crate::langue::tr("Aucun passage cité", "No quoted passage"), false),
        Etat::Echec(_) => (crate::langue::tr("La page ne s’ouvre pas", "The page does not open"), true),
        Etat::HorsLigne => (crate::langue::tr("Pas de connexion", "No connection"), true),
        Etat::Connexion => (crate::langue::tr("La page demande une connexion", "The page asks for a login"), true),
        Etat::Fichier => (crate::langue::tr("Cette adresse est un fichier", "This address is a file"), true),
        Etat::Refuse(_) => (crate::langue::tr("Lien refusé", "Link refused"), true),
        Etat::Local => (crate::langue::tr("Ce lien vise cet ordinateur", "This link points to this computer"), true),
        Etat::HorsMaquette => (crate::langue::tr("Page non préparée", "Page not prepared"), false),
        Etat::Redirection { contenu, .. } => kicker(contenu, verdict),
    }
}

fn explication(etat: &Etat, hote: &str, ecoule: f32) -> Option<String> {
    Some(match etat {
        Etat::Chargement => {
            if ecoule >= SECONDE_LENTE {
                crate::tf!("{hote} répond lentement. Rien n’est ouvert : la vigie ne demande que le texte de la page.", "{hote} is slow to answer. Nothing is open: Vigie only asks for the text of the page.")
            } else {
                crate::langue::tr("La vigie demande le texte de la page, sans script ni cookie. Rien n’est ouvert.", "Vigie asks for the text of the page, with no script and no cookie. Nothing is open.").to_string()
            }
        }
        Etat::Echec(Echec::Absente) => crate::langue::tr("La page n’existe plus à cette adresse (erreur 404). L’agent l’a lue avant : elle a pu changer ou disparaître.", "The page no longer exists at this address (error 404). The agent read it earlier: it may have changed or disappeared.").to_string(),
        Etat::Echec(Echec::Certificat) => crate::tf!("Le certificat de {hote} n’est pas valide. La vigie ne continue pas.", "The certificate of {hote} is not valid. Vigie does not go on."),
        Etat::Echec(Echec::Delai) => crate::tf!("{hote} n’a pas répondu en {SECONDE_LENTE:.0} s.", "{hote} did not answer within {SECONDE_LENTE:.0} s."),
        Etat::Echec(Echec::Serveur(c)) => crate::tf!("{hote} a répondu {c} au lieu d’une page. L’agent l’a lue avant : elle a pu changer ou se fermer.", "{hote} answered {c} instead of a page. The agent read it earlier: it may have changed or closed."),
        Etat::HorsLigne => crate::langue::tr("Cet ordinateur n’a pas de connexion. Rien n’a été chargé.", "This computer has no connection. Nothing was loaded.").to_string(),
        Etat::Connexion => crate::langue::tr("Cette page exige un compte. La vigie n’en a aucun de vous et ne se connecte pas.", "This page requires an account. Vigie has none of yours and does not log in.").to_string(),
        Etat::Fichier => crate::langue::tr("Cette adresse envoie un fichier (PDF, archive ou programme). La vigie ne télécharge rien.", "This address sends a file (PDF, archive or program). Vigie downloads nothing.").to_string(),
        Etat::Refuse(s) => crate::tf!("Le schéma « {s}: » n’est jamais ouvert par la vigie, quelle que soit la page qui le cite.", "Vigie never opens the “{s}:” scheme, whatever page cites it."),
        Etat::Local => crate::tf!("Cette adresse vise un service de votre propre machine ({hote}). La vigie demande avant de l’ouvrir.", "This address points to a service on your own machine ({hote}). Vigie asks before opening it."),
        Etat::HorsMaquette => crate::langue::tr("Cette adresse n’est pas parmi les pages préparées pour la maquette. Au clic, la vigie lirait ici son texte.", "This address is not among the pages prepared for the mockup. On click, Vigie would read its text here.").to_string(),
        _ => return None,
    })
}

/// Compose la feuille. `l` et `h` sont la fenetre, en pixels.
pub fn composer(systeme: &mut FontSystem, entree: &Entree, l: f32, h: f32, e: f32) -> Composee {
    match entree.vue {
        Vue::Passage => composer_passage(systeme, entree, l, h, e),
        Vue::Page => composer_page(systeme, entree, l, h, e),
    }
}

fn composer_passage(systeme: &mut FontSystem, entree: &Entree, l: f32, h: f32, e: f32) -> Composee {
    let etroite = l < 640.0 * e;
    let gut = if etroite { GOUTTIERE_ETROITE } else { GOUTTIERE } * e;
    let marge = if etroite { MARGE_ETROITE } else { MARGE } * e;
    let cl = (CARTE_MAX * e).min(l - 2.0 * gut);
    let cx = ((l - cl) * 0.5).round();
    let iw = cl - 2.0 * marge;
    let ix = cx + marge;
    let a = passage::adresse(entree.url);
    let classe = passage::classer(entree.url);
    let (tete, alerte) = kicker(entree.etat, entree.verdict);

    let mut c = Corps { systeme, e, x: ix, l: iw, textes: Vec::new(), aplats: Vec::new(), cibles: Vec::new() };
    // Le contenu se compose depuis y = 0, puis la carte se pose ou il faut.
    let mut y = marge;

    // Le mot de l'etat, a gauche ; la sortie, a droite.
    let couleur_tete = if alerte { palette::alerte() } else { palette::encre1() };
    let ty = y + (BOUTON * e - REPERE_H * e) * 0.5;
    let (b_tete, l_tete) = mesurer(c.systeme, tete, REPERE * e, REPERE_H * e, Weight::MEDIUM);
    c.textes.push(Texte { tampon: b_tete, x: ix, y: ty, couleur: couleur_tete });
    if classe == Classe::NonChiffre {
        let (b, _) = mesurer(c.systeme, crate::langue::tr("non chiffré", "not encrypted"), REPERE * e, REPERE_H * e, Weight::MEDIUM);
        c.textes.push(Texte { tampon: b, x: ix + l_tete + 14.0 * e, y: ty, couleur: palette::alerte() });
    }
    let (b_esc, l_esc) = mesurer(c.systeme, crate::langue::tr("Échap", "Esc"), PETIT * e, PETIT_H * e, Weight::NORMAL);
    let fermer = Boite { x: ix + iw - BOUTON * e, y, l: BOUTON * e, h: BOUTON * e };
    let (b_x, l_x) = mesurer(c.systeme, "×", GRAND * e, GRAND_H * e, Weight::NORMAL);
    c.textes.push(Texte { tampon: b_x, x: fermer.x + (fermer.l - l_x) * 0.5, y: y + (BOUTON * e - GRAND_H * e) * 0.5, couleur: palette::encre2() });
    c.textes.push(Texte { tampon: b_esc, x: fermer.x - 8.0 * e - l_esc, y: y + (BOUTON * e - PETIT_H * e) * 0.5, couleur: palette::encre3() });
    c.cibles.push((fermer, Action::Fermer));
    y += BOUTON * e + 4.0 * e;

    // L'adresse reelle : l'hote entier d'abord, le chemin ensuite, qui seul
    // s'abrege. Une ecriture trompeuse se montre brute, et le dit.
    // Un schema refuse n'a pas d'hote : son nom tient la place de l'hote.
    let nom_hote = if a.hote.is_empty() { format!("{}:", a.schema) } else { a.hote.clone() };
    let (b_hote, l_hote) = mesurer(c.systeme, &sans_trait_fondu(&nom_hote), LECTURE * e, LECTURE_H * e, Weight::MEDIUM);
    c.textes.push(Texte { tampon: b_hote, x: ix, y, couleur: palette::encre1() });
    let reste = (iw - l_hote - 8.0 * e).max(0.0);
    if reste > 40.0 * e && !matches!(entree.etat, Etat::Refuse(_)) {
        let chemin = a.chemin.clone();
        let (b, _) = vues::une_ligne(c.systeme, Metrics::new(REPERE * e, LECTURE_H * e), &barlow(Weight::NORMAL), &chemin, reste);
        c.textes.push(Texte { tampon: b, x: ix + l_hote + 8.0 * e, y, couleur: palette::encre3() });
    }
    y += LECTURE_H * e;
    if a.trompeuse {
        y += 2.0 * e;
        let mot = if a.info.is_empty() { crate::langue::tr("Écriture trompeuse : l’hôte ci-dessus est celui qui répond.", "Misleading spelling: the host above is the one that answers.") } else { crate::langue::tr("Écriture trompeuse : seul ce qui suit @ répond, ce qui précède n’est qu’un nom.", "Misleading spelling: only what follows @ answers; what comes before is just a name.") };
        y += c.texte(mot, REPERE, REPERE_H, Weight::NORMAL, palette::alerte(), y);
        // L'adresse entiere, telle qu'envoyee, avec ce qui precede l'arobase.
        let brute = if a.info.is_empty() { a.hote.clone() } else { format!("{}@{}", a.info, a.hote) };
        y += c.texte(&sans_trait_fondu(&borner(&brute, 120)), REPERE, REPERE_H, Weight::NORMAL, palette::encre2(), y);
    }
    if matches!(entree.etat, Etat::Refuse(_)) {
        // Un schema refuse se montre en entier, brut : c'est tout ce qu'il y a a voir.
        let brut = borner(entree.url, 160);
        y += 2.0 * e;
        y += c.texte(&brut, REPERE, REPERE_H, Weight::NORMAL, palette::encre3(), y);
    }
    if let Some(p) = entree.page.filter(|p| !p.titre.is_empty()).filter(|_| !matches!(entree.etat, Etat::Refuse(_) | Etat::Local)) {
        let (b, _) = vues::une_ligne(c.systeme, Metrics::new(REPERE * e, REPERE_H * e), &barlow(Weight::NORMAL), &p.titre, iw);
        c.textes.push(Texte { tampon: b, x: ix, y, couleur: palette::encre2() });
        y += REPERE_H * e;
    }
    y += 12.0 * e;
    c.aplats.push(forme([ix, y, iw, (1.0 * e).max(1.0)], palette::filet(), 1.0, 0.0));
    y += 16.0 * e;

    let mut surbrillance: Vec<Boite> = Vec::new();
    let mut bloc: Option<Boite> = None;
    // Une redirection se dit avant ce qu'on lit : de qui vient le texte.
    let mut etat = entree.etat;
    if let Etat::Redirection { vers, contenu } = entree.etat {
        y += c.texte(&crate::tf!("Cette adresse renvoie vers {vers} : le texte ci-dessous vient de là.", "This address redirects to {vers}: the text below comes from there."), REPERE, REPERE_H, Weight::MEDIUM, palette::encre1(), y);
        y += 10.0 * e;
        etat = contenu;
    }

    match etat {
        Etat::Passage(x) => {
            y = corps_du_passage(&mut c, x, y, &mut surbrillance, &mut bloc);
            y = demande(&mut c, entree.page.map_or("", |p| p.invite.as_str()), y);
        }
        Etat::Introuvable { citation, debut_page } => {
            let ligne = if entree.verdict == Sonde::Trouve {
                crate::langue::tr("Le texte lu sans script ne contenait pas le passage ; la page ouverte l’a montré.", "The text read without script did not contain the passage; the open page showed it.")
            } else {
                crate::langue::tr("Le texte que l’agent avait cité n’est pas dans la page d’aujourd’hui.", "The text the agent had quoted is not in today’s page.")
            };
            y += c.texte(ligne, LECTURE, LECTURE_H, Weight::NORMAL, palette::encre2(), y);
            y += 12.0 * e;
            y = citation_non_verifiee(&mut c, citation, y);
            if !debut_page.is_empty() {
                y += 14.0 * e;
                y += c.texte(crate::langue::tr("Début de la page", "Start of the page"), REPERE, REPERE_H, Weight::MEDIUM, palette::encre3(), y);
                y += c.texte(&borner(debut_page, 200), LECTURE, LECTURE_H, Weight::NORMAL, palette::encre2(), y);
            }
        }
        Etat::SansPassage { debut_page } => {
            if let Some(p) = entree.page.filter(|p| !p.invite.is_empty()) {
                y += c.texte(crate::langue::tr("Ce que l’agent demandait à la page", "What the agent asked of the page"), REPERE, REPERE_H, Weight::MEDIUM, palette::encre3(), y);
                y += c.texte(&borner(&p.invite, 260), LECTURE, LECTURE_H, Weight::NORMAL, palette::encre1(), y);
                y += 14.0 * e;
            }
            y += c.texte(crate::langue::tr("Il n’a cité aucun passage : la page s’ouvrirait en haut.", "It quoted no passage: the page would open at the top."), LECTURE, LECTURE_H, Weight::NORMAL, palette::encre2(), y);
            if !debut_page.is_empty() {
                y += 14.0 * e;
                y += c.texte(crate::langue::tr("Début de la page", "Start of the page"), REPERE, REPERE_H, Weight::MEDIUM, palette::encre3(), y);
                y += c.texte(&borner(debut_page, 200), LECTURE, LECTURE_H, Weight::NORMAL, palette::encre2(), y);
            }
        }
        _ => {
            let hote = a.hote.clone();
            if let Some(t) = explication(etat, &hote, entree.ecoule) {
                y += c.texte(&t, LECTURE, LECTURE_H, Weight::NORMAL, palette::encre1(), y);
            }
            if matches!(etat, Etat::Chargement) {
                y += 8.0 * e;
                y += c.texte(&format!("{:.0} s", entree.ecoule.floor()), GRAND, GRAND_H, Weight::NORMAL, palette::encre2(), y);
            }
            // Meme quand la page ne s'ouvre pas, ce que l'agent avait cite reste
            // lisible : c'est la promesse, la page n'en est que la preuve.
            if matches!(etat, Etat::Echec(_) | Etat::HorsLigne | Etat::Connexion | Etat::Fichier) {
                if let Some(cit) = entree.page.and_then(|p| p.citation.as_deref()) {
                    y += 16.0 * e;
                    y = citation_non_verifiee(&mut c, &passage::une_ligne_de(cit), y);
                }
            }
        }
    }

    // Une phrase sur ce que la vigie a lu, quand elle a lu quelque chose.
    if matches!(etat, Etat::Passage(_) | Etat::Introuvable { .. } | Etat::SansPassage { .. }) {
        let rel = entree.page.map(|p| p.releve.as_str()).filter(|r| !r.is_empty());
        let fin = if entree.verdict == Sonde::NonRetrouve { crate::langue::tr("La page ouverte ne l’a pas montré.", "The open page did not show it.") } else { crate::langue::tr("La page n’est pas ouverte.", "The page is not open.") };
        let t = match rel {
            Some(r) => crate::tf!("Texte lu sans script ni cookie, le {}. {fin}", "Text read without script or cookie, on {}. {fin}", date_fr(r)),
            None => crate::tf!("Texte lu sans script ni cookie. {fin}", "Text read without script or cookie. {fin}"),
        };
        y += 16.0 * e;
        y += c.texte(&t, PETIT, PETIT_H, Weight::NORMAL, palette::encre3(), y);
    }

    // Les actions.
    y += 18.0 * e;
    let actions = actions_de(etat, entree.ecoule);
    let mut x_droite = ix + iw;
    let mut boutons: Vec<(Boite, &str, Action, bool)> = Vec::new();
    for (texte, action, primaire) in actions.iter().rev() {
        let (_, lt) = mesurer(c.systeme, texte, REPERE * e, REPERE_H * e, Weight::MEDIUM);
        let bl = lt + 28.0 * e;
        let b = Boite { x: x_droite - bl, y, l: bl, h: BOUTON_ACTION_H * e };
        boutons.push((b, texte, *action, *primaire));
        x_droite -= bl + 10.0 * e;
    }
    for (b, texte, action, primaire) in boutons {
        let (tampon, lt) = mesurer(c.systeme, texte, REPERE * e, REPERE_H * e, Weight::MEDIUM);
        let couleur_texte = if primaire {
            c.aplats.push(forme([b.x, b.y, b.l, b.h], palette::encre1(), 1.0, 8.0 * e));
            palette::fond_octets()
        } else {
            c.aplats.push(forme([b.x, b.y, b.l, b.h], palette::filet(), 1.0, 8.0 * e));
            c.aplats.push(forme([b.x + e, b.y + e, b.l - 2.0 * e, b.h - 2.0 * e], palette::plaque(), 1.0, 7.0 * e));
            palette::encre1()
        };
        c.textes.push(Texte { tampon, x: b.x + (b.l - lt) * 0.5, y: b.y + (b.h - REPERE_H * e) * 0.5, couleur: couleur_texte });
        c.cibles.push((b, action));
    }
    y += BOUTON_ACTION_H * e + marge;

    // La carte, posee ou l'oeil tombe : un peu au-dessus du milieu.
    let hauteur = y.min(h - 2.0 * gut);
    // Le haut est fixe, quel que soit l'etat : en passant du chargement au
    // resultat, la carte ne saute pas, elle ne fait que grandir vers le bas.
    let cy = (h * 0.12).min(h - hauteur - gut).max(gut).round();
    for t in &mut c.textes {
        t.y += cy;
    }
    for a in &mut c.aplats {
        a.boite[1] += cy;
    }
    for (b, _) in &mut c.cibles {
        b.y += cy;
    }
    for b in &mut surbrillance {
        b.y += cy;
    }
    if let Some(b) = bloc.as_mut() {
        b.y += cy;
    }
    let carte = Boite { x: cx, y: cy, l: cl, h: hauteur };

    let mut aplats = vec![
        // Le voile : le fil reste en dessous, lisible a peine, et ne bouge pas.
        Forme { boite: [0.0, 0.0, l, h], couleur: [0.0, 0.0, 0.0, if palette::clair() { 0.38 } else { 0.62 }], rayon: [0.0; 4] },
        forme([cx - 2.0 * e, cy - 2.0 * e, cl + 4.0 * e, hauteur + 4.0 * e], palette::filet(), 1.0, 16.0 * e),
        // La carte a le fond de la vigie, pas la plaque : l'encre-3 ne tient 4,5:1
        // que sur lui (mesure sur la capture : 3,29:1 sur la plaque sombre).
        forme([cx, cy, cl, hauteur], palette::fond_octets(), 1.0, 14.0 * e),
    ];
    aplats.extend(c.aplats);
    Composee { aplats, textes: c.textes, cibles: c.cibles, carte, zone_page: None, surbrillance, bloc }
}

/// Ce que l'agent demandait a la page : la question que le passage repond.
fn demande(c: &mut Corps, invite: &str, mut y: f32) -> f32 {
    if invite.trim().is_empty() {
        return y;
    }
    y += 14.0 * c.e;
    y += c.texte(crate::langue::tr("Ce que l’agent demandait à la page", "What the agent asked of the page"), REPERE, REPERE_H, Weight::MEDIUM, palette::encre3(), y);
    y += c.texte(&borner(&passage::une_ligne_de(invite), 190), REPERE, REPERE_H + 2.0, Weight::NORMAL, palette::encre2(), y);
    y
}

/// Le passage en grand, entre le texte qui le precede et celui qui le suit.
fn corps_du_passage(c: &mut Corps, x: &Extrait, mut y: f32, surbrillance: &mut Vec<Boite>, sortie: &mut Option<Boite>) -> f32 {
    let e = c.e;
    if !x.avant.is_empty() {
        let t = format!("{}{}", if x.coupe_avant { "… " } else { "" }, x.avant);
        y += c.texte(&t, LECTURE, LECTURE_H, Weight::NORMAL, palette::encre2(), y);
        y += 10.0 * e;
    }
    // Le bloc du passage : bord de 2 pt, fond de la vigie, ambre dessus, texte
    // en encre-1 sur le tout.
    let rem = 12.0 * e;
    let (tampon, hauteur) = poser_bloc(c.systeme, &x.passage, GRAND * e, GRAND_H * e, Weight::NORMAL, c.l - 2.0 * rem);
    let bloc = Boite { x: c.x, y, l: c.l, h: hauteur + 2.0 * rem };
    *sortie = Some(bloc);
    c.aplats.push(forme([bloc.x, bloc.y, bloc.l, bloc.h], bord_du_passage(), 1.0, 8.0 * e));
    c.aplats.push(forme([bloc.x + 2.0 * e, bloc.y + 2.0 * e, bloc.l - 4.0 * e, bloc.h - 4.0 * e], palette::fond_octets(), 1.0, 6.0 * e));
    let alpha = alpha_ambre();
    if alpha > 0.0 {
        for run in tampon.layout_runs() {
            let (mut x0, mut x1) = (f32::MAX, 0.0f32);
            for g in run.glyphs {
                x0 = x0.min(g.x);
                x1 = x1.max(g.x + g.w);
            }
            if x1 > x0 {
                let b = Boite { x: c.x + rem + x0 - 3.0 * e, y: y + rem + run.line_top, l: x1 - x0 + 6.0 * e, h: run.line_height };
                c.aplats.push(forme([b.x, b.y, b.l, b.h], AMBRE, alpha, 3.0 * e));
                surbrillance.push(b);
            }
        }
    } else {
        // Contraste eleve : pas de teinte, un soulignement epais du systeme.
        for run in tampon.layout_runs() {
            let (mut x0, mut x1) = (f32::MAX, 0.0f32);
            for g in run.glyphs {
                x0 = x0.min(g.x);
                x1 = x1.max(g.x + g.w);
            }
            if x1 > x0 {
                let b = Boite { x: c.x + rem + x0, y: y + rem + run.line_top, l: x1 - x0, h: run.line_height };
                c.aplats.push(forme([b.x, b.y + b.h - 3.0 * e, b.l, 3.0 * e], palette::encre1(), 1.0, 0.0));
                surbrillance.push(b);
            }
        }
    }
    c.textes.push(Texte { tampon, x: c.x + rem, y: y + rem, couleur: palette::encre1() });
    y += bloc.h;
    if x.trouve == Trouve::Debut {
        y += 6.0 * e;
        y += c.texte(crate::langue::tr("Seul le début de la citation est dans la page.", "Only the start of the quotation is in the page."), REPERE, REPERE_H, Weight::NORMAL, palette::encre3(), y);
    }
    if !x.apres.is_empty() {
        y += 10.0 * e;
        let t = format!("{}{}", x.apres, if x.coupe_apres { " …" } else { "" });
        y += c.texte(&t, LECTURE, LECTURE_H, Weight::NORMAL, palette::encre2(), y);
    }
    y
}

/// Ce que l'agent a cite, quand la page ne le confirme pas : un filet gris,
/// jamais l'ambre, qui ne dit que « retrouve dans la page ».
fn citation_non_verifiee(c: &mut Corps, citation: &str, mut y: f32) -> f32 {
    let e = c.e;
    y += c.texte(crate::langue::tr("Cité par l’agent, d’après sa transcription, non vérifié dans la page", "Quoted by the agent, from its transcript, not verified in the page"), REPERE, REPERE_H, Weight::MEDIUM, palette::encre3(), y);
    y += 4.0 * e;
    let (b, h) = poser_bloc(c.systeme, &borner(citation, 400), LECTURE * e, LECTURE_H * e, Weight::NORMAL, c.l - 16.0 * e);
    c.aplats.push(forme([c.x, y, 2.0 * e, h], palette::encre3(), 1.0, e));
    c.textes.push(Texte { tampon: b, x: c.x + 14.0 * e, y, couleur: palette::encre1() });
    y + h
}

fn actions_de(etat: &Etat, ecoule: f32) -> Vec<(&'static str, Action, bool)> {
    let fermer = (crate::langue::tr("Fermer", "Close"), Action::Fermer, false);
    match etat {
        Etat::Passage(_) | Etat::Introuvable { .. } | Etat::SansPassage { .. } => vec![fermer, (crate::langue::tr("Ouvrir la page", "Open the page"), Action::OuvrirPage, true)],
        Etat::Redirection { contenu, .. } => actions_de(contenu, ecoule),
        Etat::Chargement if ecoule >= SECONDE_LENTE => vec![(crate::langue::tr("Annuler", "Cancel"), Action::Fermer, false), (crate::langue::tr("Réessayer", "Retry"), Action::Reessayer, true)],
        Etat::Chargement => vec![(crate::langue::tr("Annuler", "Cancel"), Action::Fermer, false)],
        Etat::Echec(Echec::Delai) | Etat::HorsLigne => vec![fermer, (crate::langue::tr("Réessayer", "Retry"), Action::Reessayer, true)],
        Etat::Fichier => vec![fermer, (crate::langue::tr("Ouvrir ailleurs", "Open elsewhere"), Action::OuvrirAilleurs, false)],
        Etat::Local => vec![fermer, (crate::langue::tr("Ouvrir quand même", "Open anyway"), Action::ConfirmerLocal, true)],
        _ => vec![fermer],
    }
}

/// L'action que fait Entree dans la feuille.
pub fn action_principale(etat: &Etat, ecoule: f32) -> Option<Action> {
    actions_de(etat, ecoule).into_iter().find(|(_, _, primaire)| *primaire).map(|(_, a, _)| a)
}

/// L'action de la touche R, quand le reessai a un sens.
pub fn reessai(etat: &Etat, ecoule: f32) -> Option<Action> {
    actions_de(etat, ecoule).into_iter().find(|(_, a, _)| *a == Action::Reessayer).map(|(_, a, _)| a)
}

const MOIS_EN: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

fn date_fr(iso: &str) -> String {
    const MOIS: [&str; 12] = ["janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre", "novembre", "décembre"];
    let p: Vec<&str> = iso.split('-').collect();
    match (p.first(), p.get(1).and_then(|m| m.parse::<usize>().ok()), p.get(2).and_then(|d| d.parse::<u32>().ok())) {
        (Some(a), Some(m @ 1..=12), Some(d)) if !crate::langue::fr() => format!("{} {d}, {a}", MOIS_EN[m - 1]),
        (Some(a), Some(m @ 1..=12), Some(d)) => format!("{} {} {}", if d == 1 { "1er".to_string() } else { d.to_string() }, MOIS[m - 1], a),
        _ => iso.to_string(),
    }
}

/// La page ouverte : un cadre autour d'une fenetre native que la vigie ne
/// dessine pas. Le cadre est hors du rectangle de la page ; la page, ici, est
/// un aplat gris qui le dit.
fn composer_page(systeme: &mut FontSystem, entree: &Entree, l: f32, h: f32, e: f32) -> Composee {
    let etroite = l < 640.0 * e;
    let gut = if etroite { GOUTTIERE_ETROITE } else { GOUTTIERE } * e;
    let a = passage::adresse(entree.courante.filter(|c| !c.is_empty()).unwrap_or(entree.url));
    let mut textes: Vec<Texte> = Vec::new();
    let mut aplats: Vec<Forme> = vec![forme([0.0, 0.0, l, h], palette::fond_octets(), 1.0, 0.0)];
    let mut cibles: Vec<(Boite, Action)> = Vec::new();

    // L'en-tete : reculer, avancer, l'adresse reelle, fermer. Reculer a toujours
    // quelque chose a faire (au bout de l'historique de la page, il revient au
    // passage) ; avancer n'a de cible que si le moteur garde une page apres, et
    // sinon il est grise, a la meme place : le bouton ne bouge pas.
    let entete_h = 52.0 * e;
    let by = (entete_h - BOUTON * e) * 0.5;
    let recul = Boite { x: gut, y: by, l: BOUTON * e, h: BOUTON * e };
    let avance = Boite { x: recul.x + recul.l + 2.0 * e, y: by, l: BOUTON * e, h: BOUTON * e };
    let ferme = Boite { x: l - gut - BOUTON * e, y: by, l: BOUTON * e, h: BOUTON * e };
    let couleur_avance = if entree.peut_avancer { palette::encre1() } else { palette::encre3() };
    for (b, signe, c) in [(recul, "‹", palette::encre1()), (avance, "›", couleur_avance), (ferme, "×", palette::encre2())] {
        let (t, lt) = mesurer(systeme, signe, GRAND * e, GRAND_H * e, Weight::NORMAL);
        textes.push(Texte { tampon: t, x: b.x + (b.l - lt) * 0.5, y: b.y + (b.h - GRAND_H * e) * 0.5, couleur: c });
    }
    cibles.push((recul, Action::Reculer));
    if entree.peut_avancer {
        cibles.push((avance, Action::Avancer));
    }
    cibles.push((ferme, Action::Fermer));
    let x0 = avance.x + avance.l + 14.0 * e;
    let (b_esc, l_esc) = mesurer(systeme, crate::langue::tr("Échap", "Esc"), PETIT * e, PETIT_H * e, Weight::NORMAL);
    textes.push(Texte { tampon: b_esc, x: ferme.x - 8.0 * e - l_esc, y: by + (BOUTON * e - PETIT_H * e) * 0.5, couleur: palette::encre3() });
    let dispo = (ferme.x - 8.0 * e - l_esc - 14.0 * e - x0).max(40.0 * e);
    let (b_hote, l_hote) = mesurer(systeme, &sans_trait_fondu(&a.hote), LECTURE * e, LECTURE_H * e, Weight::MEDIUM);
    textes.push(Texte { tampon: b_hote, x: x0, y: (entete_h - LECTURE_H * e) * 0.5, couleur: palette::encre1() });
    let reste = dispo - l_hote - 8.0 * e;
    if reste > 40.0 * e {
        let (b, _) = vues::une_ligne(systeme, Metrics::new(REPERE * e, LECTURE_H * e), &barlow(Weight::NORMAL), &a.chemin, reste);
        textes.push(Texte { tampon: b, x: x0 + l_hote + 8.0 * e, y: (entete_h - LECTURE_H * e) * 0.5, couleur: palette::encre3() });
    }
    aplats.push(forme([0.0, entete_h - e, l, (1.0 * e).max(1.0)], palette::filet(), 1.0, 0.0));

    // Le bandeau : le passage reste dit, meme si la page ne le montre pas.
    // Une rangee : le mot, la citation, « Revoir ». Quand la citation n'a plus la
    // place de se lire sur cette rangee (fenetre etroite), elle passe sur une
    // seconde, a pleine largeur : deux mots coupes ne disent plus ce qu'on a cite.
    let mut bandeau_h = 44.0 * e;
    let plaque = aplats.len();
    aplats.push(forme([0.0, entete_h, l, bandeau_h], palette::plaque(), 1.0, 0.0));
    let (mot, citation, couleur_mot) = match entree.etat {
        Etat::Passage(x) => (crate::langue::tr("Passage cité", "Quoted passage"), Some(x.passage.clone()), palette::encre1()),
        Etat::Introuvable { citation, .. } => (crate::langue::tr("Passage introuvable sur cette page", "Passage not found on this page"), Some(citation.clone()), palette::encre1()),
        Etat::SansPassage { .. } => (crate::langue::tr("Aucun passage cité", "No quoted passage"), entree.page.map(|p| p.invite.clone()).filter(|s| !s.is_empty()), palette::encre1()),
        Etat::Redirection { contenu, .. } => match &**contenu {
            Etat::Passage(x) => (crate::langue::tr("Passage cité", "Quoted passage"), Some(x.passage.clone()), palette::encre1()),
            _ => (crate::langue::tr("Redirigée", "Redirected"), None, palette::encre1()),
        },
        _ => ("Page", None, palette::encre1()),
    };
    let mot = match (entree.sonde, passage::etat_de_fond(entree.etat)) {
        (Sonde::NonRetrouve, Etat::Passage(_) | Etat::Introuvable { .. }) => crate::langue::tr("Passage non retrouvé dans la page", "Passage not found in the page"),
        // Absent du texte lu sans script, mais la page vivante l'avait, apres son chargement.
        (Sonde::Trouve, Etat::Introuvable { .. }) => crate::langue::tr("Passage retrouvé dans la page", "Passage found in the page"),
        (Sonde::Indisponible, _) if !crate::navigateur::page_activee() => crate::langue::tr("Le moteur de pages est désactivé", "The page engine is turned off or unavailable here"),
        (Sonde::Indisponible, _) => crate::langue::tr("Le moteur de pages n’a pas démarré", "The page engine did not start"),
        _ => mot,
    };
    let (b_mot, l_mot) = mesurer(systeme, mot, REPERE * e, REPERE_H * e, Weight::MEDIUM);
    let ty = entete_h + (bandeau_h - REPERE_H * e) * 0.5;
    let mut x = gut;
    let retrouve_apres = matches!(passage::etat_de_fond(entree.etat), Etat::Introuvable { .. }) && entree.sonde == Sonde::Trouve;
    if (matches!(entree.etat, Etat::Passage(_)) || retrouve_apres) && !matches!(entree.sonde, Sonde::NonRetrouve | Sonde::Indisponible) {
        // Le point d'ambre : le meme signe que dans le fil, avant le clic.
        aplats.push(forme([x, entete_h + (bandeau_h - 8.0 * e) * 0.5, 8.0 * e, 8.0 * e], AMBRE, 1.0, 4.0 * e));
        x += 16.0 * e;
    }
    textes.push(Texte { tampon: b_mot, x, y: ty, couleur: couleur_mot });
    let revoir_t = crate::langue::tr("Revoir le passage", "Review the passage");
    let (b_rev, l_rev) = mesurer(systeme, revoir_t, REPERE * e, REPERE_H * e, Weight::MEDIUM);
    let revoir = Boite { x: l - gut - l_rev - 24.0 * e, y: entete_h + (bandeau_h - BOUTON * e) * 0.5, l: l_rev + 24.0 * e, h: BOUTON * e };
    aplats.push(forme([revoir.x, revoir.y, revoir.l, revoir.h], palette::filet(), 1.0, 7.0 * e));
    aplats.push(forme([revoir.x + e, revoir.y + e, revoir.l - 2.0 * e, revoir.h - 2.0 * e], palette::plaque(), 1.0, 6.0 * e));
    textes.push(Texte { tampon: b_rev, x: revoir.x + 12.0 * e, y: revoir.y + (revoir.h - REPERE_H * e) * 0.5, couleur: palette::encre1() });
    cibles.push((revoir, Action::RevoirPassage));
    if let Some(cit) = citation {
        let debut = x + l_mot + 12.0 * e;
        let largeur = (revoir.x - 14.0 * e - debut).max(0.0);
        if largeur >= LARGEUR_CITATION_MIN * e {
            let (b, _) = vues::une_ligne(systeme, Metrics::new(REPERE * e, REPERE_H * e), &barlow(Weight::NORMAL), &crate::tf!("« {} »", "“{}”", cit), largeur);
            textes.push(Texte { tampon: b, x: debut, y: ty, couleur: palette::encre2() });
        } else if l - 2.0 * gut > 60.0 * e {
            let seconde = entete_h + bandeau_h - 6.0 * e;
            let (b, _) = vues::une_ligne(systeme, Metrics::new(REPERE * e, REPERE_H * e), &barlow(Weight::NORMAL), &crate::tf!("« {} »", "“{}”", cit), l - 2.0 * gut);
            textes.push(Texte { tampon: b, x: gut, y: seconde, couleur: palette::encre2() });
            bandeau_h += REPERE_H * e + 4.0 * e;
            aplats[plaque] = forme([0.0, entete_h, l, bandeau_h], palette::plaque(), 1.0, 0.0);
        }
    }
    aplats.push(forme([0.0, entete_h + bandeau_h - e, l, (1.0 * e).max(1.0)], palette::filet(), 1.0, 0.0));

    // La page : un rectangle net, au-dessus de tout dessin de la vigie.
    let haut_page = entete_h + bandeau_h;
    let zone = Boite { x: 0.0, y: haut_page, l, h: h - haut_page };
    aplats.push(forme([zone.x, zone.y, zone.l, zone.h], Color::rgb(0x9A, 0x98, 0x93), 1.0, 0.0));
    let sombre = Color::rgb(0x1A, 0x1A, 0x1A);
    let titre_zone = if entree.sonde == Sonde::Indisponible { crate::langue::tr("La page ne peut pas s’afficher", "The page cannot be shown") } else { crate::langue::tr("La page s’affiche ici", "The page shows here") };
    let (b1, l1) = mesurer(systeme, titre_zone, LECTURE * e, LECTURE_H * e, Weight::MEDIUM);
    let pied_zone = if entree.sonde == Sonde::Indisponible && !crate::navigateur::page_activee() {
        crate::langue::tr("L’ouverture de la page est coupée (VIGIE_SANS_PAGE). Rien n’a été ouvert.", "Opening the page is turned off or unavailable on this system. Nothing was opened.")
    } else if entree.sonde == Sonde::Indisponible {
        crate::langue::tr("Le moteur de pages de Windows (WebView2) n’a pas démarré. Rien n’a été ouvert.", "The Windows page engine (WebView2) did not start. Nothing was opened.")
    } else {
        crate::langue::tr("Fenêtre native de la page : la capture de la vigie ne la montre pas.", "Native page window: Vigie's capture does not show it.")
    };
    let (b2, l2) = mesurer(systeme, pied_zone, REPERE * e, REPERE_H * e, Weight::NORMAL);
    let (b3, l3) = if etroite {
        mesurer(systeme, if entree.sonde == Sonde::Indisponible && !crate::navigateur::page_activee() { crate::langue::tr("Page désactivée.", "Page turned off.") } else if entree.sonde == Sonde::Indisponible { crate::langue::tr("WebView2 n’a pas démarré.", "WebView2 did not start.") } else { crate::langue::tr("Fenêtre native, hors capture.", "Native window, not in the capture.") }, REPERE * e, REPERE_H * e, Weight::NORMAL)
    } else {
        (b2, l2)
    };
    let milieu = zone.y + zone.h * 0.38;
    textes.push(Texte { tampon: b1, x: (l - l1) * 0.5, y: milieu, couleur: sombre });
    textes.push(Texte { tampon: b3, x: (l - l3) * 0.5, y: milieu + LECTURE_H * e + 2.0 * e, couleur: sombre });

    Composee { aplats, textes, cibles, carte: Boite { x: 0.0, y: 0.0, l, h }, zone_page: Some(zone), surbrillance: Vec::new(), bloc: None }
}

#[cfg(test)]
mod essais {
    use super::*;

    fn systeme() -> FontSystem {
        let mut base = glyphon::fontdb::Database::new();
        base.load_font_data(include_bytes!("../assets/fonts/Barlow-Regular.ttf").to_vec());
        base.load_font_data(include_bytes!("../assets/fonts/Barlow-Medium.ttf").to_vec());
        FontSystem::new_with_locale_and_db("fr-CA".to_string(), base)
    }

    fn tampon(s: &mut FontSystem, texte: &str, largeur: f32) -> Buffer {
        let mut b = Buffer::new(s, Metrics::new(15.0, 23.0));
        b.set_size(Some(largeur), None);
        b.set_text(texte, &barlow(Weight::MEDIUM), Shaping::Advanced, None);
        b.shape_until_scroll(s, false);
        b
    }

    /// Le lien se retrouve par ses octets, sur la ligne ou il tombe : une boite
    /// par ligne traversee, aucune ailleurs, meme apres un retour a la ligne du
    /// texte et des accents qui decalent les octets des caracteres.
    #[test]
    fn un_lien_coupe_sur_deux_lignes_a_deux_boites_et_rien_d_autre() {
        let mut s = systeme();
        let texte = "Étude à lire\nvoir https://ecampusontario.pressbooks.pub/essentialsoflinguistics2/chapter/8-7-violating-vs-flouting-a-maxim/ puis fin";
        let b = tampon(&mut s, texte, 300.0);
        let debut = texte.find("https").unwrap();
        let fin = debut + texte[debut..].find(' ').unwrap();
        let boites = boites(&b, texte, debut, fin);
        assert!(boites.len() >= 2, "{boites:?}");
        // Toutes sous la premiere ligne du texte, dans la largeur du tampon.
        assert!(boites.iter().all(|x| x.y >= 23.0 - 0.01 && x.x + x.l <= 300.5), "{boites:?}");
        // Un intervalle vide ne laisse aucune boite.
        assert!(super::boites(&b, texte, 3, 3).is_empty());
        // Le premier mot, sur la premiere ligne, est a l'ecart du lien.
        let mot = super::boites(&b, texte, 0, "Étude".len());
        assert_eq!(mot.len(), 1);
        assert!(mot[0].y < 1.0);
    }

    fn une_page() -> Page {
        Page {
            url: "https://docs.expo.dev/versions/latest/sdk/gl-view/".into(),
            invite: "Quote exactly: whether 'Included in Expo Go'; the WebGL2 spec sentence; the list of unimplemented methods (all of them); the msaaSamples prop description and default.".into(),
            citation: Some("It resembles a WebGL2RenderingContext in the WebGL 2 spec.".into()),
            titre: "GLView - Expo documentation".into(),
            texte: "WebGL API Once the component is mounted and the OpenGL ES context has been created, the gl object received through the onContextCreate prop becomes the interface to the OpenGL ES context, providing a WebGL API. It resembles a WebGL2RenderingContext in the WebGL 2 spec. Some older Android devices may not support WebGL2 features.".into(),
            releve: "2026-10-29".into(),
        }
    }

    fn tous_les_etats(p: &Page) -> Vec<Etat> {
        let x = passage::extraire(&p.texte, p.citation.as_deref().unwrap()).unwrap();
        vec![
            Etat::Passage(x.clone()),
            Etat::Introuvable { citation: p.citation.clone().unwrap(), debut_page: "Début de la page d'aujourd'hui, qui a changé depuis la lecture de l'agent.".into() },
            Etat::SansPassage { debut_page: "Début de la page.".into() },
            Etat::Chargement,
            Etat::Echec(Echec::Absente),
            Etat::Echec(Echec::Certificat),
            Etat::Echec(Echec::Delai),
            Etat::Echec(Echec::Serveur(503)),
            Etat::HorsLigne,
            Etat::Connexion,
            Etat::Fichier,
            Etat::Refuse("javascript".into()),
            Etat::Local,
            Etat::HorsMaquette,
            Etat::Redirection { vers: "www.expo.dev".into(), contenu: Box::new(Etat::Passage(x)) },
        ]
    }

    /// C8 et C10 : a chaque echelle, en moitie d'ecran, en fenetre etroite et en
    /// plein ecran, la carte tient dans la fenetre, tout controle fait 28 points
    /// au moins dans les deux sens, rien de ce qu'elle ecrit ne passe son bord,
    /// et la page ouverte laisse 60 % de la hauteur a la page.
    #[test]
    fn la_feuille_tient_dans_la_fenetre_a_chaque_echelle_et_chaque_etat() {
        let mut s = systeme();
        let p = une_page();
        let etats = tous_les_etats(&p);
        for (lp, hp) in [(766.4f32, 784.8f32), (480.0, 760.0), (1536.0, 792.8)] {
            for e in [1.0f32, 1.25, 1.5, 2.0] {
                let (l, h) = (lp * e, hp * e);
                for etat in &etats {
                    for vue in [Vue::Passage, Vue::Page] {
                        let c = composer(&mut s, &Entree { url: &p.url, courante: None, etat, page: Some(&p), vue, ecoule: 2.0, sonde: Sonde::Attente, peut_avancer: false, verdict: Sonde::Attente }, l, h, e);
                        let nom = format!("{lp}x{hp} x{e} {etat:?} {vue:?}");
                        if vue == Vue::Passage {
                            assert!(c.carte.x >= 0.0 && c.carte.y >= 0.0 && c.carte.x + c.carte.l <= l + 0.5 && c.carte.y + c.carte.h <= h + 0.5, "carte hors fenetre : {nom} {:?}", c.carte);
                            for t in &c.textes {
                                let b = t.boite();
                                assert!(b.x + b.l <= c.carte.x + c.carte.l + 0.5, "texte qui passe le bord : {nom} {:?}", t.debut());
                                assert!(b.y + b.h <= c.carte.y + c.carte.h + 0.5 && b.y >= c.carte.y - 0.5, "texte hors de la carte : {nom} {:?}", t.debut());
                            }
                        } else {
                            let z = c.zone_page.expect("la page a son rectangle");
                            assert!(z.h >= 0.6 * h, "page trop basse : {nom} {z:?}");
                            assert!(z.x + z.l <= l + 0.5 && z.y + z.h <= h + 0.5);
                        }
                        assert!(!c.cibles.is_empty(), "aucune sortie : {nom}");
                        for (b, a) in &c.cibles {
                            assert!(b.l >= 28.0 * e - 0.5 && b.h >= 28.0 * e - 0.5, "cible sous 28 points : {nom} {a:?} {b:?}");
                            assert!(b.x >= -0.5 && b.x + b.l <= l + 0.5 && b.y >= -0.5 && b.y + b.h <= h + 0.5, "cible hors fenetre : {nom} {a:?}");
                        }
                        assert!(c.cibles.iter().any(|(_, a)| *a == Action::Fermer || *a == Action::Reculer), "pas de sortie : {nom}");
                    }
                }
            }
        }
    }

    /// D3, objection du juge de design : du chargement au resultat, la carte
    /// sautait de 133 points. Son haut ne bouge plus d'un etat a l'autre.
    #[test]
    fn la_carte_garde_son_haut_d_un_etat_a_l_autre() {
        let mut s = systeme();
        let p = une_page();
        for e in [1.0f32, 1.25, 2.0] {
            let (l, h) = (766.4 * e, 784.8 * e);
            let haut = |s: &mut FontSystem, etat: &Etat| composer(s, &Entree { url: &p.url, courante: None, etat, page: Some(&p), vue: Vue::Passage, ecoule: 2.0, sonde: Sonde::Attente, peut_avancer: false, verdict: Sonde::Attente }, l, h, e).carte.y;
            let repere = haut(&mut s, &Etat::Chargement);
            for etat in [Etat::Echec(Echec::Absente), Etat::HorsLigne, Etat::Connexion, Etat::Local, Etat::SansPassage { debut_page: "Début.".into() }] {
                assert_eq!(haut(&mut s, &etat), repere, "le haut de la carte bouge : {etat:?} x{e}");
            }
        }
    }

    /// Objection du juge de design (C2, heuristique 5) : l'ecriture trompeuse
    /// cachait `apple.com@`, la moitie qui trompe. Elle se montre, et les deux
    /// traits d'union de `xn--` ne fondent plus en un.
    #[test]
    fn l_ecriture_trompeuse_montre_ce_qui_precede_l_arobase() {
        let mut s = systeme();
        let url = "https://apple.com@xn--pple-43d.example/connexion?session=1";
        let c = composer(&mut s, &Entree { url, courante: None, etat: &Etat::Connexion, page: None, vue: Vue::Passage, ecoule: 0.0, sonde: Sonde::Attente, peut_avancer: false, verdict: Sonde::Attente }, 766.4, 784.8, 1.0);
        let lignes: Vec<String> = c.textes.iter().map(|t| t.debut()).collect();
        assert!(lignes.iter().any(|l| l.starts_with("apple.com@xn")), "l'adresse brute n'est pas montree : {lignes:?}");
        assert!(lignes.iter().any(|l| l.contains("-\u{200c}-pple")), "les deux traits d'union fondent : {lignes:?}");
        assert!(lignes.iter().any(|l| l.starts_with("Écriture trompeuse")));
    }

    /// Objection du juge de design (C10) : en fenetre etroite, la citation du
    /// bandeau tombait a deux mots. Elle passe sur sa rangee, a pleine largeur, et
    /// la page garde plus de 60 % de la hauteur.
    #[test]
    fn la_citation_du_bandeau_se_lit_en_fenetre_etroite() {
        let mut s = systeme();
        let p = une_page();
        let x = passage::extraire(&p.texte, p.citation.as_deref().unwrap()).unwrap();
        for e in [1.0f32, 1.25, 2.0] {
            let (l, h) = (480.0 * e, 760.0 * e);
            let c = composer(&mut s, &Entree { url: &p.url, courante: None, etat: &Etat::Passage(x.clone()), page: Some(&p), vue: Vue::Page, ecoule: 0.0, sonde: Sonde::Attente, peut_avancer: false, verdict: Sonde::Attente }, l, h, e);
            let ligne = c.textes.iter().find(|t| t.debut().starts_with('«')).expect("la citation n'est pas dessinee");
            let texte: String = ligne.tampon.lines.iter().map(|l| l.text().to_string()).collect::<Vec<_>>().join(" ");
            assert!(texte.split_whitespace().count() >= 8 && !texte.contains('…'), "la citation est coupee a {} mots : {texte} (x{e})", texte.split_whitespace().count());
            assert!(ligne.boite().x + ligne.boite().l <= l + 0.5, "la citation passe le bord (x{e})");
            let z = c.zone_page.expect("la page a son rectangle");
            assert!(z.h >= 0.6 * h, "page trop basse : {z:?} x{e}");
        }
    }

    /// Un bouton qui ne fait rien ment : ~~l'en-tete de la page n'a que reculer,
    /// fermer et revoir~~ (retire contre la reponse 5 de Stone : « un bouton retour
    /// (et avancer) »). Reculer et Avancer sont dans l'en-tete, a la meme place ;
    /// Avancer n'est une cible que si le moteur garde une page apres, et sinon il est
    /// dessine grise, sans cible.
    #[test]
    fn l_entete_de_la_page_a_reculer_et_avancer_et_avancer_se_grise() {
        let mut s = systeme();
        let p = une_page();
        let x = passage::extraire(&p.texte, p.citation.as_deref().unwrap()).unwrap();
        for e in [1.0f32, 1.25, 1.5, 2.0] {
            let rendu = |s: &mut FontSystem, peut_avancer: bool| {
                composer(s, &Entree { url: &p.url, courante: None, etat: &Etat::Passage(x.clone()), page: Some(&p), vue: Vue::Page, ecoule: 0.0, sonde: Sonde::Attente, peut_avancer, verdict: Sonde::Attente }, 766.4 * e, 784.8 * e, e)
            };
            let grise = rendu(&mut s, false);
            let mut actions: Vec<String> = grise.cibles.iter().map(|(_, a)| format!("{a:?}")).collect();
            actions.sort();
            assert_eq!(actions, vec!["Fermer", "Reculer", "RevoirPassage"], "x{e}");
            let signe = |c: &Composee| c.textes.iter().find(|t| t.debut() == "›").map(|t| (t.x, t.y, t.couleur));
            let (gx, gy, gc) = signe(&grise).expect("le signe « avancer » n'est pas dessine");
            assert_eq!(gc, palette::encre3(), "avancer doit etre grise sans page apres, x{e}");

            let vive = rendu(&mut s, true);
            let mut actions: Vec<String> = vive.cibles.iter().map(|(_, a)| format!("{a:?}")).collect();
            actions.sort();
            assert_eq!(actions, vec!["Avancer", "Fermer", "Reculer", "RevoirPassage"], "x{e}");
            let (vx, vy, vc) = signe(&vive).unwrap();
            assert_eq!(vc, palette::encre1(), "x{e}");
            assert_eq!((gx, gy), (vx, vy), "le bouton bouge entre ses deux etats, x{e}");
            // La cible d'« avancer » est celle d'une main : 24 pt au moins (WCAG 2.5.8).
            let (b, _) = vive.cibles.iter().find(|(_, a)| *a == Action::Avancer).unwrap();
            assert!(b.l >= 24.0 * e && b.h >= 24.0 * e, "cible trop petite : {b:?} x{e}");
            let (r, _) = vive.cibles.iter().find(|(_, a)| *a == Action::Reculer).unwrap();
            assert!(b.x >= r.x + r.l, "les deux boutons se recouvrent, x{e}");
        }
    }

    /// Objection C3(b) du juge de design : l'extrait disait « Passage cite » quand
    /// la page vivante ne le montrait pas (callstack). Il garde ce que la page a repondu.
    #[test]
    fn l_extrait_ne_dit_pas_cite_quand_la_page_vivante_ne_l_a_pas_montre() {
        let mut s = systeme();
        let p = une_page();
        let x = passage::extraire(&p.texte, p.citation.as_deref().unwrap()).unwrap();
        let etat = Etat::Passage(x);
        assert_eq!(kicker(&etat, Sonde::Attente).0, "Passage cité");
        assert_eq!(kicker(&etat, Sonde::Trouve).0, "Passage cité");
        assert_eq!(kicker(&etat, Sonde::NonRetrouve).0, "Passage non retrouvé dans la page");
        let introuvable = Etat::Introuvable { citation: "x".into(), debut_page: String::new() };
        assert_eq!(kicker(&introuvable, Sonde::Attente).0, "Passage introuvable sur cette page");
        assert_eq!(kicker(&introuvable, Sonde::Trouve).0, "Passage retrouvé dans la page");
        for e in [1.0f32, 1.25, 1.5, 2.0] {
            let rendu = |s: &mut FontSystem, verdict: Sonde| {
                composer(s, &Entree { url: &p.url, courante: None, etat: &etat, page: Some(&p), vue: Vue::Passage, ecoule: 0.0, sonde: Sonde::Attente, peut_avancer: false, verdict }, 766.4 * e, 784.8 * e, e)
            };
            let dit = |c: &Composee, debut: &str| c.textes.iter().any(|t| t.debut().starts_with(debut));
            let non = rendu(&mut s, Sonde::NonRetrouve);
            assert!(dit(&non, "Passage non retrouvé"), "l'extrait ne dit pas que la page ne l'a pas montre, x{e}");
            assert!(!dit(&non, "Passage cité"), "l'extrait dit encore « cite », x{e}");
            let neutre = rendu(&mut s, Sonde::Attente);
            assert!(dit(&neutre, "Passage cité") && !dit(&neutre, "Passage non retrouvé"), "x{e}");
            // La carte garde son haut : le mot change, la carte ne bouge pas.
            assert_eq!(non.carte.y, neutre.carte.y, "x{e}");
        }
    }

    /// Le temoin du test ci-dessus : une carte qui ignore la fenetre, sur un
    /// ecran de 300 pixels, est bien vue comme debordante.
    #[test]
    fn le_temoin_de_la_fenetre_tombe_sur_une_fenetre_trop_petite() {
        let mut s = systeme();
        let p = une_page();
        let x = passage::extraire(&p.texte, p.citation.as_deref().unwrap()).unwrap();
        let c = composer(&mut s, &Entree { url: &p.url, courante: None, etat: &Etat::Passage(x), page: Some(&p), vue: Vue::Passage, ecoule: 0.0, sonde: Sonde::Attente, peut_avancer: false, verdict: Sonde::Attente }, 300.0, 300.0, 1.0);
        // La hauteur d'un passage ne tient pas dans 300 points : le test
        // ci-dessus le verrait comme « carte hors fenetre ».
        assert!(c.carte.y + c.carte.h > 300.0 || c.textes.iter().any(|t| t.boite().y + t.boite().h > 300.0));
    }

    #[test]
    fn la_date_se_dit_en_francais() {
        assert_eq!(date_fr("2026-10-29"), "29 octobre 2026");
        assert_eq!(date_fr("2026-11-01"), "1er novembre 2026");
        assert_eq!(date_fr("n'importe quoi"), "n'importe quoi");
    }

    #[test]
    fn borner_coupe_sur_un_mot_et_le_dit() {
        assert_eq!(borner("court", 20), "court");
        let b = borner("un deux trois quatre cinq six sept", 14);
        assert!(b.ends_with('…') && b.chars().count() <= 15, "{b}");
        assert!(!b.contains("troi ") && b.starts_with("un deux"));
    }

    #[test]
    fn borner_recule_jusqu_a_l_espace_quand_la_coupe_tombe_dans_un_mot() {
        assert_eq!(borner("aaaa bbbbbbbb cc", 8), "aaaa…");
        assert_eq!(borner("aaaa bbbb cc", 9), "aaaa bbbb…");
    }

    #[test]
    fn chaque_etat_a_une_sortie() {
        let pas_de_page: Vec<Etat> = vec![
            Etat::Chargement,
            Etat::Echec(Echec::Absente),
            Etat::Echec(Echec::Certificat),
            Etat::Echec(Echec::Delai),
            Etat::HorsLigne,
            Etat::Connexion,
            Etat::Fichier,
            Etat::Refuse("file".into()),
            Etat::Local,
            Etat::HorsMaquette,
            Etat::SansPassage { debut_page: String::new() },
            Etat::Introuvable { citation: "x".into(), debut_page: String::new() },
        ];
        for e in &pas_de_page {
            let a = actions_de(e, 0.0);
            assert!(a.iter().any(|(_, act, _)| *act == Action::Fermer), "{e:?} sans sortie");
        }
        // Le chargement qui dure propose de recommencer ; au debut, pas.
        assert!(!actions_de(&Etat::Chargement, 1.0).iter().any(|(_, a, _)| *a == Action::Reessayer));
        assert!(actions_de(&Etat::Chargement, 11.0).iter().any(|(_, a, _)| *a == Action::Reessayer));
        // Le schema refuse n'offre rien d'autre que fermer.
        assert_eq!(actions_de(&Etat::Refuse("javascript".into()), 0.0).len(), 1);
    }

    #[test]
    fn le_chargement_dit_le_temps_et_ne_tourne_pas() {
        let t = explication(&Etat::Chargement, "a.org", 2.0).unwrap();
        assert!(!t.contains("lentement"));
        let t = explication(&Etat::Chargement, "a.org", 12.0).unwrap();
        assert!(t.contains("a.org répond lentement"));
    }

    /// Le bandeau de la page ouverte dit ce que la recherche du passage a
    /// trouve, et l'en-tete dit l'hote ou la page est vraiment, pas celui du clic.
    #[test]
    fn le_bandeau_de_la_page_suit_la_sonde_et_l_en_tete_suit_la_page() {
        let mut s = systeme();
        let p = une_page();
        let x = passage::extraire(&p.texte, p.citation.as_deref().unwrap()).unwrap();
        let introuvable = Etat::Introuvable { citation: p.citation.clone().unwrap(), debut_page: String::new() };
        let mut debuts = |etat: &Etat, sonde: Sonde, courante: Option<&str>| -> Vec<String> {
            let c = composer(&mut s, &Entree { url: &p.url, courante, etat, page: Some(&p), vue: Vue::Page, ecoule: 2.0, sonde, peut_avancer: false, verdict: Sonde::Attente }, 766.4, 784.8, 1.0);
            c.textes.iter().map(|t| t.debut()).collect()
        };
        let a_un = |v: &[String], debut: &str| v.iter().any(|t| t.starts_with(debut));
        let v = debuts(&Etat::Passage(x.clone()), Sonde::Attente, None);
        assert!(a_un(&v, "Passage cité") && a_un(&v, "docs.expo.dev"), "{v:?}");
        let v = debuts(&Etat::Passage(x.clone()), Sonde::NonRetrouve, None);
        assert!(a_un(&v, "Passage non retrouvé") && !a_un(&v, "Passage cité"), "{v:?}");
        let v = debuts(&introuvable, Sonde::Trouve, None);
        assert!(a_un(&v, "Passage retrouvé"), "{v:?}");
        let v = debuts(&introuvable, Sonde::Attente, None);
        assert!(a_un(&v, "Passage introuvable"), "{v:?}");
        let v = debuts(&Etat::Passage(x), Sonde::Indisponible, None);
        assert!(a_un(&v, "Le moteur de pages") && a_un(&v, "La page ne peut pas"), "{v:?}");
        // Quand la page est coupee (VIGIE_SANS_PAGE, navigateur::page_activee), la
        // feuille le dit, au lieu de dire que le moteur n'a pas demarre.
        let (dit, pas_dit) = if crate::navigateur::page_activee() {
            ("Le moteur de pages n’a pas", "Le moteur de pages est dés")
        } else {
            ("Le moteur de pages est dés", "Le moteur de pages n’a pas")
        };
        assert!(a_un(&v, dit) && !a_un(&v, pas_dit), "{v:?}");
        // Stone a suivi un lien de la page : l'en-tete dit l'hote d'arrivee.
        let v = debuts(&introuvable, Sonde::Attente, Some("https://autre.example.org/chemin"));
        assert!(a_un(&v, "autre.example.org") && !a_un(&v, "docs.expo.dev"), "{v:?}");
    }

    #[test]
    fn les_titres_et_les_boutons_de_la_feuille_suivent_la_langue() {
        use crate::langue::{avec, Langue};
        let (titre_en, _) = avec(Langue::En, || kicker(&Etat::HorsLigne, Sonde::Attente));
        let (titre_fr, _) = avec(Langue::Fr, || kicker(&Etat::HorsLigne, Sonde::Attente));
        assert_eq!((titre_en, titre_fr), ("No connection", "Pas de connexion"));
        let boutons = |l| avec(l, || actions_de(&Etat::Local, 0.0).into_iter().map(|(m, _, _)| m).collect::<Vec<_>>());
        assert_eq!(boutons(Langue::En), vec!["Close", "Open anyway"]);
        assert_eq!(boutons(Langue::Fr), vec!["Fermer", "Ouvrir quand même"]);
        assert_eq!(avec(Langue::En, || date_fr("2026-10-01")), "October 1, 2026");
        assert_eq!(avec(Langue::Fr, || date_fr("2026-10-01")), "1er octobre 2026");
    }
}
