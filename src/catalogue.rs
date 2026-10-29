//! Le catalogue : tous les chantiers, ranges par projet.
//!
//! Le direct suit le chantier le plus recent, et c'est souvent ce qu'on veut.
//! Mais quand plusieurs chantiers tournent dans plusieurs projets, celui qu'on
//! veut regarder n'est pas forcement le dernier lance. Le catalogue les dresse
//! tous, pour qu'on choisisse.
//!
//! Trois faits, releves le 2026-10-03 sur les 139 chantiers presents, decident
//! de tout ce qui suit :
//!
//! 1. Le dossier de lancement ne dit PAS le projet. Une session ouverte dans le
//!    dossier personnel range sous lui des chantiers qui travaillent dans
//!    d'autres depots.
//! 2. Le vrai lieu est le champ « cwd » des transcriptions d'agent, ecrit tantot
//!    a la Windows, tantot a la Git Bash (/c/Users/...). Il varie d'un agent a
//!    l'autre dans un meme chantier : souvent, la plupart restent au dossier
//!    personnel pendant que quelques-uns descendent dans le depot. On vote
//!    donc, sans compter les non-lieux.
//! 3. Un chantier termine laisse un bilan, <session>/workflows/wf_<id>.json,
//!    qui porte son nom, sa description et son statut. Un chantier en cours
//!    n'en a pas encore ; son nom vient alors de son script, qui peut vivre
//!    sous un AUTRE dossier de projet que ses transcriptions.
//!
//! Tout se fait dans un fil a part. Le premier parcours ouvre jusqu'a douze
//! transcriptions par chantier, soit plus d'un millier de fichiers, et ni le
//! rendu ni le suivi ne doivent l'attendre.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};

use crate::suivi::Nouvelle;

/// Le catalogue se refait toutes les trois secondes, comme le dernier chantier
/// du direct : un chantier neuf apparait en moins de trois secondes, et le
/// disque n'est pas martele. Une fois le premier parcours fait, un tour ne
/// rouvre aucun fichier d'un chantier termine.
const PERIODE: Duration = Duration::from_secs(3);

/// Sans bilan, et sans ecriture depuis ce delai, un chantier est dit
/// interrompu : sa session est morte sans conclure. Un outil au premier plan
/// peut tenir dix minutes sans rien ecrire, et une longue reponse en prendre
/// encore quelques-unes ; a quinze, on declarerait interrompu un chantier
/// vivant. Trente laisse la marge, et le prix est connu : une session morte
/// reste « en cours » une demi-heure de trop.
pub const SILENCE_MAX: Duration = Duration::from_secs(30 * 60);

/// Au-dela de ce silence, un chantier en cours se dit muet, avec la duree :
/// c'est le seul signe qu'il est peut-etre bloque, et un coup d'oeil doit le
/// lire sans comparer de tete deux heures (epreuve, tache 4). Dix minutes,
/// le plus long qu'un outil au premier plan tienne sans ecrire (SILENCE_MAX) :
/// en deca, le silence est celui d'un travail ordinaire.
pub const MUET_APRES: Duration = Duration::from_secs(10 * 60);

/// L'instant zero, pour une memoire ecrite avant que la derniere ecriture n'y
/// soit gardee : il ne rend muet aucun chantier, puisque seul un chantier en
/// cours peut l'etre, et qu'un chantier en cours n'est jamais repris de la
/// memoire.
fn epoque() -> SystemTime {
    SystemTime::UNIX_EPOCH
}

/// Les agents lus pour voter le projet d'un chantier. Mesure du 2026-10-03 sur
/// les 139 chantiers presents, contre le vote de TOUS leurs agents : douze
/// agents repartis sur la liste donnent le meme projet partout ; huit se
/// trompent une fois, cinq trois fois, un seul quatorze fois. Lire tous les
/// agents couterait 5 455 ouvertures de fichier.
const ECHANTILLON: usize = 12;

/// La premiere ligne d'une transcription porte la consigne, qui peut etre
/// longue. Au-dela d'un megaoctet on renonce a cet agent plutot que de lire un
/// fichier entier pour un seul champ.
const PREMIERE_LIGNE_MAX: u64 = 1 << 20;

/// Pendant le premier parcours, une liste partielle part toutes les demi-
/// secondes : les chantiers recents, lus d'abord, se voient sans attendre les
/// anciens.
const PARTIEL: Duration = Duration::from_millis(500);

/// Le nom lisible du dossier personnel. Son vrai nom, celui du compte, ne dit pas que
/// c'est la qu'atterrit tout ce qui n'a pas de projet.
pub const DOSSIER_PERSONNEL: &str = "dossier personnel";

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Etat {
    /// Pas de bilan, et une ecriture depuis moins de SILENCE_MAX.
    EnCours,
    /// Un bilan au statut « completed ».
    Fini,
    /// Un bilan a tout autre statut (« killed »), ou plus rien depuis
    /// SILENCE_MAX sans bilan.
    Interrompu,
}

