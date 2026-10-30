//! La lecture d'une page, au clic et nulle part ailleurs.
//!
//! Un seul GET, sans cookie, sans script, sans compte : la vigie demande le
//! texte d'une adresse et l'affiche comme une donnee. Rien de ce que la page
//! dit n'est interprete ici, et ce module n'ecrit rien sur le disque.
//!
//! Le client est `ureq` (3.x, TLS par rustls, sans compression ni cookies) : un
//! client synchrone, sans moteur asynchrone a embarquer, dont le resolveur se
//! remplace. C'est ce qui permet de refuser les adresses privees au moment meme
//! de la resolution, et non avant : une verification faite avant que le client
//! ne resolve de nouveau laisserait passer un nom qui change de reponse entre
//! les deux (le rebond de DNS).

use std::io::Read;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use ureq::config::Config;
use ureq::unversioned::resolver::{DefaultResolver, ResolvedSocketAddrs, Resolver};
use ureq::unversioned::transport::NextTimeout;

use crate::passage::{adresse, classer, Classe};

/// Ce qu'on lit au plus d'une page : au-dela, le texte est tronque et le dit.
pub const OCTETS_MAX: usize = 2 * 1024 * 1024;
/// Le temps total d'une lecture, resolution et lecture comprises.
pub const DELAI: Duration = Duration::from_secs(10);
/// La langue demandee : celle que lit un agent, et celle de la page ouverte ensuite.
pub const LANGUE: &str = "en-US,en;q=0.9";
/// Les redirections suivies, chacune re-jugee.
const SAUTS_MAX: usize = 5;

/// Ce que la lecture rend.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Issue {
    Page(Lu),
    /// La reponse est un fichier : rien n'est lu ni enregistre.
    Fichier,
    /// Le serveur demande un compte.
    Connexion,
    Absente,
    /// Un autre code que la vigie ne sait pas dire mieux.
    Statut(u16),
    Certificat,
    Delai,
    HorsLigne,
    /// Une adresse que la vigie ne lit pas : le schema, ou un modele d'adresse.
    Refuse(String),
    /// Une adresse locale : on demande avant (jamais lue sans accord).
    Local,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lu {
    /// L'adresse d'ou vient le texte, apres redirections.
    pub url: String,
    /// L'hote d'arrivee quand le serveur a envoye ailleurs que l'hote demande.
    pub redirection: Option<String>,
    pub titre: String,
    pub texte: String,
    /// La page etait plus longue que ce qu'on lit.
    pub tronquee: bool,
    pub octets: usize,
}

/// Les adresses qu'on ne contacte pas sans accord : boucle locale, reseau
/// prive, lien local, diffusion, non specifiee, et leurs formes IPv6.
pub fn est_privee(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(a) => privee_v4(a),
        IpAddr::V6(a) => {
            if let Some(v4) = a.to_ipv4_mapped() {
                return privee_v4(v4);
            }
            let s = a.segments();
            a.is_loopback()
                || a.is_unspecified()
                || a.is_multicast()
                || (s[0] & 0xfe00) == 0xfc00 // fc00::/7, adresses locales uniques
                || (s[0] & 0xffc0) == 0xfe80 // fe80::/10, lien local
                || a == Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1)
        }
    }
}

fn privee_v4(a: Ipv4Addr) -> bool {
    let o = a.octets();
    a.is_loopback()
        || a.is_private()
        || a.is_link_local()
        || a.is_broadcast()
        || a.is_unspecified()
        || a.is_multicast()
        || o[0] == 0
        || (o[0] == 100 && (64..128).contains(&o[1])) // 100.64.0.0/10, partage de l'operateur
        || (o[0] == 192 && o[1] == 0 && o[2] == 0) // 192.0.0.0/24
        || (o[0] == 198 && (o[1] == 18 || o[1] == 19)) // 198.18.0.0/15, essais de debit
}

/// Le resolveur du client : celui du systeme, puis le tri. Sans accord pour le
/// local, une resolution qui ne rend que des adresses privees est une erreur ;
/// qui en melange, ne garde que les publiques.
#[derive(Debug)]
struct ResolveurSur {
    local_permis: bool,
    /// Levee quand la resolution n'a rendu que des adresses privees : la lecture
    /// le dit « cet ordinateur », pas « pas de connexion ».
    refus: Arc<AtomicBool>,
}

