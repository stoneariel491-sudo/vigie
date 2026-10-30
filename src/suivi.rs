//! Le suivi : trouver l'agent qui travaille, et lire ce qu'il pense.
//!
//! Tout se passe dans un fil a part. Le rendu ne doit jamais attendre un
//! disque : une image qui saute parce qu'un fichier de 1,4 Mo se relit est
//! exactement ce qu'on ne veut pas d'une fenetre qu'on regarde des heures.
//!
//! Le suivi n'a rien a configurer. Il prend le chantier le plus recemment
//! touche, s'attache a la transcription qui bouge, et bascule tout seul quand
//! un autre agent prend la main — y compris apres une interruption, qui est
//! frequente. Quand on choisit un chantier dans un onglet, il suit celui-la,
//! de la meme facon, jusqu'au choix suivant ; quand on choisit un agent dans
//! la page d'un chantier, il suit cet agent, depuis son debut.
//!
//! Il voit ce que l'agent ouvre, et ce que ses gestes ecrivent sur le disque :
//! voir `Lecteur` et `medias`.

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// La plus grande largeur gardee. La colonne fait 760 px a l'echelle 1, donc
/// 950 px a l'echelle 1,25 de cet ecran : 1280 laisse de la marge au zoom sans
/// garder en memoire des pixels que rien n'affichera.
const IMAGE_LARGEUR_MAX: u32 = 1280;
/// La plus grande hauteur, qui est aussi la limite de texture des cartes les
/// plus modestes. Une capture de page entiere est reduite pour y tenir.
const IMAGE_HAUTEUR_MAX: u32 = 2048;

/// Les trois registres, et ils ne se lisent pas de la meme facon.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Registre {
    /// Ce qu'il pense. C'est l'interieur, et c'est le coeur de l'affaire.
    Pensee,
    /// Ce qu'il dit. C'est adresse au dehors, donc c'est ce qui finira dans
    /// son rapport.
    Parole,
    /// Ce qu'il fait. Une ligne suffit.
    Geste,
    /// Un changement d'agent. La vigie le signale plutot que de le subir.
    Releve,
    /// Une image que l'agent a regardee : une capture de maquette, un rendu.
    Image,
}

/// Comment une image est entree dans le fil.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Origine {
    /// L'agent l'a ouverte : la transcription la porte en entier.
    Regardee,
    /// Un de ses gestes l'a ecrite sur le disque ; la vigie l'y a lue.
    Produite,
}

/// Une image deja decodee et reduite, prete a partir vers la carte.
pub struct Image {
    pub largeur: u32,
    pub hauteur: u32,
    /// RGBA, 8 bits par canal, en sRGB.
    pub pixels: Vec<u8>,
    /// Le fichier, quand on le connait : un clic l'ouvre dans l'application du
    /// systeme.
    pub chemin: Option<PathBuf>,
    pub origine: Origine,
    /// Pour une video ou un son, sa duree en secondes ; l'image est celle du
    /// milieu d'une video, et l'onde d'un son.
    pub video: Option<f32>,
    /// Sa taille d'origine, avant reduction : c'est elle que dit la legende.
    pub source: (u32, u32),
    /// L'agent l'a nommee lui-meme : il l'a ouverte, ou son geste portait son
    /// nom. Faux quand seule la sortie d'un geste la nommait : c'est la plus
    /// faible des preuves, et la planche lui prefere un autre agent.
    pub nommee: bool,
}

impl Image {
    /// Un son : sa « video » est une duree, son image une onde.
    pub fn est_audio(&self) -> bool {
        self.chemin.as_deref().is_some_and(crate::medias::est_audio)
    }

    /// « vidéo de 3 s » ou « son de 3 s ».
    pub fn mention_duree(&self, d: f32) -> String {
        format!("{} de {}", if self.est_audio() { "son" } else { "vidéo" }, duree(d))
    }

    /// Le nom du fichier, ou rien pour une image sans fichier connu.
    pub fn nom(&self) -> Option<String> {
        self.chemin
            .as_ref()
            .and_then(|c| c.file_name())
            .map(|n| n.to_string_lossy().to_string())
    }

    /// Ce qui dit que deux images sont la meme : sa taille d'origine et un
    /// pixel sur 61 de ce qu'on en garde. Deux copies d'un meme fichier se
    /// decodent en pixels identiques ; deux rendus differents d'une meme scene
    /// different presque partout. 61 est premier, pour ne pas tomber en phase
    /// avec la largeur. Le fil et la planche s'en servent tous deux : un `cp`
    /// ne fait pas deux images ici et une seule la.
    pub fn empreinte(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (self.source, self.largeur, self.hauteur, self.video.map(f32::to_bits)).hash(&mut h);
        for o in self.pixels.iter().step_by(61) {
            o.hash(&mut h);
        }
        h.finish()
    }
}

/// « 3 s », « 1 min 12 s ».
pub fn duree(secondes: f32) -> String {
    let s = secondes.round().max(1.0) as u32;
    match (s / 60, s % 60) {
        (0, s) => format!("{s} s"),
        (m, 0) => format!("{m} min"),
        (m, s) => format!("{m} min {s:02} s"),
    }
}

/// La legende d'une image dans le fil : son nom, comment elle y est entree,
/// et sa taille d'origine. « planche.png  ·  produite  ·  2000 × 640 ».
pub fn legende(img: &Image) -> String {
    let nom = img.nom().unwrap_or_else(|| "image reçue".to_string());
    let comment = match (img.origine, img.video) {
        (_, Some(d)) => img.mention_duree(d),
        (Origine::Produite, None) => "produite".to_string(),
        (Origine::Regardee, None) => "ouverte".to_string(),
    };
    if img.est_audio() {
        return format!("{nom}  ·  {comment}");
    }
    format!("{nom}  ·  {comment}  ·  {} × {}", img.source.0, img.source.1)
}

/// La legende d'une piece jointe, sous son nom : « produite  ·  1205 × 360 ».
pub fn legende_courte(img: &Image) -> String {
    let comment = match (img.origine, img.video) {
        (_, Some(d)) => img.mention_duree(d),
        (Origine::Produite, None) => "produite".to_string(),
        (Origine::Regardee, None) => "ouverte".to_string(),
    };
    if img.est_audio() {
        return comment;
    }
    format!("{comment}  ·  {} × {}", img.source.0, img.source.1)
}