impl Etat {
    pub fn mot(self) -> &'static str {
        match self {
            Etat::EnCours => "en cours",
            Etat::Fini => "fini",
            Etat::Interrompu => "interrompu",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Chantier {
    /// wf_<id>, le nom de son dossier.
    pub id: String,
    pub nom: String,
    /// Vide quand ni le bilan ni le script n'en donnent.
    pub description: String,
    pub etat: Etat,
    /// Son lancement : la creation de son dossier. C'est sur lui que se
    /// trient les chantiers et les projets, parce qu'il ne bouge plus. Trie
    /// sur la derniere ecriture, la bande se reordonnait toutes les trois
    /// secondes tant que deux projets travaillaient, et l'onglet qu'on allait
    /// cliquer changeait de place sous la souris (vu le 2026-10-03). La
    /// derniere ecriture, elle, ne sert qu'a l'etat.
    pub debut: SystemTime,
    /// La derniere ecriture d'un de ses agents : ce qui dit qu'il est muet.
    #[serde(default = "epoque")]
    pub derniere: SystemTime,
    pub agents: usize,
    pub projet: String,
}

impl Chantier {
    /// Le silence d'un chantier en cours, des qu'il passe MUET_APRES.
    pub fn muet(&self, maintenant: SystemTime) -> Option<Duration> {
        if self.etat != Etat::EnCours {
            return None;
        }
        let silence = maintenant.duration_since(self.derniere).unwrap_or_default();
        (silence >= MUET_APRES).then_some(silence)
    }

    /// Ce que l'etat dit en un mot ; « muet depuis 12 min » quand il l'est.
    pub fn mot(&self, maintenant: SystemTime) -> String {
        match self.muet(maintenant) {
            Some(d) => format!("muet depuis {} min", d.as_secs() / 60),
            None => self.etat.mot().to_string(),
        }
    }

    /// Quelque chose y travaille en ce moment : en cours, et pas muet. C'est
    /// ce que dit la pastille, et elle ne dit que cela.
    pub fn vivant(&self, maintenant: SystemTime) -> bool {
        self.etat == Etat::EnCours && self.muet(maintenant).is_none()
    }
}

#[derive(Clone, Debug, Default)]
pub struct Catalogue {
    /// Du plus recemment actif au plus ancien.
    pub chantiers: Vec<Chantier>,
    /// Faux tant que le premier parcours n'est pas termine : un projet absent
    /// n'est alors pas encore un projet inconnu.
    pub complet: bool,
}

/// Du plus recemment lance au plus ancien ; l'identifiant departage, pour
/// que deux tours donnent le meme ordre.
fn trier(chantiers: &mut [Chantier]) {
    chantiers.sort_by(|a, b| b.debut.cmp(&a.debut).then_with(|| a.id.cmp(&b.id)));
}

impl Catalogue {
    /// Les projets, du plus recemment lance au plus ancien. Les chantiers sont
    /// deja dans cet ordre : un projet prend la place de son plus recent.
    pub fn projets(&self) -> Vec<String> {
        let mut vus: Vec<String> = Vec::new();
        for c in &self.chantiers {
            if !vus.contains(&c.projet) {
                vus.push(c.projet.clone());
            }
        }
        vus
    }

    pub fn de<'a>(&'a self, projet: &'a str) -> impl Iterator<Item = &'a Chantier> + 'a {
        self.chantiers.iter().filter(move |c| c.projet == projet)
    }

    pub fn chantier(&self, id: &str) -> Option<&Chantier> {
        self.chantiers.iter().find(|c| c.id == id)
    }
}

/// Le dossier personnel, tel que le systeme le donne.
fn accueil() -> String {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(|s| normaliser(&s))
        .unwrap_or_default()
}

/// Un chemin a la Windows, sans separateur final. L'ecriture Git Bash
/// (/c/Users/...) et la Windows (C:\Users\...) se trouvent dans le MEME
/// fichier : sans cela, un meme depot ferait deux projets.
pub fn normaliser(cwd: &str) -> String {
    let mut c = cwd.trim().replace('/', "\\");
    let b = c.as_bytes();
    if b.len() >= 2 && b[0] == b'\\' && b[1].is_ascii_alphabetic() && (b.len() == 2 || b[2] == b'\\') {
        c = format!("{}:{}", (b[1] as char).to_ascii_uppercase(), &c[2..]);
    }
    if c.len() == 2 && c.ends_with(':') {
        c.push('\\');
    }
    // On garde la barre d'une racine de disque, « C:\ » ; on ote les autres.
    while c.ends_with('\\') && !(c.len() == 3 && c.as_bytes()[1] == b':') {
        c.pop();
    }
    c
}

fn egaux(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

fn sous(chemin: &str, parent: &str) -> bool {
    egaux(chemin, parent)
        || (chemin.len() > parent.len()
            && chemin.is_char_boundary(parent.len())
            && egaux(&chemin[..parent.len()], parent)
            && chemin.as_bytes()[parent.len()] == b'\\')
}

/// Un lieu de travail, ou un non-lieu. Le dossier personnel, les fichiers
/// temporaires et les reglages de l'outil sont des endroits d'ou l'on part ou
/// ou l'on range des brouillons, pas des projets : un agent qui n'est jamais
/// descendu dans un depot ne dit rien du projet de son chantier.
pub fn est_un_lieu(chemin: &str, accueil: &str) -> bool {
    if accueil.is_empty() {
        return true;
    }
    !(egaux(chemin, accueil)
        || sous(chemin, &format!("{accueil}\\AppData"))
        || sous(chemin, &format!("{accueil}\\.claude")))
}

/// Le nom qu'on lit dans un onglet : le dernier composant du chemin, sauf pour
/// les deux dossiers dont ce composant ne dit rien.
pub fn nom_lisible(racine: &str, accueil: &str) -> String {
    if accueil.is_empty() || egaux(racine, accueil) {
        return DOSSIER_PERSONNEL.to_string();
    }
    if egaux(racine, &format!("{accueil}\\Desktop")) {
        return "Bureau".to_string();
    }
    racine
        .rsplit('\\')
        .find(|s| !s.is_empty())
        .unwrap_or(racine)
        .to_string()
}

/// Le depot qui contient un chemin : le premier parent qui porte un « .git »
/// (dossier, ou fichier pour un arbre de travail). Sans depot, le chemin
/// lui-meme. Un agent qui travaille dans veilleuse\recherche\verre-taille
/// travaille dans veilleuse.
fn racine_du_depot(chemin: &str) -> String {
    let mut p = Path::new(chemin);
    loop {
        if p.join(".git").exists() {
            return p.to_string_lossy().to_string();
        }
        match p.parent() {
            Some(q) if q != p => p = q,
            _ => return chemin.to_string(),
        }
    }
}

/// Le projet d'un chantier, a partir du (moment, lieu) de ses agents : le lieu
/// le plus frequent, ramene a la racine de son depot. A egalite, celui de
/// l'agent le plus ancien, qui a ouvert le travail. Sans aucun lieu, le
/// dossier personnel, puisque c'est de la que tout est parti.
pub fn voter(agents: &[(String, String)], accueil: &str, racine: &mut dyn FnMut(&str) -> String) -> String {
    let mut ordre: Vec<&(String, String)> = agents.iter().collect();
    ordre.sort_by(|a, b| a.0.cmp(&b.0));
    // (racine, voix, rang du premier agent)
    let mut comptes: Vec<(String, usize, usize)> = Vec::new();
    for (rang, (_, lieu)) in ordre.iter().enumerate() {
        if !est_un_lieu(lieu, accueil) {
            continue;
        }
        let r = racine(lieu);
        match comptes.iter_mut().find(|c| egaux(&c.0, &r)) {
            Some(c) => c.1 += 1,
            None => comptes.push((r, 1, rang)),
        }
    }
    comptes.sort_by(|a, b| b.1.cmp(&a.1).then(a.2.cmp(&b.2)));
    match comptes.first() {
        Some((r, _, _)) => nom_lisible(r, accueil),
        None => DOSSIER_PERSONNEL.to_string(),
    }
}

/// L'etat d'un chantier, selon la regle ecrite sur `Etat`.
pub fn etat(statut: Option<&str>, activite: SystemTime, maintenant: SystemTime) -> Etat {
    match statut {
        Some("completed") => Etat::Fini,
        Some(_) => Etat::Interrompu,
        None => {
            // Une ecriture « dans le futur » (horloges decalees) compte pour
            // une ecriture d'a l'instant.
            let silence = maintenant.duration_since(activite).unwrap_or_default();
            if silence < SILENCE_MAX {
                Etat::EnCours
            } else {
                Etat::Interrompu
            }
        }
    }
}

/// Une chaine JavaScript : '...', "..." ou `...`, avec ses echappements.
fn litteral(s: &str) -> Option<String> {
    let mut it = s.chars();
    let q = it.next()?;
    if !matches!(q, '\'' | '"' | '`') {
        return None;
    }
    let mut sortie = String::new();
    while let Some(c) = it.next() {
        match c {
            '\\' => match it.next()? {
                'n' | 't' | 'r' => sortie.push(' '),
                autre => sortie.push(autre),
            },
            c if c == q => return Some(sortie),
            c => sortie.push(c),
        }
    }
    None
}

/// La valeur d'une cle « cle: '...' ». La cle doit etre un mot entier :
/// « name » ne doit pas se lire dans « workflowName ».
fn valeur(zone: &str, cle: &str) -> Option<String> {
    let mut depart = 0;
    while let Some(i) = zone[depart..].find(cle) {
        let i = depart + i;
        let avant = zone[..i].chars().next_back();
        let mot_entier = !avant.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '$');
        let reste = zone[i + cle.len()..].trim_start();
        if mot_entier {
            if let Some(r) = reste.strip_prefix(':') {
                return litteral(r.trim_start());
            }
        }
        depart = i + cle.len();
    }
    None
}

/// Le nom et la description qu'un script declare en tete :
/// « export const meta = { name: '...', description: '...' ».
pub fn meta_du_script(texte: &str) -> (Option<String>, Option<String>) {
    let debut = texte.find("export const meta").unwrap_or(0);
    let zone = &texte[debut..];
    (valeur(zone, "name"), valeur(zone, "description"))
}

/// Le moment et le lieu du premier agent d'une transcription : la premiere
/// ligne qui porte un « cwd ». Toutes celles relevees le 2026-10-03 le
/// portaient des la premiere ligne ; on en tolere cinq.
fn premier_lieu(agent: &Path) -> Option<(String, String)> {
    let f = std::fs::File::open(agent).ok()?;
    let mut r = BufReader::new(f.take(PREMIERE_LIGNE_MAX));
    let mut ligne = String::new();
    for _ in 0..5 {
        ligne.clear();
        if r.read_line(&mut ligne).ok()? == 0 {
            return None;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(ligne.trim()) else {
            continue;
        };
        if let Some(cwd) = v.get("cwd").and_then(|x| x.as_str()) {
            let moment = v.get("timestamp").and_then(|x| x.as_str()).unwrap_or("").to_string();
            return Some((moment, normaliser(cwd)));
        }
    }
    None
}

/// Le nom, la description et le statut d'un bilan.
fn lire_bilan(chemin: &Path) -> Option<(String, String, String)> {
    let octets = std::fs::read(chemin).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&octets).ok()?;
    let s = |c: &str| v.get(c).and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
    Some((s("workflowName"), s("summary"), s("status")))
}

/// L'en-tete d'un script : quatre kilo-octets suffisent largement a « meta ».
fn lire_meta(chemin: &Path) -> (Option<String>, Option<String>) {
    let Ok(f) = std::fs::File::open(chemin) else {
        return (None, None);
    };
    let mut tete = Vec::new();
    let _ = f.take(4096).read_to_end(&mut tete);
    meta_du_script(&String::from_utf8_lossy(&tete))
}

/// Ce que le parcours garde d'un tour a l'autre. Rien de ce qui est ici ne
/// change une fois lu : la premiere ligne d'un agent, la racine d'un chemin, le
/// bilan d'un chantier termine, l'en-tete d'un script.
#[derive(Default)]
struct Memoire {
    lieux: HashMap<PathBuf, Option<(String, String)>>,
    racines: HashMap<String, String>,
    /// Par chantier : la taille et la date de son bilan, et ce qu'on en a tire.
    termines: HashMap<String, (u64, SystemTime, Chantier)>,
    metas: HashMap<PathBuf, (Option<String>, Option<String>)>,
}

/// Un dossier de chantier trouve sur le disque, avant lecture.
struct Trouve {
    id: String,
    dossier: PathBuf,
    /// Son bilan, s'il existe : chemin, taille, date.
    bilan: Option<(PathBuf, u64, SystemTime)>,
    /// La date du dossier, pour lire les recents d'abord.
    date: SystemTime,
    /// Sa creation : le lancement du chantier.
    debut: SystemTime,
}

fn entrees(d: &Path) -> Vec<std::fs::DirEntry> {
    std::fs::read_dir(d)
        .map(|r| r.filter_map(|e| e.ok()).collect())
        .unwrap_or_default()
}

/// Le dossier d'un chantier, d'apres son identifiant, sous n'importe quelle
/// session de n'importe quel projet.
pub fn dossier_du_chantier(id: &str) -> Option<PathBuf> {
    for projet in entrees(&crate::suivi::projets()) {
        for session in entrees(&projet.path()) {
            let d = session.path().join("subagents").join("workflows").join(id);
            if d.is_dir() {
                return Some(d);
            }
        }
    }
    None
}

/// Ce que la memoire laisse sur le disque, dans `%APPDATA%\Vigie`. Sans elle,
/// chaque lancement relisait la premiere ligne de chaque agent : 62,4 s pour
/// 1 470 agents le 2026-10-27, pendant lesquelles les onglets se remplissaient
/// un a un ; avec elle, 0,0 s au lancement suivant.
///
/// Seul s'y garde ce qui ne peut plus changer : la premiere ligne d'un agent,
/// quand elle a ete trouvee ; le bilan d'un chantier termine, revalide par sa
/// taille et sa date ; l'en-tete d'un script. La racine git d'un dossier n'y
/// est pas : un depot cree depuis la changerait, et la relire ne coute que
/// quelques stat.
#[derive(Serialize, Deserialize)]
struct Garde {
    version: u32,
    lieux: HashMap<PathBuf, (String, String)>,
    termines: HashMap<String, (u64, SystemTime, Chantier)>,
    metas: HashMap<PathBuf, (Option<String>, Option<String>)>,
}

/// Un changement de forme du fichier change ce numero : une memoire d'une autre
/// forme est ignoree, et refaite.
const GARDE_VERSION: u32 = 1;
const GARDE_NOM: &str = "catalogue.json";

impl Memoire {
    /// La memoire laissee par le lancement precedent, ou une memoire vide.
    fn relue() -> Memoire {
        crate::app::lire(GARDE_NOM)
            .and_then(|o| Memoire::depuis(&o))
            .unwrap_or_default()
    }

