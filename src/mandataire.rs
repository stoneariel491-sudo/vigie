//! Le mandataire de la page : la regle de Stone appliquee au reseau, pas a la page.
//!
//! Les filtres par evenement du moteur (navigation, cadre, ressource) laissent
//! passer des chemins : redirections, `<object>`, `<video>`, `EventSource`,
//! `<link rel=prefetch>` (`passage-b/juge-correction-2.md`, §4). Ici, tout le
//! trafic du moteur de page passe par un mandataire HTTP que la vigie tient dans
//! son processus, sur `127.0.0.1`, et qui juge chaque connexion APRES avoir
//! resolu le nom lui-meme :
//! - une adresse locale ou privee (boucle, plages privees, lien local, partage
//!   de l'operateur, leurs ecritures IPv6 dont mappee, NAT64 et 6to4) n'est
//!   jointe que si elle est l'hote ET le port que Stone a vus ; tout le reste du
//!   reseau prive est refuse, quel que soit le nom qui y mene (`localtest.me`,
//!   `nip.io`, une liaison DNS) ;
//! - la connexion part vers l'adresse jugee, pas vers un nom resolu une seconde
//!   fois (pas de re-liaison DNS) ;
//! - aucune redirection n'est suivie ici : le moteur rejoue chaque saut par le
//!   mandataire, qui le juge a son tour ;
//! - `CONNECT` (HTTPS, WebSocket) est un tunnel opaque vers l'adresse jugee ;
//!   `http:` est relaye en une requete par connexion ;
//! - chaque refus est dit.
//!
//! L'acces est reserve au moteur par un jeton (`Proxy-Authorization: Basic`), que
//! le moteur donne quand la vigie repond a son defi (`navigateur.rs`).

use std::io::{self, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::passage;
use crate::reseau::est_privee;

/// Les connexions simultanees au plus : une page hostile en ouvre des centaines.
const CONNEXIONS_MAX: usize = 1024;
const ENTETE_MAX: usize = 64 * 1024;
const DELAI_ENTETE: Duration = Duration::from_secs(30);
const DELAI_CONNEXION: Duration = Duration::from_secs(8);
const DELAI_INACTIF: Duration = Duration::from_secs(600);

/// L'hote et le port locaux que Stone a vus : le seul endroit de sa machine ou
/// la page a le droit d'aller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Accord {
    pub hote: String,
    pub port: u16,
}

impl Accord {
    /// L'accord que vaut l'adresse d'une page locale que Stone a confirmee.
    pub fn de(adresse: &str) -> Option<Accord> {
        let a = passage::adresse(adresse);
        let defaut = if a.schema == "https" { 443 } else { 80 };
        let (hote, port) = separer(&a.hote, defaut)?;
        Some(Accord { hote, port })
    }
}

/// `hote:port`, `[::1]:port` ou `hote` seul, en un hote normalise (minuscules,
/// adresses IP en leur forme canonique : `2130706433` et `127.0.0.1` sont le
/// meme hote) et un port.
pub fn separer(autorite: &str, defaut: u16) -> Option<(String, u16)> {
    let a = autorite.trim();
    let (hote, port) = if let Some(reste) = a.strip_prefix('[') {
        let (dedans, apres) = reste.split_once(']')?;
        let port = match apres.strip_prefix(':') {
            Some(p) => p.parse().ok()?,
            None if apres.is_empty() => defaut,
            None => return None,
        };
        (format!("[{dedans}]"), port)
    } else {
        match a.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), p.parse().ok()?),
            None => (a.to_string(), defaut),
        }
    };
    if hote.is_empty() || hote == "[]" {
        return None;
    }
    Some((normaliser(&hote), port))
}

fn normaliser(hote: &str) -> String {
    let bas = hote.to_ascii_lowercase();
    match passage::ip_de_l_hote(&bas) {
        Some(IpAddr::V6(v)) => format!("[{v}]"),
        Some(IpAddr::V4(v)) => v.to_string(),
        None => bas,
    }
}