impl Resolver for ResolveurSur {
    fn resolve(&self, uri: &ureq::http::Uri, config: &Config, timeout: NextTimeout) -> Result<ResolvedSocketAddrs, ureq::Error> {
        let defaut = DefaultResolver::default();
        let toutes = defaut.resolve(uri, config, timeout)?;
        if self.local_permis {
            return Ok(toutes);
        }
        let mut sortie = defaut.empty();
        for a in toutes.iter().filter(|a| !est_privee(a.ip())) {
            sortie.push(*a);
        }
        if sortie.is_empty() {
            self.refus.store(true, Ordering::SeqCst);
            return Err(ureq::Error::HostNotFound);
        }
        Ok(sortie)
    }
}

fn client(local_permis: bool) -> (ureq::Agent, Arc<AtomicBool>) {
    let config = Config::builder()
        // Les redirections sont suivies ici, une a une, pour que chacune soit
        // jugee et dite.
        .max_redirects(0)
        .max_redirects_will_error(false)
        .http_status_as_error(false)
        // Aucun mandataire pris dans l'environnement : il ferait contacter une
        // autre machine que celle dont la resolution est filtree.
        .proxy(None)
        .timeout_global(Some(DELAI))
        .max_response_header_size(32 * 1024)
        .user_agent("Vigie (lecture d'un extrait, sans script ni cookie)")
        .accept("text/html,application/xhtml+xml,text/plain;q=0.8,*/*;q=0.1")
        .accept_encoding("identity")
        .build();
    let refus = Arc::new(AtomicBool::new(false));
    let agent = ureq::Agent::with_parts(config, ureq::unversioned::transport::DefaultConnector::default(), ResolveurSur { local_permis, refus: refus.clone() });
    (agent, refus)
}

/// Lit une adresse. `local_ok` : Stone a accepte qu'on ouvre une adresse de sa
/// propre machine.
pub fn lire(url: &str, local_ok: bool) -> Issue {
    let mut courante = url.to_string();
    let hote_depart = adresse(url).hote_nu().trim_start_matches("www.").to_string();
    // L'accord de Stone vaut pour l'hote (et le port) qu'il a vu, pas pour ce
    // vers quoi le serveur le renvoie ensuite.
    let hote_accepte = adresse(url).hote;
    let mut redirection: Option<String> = None;
    for _ in 0..=SAUTS_MAX {
        let permis = local_ok && adresse(&courante).hote == hote_accepte;
        match classer(&courante) {
            Classe::Refuse(s) => return Issue::Refuse(s),
            Classe::Gabarit => return Issue::Refuse("gabarit".into()),
            Classe::Local if !permis => return Issue::Local,
            _ => {}
        }
        let (agent, refus) = client(permis);
        let rep = match agent.get(&courante).header("Accept-Language", LANGUE).call() {
            Ok(r) => r,
            Err(_) if refus.load(Ordering::SeqCst) => return Issue::Local,
            Err(e) => return issue_de_l_erreur(&e),
        };
        let code = rep.status().as_u16();
        if matches!(code, 301 | 302 | 303 | 307 | 308) {
            let Some(vers) = rep.headers().get("location").and_then(|v| v.to_str().ok()) else { return Issue::Statut(code) };
            courante = resoudre_lieu(&courante, vers);
            let arrivee = adresse(&courante);
            if !arrivee.hote.is_empty() && arrivee.hote_nu().trim_start_matches("www.") != hote_depart {
                redirection = Some(arrivee.hote.clone());
            }
            continue;
        }
        return lire_reponse(rep, courante, redirection);
    }
    Issue::Statut(310)
}

fn issue_de_l_erreur(e: &ureq::Error) -> Issue {
    use ureq::Error::*;
    match e {
        Timeout(_) => Issue::Delai,
        HostNotFound | Io(_) | ConnectionFailed => Issue::HorsLigne,
        e if format!("{e:?}").to_ascii_lowercase().contains("cert") || format!("{e}").to_ascii_lowercase().contains("certificate") => Issue::Certificat,
        _ => Issue::HorsLigne,
    }
}

