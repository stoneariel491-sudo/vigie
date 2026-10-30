//! Gemini CLI : `~/.gemini/tmp/<projet>/chats/session-<date>-<id>.jsonl`
//! (ou `$GEMINI_CLI_HOME/.gemini`).
//!
//! Premiere ligne : l'en-tete (`sessionId`, `startTime`). Ensuite, un message
//! par ligne (`type` : `user`, `gemini`, `info`...), et le MEME message peut
//! revenir plusieurs fois, la derniere version l'emportant : un identifiant
//! deja traduit n'est donc jamais retraduit, et les appels d'outils qui
//! arrivent plus tard sur un message connu sont ajoutes. Le dossier de
//! travail n'est pas dans la transcription : il est dans `.project_root`, a
//! cote du dossier `chats`.

use super::{appel, commande, maison, pensee, resultat, texte_assistant, texte_user, Convertisseur};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub fn racine() -> PathBuf {
    match std::env::var_os("GEMINI_CLI_HOME").filter(|v| !v.is_empty()) {
        Some(h) => PathBuf::from(h).join(".gemini"),
        None => maison().join(".gemini"),
    }
}

pub fn trouver() -> Vec<PathBuf> {
    let mut v = Vec::new();
    let Ok(projets) = std::fs::read_dir(racine().join("tmp")) else { return v };
    for p in projets.filter_map(|e| e.ok()) {
        let Ok(chats) = std::fs::read_dir(p.path().join("chats")) else { continue };
        for c in chats.filter_map(|e| e.ok()) {
            let n = c.file_name().to_string_lossy().to_string();
            if n.starts_with("session-") && n.ends_with(".jsonl") {
                v.push(c.path());
            }
        }
    }
    v
}

