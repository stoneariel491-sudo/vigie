//! Les transcriptions des autres outils, remises au format que la vigie suit.
//!
//! La vigie lit un seul format : les transcriptions de Claude Code. Plutot que
//! d'apprendre a chaque outil toute la chaine (catalogue, suivi, medias), un
//! convertisseur par outil relit sa transcription et ecrit, sous
//! `<donnees>/import/projects`, la meme chose au format de Claude Code : une
//! session devient un chantier d'un agent. Le reste de la vigie ne sait pas
//! d'ou vient la ligne.
//!
//! Chaque convertisseur ne sait que traduire : une ligne de l'outil devient
//! zero, une ou plusieurs lignes au format de Claude Code. Il ne touche pas au
//! disque. Les formats des outils ne sont pas garantis stables ; ceux-ci sont
//! ecrits d'apres leurs descriptions publiques et verifies sur des exemples,
//! pas sur toutes leurs versions. Une ligne qu'un convertisseur ne comprend
//! pas est ignoree, jamais fatale.

pub mod codex;
pub mod gemini;

use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Combien de sessions recentes on convertit, par outil. Des annees d'historique
/// ne se convertissent pas pour en regarder une.
const SESSIONS_MAX: usize = 40;

/// Ce qu'un convertisseur sait faire.
pub trait Convertisseur: Send {
    /// Une ligne de la transcription de l'outil, en lignes au format de Claude Code.
    fn ligne(&mut self, v: &Value) -> Vec<Value>;
    /// Le dossier de travail de la session, quand il est connu.
    fn cwd(&self) -> String;
    /// L'identifiant de la session, quand il est connu.
    fn session(&self) -> String;
}

/// Un outil : ou chercher ses transcriptions, et comment les traduire.
pub struct Format {
    pub nom: &'static str,
    pub trouver: fn() -> Vec<PathBuf>,
    pub nouveau: fn(&Path) -> Box<dyn Convertisseur>,
}

pub const FORMATS: [Format; 2] = [
    Format { nom: "codex", trouver: codex::trouver, nouveau: codex::nouveau },
    Format { nom: "gemini", trouver: gemini::trouver, nouveau: gemini::nouveau },
];

pub fn maison() -> PathBuf {
    let base = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).unwrap_or_default();
    PathBuf::from(base)
}

/// Une ligne au format de Claude Code.
pub fn ligne(type_: &str, contenu: Value, ts: &str, cwd: &str, session: &str) -> Value {
    json!({
        "type": type_,
        "message": { "role": type_, "content": contenu },
        "timestamp": ts,
        "cwd": cwd,
        "sessionId": session,
        "isSidechain": true,
    })
}

pub fn texte_user(t: &str, ts: &str, cwd: &str, session: &str) -> Value {
    ligne("user", json!(t), ts, cwd, session)
}

pub fn texte_assistant(t: &str, ts: &str, cwd: &str, session: &str) -> Value {
    ligne("assistant", json!([{ "type": "text", "text": t }]), ts, cwd, session)
}

pub fn pensee(t: &str, ts: &str, cwd: &str, session: &str) -> Value {
    ligne("assistant", json!([{ "type": "thinking", "thinking": t }]), ts, cwd, session)
}

pub fn appel(id: &str, nom: &str, entree: Value, ts: &str, cwd: &str, session: &str) -> Value {
    ligne("assistant", json!([{ "type": "tool_use", "id": id, "name": nom, "input": entree }]), ts, cwd, session)
}

pub fn resultat(id: &str, sortie: &str, ts: &str, cwd: &str, session: &str) -> Value {
    ligne("user", json!([{ "type": "tool_result", "tool_use_id": id, "content": sortie }]), ts, cwd, session)
}

/// La commande d'un outil de shell, dite comme Claude Code la dit : une chaine.
/// Les outils la donnent en tableau (`["bash","-lc","cargo test"]`) ou en chaine.
pub fn commande(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(a) => {
            let mots: Vec<&str> = a.iter().filter_map(|x| x.as_str()).collect();
            if mots.len() >= 3 && matches!(mots[1], "-lc" | "-c") {
                mots[2].to_string()
            } else {
                mots.join(" ")
            }
        }
        _ => String::new(),
    }
}

/// Un nom de dossier sans rien qui gene un nom de fichier.
fn nom_sur(s: &str) -> String {
    let n: String = s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' }).collect();
    if n.is_empty() { "x".into() } else { n }
}