/// La parole sans le balisage qui s'y lirait en clair : les dieses d'un
/// titre, les accents graves d'un chemin, les doubles asterisques d'une
/// emphase (cadre du tour 00, constat 5). Une asterisque seule reste : elle
/// est un produit dans `a * b` aussi souvent qu'une emphase.
pub fn sans_balisage(t: &str) -> String {
    t.lines()
        .map(|l| {
            let debut = l.trim_start();
            let l = if debut.starts_with('#') {
                debut.trim_start_matches('#').trim_start()
            } else {
                l
            };
            l.replace("**", "").replace('`', "")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

impl std::fmt::Debug for Image {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Image {} x {} {:?}", self.largeur, self.hauteur, self.chemin)
    }
}

/// Ce qu'un geste WebFetch a demande, et ce que la reponse a cite de la page.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Lecture {
    /// La consigne donnee au petit modele qui lisait la page.
    pub invite: String,
    /// Les passages entre guillemets de la reponse (huit mots au moins), dans
    /// l'ordre ou elle les a ecrits. Vide tant que la reponse n'est pas lue, ou
    /// quand l'agent n'a rien cite.
    pub citations: Vec<String>,
    /// L'horodatage de la lecture, tel que la transcription l'ecrit.
    pub lu_le: String,
    /// La reponse est arrivee.
    pub repondue: bool,
}

pub type LectureCommune = Arc<std::sync::Mutex<Lecture>>;

const MOTS_MIN_CITATION: usize = 8;
const CITATION_MAX: usize = 400;
const CITATIONS_MAX: usize = 8;

/// Les passages entre guillemets d'un texte (« ... », “...” ou "..." droits),
/// de vingt a quatre cents signes et de huit mots au moins. Ce sont des
/// candidats : la page dira lesquels s'y trouvent.
pub fn citations(texte: &str) -> Vec<String> {
    let c: Vec<char> = texte.chars().collect();
    let mut sortie: Vec<String> = Vec::new();
    let mut i = 0;
    while i < c.len() && sortie.len() < CITATIONS_MAX {
        let ferme = match c[i] {
            '«' => '»',
            '“' => '”',
            '"' => '"',
            _ => {
                i += 1;
                continue;
            }
        };
        let mut j = i + 1;
        while j < c.len() && c[j] != ferme && (ferme != '"' || c[j] != '\n') && j - i <= CITATION_MAX + 1 {
            j += 1;
        }
        if j < c.len() && c[j] == ferme {
            let s: String = c[i + 1..j].iter().collect();
            let s = s.trim().to_string();
            if s.chars().count() >= 20 && s.split_whitespace().count() >= MOTS_MIN_CITATION {
                if !sortie.contains(&s) {
                    sortie.push(s);
                }
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    sortie
}

#[derive(Clone, Debug)]
pub struct Bloc {
    pub heure: String,
    pub registre: Registre,
    /// Pour un geste, le nom de l'outil. Sinon vide.
    pub outil: String,
    pub corps: String,
    /// Pour un geste WebFetch, ce que l'agent demandait a la page et ce qu'il
    /// en a cite. Le resultat arrive apres le geste : la cellule est partagee
    /// avec le lecteur, qui la remplit quand il le lit.
    pub lecture: Option<LectureCommune>,
    /// Pour une image, ses pixels. Partages : un bloc se recopie a chaque
    /// remise en page, et cinq megaoctets ne doivent pas le suivre.
    pub image: Option<Arc<Image>>,
}

/// Ce que les fils envoient au rendu.
pub enum Nouvelle {
    Bloc(Bloc),
    /// Le role de l'agent suivi, et le nom du chantier.
    Agent { role: String, chantier: String },
    /// Le fil de suivi a pris en compte le choix portant ce numero. Tout ce qui
    /// arrive avant appartient au choix precedent, et le rendu l'ecarte.
    Depart(u64),
    /// Le chantier choisi n'existe sous aucune session.
    Introuvable(String),
    /// Le catalogue de tous les chantiers, refait par son propre fil.
    Catalogue(std::sync::Arc<crate::catalogue::Catalogue>),
    /// La fiche du chantier dont la page est ouverte, refaite par son fil.
    Fiche(std::sync::Arc<crate::chantier::Fiche>),
    /// Le fil a fini de lire ce que le fichier contenait a l'ouverture : la
    /// suite ne viendra qu'avec ce que l'agent ecrira. Une capture l'attend
    /// au lieu de se fier a une seconde sans bloc nouveau.
    Lu,
}

/// Ce que le rendu doit appliquer de ce qui arrive. `generation` est le numero
/// du dernier choix qu'il a envoye, `recue` celui du dernier depart que le fil
/// a annonce. Un depart met `recue` a jour et ne s'applique pas ; le catalogue
/// s'applique toujours, il ne depend d'aucun choix ; le reste ne passe que si
/// le fil a pris en compte le DERNIER choix. Sans cela, les blocs que le fil
/// envoyait encore pour l'ancien chantier s'afficheraient sous le nom du
/// nouveau.
pub fn garder(generation: u64, recue: &mut u64, n: &Nouvelle) -> bool {
    match n {
        Nouvelle::Depart(g) => {
            *recue = *g;
            false
        }
        // La fiche porte son chantier, et le rendu la compare a sa page.
        Nouvelle::Catalogue(_) | Nouvelle::Fiche(_) => true,
        _ => *recue == generation,
    }
}

/// Ce que le fil de suivi suit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Choix {
    /// Le chantier le plus recent, et l'agent qui y ecrit : le comportement
    /// d'origine.
    Direct,
    /// Un chantier nomme par son identifiant, wf_<id>, et l'agent qui y
    /// ecrit.
    Chantier(String),
    /// Un agent choisi dans un chantier : on le suit lui, depuis son debut,
    /// qu'il ecrive ou non.
    Agent { chantier: String, agent: String },
}

/// Les dossiers de projets a lire : celui de Claude Code, puis celui des
/// transcriptions que `import` a remises a son format (Codex, Gemini...).
pub(crate) fn racines() -> Vec<PathBuf> {
    if let Some(f) = crate::app::figee() {
        return vec![f.join("projects")];
    }
    let base = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    let mut v = vec![Path::new(&base).join(".claude").join("projects")];
    if let Some(i) = crate::import::racine() {
        v.push(i);
    }
    v
}

/// Le chantier le plus recemment touche, TOUTES SESSIONS CONFONDUES.
///
/// Les transcriptions vivent sous le dossier de la session qui a lance le
/// chantier. Viser une session en dur, c'est cesser de voir tout chantier
/// lance apres un redemarrage : la vigie restait alors attachee au dernier
/// chantier de l'ancienne session, perime, sans rien en dire.
///
/// Parcourir tous les projets coute quelques centaines de lectures de
/// repertoire ; le fil relit toutes les 400 ms, donc le resultat est garde
/// trois secondes. Un chantier neuf se voit ainsi en moins de trois secondes,
/// et le disque n'est pas martele.
fn dernier_chantier() -> Option<PathBuf> {
    use std::sync::Mutex;
    use std::time::Instant;
    static CACHE: Mutex<Option<(Instant, Option<PathBuf>)>> = Mutex::new(None);
    if let Ok(g) = CACHE.lock() {
        if let Some((quand, chemin)) = g.as_ref() {
            if quand.elapsed() < Duration::from_secs(3) {
                return chemin.clone();
            }
        }
    }
    let mut v: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    for liste_projets in racines().iter().filter_map(|r| std::fs::read_dir(r).ok()) {
        for projet in liste_projets.filter_map(|e| e.ok()) {
            let Ok(sessions) = std::fs::read_dir(projet.path()) else {
                continue;
            };
            for session in sessions.filter_map(|e| e.ok()) {
                let dossier = session.path().join("subagents").join("workflows");
                let Ok(chantiers) = std::fs::read_dir(&dossier) else {
                    continue;
                };
                for c in chantiers.filter_map(|e| e.ok()) {
                    if !c.file_name().to_string_lossy().starts_with("wf_") {
                        continue;
                    }
                    if let Some(t) = c.metadata().ok().and_then(|m| m.modified().ok()) {
                        v.push((t, c.path()));
                    }
                }
            }
        }
    }
    v.sort_by_key(|(t, _)| *t);
    let trouve = v.pop().map(|(_, p)| p);
    if let Ok(mut g) = CACHE.lock() {
        *g = Some((Instant::now(), trouve.clone()));
    }
    trouve
}

fn agent_vivant(chantier: &Path) -> Option<PathBuf> {
    let mut v: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(chantier)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let n = e.file_name().to_string_lossy().to_string();
            n.starts_with("agent-") && n.ends_with(".jsonl")
        })
        .filter_map(|e| e.metadata().ok().and_then(|m| m.modified().ok()).map(|t| (t, e.path())))
        .collect();
    v.sort_by_key(|(t, _)| *t);
    v.pop().map(|(_, p)| p)
}

/// Le role d'un agent et sa phase, lus dans la fiche que l'outil pose a cote
/// de sa transcription (`agent-<id>.meta.json`, presente depuis octobre 2026).
pub(crate) fn fiche_de(chantier: &Path, identifiant: &str) -> (String, String) {
    let texte = std::fs::read_to_string(chantier.join(format!("agent-{identifiant}.meta.json"))).unwrap_or_default();
    let v: serde_json::Value = serde_json::from_str(&texte).unwrap_or_default();
    let champ = |c: &str| v.get(c).and_then(|x| x.as_str()).unwrap_or("").to_string();
    (champ("description"), champ("workflowPhase"))
}

/// Le role d'un agent : sa fiche, sinon le journal du chantier.
pub(crate) fn role_de(chantier: &Path, identifiant: &str) -> String {
    let (role, _) = fiche_de(chantier, identifiant);
    if !role.is_empty() {
        return role;
    }
    let j = chantier.join("journal.jsonl");
    let Ok(f) = std::fs::File::open(&j) else {
        return String::new();
    };
    let mut trouve = String::new();
    for ligne in BufReader::new(f).lines().map_while(Result::ok) {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&ligne) else {
            continue;
        };
        if v.get("agentId").and_then(|x| x.as_str()) == Some(identifiant) {
            if let Some(l) = v.get("label").and_then(|x| x.as_str()) {
                trouve = l.to_string();
            }
        }
    }
    trouve
}

