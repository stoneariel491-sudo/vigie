//! Les sessions ordinaires de Claude Code : `~/.claude/projects/<projet>/<id>.jsonl`.
//!
//! La vigie suit les agents d'un workflow (`<session>/subagents/workflows/wf_*`).
//! Quelqu'un qui travaille avec Claude Code sans jamais lancer de workflow n'y
//! verrait rien : sa session est un fichier a la racine du projet. Ces lignes
//! sont deja au format que la vigie lit ; il suffit de les presenter comme un
//! chantier d'un agent, ce que fait l'import pour tous les outils.

use super::{maison, Convertisseur};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub fn racine() -> PathBuf {
    match std::env::var_os("CLAUDE_CONFIG_DIR").filter(|v| !v.is_empty()) {
        Some(h) => PathBuf::from(h).join("projects"),
        None => maison().join(".claude").join("projects"),
    }
}

pub fn trouver() -> Vec<PathBuf> {
    let mut v = Vec::new();
    let Ok(projets) = std::fs::read_dir(racine()) else { return v };
    for p in projets.filter_map(|e| e.ok()) {
        let Ok(fichiers) = std::fs::read_dir(p.path()) else { continue };
        for f in fichiers.filter_map(|e| e.ok()) {
            let chemin = f.path();
            if chemin.is_file() && chemin.extension().is_some_and(|e| e == "jsonl") {
                v.push(chemin);
            }
        }
    }
    v
}

pub fn nouveau(source: &Path) -> Box<dyn Convertisseur> {
    Box::new(Claude {
        cwd: String::new(),
        id: source.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
    })
}

struct Claude {
    cwd: String,
    id: String,
}

impl Convertisseur for Claude {
    fn cwd(&self) -> String {
        self.cwd.clone()
    }

    fn session(&self) -> String {
        self.id.clone()
    }

    fn ligne(&mut self, v: &Value) -> Vec<Value> {
        if let Some(c) = v.get("cwd").and_then(|x| x.as_str()) {
            if self.cwd.is_empty() {
                self.cwd = c.to_string();
            }
        }
        match v.get("type").and_then(|x| x.as_str()) {
            Some("user") | Some("assistant") if v.get("message").is_some() => vec![v.clone()],
            _ => vec![],
        }
    }
}

#[cfg(test)]
mod essais {
    use super::*;

    #[test]
    fn une_session_ordinaire_garde_ses_echanges_et_son_dossier() {
        let mut c = Claude { cwd: String::new(), id: "s1".into() };
        let ligne = |s: &str| serde_json::from_str::<Value>(s).unwrap();
        let mut sortie = Vec::new();
        for l in [
            r#"{"type":"summary","summary":"x"}"#,
            r#"{"type":"user","cwd":"/home/u/atelier","timestamp":"t","message":{"role":"user","content":"bonjour"}}"#,
            r#"{"type":"assistant","cwd":"/home/u/atelier","timestamp":"t","message":{"role":"assistant","content":[{"type":"text","text":"salut"}]}}"#,
            r#"{"type":"file-history-snapshot","snapshot":{}}"#,
        ] {
            sortie.extend(c.ligne(&ligne(l)));
        }
        assert_eq!(sortie.len(), 2);
        assert_eq!(c.cwd(), "/home/u/atelier");
        assert_eq!(c.session(), "s1");
    }
}