fn dernier_mot_du_chemin(cwd: &str) -> String {
    let t = cwd.trim_end_matches(['/', '\\']);
    let m = t.rsplit(['/', '\\']).next().unwrap_or("");
    if m.is_empty() { "projet".into() } else { m.to_string() }
}

/// Une session a l'oeuvre : sa source, son convertisseur, ce qui est deja lu.
struct Suivie {
    convertisseur: Box<dyn Convertisseur>,
    lu: u64,
    reste: Vec<u8>,
    cible: Option<PathBuf>,
}

/// Ou ecrire : `<donnees>/import/projects`.
pub fn racine() -> Option<PathBuf> {
    if crate::app::figee().is_some() || std::env::var_os("VIGIE_SANS_IMPORT").is_some() {
        return None;
    }
    Some(crate::app::donnees()?.join("import").join("projects"))
}

/// Les lignes completes d'un fichier depuis un decalage, et le decalage suivant.
fn lire_depuis(chemin: &Path, depuis: u64, reste: &mut Vec<u8>) -> (Vec<String>, u64) {
    let Ok(mut f) = std::fs::File::open(chemin) else { return (vec![], depuis) };
    let taille = f.metadata().map(|m| m.len()).unwrap_or(0);
    if taille < depuis {
        // Le fichier a ete reecrit plus court : on repart du debut.
        reste.clear();
        return lire_depuis(chemin, 0, reste);
    }
    if taille == depuis || f.seek(SeekFrom::Start(depuis)).is_err() {
        return (vec![], depuis);
    }
    let mut octets = Vec::new();
    let _ = f.take(64 * 1024 * 1024).read_to_end(&mut octets);
    let lus = octets.len() as u64;
    let mut tout = std::mem::take(reste);
    tout.extend_from_slice(&octets);
    let mut lignes = Vec::new();
    let mut debut = 0;
    for (i, o) in tout.iter().enumerate() {
        if *o == b'\n' {
            lignes.push(String::from_utf8_lossy(&tout[debut..i]).trim_end_matches('\r').to_string());
            debut = i + 1;
        }
    }
    *reste = tout[debut..].to_vec();
    (lignes, depuis + lus)
}

/// Le dossier de la session convertie, et le fichier de l'agent.
fn cible(racine: &Path, format: &str, source: &Path, c: &dyn Convertisseur) -> PathBuf {
    let cwd = c.cwd();
    let projet = nom_sur(&dernier_mot_du_chemin(&cwd));
    let session = nom_sur(&format!("{format}-{}", if c.session().is_empty() { source.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default() } else { c.session() }));
    let court: String = session.chars().filter(|c| c.is_ascii_alphanumeric()).rev().take(8).collect::<Vec<_>>().into_iter().rev().collect();
    let chantier = format!("wf_{format}-{court}");
    racine
        .join(projet)
        .join(&session)
        .join("subagents")
        .join("workflows")
        .join(chantier)
        .join(format!("agent-{short}.jsonl", short = nom_sur(&court)))
}

/// Le nom d'un chantier d'apres la premiere consigne.
fn nommer(format: &str, premiere: &str) -> String {
    let mots: Vec<&str> = premiere.split_whitespace().take(6).collect();
    let n = nom_sur(&mots.join("-").to_lowercase());
    if mots.is_empty() { format.to_string() } else { format!("{format}-{}", n.trim_matches('-')) }
}

fn ecrire_fiches(cible: &Path, format: &str, premiere: &str) {
    let Some(chantier) = cible.parent() else { return };
    let Some(session) = chantier.parent().and_then(|d| d.parent()).and_then(|d| d.parent()) else { return };
    let id = chantier.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let nom = nommer(format, premiere);
    let _ = std::fs::write(
        cible.with_extension("meta.json"),
        json!({ "agentType": format, "description": nom, "workflowPhase": format }).to_string(),
    );
    let scripts = session.join("workflows").join("scripts");
    let _ = std::fs::create_dir_all(&scripts);
    let _ = std::fs::write(
        scripts.join(format!("{nom}-{id}.js")),
        format!("export const meta = {{\n  name: '{nom}',\n  description: 'Session {format}',\n}}\n"),
    );
}