/// L'heure locale, au format court. Le champ est en temps universel.
fn heure_locale(ts: &str) -> String {
    // On evite une dependance de date entiere pour cinq caracteres : le champ
    // est de la forme 2026-09-12T11:07:29.299Z, et le decalage se lit une fois
    // au demarrage.
    let brut = ts.get(11..19).unwrap_or("").to_string();
    if brut.is_empty() {
        return String::new();
    }
    let d = decalage_horaire_heures();
    let (h, m, s) = (
        brut[0..2].parse::<i32>().unwrap_or(0),
        &brut[3..5],
        &brut[6..8],
    );
    let hl = (h + d).rem_euclid(24);
    format!("{hl:02}:{m}:{s}")
}

/// Le decalage local, en heures, calcule une seule fois.
pub(crate) fn decalage_horaire_heures() -> i32 {
    use std::sync::OnceLock;
    static D: OnceLock<i32> = OnceLock::new();
    *D.get_or_init(|| {
        #[cfg(windows)]
        unsafe {
            // GetTimeZoneInformation rend le biais en MINUTES a retrancher de
            // l'heure locale pour obtenir l'universelle : le signe est inverse
            // de celui qu'on attend, et c'est le piege de cette fonction.
            #[repr(C)]
            struct Fuseau {
                biais: i32,
                _reste: [u8; 168],
            }
            extern "system" {
                fn GetTimeZoneInformation(info: *mut Fuseau) -> u32;
            }
            let mut f = Fuseau {
                biais: 0,
                _reste: [0; 168],
            };
            let r = GetTimeZoneInformation(&mut f);
            // 2 = heure d'ete en vigueur, et le biais du jour vaut alors une
            // heure de moins. On se contente du biais standard plus l'ete.
            let ete = if r == 2 { -60 } else { 0 };
            -(f.biais + ete) / 60
        }
        #[cfg(not(windows))]
        {
            0
        }
    })
}