    fn depuis(octets: &[u8]) -> Option<Memoire> {
        let g = serde_json::from_slice::<Garde>(octets).ok().filter(|g| g.version == GARDE_VERSION)?;
        Some(Memoire {
            lieux: g.lieux.into_iter().map(|(k, v)| (k, Some(v))).collect(),
            racines: HashMap::new(),
            termines: g.termines,
            metas: g.metas,
        })
    }

    /// Ce qui a ete appris depuis : un compte qui ne fait que croitre.
    fn poids(&self) -> usize {
        self.lieux.values().filter(|l| l.is_some()).count() + self.termines.len() + self.metas.len()
    }

    fn garder(&self) {
        if let Some(octets) = self.en_octets() {
            crate::app::ecrire(GARDE_NOM, &octets);
        }
    }

    fn en_octets(&self) -> Option<Vec<u8>> {
        let g = Garde {
            version: GARDE_VERSION,
            lieux: self
                .lieux
                .iter()
                .filter_map(|(k, v)| Some((k.clone(), v.clone()?)))
                .collect(),
            termines: self.termines.clone(),
            metas: self.metas.clone(),
        };
        serde_json::to_vec(&g).ok()
    }

    /// Le projet d'un chantier, par le vote de son echantillon d'agents.
    fn projet(&mut self, agents: &[PathBuf], accueil: &str) -> Option<String> {
        let n = agents.len();
        let mut indices: Vec<usize> = (0..ECHANTILLON.min(n)).map(|i| i * n / ECHANTILLON.min(n)).collect();
        indices.dedup();
        let mut lus = Vec::new();
        for i in indices {
            let a = &agents[i];
            let lieu = self.lieux.entry(a.clone()).or_insert_with(|| premier_lieu(a)).clone();
            if let Some(l) = lieu {
                lus.push(l);
            }
        }
        if lus.is_empty() {
            return None;
        }
        let racines = &mut self.racines;
        let mut racine = |c: &str| -> String {
            racines
                .entry(c.to_string())
                .or_insert_with(|| racine_du_depot(c))
                .clone()
        };
        Some(voter(&lus, accueil, &mut racine))
    }