/// Un passage : convertit ce que les sessions recentes ont ajoute depuis le dernier.
fn passage(racine: &Path, suivies: &mut HashMap<PathBuf, Suivie>) {
    for f in &FORMATS {
        let mut sources: Vec<(SystemTime, PathBuf)> = (f.trouver)()
            .into_iter()
            .filter_map(|p| p.metadata().ok().and_then(|m| m.modified().ok()).map(|t| (t, p)))
            .collect();
        sources.sort_by_key(|(t, _)| std::cmp::Reverse(*t));
        sources.truncate(SESSIONS_MAX);
        for (_, source) in sources {
            let s = suivies.entry(source.clone()).or_insert_with(|| Suivie {
                convertisseur: (f.nouveau)(&source),
                lu: 0,
                reste: Vec::new(),
                cible: None,
            });
            let avant = s.lu;
            let (lignes, suite) = lire_depuis(&source, s.lu, &mut s.reste);
            if suite < avant {
                s.cible = None;
            }
            s.lu = suite;
            let mut sorties: Vec<String> = Vec::new();
            let mut premiere = String::new();
            for l in lignes {
                let Ok(v) = serde_json::from_str::<Value>(&l) else { continue };
                for out in s.convertisseur.ligne(&v) {
                    if premiere.is_empty() && out["type"] == "user" {
                        if let Some(t) = out["message"]["content"].as_str() {
                            premiere = t.to_string();
                        }
                    }
                    sorties.push(out.to_string());
                }
            }
            if sorties.is_empty() {
                continue;
            }
            let premier_passage = s.cible.is_none();
            let cible_chemin = s.cible.get_or_insert_with(|| cible(racine, f.nom, &source, s.convertisseur.as_ref())).clone();
            if let Some(d) = cible_chemin.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let mut corps = sorties.join("\n");
            corps.push('\n');
            let ecrit = if premier_passage {
                std::fs::write(&cible_chemin, corps.as_bytes())
            } else {
                std::fs::OpenOptions::new().append(true).create(true).open(&cible_chemin).and_then(|mut fh| fh.write_all(corps.as_bytes()))
            };
            if ecrit.is_ok() && premier_passage {
                ecrire_fiches(&cible_chemin, f.nom, &premiere);
            }
        }
    }
}

/// Le fil de l'import. Il ne rend jamais la main, et ne s'allume que si un
/// de ces outils a laisse des transcriptions.
pub fn lancer() {
    let Some(racine) = racine() else { return };
    std::thread::spawn(move || {
        let mut suivies: HashMap<PathBuf, Suivie> = HashMap::new();
        loop {
            passage(&racine, &mut suivies);
            std::thread::sleep(Duration::from_millis(1500));
        }
    });
}

#[cfg(test)]
mod essais {
    use super::*;

    #[test]
    fn la_commande_d_un_shell_est_dite_comme_une_chaine() {
        assert_eq!(commande(&json!(["bash", "-lc", "cargo test"])), "cargo test");
        assert_eq!(commande(&json!(["git", "status"])), "git status");
        assert_eq!(commande(&json!("ls -la")), "ls -la");
        assert_eq!(commande(&json!(null)), "");
    }

    #[test]
    fn un_nom_de_fichier_ne_garde_que_le_sur() {
        assert_eq!(nom_sur("a b/c:d"), "a-b-c-d");
        assert_eq!(nom_sur(""), "x");
        assert_eq!(dernier_mot_du_chemin(r"C:\Users\x\atelier\"), "atelier");
        assert_eq!(dernier_mot_du_chemin("/home/x/atelier"), "atelier");
    }

    #[test]
    fn les_lignes_ne_sont_rendues_que_completes() {
        let d = std::env::temp_dir().join(format!("vigie-import-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("t.jsonl");
        std::fs::write(&f, "{\"a\":1}\n{\"b\":").unwrap();
        let mut reste = Vec::new();
        let (l, suite) = lire_depuis(&f, 0, &mut reste);
        assert_eq!(l, vec!["{\"a\":1}".to_string()]);
        std::fs::OpenOptions::new().append(true).open(&f).unwrap().write_all(b"2}\n").unwrap();
        let (l2, _) = lire_depuis(&f, suite, &mut reste);
        assert_eq!(l2, vec!["{\"b\":2}".to_string()]);
        let _ = std::fs::remove_dir_all(&d);
    }
}
