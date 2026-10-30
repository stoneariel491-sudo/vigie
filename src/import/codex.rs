//! Codex CLI : `~/.codex/sessions/AAAA/MM/JJ/rollout-<date>-<uuid>.jsonl`
//! (ou `$CODEX_HOME/sessions`).
//!
//! Une ligne : `{"timestamp", "type", "payload"}`. `session_meta` porte le
//! dossier de travail ; `response_item` est l'echange avec le modele (message,
//! raisonnement resume, appel d'outil et sa sortie) ; `event_msg` n'est que le
//! journal d'affichage du terminal et repeterait le meme texte : on l'ignore.

use super::{appel, commande, maison, pensee, resultat, texte_assistant, texte_user, Convertisseur};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub fn racine() -> PathBuf {
    match std::env::var_os("CODEX_HOME").filter(|v| !v.is_empty()) {
        Some(h) => PathBuf::from(h).join("sessions"),
        None => maison().join(".codex").join("sessions"),
    }
}

pub fn trouver() -> Vec<PathBuf> {
    let mut v = Vec::new();
    parcourir(&racine(), 0, &mut v);
    v
}

fn parcourir(d: &Path, profondeur: u32, v: &mut Vec<PathBuf>) {
    if profondeur > 5 {
        return;
    }
    let Ok(liste) = std::fs::read_dir(d) else { return };
    for e in liste.filter_map(|e| e.ok()) {
        let p = e.path();
        if p.is_dir() {
            parcourir(&p, profondeur + 1, v);
        } else {
            let n = e.file_name().to_string_lossy().to_string();
            if n.starts_with("rollout-") && n.ends_with(".jsonl") {
                v.push(p);
            }
        }
    }
}

pub fn nouveau(_: &Path) -> Box<dyn Convertisseur> {
    Box::new(Codex { cwd: String::new(), id: String::new() })
}

struct Codex {
    cwd: String,
    id: String,
}

