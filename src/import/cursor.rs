//! Cursor (agent) : `~/.cursor/projects/<projet>/agent-transcripts/**/<uuid>.jsonl`
//! (ou `$CURSOR_HOME`).
//!
//! C'est le format le moins documente des quatre : les descriptions publiques
//! disent des « lignes de message dont le contenu est une liste de blocs »,
//! avec des blocs `tool_use` (`name`, `input`), sans horodatage par ligne et
//! sans dossier de travail. Ce convertisseur est donc tolerant : le role peut
//! etre en haut de la ligne ou dans `message`, le contenu une chaine ou des
//! blocs `text`, `tool_use`, `tool_result`. Faute d'horodatage, toutes les
//! lignes portent la date du fichier ; faute de dossier, le projet est le nom
//! que Cursor lui donne. A prendre comme experimental.

use super::{appel, maison, resultat, texte_assistant, texte_user, Convertisseur};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub fn racine() -> PathBuf {
    match std::env::var_os("CURSOR_HOME").filter(|v| !v.is_empty()) {
        Some(h) => PathBuf::from(h).join("projects"),
        None => maison().join(".cursor").join("projects"),
    }
}

pub fn trouver() -> Vec<PathBuf> {
    let mut v = Vec::new();
    let Ok(projets) = std::fs::read_dir(racine()) else { return v };
    for p in projets.filter_map(|e| e.ok()) {
        parcourir(&p.path().join("agent-transcripts"), 0, &mut v);
    }
    v
}

fn parcourir(d: &Path, profondeur: u32, v: &mut Vec<PathBuf>) {
    if profondeur > 3 {
        return;
    }
    let Ok(liste) = std::fs::read_dir(d) else { return };
    for e in liste.filter_map(|e| e.ok()) {
        let p = e.path();
        if p.is_dir() {
            parcourir(&p, profondeur + 1, v);
        } else if p.extension().is_some_and(|x| x == "jsonl") && p.file_stem().is_some_and(|s| s.len() >= 32) {
            v.push(p);
        }
    }
}

pub fn nouveau(source: &Path) -> Box<dyn Convertisseur> {
    // <racine>/<projet>/agent-transcripts/... : le nom du projet est celui du dossier avant `agent-transcripts`.
    let projet = source
        .ancestors()
        .find(|a| a.file_name().is_some_and(|n| n == "agent-transcripts"))
        .and_then(|a| a.parent())
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let ts = source
        .metadata()
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| super::iso_ms(d.as_millis() as u64))
        .unwrap_or_default();
    Box::new(Cursor { projet, ts, id: source.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(), n: 0 })
}

struct Cursor {
    projet: String,
    ts: String,
    id: String,
    n: u64,
}