    /// Un chantier, lu ou repris de la memoire. Aucun si personne n'y a
    /// encore ecrit : sans agent, il n'y a rien a suivre ni de projet a dire.
    fn chantier(
        &mut self,
        t: &Trouve,
        scripts: &HashMap<String, PathBuf>,
        accueil: &str,
        maintenant: SystemTime,
    ) -> Option<Chantier> {
        if let Some((chemin, taille, date)) = &t.bilan {
            if let Some((tl, dt, c)) = self.termines.get(&t.id) {
                if tl == taille && dt == date {
                    return Some(c.clone());
                }
            }
            // Un bilan illisible est un bilan en cours d'ecriture : on le
            // traite comme absent, sans rien en retenir, et le tour suivant le
            // relira.
            if let Some((nom, description, statut)) = lire_bilan(chemin) {
                let (agents, _) = agents_et_activite(&t.dossier);
                let projet = self.projet(&agents, accueil)?;
                let (nom_s, desc_s) = self.meta(&t.id, scripts);
                let c = Chantier {
                    id: t.id.clone(),
                    nom: premier_non_vide([nom, nom_s.unwrap_or_default(), nom_de_fichier(&t.id, scripts)], &t.id),
                    description: premier_non_vide([description, desc_s.unwrap_or_default()], ""),
                    etat: etat(Some(statut.as_str()), *date, maintenant),
                    debut: t.debut,
                    derniere: *date,
                    agents: agents.len(),
                    projet,
                };
                self.termines.insert(t.id.clone(), (*taille, *date, c.clone()));
                return Some(c);
            }
        }
        let (agents, activite) = agents_et_activite(&t.dossier);
        let projet = self.projet(&agents, accueil)?;
        let (nom_s, desc_s) = self.meta(&t.id, scripts);
        Some(Chantier {
            id: t.id.clone(),
            nom: premier_non_vide([nom_s.unwrap_or_default(), nom_de_fichier(&t.id, scripts)], &t.id),
            description: desc_s.unwrap_or_default(),
            etat: etat(None, activite.unwrap_or(t.date), maintenant),
            debut: t.debut,
            derniere: activite.unwrap_or(t.date),
            agents: agents.len(),
            projet,
        })
    }

