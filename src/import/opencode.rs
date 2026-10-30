//! OpenCode : `<donnees>/opencode/storage/`, ou une session n'est PAS un
//! fichier de lignes mais un dossier de petits fichiers JSON :
//!
//! - `session/<projet>/ses_<id>.json` : `id`, `directory`, `title`, `time` ;
//! - `message/<session>/msg_<id>.json` : `role`, `time.created` (millisecondes) ;
//! - `part/<message>/prt_<id>.json` : `type` (`text`, `reasoning`, `tool`...),
//!   et pour un outil `tool`, `callID` et `state` (`status`, `input`, `output`).
//!
//! Il n'y a donc rien a suivre en fin de fichier : a chaque passage on relit la
//! session et l'on ne rend que ce qui n'a pas encore ete dit. D'apres les
//! descriptions publiques du format, qui changent d'une version a l'autre.

use super::{appel, maison, pensee, resultat, texte_assistant, texte_user, Convertisseur};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Les endroits ou OpenCode range son stockage, selon le systeme.
fn candidats() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(x) = std::env::var_os("OPENCODE_DATA_DIR").filter(|x| !x.is_empty()) {
        v.push(PathBuf::from(x).join("storage"));
    }
    if let Some(x) = std::env::var_os("XDG_DATA_HOME").filter(|x| !x.is_empty()) {
        v.push(PathBuf::from(x).join("opencode").join("storage"));
    }
    v.push(maison().join(".local").join("share").join("opencode").join("storage"));
    if let Some(x) = std::env::var_os("LOCALAPPDATA") {
        v.push(PathBuf::from(x).join("opencode").join("storage"));
    }
    v
}

pub fn trouver() -> Vec<PathBuf> {
    let mut v = Vec::new();
    for racine in candidats() {
        let Ok(projets) = std::fs::read_dir(racine.join("session")) else { continue };
        for p in projets.filter_map(|e| e.ok()) {
            let Ok(fichiers) = std::fs::read_dir(p.path()) else { continue };
            for f in fichiers.filter_map(|e| e.ok()) {
                let n = f.file_name().to_string_lossy().to_string();
                if n.starts_with("ses_") && n.ends_with(".json") {
                    v.push(f.path());
                }
            }
        }
    }
    v
}

pub fn nouveau(_: &Path) -> Box<dyn Convertisseur> {
    Box::new(OpenCode { emis: HashSet::new(), cwd: String::new(), id: String::new() })
}

struct OpenCode {
    emis: HashSet<String>,
    cwd: String,
    id: String,
}

fn json_de(chemin: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(chemin).ok()?).ok()
}

fn fichiers_tries(dossier: &Path, prefixe: &str) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dossier)
        .map(|r| r.filter_map(|e| e.ok()).map(|e| e.path()).collect())
        .unwrap_or_default();
    v.retain(|p| {
        p.file_name().is_some_and(|n| {
            let n = n.to_string_lossy();
            n.starts_with(prefixe) && n.ends_with(".json")
        })
    });
    v.sort();
    v
}

/// Un appel d'outil, aux noms de Claude Code.
fn traduire_appel(nom: &str, entree: &Value) -> (String, Value) {
    let champ = |cles: &[&str]| cles.iter().find_map(|c| entree.get(*c).and_then(|x| x.as_str())).unwrap_or("").to_string();
    match nom {
        "bash" => ("Bash".into(), json!({ "command": champ(&["command"]) })),
        "read" => ("Read".into(), json!({ "file_path": champ(&["filePath", "path"]) })),
        "write" => ("Write".into(), json!({ "file_path": champ(&["filePath", "path"]) })),
        "edit" | "patch" | "multiedit" => ("Edit".into(), json!({ "file_path": champ(&["filePath", "path"]) })),
        "grep" | "glob" => ("Grep".into(), json!({ "pattern": champ(&["pattern", "query"]) })),
        autre => (autre.to_string(), if entree.is_object() { entree.clone() } else { json!({}) }),
    }
}

impl Convertisseur for OpenCode {
    fn cwd(&self) -> String {
        self.cwd.clone()
    }

    fn session(&self) -> String {
        self.id.clone()
    }

    fn ligne(&mut self, _: &Value) -> Vec<Value> {
        vec![]
    }