/// Une adresse que la regle de Stone met derriere son accord : tout ce qui est
/// de cet ordinateur ou du reseau prive, y compris quand l'adresse IPv6 ne fait
/// qu'envelopper une adresse IPv4 privee.
pub fn adresse_privee(ip: IpAddr) -> bool {
    if est_privee(ip) {
        return true;
    }
    if let IpAddr::V6(a) = ip {
        let s = a.segments();
        let o = a.octets();
        let v4 = |b: [u8; 4]| est_privee(IpAddr::V4(Ipv4Addr::from(b)));
        // 64:ff9b::/96 (NAT64), ::/96 (compatible IPv4), 2002::/16 (6to4).
        if (s[0] == 0x64 && s[1] == 0xff9b && s[2..6].iter().all(|&x| x == 0)) || s[0..6].iter().all(|&x| x == 0) {
            return v4([o[12], o[13], o[14], o[15]]);
        }
        if s[0] == 0x2002 {
            return v4([o[2], o[3], o[4], o[5]]);
        }
    }
    false
}

fn nom_local(hote: &str) -> bool {
    let nom = hote.trim_end_matches('.');
    nom == "localhost" || nom.ends_with(".localhost") || nom.ends_with(".local")
}

#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Les seules adresses vers lesquelles la connexion peut partir.
    Permis(Vec<SocketAddr>),
    Refuse(&'static str),
}

/// Le jugement : `hote` normalise, `resolu` ce que le nom a donne.
pub fn juger(hote: &str, port: u16, resolu: &[IpAddr], accord: Option<&Accord>, port_propre: u16) -> Verdict {
    let accorde = accord.is_some_and(|a| a.hote == hote && a.port == port);
    if nom_local(hote) && !accorde {
        return Verdict::Refuse("nom de cet ordinateur, non accepte");
    }
    let mut permis = Vec::new();
    let mut privees = 0;
    for &ip in resolu {
        if adresse_privee(ip) {
            privees += 1;
            // Le mandataire lui-meme n'est jamais une destination.
            if accorde && !(port == port_propre && ip.is_loopback()) {
                permis.push(SocketAddr::new(ip, port));
            }
        } else {
            permis.push(SocketAddr::new(ip, port));
        }
    }
    if !permis.is_empty() {
        // IPv4 d'abord : une machine sans IPv6 utilisable n'attend pas un delai.
        permis.sort_by_key(|a| a.is_ipv6());
        Verdict::Permis(permis)
    } else if privees > 0 {
        Verdict::Refuse("adresse locale ou privee, non acceptee")
    } else {
        Verdict::Refuse("nom sans adresse")
    }
}

fn resoudre(hote: &str, port: u16) -> Vec<IpAddr> {
    if let Some(interieur) = hote.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
        return interieur.parse::<Ipv6Addr>().map(|a| vec![IpAddr::V6(a)]).unwrap_or_default();
    }
    if let Ok(v4) = hote.parse::<Ipv4Addr>() {
        return vec![IpAddr::V4(v4)];
    }
    (hote, port).to_socket_addrs().map(|i| i.map(|a| a.ip()).collect()).unwrap_or_default()
}

/// Le jeton en Base64, tel que le client l'envoie dans `Proxy-Authorization`.
fn base64(o: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::new();
    for c in o.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        s.push(T[(n >> 18) as usize & 63] as char);
        s.push(T[(n >> 12) as usize & 63] as char);
        s.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        s.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    s
}

pub type Dire = Arc<dyn Fn(&str, &str) + Send + Sync>;

struct Contexte {
    accord: Option<Accord>,
    port: u16,
    attendu: String,
    dire: Dire,
    actives: AtomicUsize,
    vues: AtomicUsize,
}

pub struct Mandataire {
    port: u16,
    arret: Arc<AtomicBool>,
    ctx: Arc<Contexte>,
    jeton: String,
    nom: String,
}