fn texte_de(blocs: &Value) -> String {
    match blocs {
        Value::String(s) => s.clone(),
        Value::Array(a) => a
            .iter()
            .filter(|b| b.get("type").and_then(|t| t.as_str()).map_or(true, |t| t == "text"))
            .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// Un appel d'outil, aux noms de Claude Code quand on les reconnait.
fn traduire_appel(nom: &str, entree: &Value) -> (String, Value) {
    let champ = |cles: &[&str]| cles.iter().find_map(|c| entree.get(*c).and_then(|x| x.as_str())).unwrap_or("").to_string();
    match nom {
        "Shell" | "run_terminal_cmd" | "Bash" => ("Bash".into(), json!({ "command": champ(&["command", "cmd"]) })),
        "ReadFile" | "read_file" | "Read" => ("Read".into(), json!({ "file_path": champ(&["path", "target_file", "file_path"]) })),
        "Write" | "edit_file" | "StrReplace" | "Edit" | "Delete" => ("Edit".into(), json!({ "file_path": champ(&["path", "target_file", "file_path"]) })),
        "Grep" | "Glob" | "codebase_search" => ("Grep".into(), json!({ "pattern": champ(&["pattern", "query", "glob_pattern"]) })),
        autre => (autre.to_string(), if entree.is_object() { entree.clone() } else { json!({}) }),
    }
}

impl Convertisseur for Cursor {
    fn cwd(&self) -> String {
        self.projet.clone()
    }

    fn session(&self) -> String {
        self.id.clone()
    }

    fn ligne(&mut self, v: &Value) -> Vec<Value> {
        let message = v.get("message").unwrap_or(v);
        let role = v.get("role").or_else(|| message.get("role")).or_else(|| v.get("type")).and_then(|x| x.as_str()).unwrap_or("");
        let role = if role == "assistant" { "assistant" } else if role == "user" { "user" } else { return vec![] };
        let contenu = message.get("content").unwrap_or(&Value::Null);
        let (cwd, ts, sid) = (self.projet.clone(), self.ts.clone(), self.id.clone());
        let mut sortie = Vec::new();
        match contenu {
            Value::String(s) if !s.trim().is_empty() => {
                sortie.push(if role == "assistant" { texte_assistant(s, &ts, &cwd, &sid) } else { texte_user(s, &ts, &cwd, &sid) });
            }
            Value::Array(blocs) => {
                let t = texte_de(contenu);
                if !t.trim().is_empty() {
                    sortie.push(if role == "assistant" { texte_assistant(&t, &ts, &cwd, &sid) } else { texte_user(&t, &ts, &cwd, &sid) });
                }
                for b in blocs {
                    match b.get("type").and_then(|x| x.as_str()) {
                        Some("tool_use") | Some("tool_call") => {
                            self.n += 1;
                            let nom = b.get("name").and_then(|x| x.as_str()).unwrap_or("outil");
                            let id = b.get("id").and_then(|x| x.as_str()).map(str::to_string).unwrap_or_else(|| format!("cur{}", self.n));
                            let (nom, entree) = traduire_appel(nom, b.get("input").or_else(|| b.get("arguments")).unwrap_or(&Value::Null));
                            sortie.push(appel(&id, &nom, entree, &ts, &cwd, &sid));
                        }
                        Some("tool_result") => {
                            let id = b.get("tool_use_id").and_then(|x| x.as_str()).unwrap_or("");
                            sortie.push(resultat(id, &texte_de(b.get("content").unwrap_or(&Value::Null)), &ts, &cwd, &sid));
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        sortie
    }
}

#[cfg(test)]
mod essais {
    use super::*;

    fn lire(lignes: &[&str]) -> Vec<Value> {
        let mut c = Cursor { projet: "Users-u-atelier".into(), ts: "2026-10-30T12:00:00.000Z".into(), id: "s".into(), n: 0 };
        lignes.iter().flat_map(|l| c.ligne(&serde_json::from_str::<Value>(l).unwrap())).collect()
    }

    #[test]
    fn les_deux_formes_de_ligne_se_lisent() {
        let v = lire(&[
            r#"{"role":"user","message":{"content":[{"type":"text","text":"corrige le bug"}]}}"#,
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Je regarde."},{"type":"tool_use","name":"Shell","input":{"command":"ls"}}]}}"#,
            r#"{"role":"assistant","message":{"content":"Voila."}}"#,
            r#"{"type":"inconnu"}"#,
        ]);
        assert_eq!(v.len(), 4, "{v:?}");
        assert_eq!(v[0]["message"]["content"], "corrige le bug");
        assert_eq!(v[1]["message"]["content"][0]["text"], "Je regarde.");
        assert_eq!(v[2]["message"]["content"][0]["name"], "Bash");
        assert_eq!(v[2]["message"]["content"][0]["input"]["command"], "ls");
        assert_eq!(v[3]["message"]["content"][0]["text"], "Voila.");
        assert_eq!(v[0]["cwd"], "Users-u-atelier");
        assert_eq!(v[0]["timestamp"], "2026-10-30T12:00:00.000Z");
    }
}