    fn complet(&mut self, source: &Path) -> Option<Vec<Value>> {
        let session = json_de(source)?;
        self.id = session.get("id").and_then(|x| x.as_str())?.to_string();
        if let Some(d) = session.get("directory").and_then(|x| x.as_str()) {
            self.cwd = d.to_string();
        }
        // session/<projet>/<fichier> : le stockage est trois niveaux plus haut.
        let stockage = source.parent()?.parent()?.parent()?;
        let mut messages: Vec<(u64, String, Value)> = fichiers_tries(&stockage.join("message").join(&self.id), "msg_")
            .into_iter()
            .filter_map(|p| json_de(&p))
            .map(|m| {
                let t = m.pointer("/time/created").and_then(|x| x.as_u64()).unwrap_or(0);
                let id = m.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
                (t, id, m)
            })
            .collect();
        messages.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
        let (cwd, sid) = (self.cwd.clone(), self.id.clone());
        let mut sortie = Vec::new();
        for (t, mid, m) in messages {
            let role = m.get("role").and_then(|x| x.as_str()).unwrap_or("user");
            let ts = super::iso_ms(t);
            for p in fichiers_tries(&stockage.join("part").join(&mid), "prt_") {
                let Some(part) = json_de(&p) else { continue };
                let pid = part.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
                match part.get("type").and_then(|x| x.as_str()) {
                    Some("text") => {
                        let texte = part.get("text").and_then(|x| x.as_str()).unwrap_or("");
                        if !texte.trim().is_empty() && self.emis.insert(format!("{pid}#t")) {
                            sortie.push(if role == "assistant" { texte_assistant(texte, &ts, &cwd, &sid) } else { texte_user(texte, &ts, &cwd, &sid) });
                        }
                    }
                    Some("reasoning") => {
                        let texte = part.get("text").and_then(|x| x.as_str()).unwrap_or("");
                        if !texte.trim().is_empty() && self.emis.insert(format!("{pid}#r")) {
                            sortie.push(pensee(texte, &ts, &cwd, &sid));
                        }
                    }
                    Some("tool") => {
                        let nom = part.get("tool").and_then(|x| x.as_str()).unwrap_or("outil");
                        let id = part.get("callID").and_then(|x| x.as_str()).unwrap_or(&pid).to_string();
                        let etat = part.get("state").cloned().unwrap_or(Value::Null);
                        if self.emis.insert(format!("{pid}#a")) {
                            let (nom, entree) = traduire_appel(nom, etat.get("input").unwrap_or(&Value::Null));
                            sortie.push(appel(&id, &nom, entree, &ts, &cwd, &sid));
                        }
                        let fini = matches!(etat.get("status").and_then(|x| x.as_str()), Some("completed") | Some("error"));
                        if fini && self.emis.insert(format!("{pid}#s")) {
                            let corps = etat.get("output").or_else(|| etat.get("error")).and_then(|x| x.as_str()).unwrap_or("");
                            sortie.push(resultat(&id, corps, &ts, &cwd, &sid));
                        }
                    }
                    _ => {}
                }
            }
        }
        Some(sortie)
    }
}

#[cfg(test)]
mod essais {
    use super::*;

    fn ecrire(p: &Path, v: Value) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, v.to_string()).unwrap();
    }

    fn stockage(nom: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("vigie-opencode-{nom}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn une_session_en_petits_fichiers_devient_des_lignes_de_claude_code() {
        let d = stockage("a");
        ecrire(&d.join("session/p1/ses_1.json"), json!({"id":"ses_1","directory":"/home/u/atelier","time":{"created":1766529546325u64}}));
        ecrire(&d.join("message/ses_1/msg_a.json"), json!({"id":"msg_a","role":"user","time":{"created":1766529550000u64}}));
        ecrire(&d.join("part/msg_a/prt_1.json"), json!({"id":"prt_1","type":"text","text":"lance les essais"}));
        ecrire(&d.join("message/ses_1/msg_b.json"), json!({"id":"msg_b","role":"assistant","time":{"created":1766529560000u64}}));
        ecrire(&d.join("part/msg_b/prt_1.json"), json!({"id":"prt_b1","type":"reasoning","text":"Je lance cargo."}));
        ecrire(&d.join("part/msg_b/prt_2.json"), json!({"id":"prt_b2","type":"tool","callID":"c1","tool":"bash","state":{"status":"completed","input":{"command":"cargo test"},"output":"ok"}}));
        ecrire(&d.join("part/msg_b/prt_3.json"), json!({"id":"prt_b3","type":"text","text":"Fini."}));

        let source = d.join("session/p1/ses_1.json");
        let mut c = OpenCode { emis: HashSet::new(), cwd: String::new(), id: String::new() };
        let v = c.complet(&source).unwrap();
        assert_eq!(v.len(), 5, "{v:?}");
        assert_eq!(v[0]["message"]["content"], "lance les essais");
        assert_eq!(v[0]["cwd"], "/home/u/atelier");
        assert_eq!(v[1]["message"]["content"][0]["type"], "thinking");
        assert_eq!(v[2]["message"]["content"][0]["name"], "Bash");
        assert_eq!(v[2]["message"]["content"][0]["input"]["command"], "cargo test");
        assert_eq!(v[3]["message"]["content"][0]["content"], "ok");
        assert_eq!(v[4]["message"]["content"][0]["text"], "Fini.");
        assert_eq!(v[0]["timestamp"], "2025-12-23T22:39:10.000Z");
        // Rien de nouveau : rien a dire une seconde fois.
        assert!(c.complet(&source).unwrap().is_empty());
        // Un outil qui finit plus tard n'ajoute que son resultat.
        ecrire(&d.join("part/msg_b/prt_4.json"), json!({"id":"prt_b4","type":"tool","callID":"c2","tool":"read","state":{"status":"running","input":{"filePath":"/x/a.rs"}}}));
        let v = c.complet(&source).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0]["message"]["content"][0]["input"]["file_path"], "/x/a.rs");
        ecrire(&d.join("part/msg_b/prt_4.json"), json!({"id":"prt_b4","type":"tool","callID":"c2","tool":"read","state":{"status":"completed","input":{"filePath":"/x/a.rs"},"output":"fn a(){}"}}));
        let v = c.complet(&source).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0]["message"]["content"][0]["tool_use_id"], "c2");
        let _ = std::fs::remove_dir_all(&d);
    }
}