impl Mandataire {
    /// Demarre le mandataire sur un port libre de `127.0.0.1`. `dire(cible, motif)`
    /// est appelee a chaque refus. `nom` est l'identifiant de l'utilisateur du
    /// defi d'authentification.
    pub fn demarrer(accord: Option<Accord>, jeton: String, dire: Dire) -> io::Result<Mandataire> {
        let ecoute = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let port = ecoute.local_addr()?.port();
        let nom = "vigie".to_string();
        let attendu = format!("Basic {}", base64(format!("{nom}:{jeton}").as_bytes()));
        let ctx = Arc::new(Contexte { accord, port, attendu, dire, actives: AtomicUsize::new(0), vues: AtomicUsize::new(0) });
        let arret = Arc::new(AtomicBool::new(false));
        let (c, a) = (ctx.clone(), arret.clone());
        std::thread::Builder::new().name("mandataire".into()).spawn(move || {
            for flux in ecoute.incoming() {
                if a.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(flux) = flux else { continue };
                if c.actives.fetch_add(1, Ordering::SeqCst) >= CONNEXIONS_MAX {
                    c.actives.fetch_sub(1, Ordering::SeqCst);
                    continue;
                }
                let c2 = c.clone();
                let lance = std::thread::Builder::new().stack_size(256 * 1024).spawn(move || {
                    servir(flux, &c2);
                    c2.actives.fetch_sub(1, Ordering::SeqCst);
                });
                if lance.is_err() {
                    c.actives.fetch_sub(1, Ordering::SeqCst);
                }
            }
        })?;
        Ok(Mandataire { port, arret, ctx, jeton, nom })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Les identifiants que le moteur doit donner au defi.
    pub fn identifiants(&self) -> (String, String) {
        (self.nom.clone(), self.jeton.clone())
    }

    /// Les requetes que le mandataire a vues (jugees, permises ou non).
    pub fn vues(&self) -> usize {
        self.ctx.vues.load(Ordering::SeqCst)
    }
}

impl Drop for Mandataire {
    fn drop(&mut self) {
        self.arret.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect((Ipv4Addr::LOCALHOST, self.port));
    }
}

/// Lit l'entete (jusqu'a la ligne vide) ; rend l'entete et ce qui le suit deja lu.
fn lire_entete(flux: &mut TcpStream) -> io::Result<(String, Vec<u8>)> {
    let mut tampon = Vec::with_capacity(2048);
    let mut bloc = [0u8; 4096];
    loop {
        let n = flux.read(&mut bloc)?;
        if n == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        tampon.extend_from_slice(&bloc[..n]);
        if let Some(i) = tampon.windows(4).position(|w| w == b"\r\n\r\n") {
            let reste = tampon[i + 4..].to_vec();
            return Ok((String::from_utf8_lossy(&tampon[..i]).into_owned(), reste));
        }
        if tampon.len() > ENTETE_MAX {
            return Err(io::ErrorKind::InvalidData.into());
        }
    }
}

fn repondre(flux: &mut TcpStream, ligne: &str, extra: &str) {
    let corps = "Refuse par la vigie";
    let _ = write!(flux, "HTTP/1.1 {ligne}\r\n{extra}Content-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{corps}", corps.len());
    let _ = flux.flush();
}

fn servir(mut client: TcpStream, ctx: &Contexte) {
    let _ = client.set_nodelay(true);
    let _ = client.set_read_timeout(Some(DELAI_ENTETE));
    let Ok((tete, reste)) = lire_entete(&mut client) else { return };
    ctx.vues.fetch_add(1, Ordering::SeqCst);
    let mut lignes = tete.split("\r\n");
    let requete = lignes.next().unwrap_or("");
    let mut parties = requete.splitn(3, ' ');
    let (Some(methode), Some(cible)) = (parties.next(), parties.next()) else { return repondre(&mut client, "400 Bad Request", "") };
    let entetes: Vec<(&str, &str)> = lignes.filter_map(|l| l.split_once(':')).map(|(k, v)| (k.trim(), v.trim())).collect();
    let champ = |nom: &str| entetes.iter().find(|(k, _)| k.eq_ignore_ascii_case(nom)).map(|(_, v)| *v);

    match champ("proxy-authorization") {
        None => return repondre(&mut client, "407 Proxy Authentication Required", "Proxy-Authenticate: Basic realm=\"vigie\"\r\n"),
        Some(v) if v == ctx.attendu => {}
        Some(_) => return repondre(&mut client, "403 Forbidden", ""),
    }

    let connecter = methode.eq_ignore_ascii_case("CONNECT");
    let (hote, port, chemin) = if connecter {
        match separer(cible, 443) {
            Some((h, p)) => (h, p, String::new()),
            None => return repondre(&mut client, "400 Bad Request", ""),
        }
    } else {
        let Some((schema, reste_url)) = cible.split_once("://") else { return repondre(&mut client, "400 Bad Request", "") };
        if !schema.eq_ignore_ascii_case("http") {
            return repondre(&mut client, "400 Bad Request", "");
        }
        let fin = reste_url.find(['/', '?', '#']).unwrap_or(reste_url.len());
        let (autorite, chemin) = reste_url.split_at(fin);
        let autorite = autorite.rsplit_once('@').map_or(autorite, |(_, h)| h);
        match separer(autorite, 80) {
            Some((h, p)) => (h, p, if chemin.starts_with('/') { chemin.to_string() } else { format!("/{chemin}") }),
            None => return repondre(&mut client, "400 Bad Request", ""),
        }
    };

    let resolu = resoudre(&hote, port);
    let cibles = match juger(&hote, port, &resolu, ctx.accord.as_ref(), ctx.port) {
        Verdict::Permis(c) => c,
        Verdict::Refuse(motif) => {
            (ctx.dire)(&format!("{hote}:{port}"), motif);
            return repondre(&mut client, "403 Forbidden", "");
        }
    };
    let mut amont = None;
    for a in &cibles {
        if let Ok(s) = TcpStream::connect_timeout(a, DELAI_CONNEXION) {
            amont = Some(s);
            break;
        }
    }
    let Some(mut amont) = amont else { return repondre(&mut client, "502 Bad Gateway", "") };
    let _ = amont.set_nodelay(true);

    if connecter {
        if client.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n").is_err() {
            return;
        }
    } else {
        // L'amont reçoit la requete sous sa forme d'origine : chemin seul, sans
        // les en-tetes du mandataire ; une connexion par requete.
        let mut sortie = format!("{methode} {chemin} HTTP/1.1\r\n");
        let mut montee = false;
        for (k, v) in &entetes {
            let bas = k.to_ascii_lowercase();
            if bas.starts_with("proxy-") || bas == "keep-alive" {
                continue;
            }
            if bas == "connection" {
                montee = v.to_ascii_lowercase().contains("upgrade");
                continue;
            }
            sortie.push_str(&format!("{k}: {v}\r\n"));
        }
        sortie.push_str(if montee { "Connection: Upgrade\r\n\r\n" } else { "Connection: close\r\n\r\n" });
        if amont.write_all(sortie.as_bytes()).is_err() {
            return;
        }
    }
    if !reste.is_empty() && amont.write_all(&reste).is_err() {
        return;
    }
    relayer(client, amont);
}