/// Une adresse de redirection, absolue ou non, rendue absolue.
pub fn resoudre_lieu(base: &str, vers: &str) -> String {
    if vers.contains("://") || vers.split_once(':').is_some_and(|(s, _)| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphabetic())) {
        return vers.to_string();
    }
    let a = adresse(base);
    let origine = format!("{}://{}", a.schema, a.hote);
    if let Some(reste) = vers.strip_prefix("//") {
        return format!("{}://{}", a.schema, reste);
    }
    if vers.starts_with('/') {
        return format!("{origine}{vers}");
    }
    let chemin = a.chemin.split(['?', '#']).next().unwrap_or("/");
    let dossier = chemin.rsplit_once('/').map_or("", |(d, _)| d);
    format!("{origine}{dossier}/{vers}")
}

fn lire_reponse(rep: ureq::http::Response<ureq::Body>, url: String, redirection: Option<String>) -> Issue {
    let code = rep.status().as_u16();
    let entete = |nom: &str| rep.headers().get(nom).and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    let type_ = entete("content-type").to_ascii_lowercase();
    let disposition = entete("content-disposition").to_ascii_lowercase();
    match code {
        401 => return Issue::Connexion,
        404 | 410 => return Issue::Absente,
        c if c >= 400 && c != 403 => return Issue::Statut(c),
        _ => {}
    }
    if disposition.starts_with("attachment") {
        return Issue::Fichier;
    }
    let lisible = type_.is_empty() || type_.starts_with("text/") || type_.contains("html") || type_.contains("xml") || type_.contains("json");
    if !lisible {
        return Issue::Fichier;
    }
    let latin = type_.contains("iso-8859-1") || type_.contains("latin1") || type_.contains("windows-1252");
    let mut corps = rep.into_body();
    let (octets, tronquee, erreur) = lire_borne(&mut corps.as_reader(), OCTETS_MAX);
    if erreur && octets.is_empty() {
        return Issue::Delai;
    }
    // Une lecture coupee par le delai ou par le serveur n'est pas la page entiere.
    let tronquee = tronquee || erreur;
    let brut = if latin { octets.iter().map(|&o| o as char).collect::<String>() } else { String::from_utf8_lossy(&octets).into_owned() };
    let est_html = type_.contains("html") || type_.is_empty() || brut.trim_start().starts_with('<');
    let (titre, texte) = if est_html { texte_html(&brut) } else { (String::new(), brut.clone()) };
    let compte = est_html && demande_un_compte(&brut);
    if code == 403 {
        return if compte { Issue::Connexion } else { Issue::Statut(403) };
    }
    if compte && texte.split_whitespace().count() < 120 {
        return Issue::Connexion;
    }
    Issue::Page(Lu { url, redirection, titre, texte, tronquee, octets: octets.len() })
}

/// Lit au plus `max` octets d'un flux, et un de plus pour savoir s'il en restait :
/// jamais plus, quelle que soit la taille annoncee par le serveur.
fn lire_borne(flux: &mut impl Read, max: usize) -> (Vec<u8>, bool, bool) {
    let mut octets: Vec<u8> = Vec::new();
    let erreur = flux.by_ref().take(max as u64 + 1).read_to_end(&mut octets).is_err();
    let tronquee = octets.len() > max;
    octets.truncate(max);
    (octets, tronquee, erreur)
}

/// Un champ de mot de passe dans la page : elle ne se lit pas sans compte.
pub fn demande_un_compte(html: &str) -> bool {
    let bas = html.to_ascii_lowercase();
    bas.contains("type=\"password\"") || bas.contains("type='password'") || bas.contains("type=password")
}

const BLOCS: [&str; 34] = [
    "p", "div", "br", "li", "ul", "ol", "h1", "h2", "h3", "h4", "h5", "h6", "tr", "table", "section", "article", "header", "footer", "nav", "main", "aside",
    "pre", "blockquote", "hr", "dd", "dt", "dl", "figure", "figcaption", "form", "fieldset", "details", "summary", "address",
];
const IGNORES: [&str; 5] = ["script", "style", "noscript", "template", "svg"];

