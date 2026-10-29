//! La fiche d'un chantier : ses agents, et tout ce qu'ils ont vu ou produit.
//!
//! Entre la liste d'un projet et le fil d'un agent, il manquait le chantier
//! lui-meme. Un chantier de design a cinq chercheurs ; le direct ne montre que
//! celui qui ecrit, et c'est rarement celui qui fait les images. La fiche les
//! nomme tous, dit ou en est chacun, et pose sur une seule planche toutes les
//! images du chantier, chacune sous le nom de l'agent qui l'a faite.
//!
//! Elle se dresse dans son propre fil, avec le MEME lecteur que le fil d'un
//! agent : une image est sur la planche si et seulement si elle est dans un
//! fil. Chaque transcription est lue au fil de l'eau, une seule fois ; seul
//! ce qui s'y ajoute est relu.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use crate::suivi::{Image, Lecteur, Nouvelle, Registre, Suite};

/// Une vignette tient dans 480 px : trois de front dans la colonne, a
/// l'echelle 1,25 de cet ecran, en prennent chacune un peu plus de 300, et
/// 480 laisse la marge d'un zoom. Plus grand, un chantier de figures (146
/// images le 2026-10-27) tiendrait pres de deux cents megaoctets.
const VIGNETTE_MAX: (u32, u32) = (480, 360);
/// Au-dela, la planche s'arrete et le dit. Deux cents vignettes font environ
/// cent quarante megaoctets ; aucun chantier vu n'en approche.
pub const VIGNETTES_MAX: usize = 200;
/// Le rythme de relecture d'un chantier ouvert. Un peu plus lent que le fil :
/// la fiche se regarde, elle ne se lit pas mot a mot.
const PERIODE: Duration = Duration::from_millis(1500);
/// Un agent sans bilan dont la transcription n'a pas bouge depuis ce temps
/// n'est plus dit au travail. La meme raison que les trente minutes d'un
/// chantier, a l'echelle d'un agent : un outil long peut tenir dix minutes.
const SILENCE_AGENT: Duration = Duration::from_secs(15 * 60);

#[derive(Clone, Debug)]
pub struct FicheAgent {
    pub id: String,
    pub role: String,
    pub phase: String,
    pub debut: SystemTime,
    pub vivant: bool,
    /// Il a rendu son bilan au chantier.
    pub fini: bool,
    pub images: usize,
    /// La premiere ligne de son bilan, sinon de ce qu'il a dit en dernier.
    pub resume: String,
    /// Sa derniere ecriture : la fin de sa barre sur la chronologie.
    pub fin: SystemTime,
    /// L'instant de chaque entree de sa transcription qui a donne un bloc :
    /// la densite de son travail, le long de la chronologie.
    pub activite: Vec<SystemTime>,
}

#[derive(Clone, Debug)]
pub struct Vignette {
    pub agent: String,
    pub role: String,
    /// L'heure ou elle est entree dans le fil.
    pub heure: String,
    /// Le meme instant, pour la placer sur la chronologie.
    pub quand: Option<SystemTime>,
    pub image: Arc<Image>,
}

#[derive(Debug, Default)]
pub struct Fiche {
    pub id: String,
    pub agents: Vec<FicheAgent>,
    pub vignettes: Vec<Vignette>,
    /// Toutes les images du chantier, y compris celles qu'une planche trop
    /// longue ne montre pas.
    pub images: usize,
    /// Faux pour une fiche partielle, envoyee pendant la premiere lecture :
    /// ses comptes ne sont pas encore ceux du chantier.
    pub complete: bool,
}

/// Ce qu'on garde d'un agent entre deux relectures.
struct Lu {
    suite: Suite,
    lecteur: Lecteur,
    debut: Option<SystemTime>,
    derniere: SystemTime,
    vignettes: Vec<Vignette>,
    dernier_mot: String,
    activite: Vec<SystemTime>,
}

/// La premiere ligne qui dise quelque chose, sans le balisage Markdown qui
/// s'y lirait en clair : les diese d'un titre, les accents graves d'un
/// chemin, les asterisques d'une emphase.
fn premiere_ligne(t: &str) -> String {
    t.lines()
        .map(|l| l.trim().trim_start_matches('#').trim())
        .find(|l| !l.is_empty())
        .unwrap_or("")
        .replace(['`', '*'], "")
}