/// Copie dans les deux sens jusqu'a ce que l'un ferme.
fn relayer(client: TcpStream, amont: TcpStream) {
    let _ = client.set_read_timeout(Some(DELAI_INACTIF));
    let _ = amont.set_read_timeout(Some(DELAI_INACTIF));
    let (Ok(mut c2), Ok(mut a2)) = (client.try_clone(), amont.try_clone()) else { return };
    let (mut c1, mut a1) = (client, amont);
    let montee = std::thread::Builder::new().stack_size(256 * 1024).spawn(move || {
        let _ = io::copy(&mut c2, &mut a2);
        let _ = a2.shutdown(Shutdown::Write);
    });
    let _ = io::copy(&mut a1, &mut c1);
    let _ = c1.shutdown(Shutdown::Write);
    if let Ok(m) = montee {
        let _ = m.join();
    }
}

#[cfg(test)]
mod essais {
    use super::*;
    use std::sync::Mutex;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn le_local_et_le_prive_ne_se_joignent_que_sur_l_hote_et_le_port_vus() {
        let accord = Accord { hote: "127.0.0.1".into(), port: 8801 };
        let local = [ip("127.0.0.1")];
        assert!(matches!(juger("127.0.0.1", 8801, &local, Some(&accord), 1), Verdict::Permis(_)));
        // Le port voisin, l'autre ecriture, aucun accord : refuses.
        assert!(matches!(juger("127.0.0.1", 8802, &local, Some(&accord), 1), Verdict::Refuse(_)));
        assert!(matches!(juger("127.0.0.1", 8801, &local, None, 1), Verdict::Refuse(_)));
        assert!(matches!(juger("[::1]", 8801, &[ip("::1")], Some(&accord), 1), Verdict::Refuse(_)));
        // Le reste du reseau prive reste ferme, accord ou non.
        for p in ["10.0.0.1", "192.168.1.5", "172.16.0.9", "169.254.169.254", "100.64.0.1", "198.18.0.1", "fe80::1", "fd00::1", "::ffff:127.0.0.1"] {
            assert!(matches!(juger("hote", 80, &[ip(p)], Some(&accord), 1), Verdict::Refuse(_)), "{p} devrait etre refusee");
        }
    }