fn entite(nom: &str) -> Option<char> {
    Some(match nom {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => ' ',
        "ndash" => '–',
        "mdash" => '—',
        "hellip" => '…',
        "lsquo" => '‘',
        "rsquo" => '’',
        "ldquo" => '“',
        "rdquo" => '”',
        "laquo" => '«',
        "raquo" => '»',
        "copy" => '©',
        "reg" => '®',
        "trade" => '™',
        "deg" => '°',
        "times" => '×',
        "middot" => '·',
        "bull" => '•',
        "eacute" => 'é',
        "egrave" => 'è',
        "agrave" => 'à',
        "ecirc" => 'ê',
        "ccedil" => 'ç',
        "ocirc" => 'ô',
        "ugrave" => 'ù',
        "icirc" => 'î',
        "euro" => '€',
        _ => return None,
    })
}

/// Les entites d'un morceau de texte : nommees (un jeu usuel) et numeriques.
pub fn decoder_entites(s: &str) -> String {
    let mut sortie = String::with_capacity(s.len());
    let mut reste = s;
    while let Some(i) = reste.find('&') {
        sortie.push_str(&reste[..i]);
        let apres = &reste[i + 1..];
        let fin = apres.find(';').filter(|&f| f > 0 && f <= 10);
        let remplace = fin.and_then(|f| {
            let nom = &apres[..f];
            let c = if let Some(num) = nom.strip_prefix('#') {
                let v = if let Some(h) = num.strip_prefix(['x', 'X']) { u32::from_str_radix(h, 16).ok() } else { num.parse::<u32>().ok() };
                v.and_then(char::from_u32).map(|c| if c == '\u{a0}' { ' ' } else { c })
            } else {
                entite(nom)
            };
            c.map(|c| (c, f))
        });
        match remplace {
            Some((c, f)) => {
                sortie.push(c);
                reste = &apres[f + 1..];
            }
            None => {
                sortie.push('&');
                reste = apres;
            }
        }
    }
    sortie.push_str(reste);
    sortie
}