/// La phrase d'un bilan. Un bilan est un texte, ou un objet quand le
/// chantier demande une reponse structuree : on y prend alors le champ qui
/// conclut, sinon le premier texte, sinon le compte d'une liste (« 6
/// figures »), vus le 2026-10-27 sur un chantier de figures.
fn bilan(v: &serde_json::Value) -> String {
    const CONCLUSIONS: [&str; 7] = ["verdict", "resume", "résumé", "summary", "bilan", "conclusion", "message"];
    match v {
        serde_json::Value::String(s) => premiere_ligne(s),
        serde_json::Value::Object(o) => {
            let texte = CONCLUSIONS
                .iter()
                .filter_map(|c| o.get(*c)?.as_str())
                .chain(o.values().filter_map(|x| x.as_str()))
                .map(premiere_ligne)
                .find(|s| !s.is_empty());
            texte
                .or_else(|| {
                    o.iter().find_map(|(cle, x)| {
                        let n = x.as_array()?.len();
                        Some(format!("{n} {}", cle.replace('_', " ")))
                    })
                })
                .unwrap_or_default()
        }
        _ => String::new(),
    }
}

/// Les agents d'un chantier, par leur identifiant.
fn agents_du(dossier: &Path) -> Vec<(String, PathBuf)> {
    let Ok(lecture) = std::fs::read_dir(dossier) else { return Vec::new() };
    lecture
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().to_string();
            let id = n.strip_prefix("agent-")?.strip_suffix(".jsonl")?.to_string();
            Some((id, e.path()))
        })
        .collect()
}

/// L'instant de la premiere entree d'une transcription, sans lire le reste.
/// Au-dela de quatre megaoctets sans fin de ligne, on renonce.
fn premier_instant(chemin: &Path) -> Option<SystemTime> {
    use std::io::{BufRead, Read};
    let f = std::fs::File::open(chemin).ok()?;
    let mut ligne = Vec::new();
    std::io::BufReader::new(f).take(4 << 20).read_until(b'\n', &mut ligne).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&ligne).ok()?;
    crate::dates::instant_iso(v.get("timestamp")?.as_str()?)
}

/// Le journal du chantier : pour chaque agent, son role, sa phase et son bilan.
fn journal(dossier: &Path) -> HashMap<String, (String, String, Option<String>)> {
    let mut m: HashMap<String, (String, String, Option<String>)> = HashMap::new();
    let texte = std::fs::read_to_string(dossier.join("journal.jsonl")).unwrap_or_default();
    for ligne in texte.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(ligne) else { continue };
        let Some(id) = v.get("agentId").and_then(|x| x.as_str()) else { continue };
        let e = m.entry(id.to_string()).or_default();
        let champ = |c: &str| v.get(c).and_then(|x| x.as_str()).unwrap_or("").to_string();
        if !champ("label").is_empty() {
            e.0 = champ("label");
        }
        if !champ("phase").is_empty() {
            e.1 = champ("phase");
        }
        if v.get("type").and_then(|x| x.as_str()) == Some("result") {
            e.2 = Some(v.get("result").map(bilan).unwrap_or_default());
        }
    }
    m
}

/// L'identifiant d'un agent donne par son role ou par son identifiant, pour
/// --agent=.
pub fn agent_nomme(chantier: &str, nom: &str) -> Option<String> {
    let dossier = crate::catalogue::dossier_du_chantier(chantier)?;
    let nom = nom.trim_start_matches("agent-");
    agents_du(&dossier).into_iter().map(|(id, _)| id).find(|id| {
        id == nom || crate::suivi::role_de(&dossier, id).eq_ignore_ascii_case(nom)
    })
}