    #[test]
    fn un_nom_qui_resout_vers_le_local_est_refuse_apres_resolution() {
        let accord = Accord { hote: "127.0.0.1".into(), port: 8801 };
        // localtest.me, 127.0.0.1.nip.io : des noms publics dont l'adresse est locale.
        assert_eq!(juger("localtest.me", 8801, &[ip("127.0.0.1")], Some(&accord), 1), Verdict::Refuse("adresse locale ou privee, non acceptee"));
        assert_eq!(juger("127.0.0.1.nip.io", 8802, &[ip("127.0.0.1")], None, 1), Verdict::Refuse("adresse locale ou privee, non acceptee"));
        // Une liaison DNS vers l'interieur : une adresse privee.
        assert!(matches!(juger("intranet.exemple", 443, &[ip("10.1.2.3")], None, 1), Verdict::Refuse(_)));
        // Un nom mele de publique et de privee ne garde que la publique, et la connexion y va.
        assert_eq!(juger("mele.exemple", 443, &[ip("127.0.0.1"), ip("93.184.216.34")], None, 1), Verdict::Permis(vec!["93.184.216.34:443".parse().unwrap()]));
    }

    #[test]
    fn un_nom_de_cet_ordinateur_est_refuse_meme_si_la_resolution_le_croit_public() {
        // `localhost`, `*.localhost` et `*.local` sont de la machine ou de son reseau,
        // quoi qu'un resolveur en dise : le nom seul suffit au refus.
        for nom in ["localhost", "localhost.", "app.localhost", "imprimante.local"] {
            assert!(matches!(juger(nom, 80, &[ip("93.184.216.34")], None, 1), Verdict::Refuse(_)), "{nom} devrait etre refuse");
        }
        assert!(matches!(juger("exemple.org", 80, &[ip("93.184.216.34")], None, 1), Verdict::Permis(_)));
        // Accepte par Stone, sous son nom exact : il passe.
        let accord = Accord { hote: "localhost".into(), port: 3000 };
        assert!(matches!(juger("localhost", 3000, &[ip("127.0.0.1")], Some(&accord), 1), Verdict::Permis(_)));
    }