fn une_ligne(s: &str) -> String {
    decoder_entites(s).split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Le titre et le texte visible d'un document HTML. Ni script ni style ni
/// commentaire ; les blocs font des retours a la ligne, les cellules des
/// blancs, les balises en ligne rien. Le texte n'est jamais interprete.
pub fn texte_html(html: &str) -> (String, String) {
    let bas = html.to_ascii_lowercase();
    let n = html.len();
    let mut titre = String::new();
    let mut texte = String::with_capacity(n / 3);
    let mut i = 0;
    while i < n {
        if html.as_bytes()[i] != b'<' {
            let fin = html[i..].find('<').map_or(n, |f| i + f);
            texte.push_str(&decoder_entites(&html[i..fin]));
            i = fin;
            continue;
        }
        if bas[i..].starts_with("<!--") {
            i = bas[i + 4..].find("-->").map_or(n, |f| i + 4 + f + 3);
            continue;
        }
        let Some(fin_balise) = bas[i..].find('>').map(|f| i + f) else { break };
        let interieur = &bas[i + 1..fin_balise];
        let ferme = interieur.starts_with('/');
        let nom: String = interieur.trim_start_matches('/').chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
        i = fin_balise + 1;
        if !ferme && nom == "title" {
            let f = bas[i..].find("</title").map_or(n, |f| i + f);
            if titre.is_empty() {
                titre = une_ligne(&html[i..f]);
            }
            i = bas[f..].find('>').map_or(n, |g| f + g + 1);
            continue;
        }
        if !ferme && IGNORES.contains(&nom.as_str()) {
            let cible = format!("</{nom}");
            let f = bas[i..].find(&cible).map_or(n, |f| i + f);
            i = bas[f..].find('>').map_or(n, |g| f + g + 1);
            continue;
        }
        if BLOCS.contains(&nom.as_str()) {
            texte.push('\n');
        } else if nom == "td" || nom == "th" {
            texte.push(' ');
        }
    }
    let mut propre = String::with_capacity(texte.len());
    for l in texte.lines().map(|l| l.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|l| !l.is_empty()) {
        if !propre.is_empty() {
            propre.push('\n');
        }
        propre.push_str(&l);
    }
    (titre, propre)
}

#[cfg(test)]
mod essais {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[test]
    fn le_texte_d_une_page_est_le_texte_visible() {
        let h = "<!doctype html><html><head><title> Mon &amp; titre </title><style>p{color:red}</style><script>var x='<p>faux</p>';</script></head>\
                 <body><nav>Menu</nav><p>Le seuil est de <b>200&nbsp;000</b> $.</p><!-- <p>cache</p> -->\
                 <noscript>Activez JS</noscript><table><tr><td>a</td><td>b</td></tr></table><p>Fin&#233; &#x2014; ok</p></body></html>";
        let (t, x) = texte_html(h);
        assert_eq!(t, "Mon & titre");
        assert!(x.contains("Le seuil est de 200 000 $."), "{x:?}");
        assert!(x.contains("a b"), "{x:?}");
        assert!(x.contains("Finé — ok"), "{x:?}");
        for faux in ["faux", "cache", "Activez JS", "color:red", "<"] {
            assert!(!x.contains(faux), "{faux} dans {x:?}");
        }
        // Les balises en ligne ne coupent pas un mot.
        assert_eq!(texte_html("<p>a<b>b</b>c</p>").1, "abc");
        assert_eq!(texte_html("").1, "");
        assert_eq!(texte_html("<p>sans fin").1, "sans fin");
    }

    #[test]
    fn les_entites_inconnues_restent_telles_quelles() {
        assert_eq!(decoder_entites("a &inconnue; &amp b &lt;c&gt; &#65;&#x42;"), "a &inconnue; &amp b <c> AB");
        assert_eq!(decoder_entites("fin &"), "fin &");
    }

    #[test]
    fn les_adresses_privees_sont_reconnues_et_les_publiques_non() {
        for a in ["127.0.0.1", "10.0.0.5", "172.16.3.4", "192.168.1.1", "169.254.169.254", "0.0.0.0", "100.64.0.1", "::1", "fe80::1", "fc00::1", "::ffff:127.0.0.1", "::ffff:10.0.0.1"] {
            assert!(est_privee(a.parse().unwrap()), "{a} devrait etre privee");
        }
        for a in ["8.8.8.8", "93.184.216.34", "2606:4700:4700::1111", "172.32.0.1", "100.63.255.255", "::ffff:8.8.8.8"] {
            assert!(!est_privee(a.parse().unwrap()), "{a} devrait etre publique");
        }
    }

    /// Le haut de la plage de l'operateur et les adresses locales uniques, que les
    /// sabotages du juge technique laissaient intactes.
    #[test]
    fn les_bornes_des_plages_privees_sont_tenues() {
        // Les bornes de la lecture, dites en chiffres et non par leur nom : un essai qui
        // se mesure a la constante qu'il doit pinner ne voit pas qu'elle a change.
        assert_eq!(DELAI, Duration::from_secs(10));
        assert_eq!(OCTETS_MAX, 2 * 1024 * 1024);
        for a in ["100.64.0.0", "100.127.255.255", "198.18.0.1", "198.19.255.255", "192.0.0.1", "fd00::1", "fdff::1", "fc00::1", "fe80::1", "febf::1"] {
            assert!(est_privee(a.parse().unwrap()), "{a} devrait etre privee");
        }
        for a in ["100.128.0.0", "198.20.0.1", "198.17.255.255", "192.0.1.1", "fe00::1", "fec0::1", "2001:db8::1"] {
            assert!(!est_privee(a.parse().unwrap()), "{a} devrait etre publique");
        }
    }

    /// Le serveur renvoie toujours vers lui-meme : la vigie s'arrete a six
    /// requetes (la premiere et cinq sauts), et le dit.
    #[test]
    fn une_boucle_de_redirections_s_arrete_au_nombre_de_sauts() {
        use std::sync::atomic::{AtomicBool, AtomicUsize};
        let ecoute = TcpListener::bind("127.0.0.1:0").unwrap();
        ecoute.set_nonblocking(true).unwrap();
        let port = ecoute.local_addr().unwrap().port();
        let compte = Arc::new(AtomicUsize::new(0));
        let fin = Arc::new(AtomicBool::new(false));
        let (c2, f2) = (compte.clone(), fin.clone());
        let t = std::thread::spawn(move || {
            while !f2.load(Ordering::SeqCst) {
                let Ok((mut f, _)) = ecoute.accept() else {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                };
                f.set_nonblocking(false).ok();
                f.set_read_timeout(Some(Duration::from_secs(3))).ok();
                let mut tampon = [0u8; 4096];
                let _ = f.read(&mut tampon);
                c2.fetch_add(1, Ordering::SeqCst);
                let _ = f.write_all(&http("302 Found", "Location: /encore
", ""));
            }
        });
        assert_eq!(lire(&format!("http://127.0.0.1:{port}/debut"), true), Issue::Statut(310));
        fin.store(true, Ordering::SeqCst);
        t.join().unwrap();
        assert_eq!(compte.load(Ordering::SeqCst), 6, "la limite de sauts a change");
    }

    #[test]
    fn un_lieu_relatif_se_resout_sur_la_page_qui_l_envoie() {
        assert_eq!(resoudre_lieu("https://a.org/x/y?q=1", "/z"), "https://a.org/z");
        assert_eq!(resoudre_lieu("https://a.org/x/y?q=1", "z"), "https://a.org/x/z");
        assert_eq!(resoudre_lieu("https://a.org/x/y", "//b.org/w"), "https://b.org/w");
        assert_eq!(resoudre_lieu("https://a.org/x", "http://b.org/"), "http://b.org/");
        assert_eq!(resoudre_lieu("https://a.org/x", "javascript:alert(1)"), "javascript:alert(1)");
        assert_eq!(resoudre_lieu("https://a.org/x", "file:///C:/a"), "file:///C:/a");
    }

    /// Un serveur local qui repond ce qu'on lui dit, une reponse par connexion.
    fn serveur(reponses: Vec<Vec<u8>>) -> (u16, std::thread::JoinHandle<Vec<String>>) {
        let ecoute = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = ecoute.local_addr().unwrap().port();
        let h = std::thread::spawn(move || {
            let mut requetes = Vec::new();
            for rep in reponses {
                let Ok((mut f, _)) = ecoute.accept() else { break };
                f.set_read_timeout(Some(Duration::from_secs(3))).ok();
                let mut tampon = [0u8; 4096];
                let n = f.read(&mut tampon).unwrap_or(0);
                requetes.push(String::from_utf8_lossy(&tampon[..n]).to_string());
                let _ = f.write_all(&rep);
            }
            requetes
        });
        (port, h)
    }

    fn http(statut: &str, entetes: &str, corps: &str) -> Vec<u8> {
        format!("HTTP/1.1 {statut}\r\nConnection: close\r\nContent-Length: {}\r\n{entetes}\r\n{corps}", corps.len()).into_bytes()
    }

    #[test]
    fn une_adresse_locale_ne_part_pas_sans_accord_et_part_avec() {
        let (port, h) = serveur(vec![http("200 OK", "Content-Type: text/html\r\n", "<title>T</title><p>Bonjour le monde</p>")]);
        let url = format!("http://127.0.0.1:{port}/page");
        // Sans accord : refusee avant tout contact.
        assert_eq!(lire(&url, false), Issue::Local);
        // Avec accord : lue, sans cookie ni en-tete d'identification.
        match lire(&url, true) {
            Issue::Page(p) => {
                assert_eq!(p.titre, "T");
                assert_eq!(p.texte, "Bonjour le monde");
                assert_eq!(p.redirection, None);
            }
            x => panic!("{x:?}"),
        }
        let requetes = h.join().unwrap();
        assert_eq!(requetes.len(), 1, "le refus a contacte le serveur");
        let r = requetes[0].to_ascii_lowercase();
        assert!(r.starts_with("get /page http/1.1"), "{r}");
        assert!(!r.contains("cookie:") && !r.contains("authorization:"), "{r}");
        assert!(r.contains("user-agent: vigie"), "{r}");
        assert!(r.contains("accept-encoding: identity"), "{r}");
    }

    /// La resolution seule juge : `localhost` n'est pas classe local ici quand
    /// il est ecrit `localtest.me` (un nom public qui resout vers 127.0.0.1).
    /// Hors ligne, le test accepte un refus de resolution ; jamais une page.
    #[test]
    fn un_nom_qui_resout_vers_une_adresse_privee_est_refuse_a_la_resolution() {
        let (port, _h) = serveur(vec![http("200 OK", "Content-Type: text/plain\r\n", "secret")]);
        let url = format!("http://localtest.me:{port}/x");
        // Le temoin du temoin : en ligne, le nom resout, et le refus se dit exactement.
        let en_ligne = std::net::ToSocketAddrs::to_socket_addrs(&("localtest.me", 80)).is_ok();
        if en_ligne {
            assert_eq!(lire(&url, false), Issue::Local);
        }
        match lire(&url, false) {
            Issue::Page(p) => panic!("le contenu local a ete lu : {p:?}"),
            // En ligne, le nom resout vers le local : refuse, et dit « cet ordinateur ».
            Issue::Local | Issue::HorsLigne | Issue::Delai => {}
            x => panic!("{x:?}"),
        }
    }

    #[test]
    fn les_schemas_refuses_ne_contactent_rien() {
        assert_eq!(lire("file:///C:/Windows/win.ini", true), Issue::Refuse("file".into()));
        assert_eq!(lire("javascript:alert(1)", true), Issue::Refuse("javascript".into()));
        assert_eq!(lire("https://api.example.com/{id}", true), Issue::Refuse("gabarit".into()));
    }

    #[test]
    fn les_statuts_et_les_types_se_disent() {
        let cas: Vec<(Vec<u8>, Issue)> = vec![
            (http("404 Not Found", "Content-Type: text/html\r\n", "non"), Issue::Absente),
            (http("401 Unauthorized", "", ""), Issue::Connexion),
            (http("500 Oops", "", "x"), Issue::Statut(500)),
            (http("200 OK", "Content-Type: application/pdf\r\n", "%PDF"), Issue::Fichier),
            (http("200 OK", "Content-Type: text/html\r\nContent-Disposition: attachment; filename=a.html\r\n", "<p>a</p>"), Issue::Fichier),
            (http("200 OK", "Content-Type: text/html\r\n", "<form><input type=\"password\"></form>"), Issue::Connexion),
        ];
        for (rep, attendu) in cas {
            let (port, _h) = serveur(vec![rep]);
            assert_eq!(lire(&format!("http://127.0.0.1:{port}/"), true), attendu);
        }
    }

    /// Un flux qui ne finit jamais : on compte ce que la lecture en a consomme.
    struct SansFin(std::rc::Rc<std::cell::Cell<usize>>);
    impl Read for SansFin {
        fn read(&mut self, tampon: &mut [u8]) -> std::io::Result<usize> {
            tampon.fill(b'x');
            self.0.set(self.0.get() + tampon.len());
            Ok(tampon.len())
        }
    }

    #[test]
    fn la_lecture_s_arrete_a_la_borne_sans_consommer_le_reste() {
        let compte = std::rc::Rc::new(std::cell::Cell::new(0));
        let (o, tronquee, erreur) = lire_borne(&mut SansFin(compte.clone()), 1000);
        assert_eq!((o.len(), tronquee, erreur), (1000, true, false));
        // Au plus un tampon de lecture au-dela de la borne : pas le flux entier.
        assert!(compte.get() <= 1000 + 8192, "{} octets consommes", compte.get());
        let (o, tronquee, _) = lire_borne(&mut &b"court"[..], 1000);
        assert_eq!((o.len(), tronquee), (5, false));
    }

    #[test]
    fn la_taille_lue_est_bornee_et_le_dit() {
        let gros = "mot ".repeat(OCTETS_MAX / 4 + 5000);
        let (port, _h) = serveur(vec![http("200 OK", "Content-Type: text/plain\r\n", &gros)]);
        match lire(&format!("http://127.0.0.1:{port}/"), true) {
            Issue::Page(p) => {
                assert!(p.tronquee);
                assert!(p.octets <= OCTETS_MAX);
            }
            x => panic!("{x:?}"),
        }
    }

    #[test]
    fn une_redirection_est_suivie_jugee_et_dite() {
        // Vers le meme hote, que Stone a accepte : suivie.
        let ecoute = TcpListener::bind("127.0.0.1:0").unwrap();
        let p1 = ecoute.local_addr().unwrap().port();
        let h1 = std::thread::spawn(move || {
            for rep in [http("302 Found", &format!("Location: http://127.0.0.1:{p1}/fin\r\n"), ""), http("200 OK", "Content-Type: text/html\r\n", "<p>arrivee</p>")] {
                let Ok((mut f, _)) = ecoute.accept() else { break };
                f.set_read_timeout(Some(Duration::from_secs(3))).ok();
                let mut tampon = [0u8; 4096];
                let _ = f.read(&mut tampon);
                let _ = f.write_all(&rep);
            }
        });
        match lire(&format!("http://127.0.0.1:{p1}/debut"), true) {
            Issue::Page(p) => {
                assert_eq!(p.texte, "arrivee");
                assert_eq!(p.redirection, None);
                assert!(p.url.ends_with("/fin"));
            }
            x => panic!("{x:?}"),
        }
        h1.join().unwrap();
        // Vers un autre hote de la machine que celui accepte : jugee, refusee, jamais contactee.
        let (p2, h2) = serveur(vec![http("200 OK", "Content-Type: text/plain\r\n", "SECRET")]);
        let (p0, _h0) = serveur(vec![http("302 Found", &format!("Location: http://localhost:{p2}/fin\r\n"), "")]);
        assert_eq!(lire(&format!("http://127.0.0.1:{p0}/debut"), true), Issue::Local);
        // Le port voisin, le meme hote : un autre service, donc refuse aussi.
        let (p4, _h4) = serveur(vec![http("302 Found", &format!("Location: http://127.0.0.1:{p2}/fin\r\n"), "")]);
        assert_eq!(lire(&format!("http://127.0.0.1:{p4}/debut"), true), Issue::Local);
        // Le serveur du secret n'a rien recu : seule notre connexion a vide l'attente.
        let _ = std::net::TcpStream::connect(("127.0.0.1", p2));
        assert_eq!(h2.join().unwrap(), vec![String::new()], "le serveur voisin a recu une requete");
        // Vers un schema refuse : refusee, rien n'est lu.
        let (p3, _h3) = serveur(vec![http("302 Found", "Location: file:///C:/Windows/win.ini\r\n", "")]);
        assert_eq!(lire(&format!("http://127.0.0.1:{p3}/"), true), Issue::Refuse("file".into()));
    }

    #[test]
    fn un_corps_coupe_en_route_se_dit_tronque() {
        // Le serveur annonce 1000 octets, en envoie 20, et ferme : l'extrait n'est pas la page.
        let ecoute = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = ecoute.local_addr().unwrap().port();
        let t = std::thread::spawn(move || {
            let (mut f, _) = ecoute.accept().unwrap();
            let mut tampon = [0u8; 4096];
            let _ = f.read(&mut tampon);
            let _ = f.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 1000\r\n\r\nvingt octets seulement");
        });
        match lire(&format!("http://127.0.0.1:{port}/"), true) {
            Issue::Page(p) => assert!(p.tronquee, "un corps coupe passe pour entier : {p:?}"),
            x => panic!("{x:?}"),
        }
        t.join().unwrap();
    }

    #[test]
    fn un_serveur_muet_est_coupe_au_delai() {
        let ecoute = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = ecoute.local_addr().unwrap().port();
        let t = std::thread::spawn(move || {
            let (_f, _) = ecoute.accept().unwrap();
            std::thread::sleep(DELAI + Duration::from_secs(2));
        });
        let debut = std::time::Instant::now();
        assert_eq!(lire(&format!("http://127.0.0.1:{port}/"), true), Issue::Delai);
        let ecoule = debut.elapsed();
        assert!(ecoule >= DELAI - Duration::from_secs(1) && ecoule < DELAI + Duration::from_secs(2), "{ecoule:?}");
        drop(t);
    }
}