pub fn nouveau(source: &Path) -> Box<dyn Convertisseur> {
    let cwd = source
        .parent()
        .and_then(|chats| chats.parent())
        .map(|p| p.join(".project_root"))
        .and_then(|f| std::fs::read_to_string(f).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    Box::new(Gemini { cwd, id: String::new(), textes: HashSet::new(), appels: HashSet::new(), resultats: HashSet::new() })
}

struct Gemini {
    cwd: String,
    id: String,
    textes: HashSet<String>,
    appels: HashSet<String>,
    resultats: HashSet<String>,
}

fn texte_de(contenu: &Value) -> String {
    match contenu {
        Value::String(s) => s.clone(),
        Value::Array(a) => a.iter().filter_map(|b| b.get("text").and_then(|t| t.as_str())).collect::<Vec<_>>().join("\n"),
        _ => String::new(),
    }
}

/// Un appel d'outil, aux noms de Claude Code.
fn traduire_appel(nom: &str, args: &Value) -> (String, Value) {
    let champ = |c: &str| args.get(c).and_then(|x| x.as_str()).unwrap_or("").to_string();
    // Le premier de ces champs qui est rempli : les versions de l'outil ne les nomment pas pareil.
    let un = |cles: &[&str]| cles.iter().map(|c| champ(c)).find(|s| !s.is_empty()).unwrap_or_default();
    match nom {
        "run_shell_command" => ("Bash".into(), json!({ "command": args.get("command").map(commande).unwrap_or_default() })),
        "read_file" | "read_many_files" => ("Read".into(), json!({ "file_path": un(&["absolute_path", "file_path", "path"]) })),
        "write_file" => ("Write".into(), json!({ "file_path": un(&["file_path", "absolute_path"]) })),
        "replace" => ("Edit".into(), json!({ "file_path": un(&["file_path", "absolute_path"]) })),
        "search_file_content" | "grep" | "glob" => ("Grep".into(), json!({ "pattern": un(&["pattern", "query"]) })),
        autre => (autre.to_string(), if args.is_object() { args.clone() } else { json!({}) }),
    }
}

/// Le texte qu'un outil a rendu : `resultDisplay`, ou `result`, en chaine.
fn sortie_de(c: &Value) -> String {
    for cle in ["resultDisplay", "result", "output"] {
        match c.get(cle) {
            Some(Value::String(s)) if !s.is_empty() => return s.clone(),
            Some(v @ (Value::Object(_) | Value::Array(_))) => {
                if let Some(s) = v.pointer("/output").and_then(|x| x.as_str()) {
                    return s.to_string();
                }
                if !v.is_null() {
                    return v.to_string();
                }
            }
            _ => {}
        }
    }
    String::new()
}

impl Convertisseur for Gemini {
    fn cwd(&self) -> String {
        self.cwd.clone()
    }

    fn session(&self) -> String {
        self.id.clone()
    }

    fn ligne(&mut self, v: &Value) -> Vec<Value> {
        if let Some(i) = v.get("sessionId").and_then(|x| x.as_str()) {
            self.id = i.to_string();
        }
        let Some(type_) = v.get("type").and_then(|x| x.as_str()) else { return vec![] };
        let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let ts = v.get("timestamp").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let (cwd, sid) = (self.cwd.clone(), self.id.clone());
        let mut sortie = Vec::new();
        match type_ {
            "user" => {
                if let Some(parties) = v.get("content").and_then(|c| c.as_array()) {
                    for p in parties {
                        if let Some(r) = p.get("functionResponse") {
                            let appel_id = r.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
                            if self.resultats.insert(appel_id.clone()) {
                                let corps = r.get("response").map(|x| x.get("output").and_then(|o| o.as_str()).map(str::to_string).unwrap_or_else(|| x.to_string())).unwrap_or_default();
                                sortie.push(resultat(&appel_id, &corps, &ts, &cwd, &sid));
                            }
                        }
                    }
                }
                let t = texte_de(v.get("content").unwrap_or(&Value::Null));
                if !t.trim().is_empty() && self.textes.insert(id) {
                    sortie.push(texte_user(&t, &ts, &cwd, &sid));
                }
            }
            "gemini" => {
                if let Some(pensees) = v.get("thoughts").and_then(|x| x.as_array()) {
                    let t: Vec<String> = pensees
                        .iter()
                        .map(|p| format!("{} {}", p.get("subject").and_then(|x| x.as_str()).unwrap_or(""), p.get("description").and_then(|x| x.as_str()).unwrap_or("")).trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                    if !t.is_empty() && self.textes.insert(format!("{id}#pensee")) {
                        sortie.push(pensee(&t.join("\n"), &ts, &cwd, &sid));
                    }
                }
                let t = texte_de(v.get("content").unwrap_or(&Value::Null));
                if !t.trim().is_empty() && self.textes.insert(id.clone()) {
                    sortie.push(texte_assistant(&t, &ts, &cwd, &sid));
                }
                if let Some(appels) = v.get("toolCalls").and_then(|x| x.as_array()) {
                    for a in appels {
                        let aid = a.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
                        let nom = a.get("name").and_then(|x| x.as_str()).unwrap_or("outil");
                        let ats = a.get("timestamp").and_then(|x| x.as_str()).unwrap_or(&ts).to_string();
                        if self.appels.insert(aid.clone()) {
                            let (nom, entree) = traduire_appel(nom, a.get("args").unwrap_or(&Value::Null));
                            sortie.push(appel(&aid, &nom, entree, &ats, &cwd, &sid));
                        }
                        let corps = sortie_de(a);
                        if !corps.is_empty() && self.resultats.insert(aid.clone()) {
                            sortie.push(resultat(&aid, &corps, &ats, &cwd, &sid));
                        }
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
        let mut c = Gemini { cwd: "/home/u/atelier".into(), id: String::new(), textes: HashSet::new(), appels: HashSet::new(), resultats: HashSet::new() };
        lignes.iter().flat_map(|l| c.ligne(&serde_json::from_str::<Value>(l).unwrap())).collect()
    }

    const ENTETE: &str = r#"{"sessionId":"s-1","startTime":"2026-09-05T09:00:00.000Z","kind":"main"}"#;

    #[test]
    fn une_session_devient_des_lignes_de_claude_code() {
        let v = lire(&[
            ENTETE,
            r#"{"id":"u1","timestamp":"2026-09-05T09:00:01.000Z","type":"user","content":[{"text":"liste les fichiers"}]}"#,
            r#"{"id":"g1","timestamp":"2026-09-05T09:00:02.000Z","type":"gemini","content":"Je regarde.","toolCalls":[{"id":"c1","name":"run_shell_command","args":{"command":"ls"},"status":"success","resultDisplay":"a.rs"}]}"#,
        ]);
        assert_eq!(v.len(), 4);
        assert_eq!(v[0]["message"]["content"], "liste les fichiers");
        assert_eq!(v[0]["sessionId"], "s-1");
        assert_eq!(v[1]["message"]["content"][0]["text"], "Je regarde.");
        assert_eq!(v[2]["message"]["content"][0]["name"], "Bash");
        assert_eq!(v[2]["message"]["content"][0]["input"]["command"], "ls");
        assert_eq!(v[3]["message"]["content"][0]["content"], "a.rs");
    }

    #[test]
    fn un_message_reecrit_n_est_pas_dit_deux_fois_mais_ses_appels_tardifs_si() {
        let v = lire(&[
            ENTETE,
            r#"{"id":"g1","timestamp":"t","type":"gemini","content":"Je regarde."}"#,
            r#"{"id":"g1","timestamp":"t","type":"gemini","content":"Je regarde.","toolCalls":[{"id":"c1","name":"read_file","args":{"absolute_path":"/x/a.rs"}}]}"#,
            r#"{"id":"g1","timestamp":"t","type":"gemini","content":"Je regarde.","toolCalls":[{"id":"c1","name":"read_file","args":{"absolute_path":"/x/a.rs"}}]}"#,
        ]);
        assert_eq!(v.len(), 2, "{v:?}");
        assert_eq!(v[1]["message"]["content"][0]["name"], "Read");
        assert_eq!(v[1]["message"]["content"][0]["input"]["file_path"], "/x/a.rs");
    }

    #[test]
    fn les_reponses_d_outils_et_les_lignes_de_controle_sont_traitees() {
        let v = lire(&[
            ENTETE,
            r#"{"$set":{"lastUpdated":"t"}}"#,
            r#"{"id":"u2","timestamp":"t","type":"user","content":[{"functionResponse":{"id":"c9","response":{"output":"fait"}}}]}"#,
            r#"{"id":"i1","timestamp":"t","type":"info","content":"note"}"#,
        ]);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0]["message"]["content"][0]["tool_use_id"], "c9");
        assert_eq!(v[0]["message"]["content"][0]["content"], "fait");
    }
}