    #[test]
    fn les_enveloppes_ipv6_d_une_adresse_privee_sont_privees() {
        for p in ["::ffff:10.0.0.1", "::ffff:7f00:1", "64:ff9b::7f00:1", "64:ff9b::a00:1", "2002:7f00:1::", "2002:c0a8:101::", "::7f00:1", "::a00:1"] {
            assert!(adresse_privee(ip(p)), "{p} devrait etre privee");
        }
        for p in ["2606:4700::1111", "93.184.216.34", "64:ff9b::5db8:d822", "2002:5db8:d822::"] {
            assert!(!adresse_privee(ip(p)), "{p} devrait etre publique");
        }
    }

    #[test]
    fn les_ecritures_d_un_meme_hote_s_accordent() {
        assert_eq!(separer("2130706433:8801", 80), Some(("127.0.0.1".into(), 8801)));
        assert_eq!(separer("0x7f.1", 80), Some(("127.0.0.1".into(), 80)));
        assert_eq!(separer("[::1]:9", 80), Some(("[::1]".into(), 9)));
        assert_eq!(separer("LocalHost:3000", 80), Some(("localhost".into(), 3000)));
        assert_eq!(separer("exemple.org", 443), Some(("exemple.org".into(), 443)));
        assert_eq!(separer("hote:pas-un-port", 80), None);
        assert_eq!(Accord::de("http://127.0.0.1:8801/n4.html"), Some(Accord { hote: "127.0.0.1".into(), port: 8801 }));
        assert_eq!(Accord::de("https://localhost/x"), Some(Accord { hote: "localhost".into(), port: 443 }));
    }

    #[test]
    fn le_mandataire_lui_meme_n_est_jamais_une_destination() {
        let accord = Accord { hote: "127.0.0.1".into(), port: 5555 };
        assert!(matches!(juger("127.0.0.1", 5555, &[ip("127.0.0.1")], Some(&accord), 5555), Verdict::Refuse(_)));
        assert!(matches!(juger("127.0.0.1", 5555, &[ip("127.0.0.1")], Some(&accord), 5556), Verdict::Permis(_)));
    }

    #[test]
    fn le_jeton_s_encode_comme_le_client() {
        assert_eq!(base64(b"vigie:abc"), "dmlnaWU6YWJj");
        assert_eq!(base64(b"a"), "YQ==");
        assert_eq!(base64(b"ab"), "YWI=");
    }

    /// Un « autre service de la machine » : compte les connexions qu'il recoit.
    fn service() -> (u16, Arc<AtomicUsize>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let n = Arc::new(AtomicUsize::new(0));
        let n2 = n.clone();
        std::thread::spawn(move || {
            for mut s in l.incoming().flatten() {
                n2.fetch_add(1, Ordering::SeqCst);
                let mut b = [0u8; 2048];
                let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
                let _ = s.read(&mut b);
                let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
            }
        });
        (port, n)
    }

    fn demander(mand: &Mandataire, requete: &str) -> String {
        let mut s = TcpStream::connect((Ipv4Addr::LOCALHOST, mand.port())).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        s.write_all(requete.as_bytes()).unwrap();
        let mut sortie = Vec::new();
        let _ = s.read_to_end(&mut sortie);
        String::from_utf8_lossy(&sortie).into_owned()
    }

    fn avec_jeton(corps: &str, mand: &Mandataire) -> String {
        let (nom, jeton) = mand.identifiants();
        corps.replace("@@", &format!("Proxy-Authorization: Basic {}\r\n", base64(format!("{nom}:{jeton}").as_bytes())))
    }