    fn meta(&mut self, id: &str, scripts: &HashMap<String, PathBuf>) -> (Option<String>, Option<String>) {
        match scripts.get(id) {
            Some(s) => self.metas.entry(s.clone()).or_insert_with(|| lire_meta(s)).clone(),
            None => (None, None),
        }
    }
}

/// Le nom tire du fichier du script, « <nom>-wf_<id>.js » :
/// le recours quand l'en-tete ne se lit pas.
fn nom_de_fichier(id: &str, scripts: &HashMap<String, PathBuf>) -> String {
    scripts
        .get(id)
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .and_then(|n| n.split(&format!("-{id}")).next().map(str::to_string))
        .filter(|n| !n.ends_with(".js"))
        .unwrap_or_default()
}

fn premier_non_vide<const N: usize>(candidats: [String; N], repli: &str) -> String {
    candidats
        .into_iter()
        .find(|s| !s.trim().is_empty())
        .unwrap_or_else(|| repli.to_string())
}

/// Les transcriptions d'agent d'un chantier, triees par nom, et la derniere
/// ecriture dans son dossier. Sous Windows, la date d'une entree vient avec la
/// liste du repertoire : aucun fichier n'est ouvert.
fn agents_et_activite(dossier: &Path) -> (Vec<PathBuf>, Option<SystemTime>) {
    let mut agents = Vec::new();
    let mut derniere: Option<SystemTime> = None;
    for e in entrees(dossier) {
        let nom = e.file_name().to_string_lossy().to_string();
        if let Some(t) = e.metadata().ok().and_then(|m| m.modified().ok()) {
            derniere = Some(derniere.map_or(t, |d| d.max(t)));
        }
        if nom.starts_with("agent-") && nom.ends_with(".jsonl") {
            agents.push(e.path());
        }
    }
    agents.sort();
    (agents, derniere)
}

/// Tous les dossiers de chantier, avec leur bilan, et l'index des scripts par
/// identifiant. Rien n'est ouvert ici : ce ne sont que des listes de
/// repertoires.
fn inventaire() -> (Vec<Trouve>, HashMap<String, PathBuf>) {
    let mut trouves = Vec::new();
    let mut scripts = HashMap::new();
    for projet in entrees(&crate::suivi::projets()) {
        for session in entrees(&projet.path()) {
            let s = session.path();
            for f in entrees(&s.join("workflows").join("scripts")) {
                let n = f.file_name().to_string_lossy().to_string();
                if let Some(i) = n.rfind("-wf_") {
                    if let Some(id) = n[i + 1..].strip_suffix(".js") {
                        scripts.insert(id.to_string(), f.path());
                    }
                }
            }
            let mut bilans: HashMap<String, (PathBuf, u64, SystemTime)> = HashMap::new();
            for f in entrees(&s.join("workflows")) {
                let n = f.file_name().to_string_lossy().to_string();
                if let (Some(id), Ok(m)) = (n.strip_suffix(".json"), f.metadata()) {
                    if id.starts_with("wf_") && m.is_file() {
                        let date = m.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                        bilans.insert(id.to_string(), (f.path(), m.len(), date));
                    }
                }
            }
            for c in entrees(&s.join("subagents").join("workflows")) {
                let id = c.file_name().to_string_lossy().to_string();
                if !id.starts_with("wf_") {
                    continue;
                }
                let m = c.metadata().ok();
                let date = m
                    .as_ref()
                    .and_then(|m| m.modified().ok())
                    .unwrap_or(SystemTime::UNIX_EPOCH);
                let debut = m.as_ref().and_then(|m| m.created().ok()).unwrap_or(date);
                trouves.push(Trouve {
                    bilan: bilans.remove(&id),
                    id,
                    dossier: c.path(),
                    date,
                    debut,
                });
            }
        }
    }
    (trouves, scripts)
}

/// Le fil du catalogue. Il ne rend jamais la main.
pub fn lancer(envoi: Sender<Nouvelle>) {
    std::thread::spawn(move || {
        let mut memoire = Memoire::relue();
        let mut garde = memoire.poids();
        let accueil = accueil();
        let mut complet = false;
        loop {
            let debut = Instant::now();
            let (mut trouves, scripts) = inventaire();
            // Les plus recents d'abord : ce sont ceux qu'on cherche, et ils
            // arrivent ainsi des les premieres listes partielles.
            let date = |t: &Trouve| t.bilan.as_ref().map_or(t.date, |(_, _, d)| (*d).max(t.date));
            trouves.sort_by_key(|t| std::cmp::Reverse(date(t)));
            let maintenant = crate::app::maintenant();
            let mut chantiers = Vec::with_capacity(trouves.len());
            let mut dernier_envoi = Instant::now();
            for t in &trouves {
                if let Some(c) = memoire.chantier(t, &scripts, &accueil, maintenant) {
                    chantiers.push(c);
                }
                if !complet && dernier_envoi.elapsed() >= PARTIEL {
                    let mut partiel = chantiers.clone();
                    trier(&mut partiel);
                    let _ = envoi.send(Nouvelle::Catalogue(Arc::new(Catalogue {
                        chantiers: partiel,
                        complet: false,
                    })));
                    dernier_envoi = Instant::now();
                }
            }
            trier(&mut chantiers);
            if !complet {
                eprintln!(
                    "catalogue : {} chantiers, {} agents lus, premier parcours en {:.1} s",
                    chantiers.len(),
                    memoire.lieux.len(),
                    debut.elapsed().as_secs_f32()
                );
            }
            complet = true;
            // Gardee quand elle a appris quelque chose : apres le premier
            // parcours, et a chaque chantier neuf ou termine.
            if memoire.poids() != garde {
                memoire.garder();
                garde = memoire.poids();
            }
            if envoi
                .send(Nouvelle::Catalogue(Arc::new(Catalogue { chantiers, complet })))
                .is_err()
            {
                return;
            }
            std::thread::sleep(PERIODE);
        }
    });
}

#[cfg(test)]
mod essais {
    use super::*;