/// Relit ce qui s'est ajoute aux transcriptions ; dit si quelque chose a
/// change. `apres` est appele apres chaque agent relu, pour qu'une premiere
/// lecture longue se montre au fur et a mesure.
fn relire(dossier: &Path, lus: &mut HashMap<String, Lu>, apres: &mut dyn FnMut(&HashMap<String, Lu>)) -> bool {
    let mut change = false;
    // Dans l'ordre ou chacun a commence, celui de la page : lus dans l'ordre
    // du dossier, les agents s'inseraient au-dessus de ceux deja montres, et
    // la ligne qu'on allait cliquer descendait sous la main
    // (`chercheur-forme` ouvert au lieu de
    // `chercheur-direction-artistique`, 3 fois sur 4). Seul un agent neuf est
    // ouvert pour cela, une fois.
    let mut agents: Vec<(Option<SystemTime>, String, PathBuf)> = agents_du(dossier)
        .into_iter()
        .map(|(id, chemin)| {
            let debut = match lus.get(&id) {
                Some(lu) => lu.debut,
                None => premier_instant(&chemin),
            };
            (debut, id, chemin)
        })
        .collect();
    agents.sort();
    for (_, id, chemin) in agents {
        let lu = lus.entry(id.clone()).or_insert_with(|| {
            change = true;
            Lu {
                suite: Suite::nouvelle(chemin.clone()),
                lecteur: Lecteur::nouveau(VIGNETTE_MAX),
                debut: None,
                derniere: SystemTime::UNIX_EPOCH,
                vignettes: Vec::new(),
                dernier_mot: String::new(),
                activite: Vec::new(),
            }
        });
        lu.derniere = std::fs::metadata(&chemin).and_then(|m| m.modified()).unwrap_or(lu.derniere);
        let role = Some(crate::suivi::role_de(dossier, &id)).filter(|r| !r.is_empty()).unwrap_or_else(|| id.clone());
        let lignes = lu.suite.lignes();
        change |= !lignes.is_empty();
        for l in &lignes {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(l) else { continue };
            let quand = v.get("timestamp").and_then(|x| x.as_str()).and_then(crate::dates::instant_iso);
            if lu.debut.is_none() {
                lu.debut = quand;
            }
            let blocs = lu.lecteur.blocs(&v);
            if let (false, Some(q)) = (blocs.is_empty(), quand) {
                lu.activite.push(q);
            }
            for b in blocs {
                match (b.registre, b.image) {
                    (Registre::Image, Some(image)) => lu.vignettes.push(Vignette {
                        agent: id.clone(),
                        role: role.clone(),
                        heure: b.heure.get(..5).unwrap_or("").to_string(),
                        quand,
                        image,
                    }),
                    (Registre::Parole, _) => lu.dernier_mot = premiere_ligne(&b.corps),
                    _ => {}
                }
            }
        }
        if !lignes.is_empty() {
            apres(lus);
        }
    }
    change
}

fn dresser(id: &str, dossier: &Path, lus: &HashMap<String, Lu>, complete: bool) -> Fiche {
    let roles = journal(dossier);
    let maintenant = crate::app::maintenant();
    let mut agents: Vec<FicheAgent> = lus
        .iter()
        .map(|(aid, lu)| {
            let (role_j, phase_j, bilan) = roles.get(aid).cloned().unwrap_or_default();
            let (role_f, phase_f) = crate::suivi::fiche_de(dossier, aid);
            let role = [role_f, role_j].into_iter().find(|r| !r.is_empty()).unwrap_or_else(|| aid.clone());
            let phase = if phase_f.is_empty() { phase_j } else { phase_f };
            let silence = maintenant.duration_since(lu.derniere).unwrap_or_default();
            FicheAgent {
                id: aid.clone(),
                role,
                phase,
                debut: lu.debut.unwrap_or(lu.derniere),
                vivant: bilan.is_none() && silence < SILENCE_AGENT,
                fini: bilan.is_some(),
                images: lu.vignettes.len(),
                resume: bilan.filter(|b| !b.is_empty()).unwrap_or_else(|| lu.dernier_mot.clone()),
                fin: lu.activite.last().copied().unwrap_or(lu.derniere),
                activite: lu.activite.clone(),
            }
        })
        .collect();
    // Dans l'ordre ou ils ont commence : c'est l'ordre du chantier, ses phases.
    agents.sort_by(|a, b| a.debut.cmp(&b.debut).then_with(|| a.id.cmp(&b.id)));
    let rang: HashMap<&str, usize> = agents.iter().enumerate().map(|(i, a)| (a.id.as_str(), i)).collect();
    let mut vignettes = attribuer(lus.values().flat_map(|lu| lu.vignettes.iter().cloned()).collect(), &rang);
    // Le compte d'un agent est celui de ses images sur la planche : la somme
    // des comptes est alors le total du chantier.
    for a in agents.iter_mut() {
        a.images = vignettes.iter().filter(|v| v.agent == a.id).count();
    }
    let images = vignettes.len();
    vignettes.truncate(VIGNETTES_MAX);
    Fiche { id: id.to_string(), agents, vignettes, images, complete }
}