fn abreger_outil(nom: &str, entree: &serde_json::Value) -> String {
    let s = |c: &str| entree.get(c).and_then(|x| x.as_str()).unwrap_or("").to_string();
    match nom {
        "Bash" | "PowerShell" => {
            let c = s("command");
            c.split_whitespace().collect::<Vec<_>>().join(" ")
        }
        "Write" | "Edit" | "Read" | "NotebookEdit" => {
            let p = s("file_path");
            p.rsplit(['\\', '/']).next().unwrap_or(&p).to_string()
        }
        "Grep" | "Glob" => s("pattern"),
        "WebSearch" => s("query"),
        "WebFetch" => s("url"),
        _ => String::new(),
    }
}

/// Le base64 standard et sa variante d'adresse. Une caisse entiere pour vingt
/// lignes n'en vaut pas la peine.
fn decoder_base64(texte: &str) -> Option<Vec<u8>> {
    let mut sortie = Vec::with_capacity(texte.len() / 4 * 3);
    let mut accu: u32 = 0;
    let mut bits = 0u32;
    for c in texte.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => break,
            b'\r' | b'\n' | b' ' => continue,
            _ => return None,
        };
        accu = (accu << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            sortie.push((accu >> bits) as u8);
            accu &= (1 << bits) - 1;
        }
    }
    Some(sortie)
}

/// Une image tiree des octets d'une transcription, reduite aux bornes. Cela se
/// fait dans le fil qui lit, jamais dans celui du rendu : decoder un PNG de
/// deux megaoctets prend plusieurs dizaines de millisecondes.
fn image_de(source: &serde_json::Value, bornes: (u32, u32)) -> Option<image::RgbaImage> {
    if source.get("type").and_then(|x| x.as_str()) != Some("base64") {
        return None;
    }
    let octets = decoder_base64(source.get("data")?.as_str()?)?;
    let img = image::load_from_memory(&octets).ok()?.to_rgba8();
    Some(crate::medias::reduire(img, bornes))
}

/// Ce qu'on garde d'un appel d'outil jusqu'a son resultat.
struct Appel {
    quand: SystemTime,
    /// La commande, ou l'entree entiere pour un autre outil : c'est la qu'un
    /// nom de fichier se lit.
    texte: String,
    /// Le dossier ou elle tournait.
    dossier: String,
    /// Pour une lecture, le fichier lu.
    lu: Option<String>,
    /// Pour un WebFetch, la cellule que le bloc du geste partage.
    web: Option<LectureCommune>,
}

/// La lecture d'UNE transcription, entree apres entree. Elle a une memoire,
/// parce qu'un resultat d'outil ne se comprend qu'avec son appel : c'est
/// l'appel qui dit quel fichier on ouvrait, dans quel dossier tournait la
/// commande, et a quel moment elle a commence.
pub struct Lecteur {
    bornes: (u32, u32),
    /// Le dossier de l'agent, tel que la derniere entree le donne.
    cwd: String,
    /// La premiere entree : rien d'ecrit avant n'est de lui.
    debut: Option<SystemTime>,
    appels: HashMap<String, Appel>,
    /// Les fichiers deja montres, pour qu'une planche ouverte trois fois ne se
    /// voie qu'une, la ou elle est apparue.
    vus: HashSet<String>,
    /// Muet, il tient sa memoire sans rien produire : c'est ainsi qu'on reprend
    /// une transcription par la fin sans perdre ce que son debut annonce.
    pub muet: bool,
}

fn cle(chemin: &str) -> String {
    crate::catalogue::normaliser(chemin).to_lowercase()
}

impl Lecteur {
    pub fn nouveau(bornes: (u32, u32)) -> Lecteur {
        Lecteur {
            bornes,
            cwd: String::new(),
            debut: None,
            appels: HashMap::new(),
            vus: HashSet::new(),
            muet: false,
        }
    }

    fn bloc_image(
        heure: &str,
        img: image::RgbaImage,
        chemin: Option<PathBuf>,
        origine: Origine,
        source: (u32, u32),
        video: Option<f32>,
        nommee: bool,
    ) -> Bloc {
        let img = Image {
            largeur: img.width(),
            hauteur: img.height(),
            pixels: img.into_raw(),
            chemin,
            origine,
            video,
            source,
            nommee,
        };
        Bloc {
            heure: heure.to_string(),
            registre: Registre::Image,
            outil: String::new(),
            lecture: None,
            corps: legende(&img),
            image: Some(Arc::new(img)),
        }
    }

    /// Une image que la transcription porte en entier : celle qu'un outil de
    /// lecture a rendue, ou une image collee.
    fn regardee(&mut self, heure: &str, source: &serde_json::Value, lu: Option<&str>, sortie: &mut Vec<Bloc>) {
        let chemin = lu.map(|c| crate::medias::resoudre(c, &self.cwd));
        if let Some(c) = &chemin {
            if !self.vus.insert(cle(&c.to_string_lossy())) {
                return;
            }
        }
        if self.muet {
            return;
        }
        let Some(img) = image_de(source, self.bornes) else { return };
        // La taille d'origine, lue sur le fichier s'il est encore la ; sinon
        // celle de l'image recue.
        let chemin = chemin.filter(|c| c.exists());
        let taille = chemin
            .as_ref()
            .and_then(|c| image::image_dimensions(c).ok())
            .unwrap_or((img.width(), img.height()));
        sortie.push(Self::bloc_image(heure, img, chemin, Origine::Regardee, taille, None, true));
    }

    /// Les fichiers qu'un geste a produits : ses noms de medias, et ceux de sa
    /// sortie, quand la regle de `medias::produit_par` les lui attribue.
    fn produites(&mut self, heure: &str, appel: &Appel, sortie_outil: &str, quand: SystemTime, sortie: &mut Vec<Bloc>) {
        if self.muet {
            return;
        }
        let debut = self.debut.unwrap_or(appel.quand);
        let mut groupes = crate::medias::noms(&appel.texte);
        let du_geste: Vec<String> = groupes.iter().flatten().cloned().collect();
        for g in crate::medias::noms(sortie_outil) {
            if !groupes.contains(&g) {
                groupes.push(g);
            }
        }
        for variantes in groupes {
            // La plus longue variante qui existe sur le disque : « capture
            // 1.png » plutot que « 1.png ».
            let Some((nom, chemin, m)) = variantes.into_iter().find_map(|nom| {
                let chemin = crate::medias::resoudre(&nom, &appel.dossier);
                let m = std::fs::metadata(&chemin).ok().filter(|m| m.is_file())?;
                Some((nom, chemin, m))
            }) else {
                continue;
            };
            let k = cle(&chemin.to_string_lossy());
            if self.vus.contains(&k) {
                continue;
            }
            let Ok(ecrit) = m.modified() else { continue };
            let nommee = du_geste.contains(&nom);
            if !crate::medias::produit_par(ecrit, appel.quand, quand, debut, nommee) {
                continue;
            }
            let taille = if crate::medias::est_audio(&chemin) {
                Some(crate::medias::ONDE)
            } else if crate::medias::est_video(&chemin) {
                crate::medias::taille_video(&chemin)
            } else {
                image::image_dimensions(&chemin).ok()
            };
            let Some((img, duree)) = crate::medias::charger(&chemin, self.bornes) else { continue };
            self.vus.insert(k);
            let taille = taille.unwrap_or((img.width(), img.height()));
            sortie.push(Self::bloc_image(heure, img, Some(chemin), Origine::Produite, taille, duree, nommee));
        }
    }