    const ACCUEIL: &str = r"C:\Users\moi";

    #[test]
    fn les_deux_ecritures_d_un_chemin_se_confondent() {
        let attendu = r"C:\Users\moi\dev\active\relais";
        assert_eq!(normaliser("/c/Users/moi/dev/active/relais"), attendu);
        assert_eq!(normaliser(r"C:\Users\moi\dev\active\relais\"), attendu);
        assert_eq!(normaliser("C:/Users/moi/dev/active/relais"), attendu);
        assert_eq!(normaliser("/c"), r"C:\");
        assert_eq!(normaliser(r"C:\"), r"C:\");
    }

    #[test]
    fn les_non_lieux_ne_votent_pas() {
        assert!(!est_un_lieu(ACCUEIL, ACCUEIL));
        assert!(!est_un_lieu(r"c:\users\moi", ACCUEIL));
        assert!(!est_un_lieu(r"C:\Users\moi\AppData\Local\Temp\claude\x\scratchpad", ACCUEIL));
        assert!(!est_un_lieu(r"C:\Users\moi\.claude\projects", ACCUEIL));
        assert!(est_un_lieu(r"C:\Users\moi\dev\active\relais", ACCUEIL));
        assert!(est_un_lieu(r"C:\Users\moi\Desktop", ACCUEIL));
        // Un dossier dont le nom COMMENCE comme un non-lieu n'en est pas un.
        assert!(est_un_lieu(r"C:\Users\moi\.claudette", ACCUEIL));
    }

    #[test]
    fn les_noms_lisibles() {
        assert_eq!(nom_lisible(ACCUEIL, ACCUEIL), DOSSIER_PERSONNEL);
        assert_eq!(nom_lisible(r"C:\Users\moi\Desktop", ACCUEIL), "Bureau");
        assert_eq!(nom_lisible(r"C:\Users\moi\dev\active\relais", ACCUEIL), "relais");
    }

    fn sans_depot(c: &str) -> String {
        c.to_string()
    }

    fn a(moment: &str, lieu: &str) -> (String, String) {
        (moment.to_string(), lieu.to_string())
    }

    /// La forme relevee sur un chantier reel : la plupart des
    /// agents au dossier personnel, deux dans le depot. Le depot l'emporte.
    #[test]
    fn le_depot_l_emporte_sur_le_dossier_personnel() {
        let agents = [
            a("2026-09-01T10:00:00Z", ACCUEIL),
            a("2026-09-01T10:01:00Z", ACCUEIL),
            a("2026-09-01T10:02:00Z", r"C:\Users\moi\dev\active\rudin-recompose"),
            a("2026-09-01T10:03:00Z", ACCUEIL),
            a("2026-09-01T10:04:00Z", "/c/Users/moi/dev/active/rudin-recompose"),
        ];
        let agents: Vec<_> = agents.iter().map(|(m, l)| (m.clone(), normaliser(l))).collect();
        assert_eq!(voter(&agents, ACCUEIL, &mut sans_depot), "rudin-recompose");
    }

    #[test]
    fn a_egalite_l_agent_le_plus_ancien_decide() {
        let agents = [
            a("2026-09-01T10:05:00Z", r"C:\Users\moi\dev\active\atelier"),
            a("2026-09-01T10:00:00Z", r"C:\Users\moi\dev\active\projecteur"),
            a("2026-09-01T10:09:00Z", r"C:\Users\moi\dev\active\atelier"),
            a("2026-09-01T10:01:00Z", r"C:\Users\moi\dev\active\projecteur"),
        ];
        assert_eq!(voter(&agents, ACCUEIL, &mut sans_depot), "projecteur");
    }

    #[test]
    fn sans_aucun_lieu_le_dossier_personnel() {
        let agents = [
            a("1", ACCUEIL),
            a("2", r"C:\Users\moi\AppData\Local\Temp\claude\s\scratchpad"),
        ];
        assert_eq!(voter(&agents, ACCUEIL, &mut sans_depot), DOSSIER_PERSONNEL);
        assert_eq!(voter(&[], ACCUEIL, &mut sans_depot), DOSSIER_PERSONNEL);
    }

    #[test]
    fn un_sous_dossier_se_ramene_a_son_depot() {
        let mut racine = |c: &str| {
            if c.to_lowercase().starts_with(r"c:\users\moi\dev\active\veilleuse") {
                r"C:\Users\moi\dev\active\veilleuse".to_string()
            } else {
                c.to_string()
            }
        };
        let agents = [a("1", r"C:\Users\moi\dev\active\veilleuse\recherche\verre-taille")];
        assert_eq!(voter(&agents, ACCUEIL, &mut racine), "veilleuse");
    }

    #[test]
    fn la_regle_d_etat() {
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(1_790_000_000);
        let minute = Duration::from_secs(60);
        assert_eq!(etat(Some("completed"), t, t + 5 * minute), Etat::Fini);
        assert_eq!(etat(Some("killed"), t, t + minute), Etat::Interrompu);
        assert_eq!(etat(None, t, t + minute), Etat::EnCours);
        assert_eq!(etat(None, t, t + 29 * minute), Etat::EnCours);
        assert_eq!(etat(None, t, t + 31 * minute), Etat::Interrompu);
        // Horloges decalees : une ecriture dans le futur est une ecriture.
        assert_eq!(etat(None, t + minute, t), Etat::EnCours);
    }

    /// Le muet de l'epreuve, tache 4 : l'agent qui se tait 2 min est en cours,
    /// celui qui se tait 25 min le dit, et un chantier fini n'est jamais muet.
    #[test]
    fn un_silence_long_se_dit() {
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(1_793_114_556);
        let minute = Duration::from_secs(60);
        let c = |etat: Etat| Chantier {
            id: "wf_a".into(),
            nom: "a".into(),
            description: String::new(),
            etat,
            debut: t,
            derniere: t,
            agents: 1,
            projet: "p".into(),
        };
        let vif = c(Etat::EnCours);
        assert_eq!(vif.mot(t + 2 * minute), "en cours");
        assert!(vif.vivant(t + 2 * minute));
        assert_eq!(vif.mot(t + 9 * minute + Duration::from_secs(59)), "en cours");
        assert_eq!(vif.mot(t + 10 * minute), "muet depuis 10 min");
        assert_eq!(vif.mot(t + 25 * minute), "muet depuis 25 min");
        assert!(!vif.vivant(t + 25 * minute));
        assert_eq!(c(Etat::Fini).mot(t + 25 * minute), "fini");
        assert!(c(Etat::Fini).muet(t + 25 * minute).is_none());
    }

    /// L'en-tete exact d'un vrai script, releve le 2026-10-03,
    /// et un echappement que les scripts de Stone contiennent souvent.
    #[test]
    fn l_en_tete_d_un_script() {
        let texte = "export const meta = {\n  name: 'relais-studio-design',\n  description: 'Studio de design de relais : fondations, deux directions artistiques avec maquettes et motion, juge indépendant',\n  phases: [\n";
        let (n, d) = meta_du_script(texte);
        assert_eq!(n.as_deref(), Some("relais-studio-design"));
        assert_eq!(
            d.as_deref(),
            Some("Studio de design de relais : fondations, deux directions artistiques avec maquettes et motion, juge indépendant")
        );
        // « filename » finit par « name » : la cle doit etre un mot entier.
        let (n, d) = meta_du_script("export const meta = { filename: 'non', name: \"l'outil\", description: 'l\\'agent dit' }");
        assert_eq!(n.as_deref(), Some("l'outil"));
        assert_eq!(d.as_deref(), Some("l'agent dit"));
        assert_eq!(meta_du_script("const x = 1"), (None, None));
    }

    #[test]
    fn le_nom_tire_du_fichier() {
        let mut s = HashMap::new();
        s.insert(
            "wf_5f0590e4-61c".to_string(),
            PathBuf::from(r"C:\x\workflows\scripts\relais-briques-wf_5f0590e4-61c.js"),
        );
        assert_eq!(nom_de_fichier("wf_5f0590e4-61c", &s), "relais-briques");
        assert_eq!(nom_de_fichier("wf_absent", &s), "");
    }

    #[test]
    fn les_projets_par_lancement_recent() {
        let t = SystemTime::UNIX_EPOCH;
        let c = |id: &str, projet: &str, s: u64| Chantier {
            id: id.into(),
            nom: id.into(),
            description: String::new(),
            etat: Etat::Fini,
            debut: t + Duration::from_secs(s),
            derniere: t + Duration::from_secs(s),
            agents: 1,
            projet: projet.into(),
        };
        let mut v = vec![c("a", "relais", 10), c("b", "vigie", 30), c("c", "relais", 50)];
        trier(&mut v);
        let cat = Catalogue { chantiers: v, complet: true };
        assert_eq!(cat.projets(), vec!["relais", "vigie"]);
        assert_eq!(cat.de("relais").map(|c| c.id.as_str()).collect::<Vec<_>>(), vec!["c", "a"]);
    }
}

#[cfg(test)]
mod essais_garde {
    use super::*;

    /// La memoire relue est celle qu'on a gardee, a une exception voulue : un
    /// agent dont la premiere ligne n'a pas ete trouvee n'est pas garde, pour
    /// etre relu au lancement suivant. Et une memoire d'une autre version est
    /// ignoree.
    #[test]
    fn la_memoire_se_relit_au_lancement_suivant() {
        let mut m = Memoire::default();
        m.lieux.insert(PathBuf::from(r"C:\a\agent-1.jsonl"), Some(("2026-10-27T09:49:00Z".into(), r"C:\dev\vigie".into())));
        m.lieux.insert(PathBuf::from(r"C:\a\agent-2.jsonl"), None);
        m.racines.insert(r"C:\dev\vigie\src".into(), r"C:\dev\vigie".into());
        let c = Chantier {
            id: "wf_a".into(),
            nom: "design".into(),
            description: String::new(),
            etat: Etat::Fini,
            debut: SystemTime::UNIX_EPOCH + Duration::from_secs(1_793_109_127),
            derniere: SystemTime::UNIX_EPOCH + Duration::from_secs(1_793_109_127),
            agents: 7,
            projet: "cours-llm".into(),
        };
        m.termines.insert("wf_a".into(), (42, SystemTime::UNIX_EPOCH + Duration::from_millis(1_793_109_127_250), c));
        m.metas.insert(PathBuf::from(r"C:\s\x.js"), (Some("design".into()), None));

        let r = Memoire::depuis(&m.en_octets().unwrap()).unwrap();
        assert_eq!(r.lieux.len(), 1);
        assert_eq!(r.lieux[&PathBuf::from(r"C:\a\agent-1.jsonl")], m.lieux[&PathBuf::from(r"C:\a\agent-1.jsonl")]);
        assert!(r.racines.is_empty());
        let (t, d, c) = &r.termines["wf_a"];
        assert_eq!((*t, *d, c.nom.as_str(), c.etat, c.debut, c.agents), (42, m.termines["wf_a"].1, "design", Etat::Fini, m.termines["wf_a"].2.debut, 7));
        assert_eq!(r.metas, m.metas);
        assert_eq!(r.poids(), m.poids());

        let autre = String::from_utf8(m.en_octets().unwrap()).unwrap().replacen("\"version\":1", "\"version\":0", 1);
        assert!(Memoire::depuis(autre.as_bytes()).is_none());
        assert!(Memoire::depuis(b"{tronque").is_none());
    }
}