/// La planche : chaque image une seule fois, chez un seul agent, par agent
/// dans l'ordre du chantier puis dans l'ordre ou chacun les a faites. La
/// planche se lit comme le travail s'est fait.
///
/// Deux agents qui travaillent dans le meme dossier au meme moment peuvent
/// voir passer la meme image : l'un l'a ecrite, l'autre l'a listee. L'image va
/// a celui qui l'a NOMMEE lui-meme (il l'a ouverte, ou sa commande portait son
/// nom) plutot qu'a celui qui ne l'a vue que dans la sortie d'un geste ; a
/// preuve egale, au premier agent du chantier.
fn attribuer(mut v: Vec<Vignette>, rang: &HashMap<&str, usize>) -> Vec<Vignette> {
    let r = |x: &Vignette| rang.get(x.agent.as_str()).copied().unwrap_or(usize::MAX);
    // Stable : l'ordre de chaque agent est garde.
    v.sort_by_key(r);
    let mut elu: HashMap<PathBuf, (bool, usize)> = HashMap::new();
    for x in &v {
        let Some(c) = &x.image.chemin else { continue };
        let candidat = (x.image.nommee, r(x));
        elu.entry(c.clone())
            .and_modify(|e| {
                if candidat.0 && !e.0 {
                    *e = candidat;
                }
            })
            .or_insert(candidat);
    }
    let mut pris = std::collections::HashSet::new();
    v.retain(|x| match &x.image.chemin {
        None => true,
        Some(c) => elu.get(c) == Some(&(x.image.nommee, r(x))) && pris.insert(c.clone()),
    });
    // Une image recopiee sous un autre nom (un `cp`) est la meme image : le
    // fil la dit « la même que » et ne la compte pas ; la planche la montrait
    // et le compte de l'agent l'ajoutait, 13 images sur la page pour 12 dans
    // le fil. La premiere venue reste, comme dans le fil.
    let mut vues = std::collections::HashSet::new();
    v.retain(|x| vues.insert(x.image.empreinte()));
    v
}

/// Le fil de la fiche. Il dresse celle du chantier demande, la renvoie quand
/// elle change, et oublie tout quand on en demande une autre.
pub fn lancer(envoi: Sender<Nouvelle>, demandes: Receiver<Option<String>>) {
    std::thread::spawn(move || {
        let mut voulu: Option<String> = None;
        let mut dossier: Option<PathBuf> = None;
        let mut lus: HashMap<String, Lu> = HashMap::new();
        // Un agent cesse d'etre « au travail » sans que rien ne s'ecrive : la
        // fiche se renvoie donc aussi de temps en temps, meme inchangee.
        let mut envoyee = std::time::Instant::now();
        loop {
            let mut recu = match demandes.recv_timeout(PERIODE) {
                Ok(d) => Some(d),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => None,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            };
            while let Ok(d) = demandes.try_recv() {
                recu = Some(d);
            }
            let mut neuf = false;
            if let Some(d) = recu {
                if d != voulu {
                    voulu = d;
                    dossier = None;
                    lus.clear();
                    neuf = true;
                }
            }
            let Some(id) = voulu.clone() else { continue };
            if dossier.is_none() {
                dossier = crate::catalogue::dossier_du_chantier(&id);
            }
            let Some(d) = dossier.clone() else { continue };
            let chrono = std::time::Instant::now();
            let premiere = lus.is_empty();
            // Pendant une premiere lecture, la fiche partielle part toutes les
            // demi-secondes : les agents se voient sans attendre les images
            // du dernier.
            let mut partielle = std::time::Instant::now();
            let mut montrer = |lus: &HashMap<String, Lu>| {
                if premiere && partielle.elapsed() > Duration::from_millis(500) {
                    let _ = envoi.send(Nouvelle::Fiche(Arc::new(dresser(&id, &d, lus, false))));
                    partielle = std::time::Instant::now();
                }
            };
            let change = relire(&d, &mut lus, &mut montrer);
            if neuf || change || envoyee.elapsed() > Duration::from_secs(20) {
                let fiche = dresser(&id, &d, &lus, true);
                if premiere {
                    // La premiere lecture est la seule qui coute : elle se dit,
                    // comme le premier parcours du catalogue.
                    eprintln!(
                        "fiche {id} : {} agents, {} images, lue en {:.1} s",
                        fiche.agents.len(),
                        fiche.images,
                        chrono.elapsed().as_secs_f32()
                    );
                }
                let _ = envoi.send(Nouvelle::Fiche(Arc::new(fiche)));
                envoyee = std::time::Instant::now();
            }
        }
    });
}