    /// Les blocs d'une entree de transcription.
    pub fn blocs(&mut self, v: &serde_json::Value) -> Vec<Bloc> {
        let mut sortie = Vec::new();
        if v.get("type").and_then(|x| x.as_str()) == Some("attachment") {
            return sortie;
        }
        if let Some(c) = v.get("cwd").and_then(|x| x.as_str()) {
            self.cwd = c.to_string();
        }
        let ts = v.get("timestamp").and_then(|x| x.as_str()).unwrap_or("");
        let quand = crate::dates::instant_iso(ts).unwrap_or_else(SystemTime::now);
        if self.debut.is_none() && !ts.is_empty() {
            self.debut = Some(quand);
        }
        let venant_de_l_agent = v.get("type").and_then(|x| x.as_str()) == Some("assistant");
        let heure = heure_locale(ts);
        let Some(contenu) = v.pointer("/message/content").and_then(|x| x.as_array()) else {
            return sortie;
        };
        let texte = |registre: Registre, t: &str| Bloc {
            heure: heure.clone(),
            registre,
            outil: String::new(),
            lecture: None,
            corps: t.to_string(),
            image: None,
        };
        for b in contenu {
            let genre = b.get("type").and_then(|x| x.as_str()).unwrap_or("");
            match genre {
                "thinking" => {
                    let t = b.get("thinking").and_then(|x| x.as_str()).unwrap_or("").trim();
                    if !t.is_empty() && !self.muet {
                        sortie.push(texte(Registre::Pensee, t));
                    }
                }
                "text" if venant_de_l_agent => {
                    let t = b.get("text").and_then(|x| x.as_str()).unwrap_or("").trim();
                    if !t.is_empty() && !self.muet {
                        sortie.push(texte(Registre::Parole, &sans_balisage(t)));
                    }
                }
                "tool_use" => {
                    let nom = b.get("name").and_then(|x| x.as_str()).unwrap_or("?");
                    let vide = serde_json::Value::Null;
                    let entree = b.get("input").unwrap_or(&vide);
                    let champ = |c: &str| entree.get(c).and_then(|x| x.as_str()).map(str::to_string);
                    let (texte_appel, dossier) = match nom {
                        "Bash" | "PowerShell" => {
                            let c = champ("command").unwrap_or_default();
                            let d = crate::medias::dossier_de(&c, &self.cwd);
                            (c, d)
                        }
                        _ => (entree.to_string(), crate::catalogue::normaliser(&self.cwd)),
                    };
                    let web: Option<LectureCommune> = (nom == "WebFetch").then(|| {
                        let invite = champ("prompt").unwrap_or_default();
                        Arc::new(std::sync::Mutex::new(Lecture {
                            citations: citations(&invite),
                            invite,
                            lu_le: ts.to_string(),
                            repondue: false,
                        }))
                    });
                    if let Some(id) = b.get("id").and_then(|x| x.as_str()) {
                        self.appels.insert(
                            id.to_string(),
                            Appel {
                                quand,
                                texte: texte_appel,
                                dossier,
                                lu: if nom == "Read" { champ("file_path") } else { None },
                                web: web.clone(),
                            },
                        );
                    }
                    if !self.muet {
                        sortie.push(Bloc {
                            heure: heure.clone(),
                            registre: Registre::Geste,
                            outil: nom.to_string(),
                            lecture: web,
                            corps: abreger_outil(nom, entree),
                            image: None,
                        });
                    }
                }
                // Ce que l'agent a regarde arrive dans le RESULTAT de son outil
                // de lecture, sous la forme {"type":"image","source":{"type":
                // "base64","media_type":"image/png","data":...}} : c'est la que
                // se trouve l'image entiere. Ce qu'il a produit, lui, n'y est
                // que nomme.
                "tool_result" => {
                    let appel = b
                        .get("tool_use_id")
                        .and_then(|x| x.as_str())
                        .and_then(|id| self.appels.remove(id));
                    let lu = appel.as_ref().and_then(|a| a.lu.clone());
                    let mut ecrit = String::new();
                    let mut vue = false;
                    match b.get("content") {
                        Some(serde_json::Value::String(s)) => ecrit.push_str(s),
                        Some(serde_json::Value::Array(parties)) => {
                            for partie in parties {
                                match partie.get("type").and_then(|x| x.as_str()) {
                                    Some("image") => {
                                        vue = true;
                                        if let Some(s) = partie.get("source") {
                                            self.regardee(&heure, s, lu.as_deref(), &mut sortie);
                                        }
                                    }
                                    Some("text") => {
                                        ecrit.push_str(partie.get("text").and_then(|x| x.as_str()).unwrap_or(""));
                                        ecrit.push('\n');
                                    }
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
                    }
                    if let Some(cellule) = appel.as_ref().and_then(|a| a.web.as_ref()) {
                        if let Ok(mut l) = cellule.lock() {
                            // Celles de la reponse d'abord : ce sont elles que le petit
                            // modele a copiees de la page ; celles de l'invite sont ce que
                            // l'agent croyait y trouver.
                            let mut toutes = citations(&ecrit);
                            let anciennes = std::mem::take(&mut l.citations);
                            for c in anciennes {
                                if !toutes.contains(&c) {
                                    toutes.push(c);
                                }
                            }
                            toutes.truncate(CITATIONS_MAX);
                            l.citations = toutes;
                            l.repondue = true;
                        }
                    }
                    if let (false, Some(a)) = (vue, appel.as_ref()) {
                        self.produites(&heure, a, &ecrit, quand, &mut sortie);
                    }
                }
                "image" => {
                    if let Some(s) = b.get("source") {
                        self.regardee(&heure, s, None, &mut sortie);
                    }
                }
                _ => {}
            }
        }
        sortie
    }
}

/// La lecture au fil de l'eau d'un fichier qui grandit. Une ligne peut etre
/// coupee en deux par une ecriture en cours, et meme un caractere : on garde
/// les octets du morceau et on les recolle au tour suivant.
pub struct Suite {
    pub chemin: PathBuf,
    place: u64,
    reste: Vec<u8>,
}

impl Suite {
    pub fn nouvelle(chemin: PathBuf) -> Suite {
        Suite { chemin, place: 0, reste: Vec::new() }
    }

    /// Les lignes completes ecrites depuis l'appel precedent.
    pub fn lignes(&mut self) -> Vec<String> {
        use std::io::Read;
        let Ok(mut f) = std::fs::File::open(&self.chemin) else { return Vec::new() };
        let taille = f.metadata().map(|m| m.len()).unwrap_or(0);
        if taille < self.place {
            // Le fichier a retreci : ce n'est plus le meme.
            self.place = 0;
            self.reste.clear();
        }
        if taille == self.place || f.seek(SeekFrom::Start(self.place)).is_err() {
            return Vec::new();
        }
        let mut octets = Vec::new();
        if f.read_to_end(&mut octets).is_err() {
            return Vec::new();
        }
        self.place += octets.len() as u64;
        self.reste.extend_from_slice(&octets);
        let Some(fin) = self.reste.iter().rposition(|&o| o == b'\n') else { return Vec::new() };
        let complet: Vec<u8> = self.reste.drain(..=fin).collect();
        String::from_utf8_lossy(&complet)
            .split('\n')
            .filter(|l| !l.trim().is_empty())
            .map(str::to_string)
            .collect()
    }
}

#[cfg(test)]
mod essais {
    use super::*;

    /// Les formes relevees dans le fil de `chercheur-direction-artistique` :
    /// un chemin entre accents graves, une emphase double, un titre. Le
    /// produit `a * b` garde son asterisque.
    #[test]
    fn la_parole_se_lit_sans_balisage() {
        assert_eq!(
            sans_balisage("## Bilan\nVoir `C:/Users/x.md` et **Manim** ; a * b"),
            "Bilan\nVoir C:/Users/x.md et Manim ; a * b"
        );
        assert_eq!(sans_balisage("  # titre"), "titre");
    }

    fn base64(octets: &[u8]) -> String {
        const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut s = String::new();
        for c in octets.chunks(3) {
            let n = (c[0] as u32) << 16
                | (*c.get(1).unwrap_or(&0) as u32) << 8
                | *c.get(2).unwrap_or(&0) as u32;
            for i in 0..4 {
                if i <= c.len() {
                    s.push(T[(n >> (18 - 6 * i)) as usize & 63] as char);
                } else {
                    s.push('=');
                }
            }
        }
        s
    }

    #[test]
    fn le_base64_se_decode_a_l_identique() {
        for n in 0..10u8 {
            let o: Vec<u8> = (0..n).map(|x| x.wrapping_mul(37).wrapping_add(n)).collect();
            assert_eq!(decoder_base64(&base64(&o)).unwrap(), o);
        }
    }

    /// La forme exacte relevee le 2026-10-03 dans la transcription d'un agent
    /// qui ouvrait une capture : le resultat de l'outil de lecture porte
    /// l'image entiere, en base64.
    #[test]
    fn une_capture_ouverte_devient_un_bloc_image() {
        let (l, h) = (3u32, 2u32);
        let pixels: Vec<u8> = (0..l * h).flat_map(|i| [i as u8 * 40, 200, 10, 255]).collect();
        let mut png_octets = Vec::new();
        {
            let mut e = png::Encoder::new(&mut png_octets, l, h);
            e.set_color(png::ColorType::Rgba);
            e.set_depth(png::BitDepth::Eight);
            e.write_header().unwrap().write_image_data(&pixels).unwrap();
        }
        let ligne = serde_json::json!({
            "type": "user",
            "timestamp": "2026-10-03T05:00:00.000Z",
            "message": {"role": "user", "content": [{
                "type": "tool_result",
                "tool_use_id": "x",
                "content": [{"type": "image", "source": {
                    "type": "base64", "data": base64(&png_octets), "media_type": "image/png"
                }}]
            }]}
        });
        let blocs = Lecteur::nouveau((1280, 2048)).blocs(&ligne);
        assert_eq!(blocs.len(), 1);
        assert_eq!(blocs[0].registre, Registre::Image);
        let img = blocs[0].image.as_ref().expect("des pixels");
        assert_eq!((img.largeur, img.hauteur), (l, h));
        assert_eq!(img.pixels, pixels);
    }

    /// Un geste qui ecrit une image, puis un geste qui en LISTE une autre,
    /// ecrite a un autre moment : la premiere entre dans le fil, la seconde
    /// non. Et une image deja montree ne revient pas quand on la liste.
    #[test]
    fn une_image_produite_entre_dans_le_fil() {
        let dossier = std::env::temp_dir().join(format!("vigie-essai-{}", std::process::id()));
        std::fs::create_dir_all(&dossier).unwrap();
        let img = image::RgbaImage::from_pixel(4, 3, image::Rgba([10, 20, 30, 255]));
        img.save(dossier.join("planche.png")).unwrap();
        img.save(dossier.join("ancienne.png")).unwrap();
        let d = dossier.to_string_lossy().replace('\\', "/");
        let entree = |ts: &str, contenu: serde_json::Value| {
            serde_json::json!({"type": "assistant", "timestamp": ts, "cwd": "C:/ailleurs", "message": {"content": [contenu]}})
        };
        let mut l = Lecteur::nouveau((1280, 2048));
        // L'agent commence en l'an 2000 ; la planche est ecrite pendant son
        // premier geste, qui s'etend jusqu'en 2100.
        l.blocs(&entree("2000-01-01T00:00:00.000Z", serde_json::json!({"type": "text", "text": "debut"})));
        l.blocs(&entree(
            "2000-01-01T00:00:01.000Z",
            serde_json::json!({"type": "tool_use", "id": "t1", "name": "Bash",
                "input": {"command": format!("cd {d} && ffmpeg -i a.png planche.png")}}),
        ));
        let blocs = l.blocs(&entree(
            "2100-01-01T00:00:00.000Z",
            serde_json::json!({"type": "tool_result", "tool_use_id": "t1", "content": "ok"}),
        ));
        let images: Vec<_> = blocs.iter().filter_map(|b| b.image.as_ref()).collect();
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].nom().as_deref(), Some("planche.png"));
        assert_eq!(images[0].origine, Origine::Produite);
        assert_eq!(images[0].source, (4, 3));
        // Un « ls » de 2001, dont la sortie nomme une image ecrite hors de sa
        // fenetre, et la planche deja montree.
        l.blocs(&entree(
            "2001-01-01T00:00:00.000Z",
            serde_json::json!({"type": "tool_use", "id": "t2", "name": "Bash", "input": {"command": format!("cd {d} && ls")}}),
        ));
        let blocs = l.blocs(&entree(
            "2001-01-01T00:00:01.000Z",
            serde_json::json!({"type": "tool_result", "tool_use_id": "t2", "content": "ancienne.png planche.png"}),
        ));
        assert!(blocs.iter().all(|b| b.image.is_none()));
        let _ = std::fs::remove_dir_all(&dossier);
    }

    fn bloc(corps: &str) -> Nouvelle {
        Nouvelle::Bloc(Bloc {
            heure: String::new(),
            registre: Registre::Pensee,
            outil: String::new(),
            lecture: None,
            corps: corps.to_string(),
            image: None,
        })
    }

    fn gardes(generation: u64, arrivees: &[Nouvelle]) -> Vec<String> {
        let mut recue = 0;
        arrivees
            .iter()
            .filter(|n| garder(generation, &mut recue, n))
            .map(|n| match n {
                Nouvelle::Bloc(b) => b.corps.clone(),
                Nouvelle::Agent { role, .. } => role.clone(),
                Nouvelle::Introuvable(id) => id.clone(),
                _ => "?".to_string(),
            })
            .collect()
    }

    /// Le rendu vient de choisir (choix 1) ; le fil envoyait encore pour
    /// l'ancien chantier quand il l'a appris.
    #[test]
    fn un_choix_abandonne_ne_laisse_rien_passer() {
        let arrivees = [
            bloc("ancien"),
            Nouvelle::Agent { role: "ancien agent".into(), chantier: "wf_a".into() },
            Nouvelle::Depart(1),
            Nouvelle::Agent { role: "nouvel agent".into(), chantier: "wf_b".into() },
            bloc("neuf"),
        ];
        assert_eq!(gardes(1, &arrivees), vec!["nouvel agent", "neuf"]);
    }

    /// Deux choix coup sur coup : le depart du premier n'ouvre pas la porte,
    /// puisque le rendu attend deja le second.
    #[test]
    fn deux_choix_coup_sur_coup() {
        let arrivees = [
            Nouvelle::Depart(1),
            bloc("premier"),
            Nouvelle::Introuvable("wf_premier".into()),
            Nouvelle::Depart(2),
            bloc("second"),
        ];
        assert_eq!(gardes(2, &arrivees), vec!["second"]);
    }

    /// Le meme contrat, sur une VRAIE ligne de transcription extraite dans un
    /// fichier : VIGIE_LIGNE=chemin cargo test --release -- --ignored
    #[test]
    #[ignore]
    fn une_vraie_ligne_de_transcription() {
        let chemin = std::env::var("VIGIE_LIGNE").expect("VIGIE_LIGNE");
        let texte = std::fs::read_to_string(chemin).unwrap();
        let v: serde_json::Value = serde_json::from_str(texte.trim()).unwrap();
        let images: Vec<_> = Lecteur::nouveau((1280, 2048)).blocs(&v).into_iter().filter(|b| b.registre == Registre::Image).collect();
        assert!(!images.is_empty(), "aucune image lue");
        for b in &images {
            let i = b.image.as_ref().unwrap();
            eprintln!("image {} x {}, {} octets", i.largeur, i.hauteur, i.pixels.len());
            assert_eq!(i.pixels.len(), (i.largeur * i.hauteur * 4) as usize);
        }
    }

    #[test]
    fn les_citations_d_une_reponse_se_lisent_entre_guillemets_de_huit_mots_au_moins() {
        let t = "Selon la page : \"Le seuil de Unity Personal est de 200 000 dollars par année\" et aussi « une phrase courte » puis “Une autre phrase assez longue pour compter ici même”.";
        let c = citations(t);
        assert_eq!(c, ["Le seuil de Unity Personal est de 200 000 dollars par année", "Une autre phrase assez longue pour compter ici même"]);
        assert!(citations("rien de cite ici, ni \"deux mots\"").is_empty());
        // Vingt signes mais cinq mots : trop court pour un fragment de texte.
        assert!(citations("elle dit \"cinq mots assez longs ici\"").is_empty());
        assert!(citations("").is_empty());
    }

    #[test]
    fn une_citation_de_huit_mots_compte_et_une_de_sept_non() {
        assert_eq!(citations("il dit \"un deux trois quatre cinq six sept huit\" fin"), ["un deux trois quatre cinq six sept huit"]);
        assert!(citations("il dit \"un deux trois quatre cinq six sept\" fin").is_empty());
    }

    #[test]
    fn une_reponse_ne_garde_que_huit_citations() {
        let t: String = (0..12).map(|i| format!("phrase \"citation numero {i} de huit mots au moins ici\" ")).collect();
        assert_eq!(citations(&t).len(), 8);
        assert_eq!(citations(&t)[7], "citation numero 7 de huit mots au moins ici");
    }

    #[test]
    fn un_webfetch_garde_son_invite_et_apprend_sa_citation_au_resultat() {
        let mut l = Lecteur::nouveau((1280, 2048));
        let entree = |ts: &str, c: serde_json::Value| serde_json::json!({"type": "assistant", "timestamp": ts, "message": {"content": [c]}});
        let blocs = l.blocs(&entree(
            "2026-10-29T10:00:00.000Z",
            serde_json::json!({"type": "tool_use", "id": "w1", "name": "WebFetch",
                "input": {"url": "https://a.org/x", "prompt": "Quote the threshold verbatim"}}),
        ));
        let cellule = blocs[0].lecture.clone().expect("le geste porte sa lecture");
        assert_eq!(cellule.lock().unwrap().invite, "Quote the threshold verbatim");
        assert!(!cellule.lock().unwrap().repondue);
        l.blocs(&serde_json::json!({"type": "user", "timestamp": "2026-10-29T10:00:03.000Z", "message": {"content": [
            {"type": "tool_result", "tool_use_id": "w1",
             "content": "The page says: \"Le seuil de Unity Personal est de 200 000 dollars par année\" ok."}]}}));
        let v = cellule.lock().unwrap().clone();
        assert!(v.repondue);
        assert_eq!(v.citations, ["Le seuil de Unity Personal est de 200 000 dollars par année"]);
        // Un geste qui n'est pas un WebFetch ne porte rien.
        let bash = l.blocs(&entree("2026-10-29T10:00:04.000Z", serde_json::json!({"type": "tool_use", "id": "b1", "name": "Bash", "input": {"command": "ls"}})));
        assert!(bash[0].lecture.is_none());
    }
}

/// Le fil de suivi. Il ne rend jamais la main.
///
/// `depuis` est le nombre d'entrees a montrer en arriere quand il s'attache a
/// l'agent qui bouge : assez pour raccrocher le fil, pas assez pour noyer ce
/// qui arrive. Un agent CHOISI se montre en entier : on l'a choisi pour le
/// lire. Dans les deux cas, tout le debut est lu en silence, pour que le
/// lecteur sache a quel appel chaque resultat d'outil repond.
///
/// `choix` porte les choix du rendu, numerotes. Un choix neuf remet tout a
/// zero : le fil se detache, annonce le numero pris en compte, et s'attache a
/// l'agent qui bouge dans ce qu'on lui demande de suivre, ou a celui qu'on a
/// choisi.
pub fn lancer(envoi: Sender<Nouvelle>, choix: Receiver<(u64, Choix)>, premier: Choix, depuis: usize) {
    std::thread::spawn(move || {
        let mut suite: Option<Suite> = None;
        let mut lecteur = Lecteur::nouveau((IMAGE_LARGEUR_MAX, IMAGE_HAUTEUR_MAX));
        let mut voulu = premier;
        // Le dossier du chantier choisi, cherche une fois par choix.
        let mut dossier: Option<PathBuf> = None;
        let mut introuvable_dit = false;

        loop {
            // On attend un choix au plus 400 ms, ce qui remplace l'ancienne
            // pause : un clic prend effet tout de suite, et le disque est relu
            // au meme rythme qu'avant. Si plusieurs choix attendent, seul le
            // dernier compte.
            let mut recu = match choix.recv_timeout(Duration::from_millis(400)) {
                Ok(c) => Some(c),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => None,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    std::thread::sleep(Duration::from_millis(400));
                    None
                }
            };
            while let Ok(c) = choix.try_recv() {
                recu = Some(c);
            }
            if let Some((numero, c)) = recu {
                voulu = c;
                suite = None;
                dossier = None;
                introuvable_dit = false;
                let _ = envoi.send(Nouvelle::Depart(numero));
            }

            let chantier = match &voulu {
                Choix::Direct => dernier_chantier(),
                Choix::Chantier(id) | Choix::Agent { chantier: id, .. } => {
                    if dossier.is_none() {
                        dossier = crate::catalogue::dossier_du_chantier(id);
                    }
                    if dossier.is_none() && !introuvable_dit {
                        let _ = envoi.send(Nouvelle::Introuvable(id.clone()));
                        introuvable_dit = true;
                    }
                    dossier.clone()
                }
            };
            let Some(chantier) = chantier else {
                continue;
            };
            let vivant = match &voulu {
                Choix::Agent { agent, .. } => Some(chantier.join(format!("agent-{agent}.jsonl"))).filter(|p| p.exists()),
                _ => agent_vivant(&chantier),
            };
            let Some(vivant) = vivant else {
                continue;
            };

            if suite.as_ref().map(|s| &s.chemin) != Some(&vivant) {
                let identifiant = vivant
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default()
                    .trim_start_matches("agent-")
                    .trim_end_matches(".jsonl")
                    .to_string();
                let role = role_de(&chantier, &identifiant);
                let nom_chantier = chantier
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();

                let _ = envoi.send(Nouvelle::Agent {
                    role: if role.is_empty() { identifiant.clone() } else { role.clone() },
                    chantier: nom_chantier,
                });
                let _ = envoi.send(Nouvelle::Bloc(Bloc {
                    heure: String::new(),
                    registre: Registre::Releve,
                    outil: String::new(),
                    lecture: None,
                    corps: if role.is_empty() { identifiant } else { role },
                    image: None,
                }));

                let mut s = Suite::nouvelle(vivant.clone());
                let lignes = s.lignes();
                let montre = match voulu {
                    Choix::Agent { .. } => lignes.len(),
                    _ => depuis,
                };
                let debut = lignes.len().saturating_sub(montre);
                lecteur = Lecteur::nouveau((IMAGE_LARGEUR_MAX, IMAGE_HAUTEUR_MAX));
                for (i, l) in lignes.iter().enumerate() {
                    lecteur.muet = i < debut;
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(l) {
                        for b in lecteur.blocs(&v) {
                            let _ = envoi.send(Nouvelle::Bloc(b));
                        }
                    }
                }
                lecteur.muet = false;
                suite = Some(s);
                let _ = envoi.send(Nouvelle::Lu);
                continue;
            }

            // La suite du fichier, depuis ou on s'etait arrete.
            if let Some(s) = suite.as_mut() {
                for l in s.lignes() {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&l) {
                        for b in lecteur.blocs(&v) {
                            let _ = envoi.send(Nouvelle::Bloc(b));
                        }
                    }
                }
            }
        }
    });

}

#[cfg(test)]
mod essais_balisage {
    /// Les deux formes relevees par le cadre, et un texte sans balisage, rendu
    /// tel quel.
    #[test]
    fn la_parole_perd_son_balisage() {
        assert_eq!(super::sans_balisage("- **Manim** : venv dans `C:/Users/x`"), "- Manim : venv dans C:/Users/x");
        assert_eq!(super::sans_balisage("a * b = c"), "a * b = c");
    }

}