    #[test]
    fn le_mandataire_relaie_le_permis_et_refuse_le_reste_sans_rien_envoyer() {
        let (autre, vu_autre) = service();
        let (permis, vu_permis) = service();
        let refus: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let r2 = refus.clone();
        let m = Mandataire::demarrer(Some(Accord { hote: "127.0.0.1".into(), port: permis }), "jeton-d-essai".into(), Arc::new(move |c, mo| r2.lock().unwrap().push(format!("{c} {mo}")))).unwrap();

        // Sans jeton : defi, rien n'est relaye.
        let r = demander(&m, &format!("CONNECT 127.0.0.1:{permis} HTTP/1.1\r\nHost: x\r\n\r\n"));
        assert!(r.starts_with("HTTP/1.1 407"), "{r}");
        // Mauvais jeton.
        let r = demander(&m, &format!("CONNECT 127.0.0.1:{permis} HTTP/1.1\r\nProxy-Authorization: Basic eHg6eHg=\r\n\r\n"));
        assert!(r.starts_with("HTTP/1.1 403"), "{r}");
        assert_eq!(vu_permis.load(Ordering::SeqCst), 0);

        // Un autre port local : CONNECT et GET refuses, aucune connexion recue.
        let r = demander(&m, &avec_jeton(&format!("CONNECT 127.0.0.1:{autre} HTTP/1.1\r\nHost: x\r\n@@\r\n"), &m));
        assert!(r.starts_with("HTTP/1.1 403"), "{r}");
        let r = demander(&m, &avec_jeton(&format!("GET http://127.0.0.1:{autre}/secret HTTP/1.1\r\nHost: x\r\n@@\r\n"), &m));
        assert!(r.starts_with("HTTP/1.1 403"), "{r}");
        // Un nom qui resout vers le local (localhost tient la place de localtest.me).
        let r = demander(&m, &avec_jeton(&format!("GET http://localhost:{permis}/x HTTP/1.1\r\nHost: x\r\n@@\r\n"), &m));
        assert!(r.starts_with("HTTP/1.1 403"), "{r}");
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(vu_autre.load(Ordering::SeqCst), 0, "le service interdit a recu une connexion");
        assert_eq!(vu_permis.load(Ordering::SeqCst), 0, "le nom du local n'est pas l'hote accepte");
        assert_eq!(refus.lock().unwrap().len(), 3);

        // L'hote et le port vus : relaye, avec le chemin seul et sans l'en-tete du mandataire.
        let r = demander(&m, &avec_jeton(&format!("GET http://127.0.0.1:{permis}/page HTTP/1.1\r\nHost: 127.0.0.1\r\n@@\r\n"), &m));
        assert!(r.starts_with("HTTP/1.1 200") && r.ends_with("ok"), "{r}");
        assert_eq!(vu_permis.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn une_redirection_n_est_jamais_suivie_par_le_mandataire_et_son_saut_est_juge() {
        // Un service « public » (ici l'hote accepte) qui redirige vers un service interdit :
        // le mandataire rend le 302 tel quel, et le saut suivant, rejoue par le moteur,
        // est refuse comme n'importe quelle requete.
        let (interdit, vu) = service();
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for mut s in l.incoming().flatten() {
                let mut b = [0u8; 2048];
                let _ = s.read(&mut b);
                let _ = write!(s, "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:{interdit}/saut\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            }
        });
        let m = Mandataire::demarrer(Some(Accord { hote: "127.0.0.1".into(), port }), "j".into(), Arc::new(|_, _| {})).unwrap();
        let r = demander(&m, &avec_jeton(&format!("GET http://127.0.0.1:{port}/redir HTTP/1.1\r\nHost: x\r\n@@\r\n"), &m));
        assert!(r.starts_with("HTTP/1.1 302"), "{r}");
        assert_eq!(vu.load(Ordering::SeqCst), 0);
        let r = demander(&m, &avec_jeton(&format!("GET http://127.0.0.1:{interdit}/saut HTTP/1.1\r\nHost: x\r\n@@\r\n"), &m));
        assert!(r.starts_with("HTTP/1.1 403"), "{r}");
        assert_eq!(vu.load(Ordering::SeqCst), 0);
    }
}