#[cfg(test)]
mod essais {
    use super::*;

    /// Les trois formes de bilan relevees le 2026-10-27 : un texte, un objet
    /// a verdict, un objet qui n'est qu'une liste.
    #[test]
    fn un_bilan_se_resume_en_une_phrase() {
        let t = serde_json::json!("## Fichier produit : `recherche/forme.md`\n\nLa suite.");
        assert_eq!(bilan(&t), "Fichier produit : recherche/forme.md");
        let o = serde_json::json!({"compte": 3, "verdict": "Les chiffres tiennent.\nDetail."});
        assert_eq!(bilan(&o), "Les chiffres tiennent.");
        let l = serde_json::json!({"figures": [{"numero": "III-7.6"}, {"numero": "III-9.1"}]});
        assert_eq!(bilan(&l), "2 figures");
        assert_eq!(bilan(&serde_json::json!(null)), "");
    }

    fn vignette(agent: &str, chemin: &str, nommee: bool) -> Vignette {
        Vignette {
            agent: agent.to_string(),
            role: agent.to_string(),
            heure: String::new(),
            quand: None,
            image: Arc::new(Image {
                largeur: 1,
                hauteur: 1,
                // Des pixels propres a chaque fichier : deux noms, deux images.
                pixels: chemin.bytes().collect(),
                chemin: Some(PathBuf::from(chemin)),
                origine: crate::suivi::Origine::Produite,
                video: None,
                source: (1, 1),
                nommee,
            }),
        }
    }

    /// Le premier agent du chantier liste a.png que le second a ecrite :
    /// elle va au second. b.png, nommee par les deux, va au premier. Chacune
    /// une seule fois, et chaque agent garde son ordre.
    #[test]
    fn une_image_va_a_un_seul_agent() {
        let rang: HashMap<&str, usize> = [("un", 0), ("deux", 1)].into_iter().collect();
        let v = vec![
            vignette("deux", "a.png", true),
            vignette("deux", "b.png", true),
            vignette("un", "a.png", false),
            vignette("un", "b.png", true),
            vignette("un", "c.png", false),
        ];
        let p: Vec<(String, String)> = attribuer(v, &rang)
            .iter()
            .map(|x| (x.agent.clone(), x.image.nom().unwrap()))
            .collect();
        let attendu = [("un", "b.png"), ("un", "c.png"), ("deux", "a.png")];
        assert_eq!(p, attendu.map(|(a, n)| (a.to_string(), n.to_string())));
    }

    /// tan_manim.png recopiee en Tangente_ManimCE_v0.22.0.png par un `cp` :
    /// les memes pixels sous deux noms. La planche n'en garde qu'une, la
    /// premiere, et le compte de l'agent est celui du fil.
    #[test]
    fn une_copie_ne_compte_pas_deux_fois() {
        let rang: HashMap<&str, usize> = [("un", 0)].into_iter().collect();
        let copie = |nom: &str| Vignette {
            image: Arc::new(Image {
                largeur: 1,
                hauteur: 1,
                pixels: vec![7; 12],
                chemin: Some(PathBuf::from(nom)),
                origine: crate::suivi::Origine::Produite,
                video: None,
                source: (1, 1),
                nommee: true,
            }),
            ..vignette("un", nom, true)
        };
        let v = vec![copie("Tangente_ManimCE_v0.22.0.png"), vignette("un", "tan_wolfram.png", true), copie("tan_manim.png")];
        let noms: Vec<String> = attribuer(v, &rang).iter().map(|x| x.image.nom().unwrap()).collect();
        assert_eq!(noms, ["Tangente_ManimCE_v0.22.0.png", "tan_wolfram.png"]);
    }
}