fn texte_des_blocs(contenu: &Value) -> String {
    match contenu {
        Value::String(s) => s.clone(),
        Value::Array(a) => a
            .iter()
            .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// Le texte qu'un harnais ajoute devant la vraie consigne : pas une parole de l'utilisateur.
fn est_du_cadre(t: &str) -> bool {
    let t = t.trim_start();
    t.starts_with("<environment_context>") || t.starts_with("<user_instructions>") || t.starts_with("# AGENTS.md")
}

/// La sortie d'un outil : parfois une chaine, parfois `{"output": "..."}` encode en texte.
fn sortie_de(v: &Value) -> String {
    match v {
        Value::String(s) => match serde_json::from_str::<Value>(s) {
            Ok(Value::Object(o)) => o.get("output").and_then(|x| x.as_str()).map(str::to_string).unwrap_or_else(|| s.clone()),
            _ => s.clone(),
        },
        Value::Object(o) => o.get("output").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        _ => String::new(),
    }
}

/// Un appel d'outil, ramene aux noms et aux champs de Claude Code, que la vigie sait resumer.
fn traduire_appel(nom: &str, arguments: &Value) -> (String, Value) {
    let args = match arguments {
        Value::String(s) => serde_json::from_str::<Value>(s).unwrap_or_else(|_| json!({ "input": s })),
        autre => autre.clone(),
    };
    match nom {
        "exec_command" | "shell" | "shell_command" | "container.exec" | "local_shell" => {
            let cmd = args.get("cmd").or_else(|| args.get("command")).map(commande).unwrap_or_default();
            ("Bash".into(), json!({ "command": cmd }))
        }
        "apply_patch" => {
            let patch = args.get("input").and_then(|x| x.as_str()).unwrap_or("");
            let fichier = patch
                .lines()
                .find_map(|l| l.strip_prefix("*** Update File: ").or_else(|| l.strip_prefix("*** Add File: ")).or_else(|| l.strip_prefix("*** Delete File: ")))
                .unwrap_or("")
                .trim()
                .to_string();
            ("Edit".into(), json!({ "file_path": fichier }))
        }
        autre => (autre.to_string(), args),
    }
}

impl Convertisseur for Codex {
    fn cwd(&self) -> String {
        self.cwd.clone()
    }

    fn session(&self) -> String {
        self.id.clone()
    }

    fn ligne(&mut self, v: &Value) -> Vec<Value> {
        let ts = v.get("timestamp").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let p = v.get("payload").cloned().unwrap_or(Value::Null);
        let (cwd, sid) = (self.cwd.clone(), self.id.clone());
        match v.get("type").and_then(|x| x.as_str()) {
            Some("session_meta") => {
                if let Some(c) = p.get("cwd").and_then(|x| x.as_str()) {
                    self.cwd = c.to_string();
                }
                if let Some(i) = p.get("id").and_then(|x| x.as_str()) {
                    self.id = i.to_string();
                }
                vec![]
            }
            Some("turn_context") => {
                if self.cwd.is_empty() {
                    if let Some(c) = p.get("cwd").and_then(|x| x.as_str()) {
                        self.cwd = c.to_string();
                    }
                }
                vec![]
            }
            Some("response_item") => match p.get("type").and_then(|x| x.as_str()) {
                Some("message") => {
                    let t = texte_des_blocs(p.get("content").unwrap_or(&Value::Null));
                    if t.trim().is_empty() {
                        return vec![];
                    }
                    match p.get("role").and_then(|x| x.as_str()) {
                        Some("assistant") => vec![texte_assistant(&t, &ts, &cwd, &sid)],
                        Some("user") if !est_du_cadre(&t) => vec![texte_user(&t, &ts, &cwd, &sid)],
                        _ => vec![],
                    }
                }
                Some("reasoning") => {
                    let t = texte_des_blocs(p.get("summary").unwrap_or(&Value::Null));
                    if t.trim().is_empty() { vec![] } else { vec![pensee(&t, &ts, &cwd, &sid)] }
                }
                Some("function_call") | Some("custom_tool_call") => {
                    let nom = p.get("name").and_then(|x| x.as_str()).unwrap_or("outil");
                    let id = p.get("call_id").and_then(|x| x.as_str()).unwrap_or("");
                    let args = p.get("arguments").or_else(|| p.get("input")).cloned().unwrap_or(Value::Null);
                    let (nom, entree) = traduire_appel(nom, &args);
                    vec![appel(id, &nom, entree, &ts, &cwd, &sid)]
                }
                Some("local_shell_call") => {
                    let id = p.get("call_id").and_then(|x| x.as_str()).unwrap_or("");
                    let cmd = p.get("action").and_then(|a| a.get("command")).map(commande).unwrap_or_default();
                    vec![appel(id, "Bash", json!({ "command": cmd }), &ts, &cwd, &sid)]
                }
                Some("function_call_output") | Some("custom_tool_call_output") => {
                    let id = p.get("call_id").and_then(|x| x.as_str()).unwrap_or("");
                    let sortie = sortie_de(p.get("output").unwrap_or(&Value::Null));
                    vec![resultat(id, &sortie, &ts, &cwd, &sid)]
                }
                _ => vec![],
            },
            _ => vec![],
        }
    }
}

#[cfg(test)]
mod essais {
    use super::*;

    fn lire(lignes: &[&str]) -> Vec<Value> {
        let mut c = Codex { cwd: String::new(), id: String::new() };
        lignes.iter().flat_map(|l| c.ligne(&serde_json::from_str::<Value>(l).unwrap())).collect()
    }

    const META: &str = r#"{"timestamp":"2026-08-10T13:02:51.000Z","type":"session_meta","payload":{"id":"abc-123","cwd":"/home/u/atelier"}}"#;

    #[test]
    fn une_session_devient_des_lignes_de_claude_code() {
        let v = lire(&[
            META,
            r#"{"timestamp":"2026-08-10T13:03:00.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"fais les essais"}]}}"#,
            r#"{"timestamp":"2026-08-10T13:03:01.000Z","type":"response_item","payload":{"type":"reasoning","summary":[{"type":"summary_text","text":"Je lance cargo."}]}}"#,
            r#"{"timestamp":"2026-08-10T13:03:02.000Z","type":"response_item","payload":{"type":"function_call","name":"exec_command","arguments":"{\"cmd\":[\"bash\",\"-lc\",\"cargo test\"]}","call_id":"c1"}}"#,
            r#"{"timestamp":"2026-08-10T13:03:03.000Z","type":"response_item","payload":{"type":"function_call_output","call_id":"c1","output":"{\"output\":\"ok\"}"}}"#,
            r#"{"timestamp":"2026-08-10T13:03:04.000Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Fini."}]}}"#,
        ]);
        assert_eq!(v.len(), 5);
        assert_eq!(v[0]["message"]["content"], "fais les essais");
        assert_eq!(v[0]["cwd"], "/home/u/atelier");
        assert_eq!(v[1]["message"]["content"][0]["type"], "thinking");
        assert_eq!(v[2]["message"]["content"][0]["name"], "Bash");
        assert_eq!(v[2]["message"]["content"][0]["input"]["command"], "cargo test");
        assert_eq!(v[3]["message"]["content"][0]["tool_use_id"], "c1");
        assert_eq!(v[3]["message"]["content"][0]["content"], "ok");
        assert_eq!(v[4]["message"]["content"][0]["text"], "Fini.");
        assert_eq!(v[4]["timestamp"], "2026-08-10T13:03:04.000Z");
    }

    #[test]
    fn le_cadre_du_harnais_et_le_journal_d_affichage_sont_ignores() {
        let v = lire(&[
            META,
            r#"{"timestamp":"t","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<environment_context>cwd: /x</environment_context>"}]}}"#,
            r#"{"timestamp":"t","type":"event_msg","payload":{"type":"agent_message","message":"doublon"}}"#,
            r#"{"timestamp":"t","type":"inconnu","payload":{}}"#,
            "{}",
        ]);
        assert!(v.is_empty(), "{v:?}");
    }

    #[test]
    fn un_correctif_devient_une_modification_du_fichier_nomme() {
        let (nom, e) = traduire_appel("apply_patch", &json!({ "input": "*** Begin Patch\n*** Update File: src/a.rs\n@@\n-x\n+y\n*** End Patch" }));
        assert_eq!(nom, "Edit");
        assert_eq!(e["file_path"], "src/a.rs");
    }
}
