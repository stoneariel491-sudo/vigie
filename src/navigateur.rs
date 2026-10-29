//! La page, dans une fenetre enfant de la vigie : WebView2, par `wry`.
//!
//! Elle n'existe que pendant que la page est ouverte (second geste de Stone),
//! et rien n'est charge avant : `wry` n'est touche qu'ici, au premier appel de
//! `ouvrir`. Ce que la page peut faire est ce que la securite du cadre autorise
//! et rien de plus : profil InPrivate vide dans un dossier a part, supprime a la
//! fermeture ; aucun telechargement ; aucune permission ; aucune fenetre fille ;
//! seuls `http:` et `https:` naviguent ; aucun pont de messages (la vigie
//! lit l'etat de la page en l'interrogeant, jamais l'inverse).
//!
//! Le passage est cherche de deux manieres : le fragment de texte dans
//! l'adresse (`#:~:text=`), que le moteur applique au chargement, puis, si la
//! page n'a pas defile apres un delai, un script de la vigie qui cherche le
//! texte jusqu'a quinze secondes (les pages remplies apres coup) et le surligne
//! par l'API des surbrillances CSS. Ce script ne recoit que le texte a chercher.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use wry::dpi::{PhysicalPosition, PhysicalSize};
use wry::{NewWindowResponse, PageLoadEvent, PermissionResponse, Rect, WebContext, WebView, WebViewBuilder};

use crate::passage::{classer, Classe};

/// Le delai apres lequel une page qui n'a pas defile est cherchee par script.
const DELAI_SONDE: Duration = Duration::from_millis(2500);
/// L'intervalle entre deux interrogations de la page.
const PAS_SONDE: Duration = Duration::from_millis(300);
/// Au-dela, un passage encore cherche est dit non retrouve.
const DELAI_ABANDON: Duration = Duration::from_secs(20);

/// Ce que la vigie sait du passage dans la page ouverte.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Sonde {
    /// La page charge ou la recherche court.
    #[default]
    Attente,
    /// La page a defile jusqu'au passage, ou le script l'a surligne.
    Trouve,
    /// Quinze secondes de recherche, et rien.
    NonRetrouve,
    /// Aucun passage a chercher : la page est ouverte en haut.
    Sans,
    /// Le moteur de pages n'a pas demarre.
    Indisponible,
}

#[derive(Default)]
struct Etat {
    sonde: Sonde,
    charge: Option<Instant>,
    script_lance: bool,
    y: i64,
    echap: bool,
    profondeur: u32,
    courante: String,
    refus: Vec<String>,
}

pub struct Navigateur {
    // L'ordre compte : la vue tombe avant son contexte, qui tombe avant que le
    // dossier du profil ne soit supprime.
    vue: WebView,
    _contexte: WebContext,
    profil: PathBuf,
    etat: Arc<Mutex<Etat>>,
    cherche: bool,
    prochaine: Instant,
    derniere: [i32; 4],
    ouverte: Instant,
}

/// Le dossier d'un profil : `vigie-navigateur-<pid>-<n>` sous le dossier temporaire.
fn dossier_de_profil(n: u32) -> PathBuf {
    std::env::temp_dir().join(format!("vigie-navigateur-{}-{n}", std::process::id()))
}

/// Un processus de ce numero tourne-t-il encore ?
#[cfg(windows)]
fn vivant(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return false;
        }
        let mut code = 0u32;
        let ok = GetExitCodeProcess(h, &mut code) != 0;
        CloseHandle(h);
        ok && code == 259 // STILL_ACTIVE
    }
}

#[cfg(not(windows))]
fn vivant(_pid: u32) -> bool {
    false
}

/// Au lancement : les profils qu'un arret force a laisses. Un dossier dont le
/// processus tourne encore est celui d'une autre vigie ouverte : on n'y touche pas.
pub fn balayer_orphelins() -> usize {
    balayer_dans(&std::env::temp_dir(), &vivant)
}

pub fn balayer_dans(dossier: &std::path::Path, vivant: &dyn Fn(u32) -> bool) -> usize {
    let Ok(lecture) = std::fs::read_dir(dossier) else { return 0 };
    let mut retires = 0;
    for e in lecture.flatten() {
        let nom = e.file_name().to_string_lossy().to_string();
        let Some(reste) = nom.strip_prefix("vigie-navigateur-") else { continue };
        let Some(pid) = reste.split('-').next().and_then(|p| p.parse::<u32>().ok()) else { continue };
        if vivant(pid) {
            continue;
        }
        if std::fs::remove_dir_all(e.path()).is_ok() {
            retires += 1;
        }
    }
    retires
}

/// Le script que la vigie pose dans la page : l'echappement, et la recherche du
/// passage. `passage` est la seule donnee qui y entre, en chaine JSON.
fn script(passage: Option<&str>) -> String {
    let texte = passage.map(|p| serde_json::to_string(p).unwrap_or_else(|_| "null".into())).unwrap_or_else(|| "null".into());
    format!(
        r#"(()=>{{
if(window.__vigie)return;window.__vigie=1;window.__vigie_echap=false;window.__vigie_etat='';
addEventListener('keydown',e=>{{if(e.key==='Escape')window.__vigie_echap=true;}},true);
const T={texte};
if(T===null)return;
const pli=s=>s.toLowerCase().replace(/[‘’]/g,"'").replace(/[“”«»]/g,'"').replace(/[–—]/g,'-');
function trouver(){{
  const racine=document.body||document.documentElement;if(!racine)return null;
  const w=document.createTreeWalker(racine,NodeFilter.SHOW_TEXT,{{acceptNode(n){{const p=n.parentElement;if(!p)return 2;const t=p.tagName;return(t==='SCRIPT'||t==='STYLE'||t==='NOSCRIPT'||t==='TEMPLATE')?2:1;}}}});
  let texte='';const carte=[];
  for(let n=w.nextNode();n;n=w.nextNode()){{const s=n.nodeValue;for(let i=0;i<s.length;i++){{const c=s[i];if(/\s/.test(c))continue;for(const ch of pli(c)){{texte+=ch;carte.push([n,i]);}}}}}}
  const q=pli(T).replace(/\s+/g,'');if(!q)return null;
  const j=texte.indexOf(q);if(j<0)return null;
  const a=carte[j],b=carte[j+q.length-1];
  const r=document.createRange();r.setStart(a[0],a[1]);r.setEnd(b[0],b[1]+1);return r;
}}
function surligner(r){{
  try{{
    if(window.CSS&&CSS.highlights&&typeof Highlight==='function'){{
      if(!document.getElementById('__vigie_style')){{const st=document.createElement('style');st.id='__vigie_style';st.textContent='::highlight(vigie){{background-color:#ffc857;color:#1a1a1a;}}';(document.head||document.documentElement).appendChild(st);}}
      CSS.highlights.set('vigie',new Highlight(r));
    }}else{{const s=getSelection();s.removeAllRanges();s.addRange(r);}}
  }}catch(e){{}}
  const rc=r.getBoundingClientRect();scrollBy(0,rc.top-innerHeight/3);
}}
window.__vigie_chercher=()=>{{
  if(window.__vigie_etat==='cherche'||window.__vigie_etat==='trouve')return;
  window.__vigie_etat='cherche';let n=0;
  const iv=setInterval(()=>{{
    n++;
    if(document.readyState!=='loading'){{const r=trouver();if(r){{surligner(r);window.__vigie_etat='trouve';clearInterval(iv);return;}}}}
    if(n>60){{window.__vigie_etat='non';clearInterval(iv);}}
  }},250);
}};
}})();"#
    )
}

impl Navigateur {
    /// Ouvre la page dans la zone donnee (pixels physiques de la fenetre).
    /// `adresse` porte deja le fragment de texte ; `passage` est ce que le script
    /// cherchera si le fragment n'a pas suffi. `local_ok` : Stone a accepte
    /// l'adresse de sa machine.
    pub fn ouvrir(fenetre: &winit::window::Window, zone: [f32; 4], adresse: &str, passage: Option<&str>, local_ok: bool) -> Result<Navigateur, String> {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let profil = dossier_de_profil(n);
        let _ = std::fs::remove_dir_all(&profil);
        let etat = Arc::new(Mutex::new(Etat { sonde: if passage.is_some() { Sonde::Attente } else { Sonde::Sans }, courante: adresse.split('#').next().unwrap_or("").to_string(), ..Etat::default() }));
        let mut contexte = WebContext::new(Some(profil.clone()));
        let (e_nav, e_charge) = (etat.clone(), etat.clone());
        let b = WebViewBuilder::new_with_web_context(&mut contexte)
            .with_incognito(true)
            .with_clipboard(false)
            .with_visible(true)
            .with_bounds(rect(zone))
            .with_initialization_script_for_main_only(script(passage), true)
            .with_permission_handler(|_| PermissionResponse::Deny)
            .with_new_window_req_handler(|_, _| NewWindowResponse::Deny)
            .with_download_started_handler(move |u, _| {
                eprintln!("telechargement refuse : {u}");
                false
            })
            .with_navigation_handler(move |u| {
                let bas = u.to_ascii_lowercase();
                let ouvrable = bas == "about:blank"
                    || match classer(&u) {
                        Classe::Web | Classe::NonChiffre => true,
                        Classe::Local => local_ok,
                        _ => false,
                    };
                let mut e = e_nav.lock().unwrap();
                if ouvrable {
                    if bas != "about:blank" {
                        e.courante = u.split('#').next().unwrap_or("").to_string();
                    }
                } else {
                    eprintln!("navigation refusee : {}", u.chars().take(80).collect::<String>());
                    e.refus.push(u.chars().take(80).collect());
                }
                ouvrable
            })
            .with_on_page_load_handler(move |ev, _| {
                if matches!(ev, PageLoadEvent::Finished) {
                    let mut e = e_charge.lock().unwrap();
                    e.charge = Some(Instant::now());
                    e.script_lance = false;
                }
            })
            .with_url(adresse);
        let vue = b.build_as_child(fenetre).map_err(|e| e.to_string())?;
        Ok(Navigateur {
            vue,
            _contexte: contexte,
            profil,
            etat,
            cherche: passage.is_some(),
            prochaine: Instant::now() + PAS_SONDE,
            derniere: [zone[0] as i32, zone[1] as i32, zone[2] as i32, zone[3] as i32],
            ouverte: Instant::now(),
        })
    }

    /// Pose la page dans sa zone, si elle a bouge.
    pub fn poser(&mut self, zone: [f32; 4]) {
        let z = [zone[0] as i32, zone[1] as i32, zone[2] as i32, zone[3] as i32];
        if z != self.derniere {
            self.derniere = z;
            let _ = self.vue.set_bounds(rect(zone));
        }
    }

    /// Une trame : interroge la page, a intervalle, sans rien lui envoyer d'autre
    /// que la relance de la recherche.
    pub fn tic(&mut self) {
        if Instant::now() < self.prochaine {
            return;
        }
        self.prochaine = Instant::now() + PAS_SONDE;
        let e = self.etat.clone();
        let _ = self.vue.evaluate_script_with_callback(
            "(()=>{try{return JSON.stringify({y:Math.round(scrollY),n:history.length,e:window.__vigie_echap===true,s:String(window.__vigie_etat||'')})}catch(x){return '{}'}})()",
            move |r| {
                // Du JSON dans du JSON ; seuls des nombres, un booleen et l'un de
                // quatre mots en sortent, rien qui s'affiche ou s'execute.
                let interne: String = serde_json::from_str(&r).unwrap_or_default();
                let Ok(v) = serde_json::from_str::<serde_json::Value>(&interne) else { return };
                let mut e = e.lock().unwrap();
                e.profondeur = v.get("n").and_then(|x| x.as_u64()).unwrap_or(1).saturating_sub(1).min(1000) as u32;
                e.y = v.get("y").and_then(|x| x.as_i64()).unwrap_or(0);
                if v.get("e").and_then(|x| x.as_bool()) == Some(true) {
                    e.echap = true;
                }
                match v.get("s").and_then(|x| x.as_str()) {
                    Some("trouve") => e.sonde = Sonde::Trouve,
                    Some("non") if e.sonde == Sonde::Attente => e.sonde = Sonde::NonRetrouve,
                    _ => {}
                }
            },
        );
        if !self.cherche {
            return;
        }
        let mut lancer = false;
        {
            let mut e = self.etat.lock().unwrap();
            if e.sonde != Sonde::Attente {
                return;
            }
            match (e.charge, e.script_lance) {
                (Some(t), false) if t.elapsed() >= DELAI_SONDE => {
                    // Le fragment a-t-il defile la page ? Sinon, le script cherche.
                    if e.y > 0 {
                        e.sonde = Sonde::Trouve;
                    } else {
                        e.script_lance = true;
                        lancer = true;
                    }
                }
                (_, true) if self.ouverte.elapsed() >= DELAI_ABANDON => e.sonde = Sonde::NonRetrouve,
                _ => {}
            }
        }
        if lancer {
            let _ = self.vue.evaluate_script("window.__vigie_chercher&&window.__vigie_chercher()");
        }
    }

    pub fn sonde(&self) -> Sonde {
        self.etat.lock().unwrap().sonde
    }

    /// Echap a ete presse dans la page (une seule fois).
    pub fn echap_demande(&self) -> bool {
        let mut e = self.etat.lock().unwrap();
        if e.echap {
            e.echap = false;
            let _ = self.vue.evaluate_script("window.__vigie_echap=false");
            return true;
        }
        false
    }

    /// Y a-t-il une page a laquelle revenir dans l'historique de la page ?
    pub fn peut_reculer(&self) -> bool {
        self.etat.lock().unwrap().profondeur > 0
    }

    pub fn reculer(&self) {
        let _ = self.vue.evaluate_script("history.back()");
    }

    /// L'adresse reelle de la page affichee, sans son fragment.
    pub fn courante(&self) -> String {
        self.etat.lock().unwrap().courante.clone()
    }
}

impl Drop for Navigateur {
    fn drop(&mut self) {
        // Le profil disparait avec la page : le moteur lache ses fichiers un
        // peu apres sa fermeture, d'ou les reprises ; ce qui resterait apres un
        // arret force est balaye au lancement suivant.
        let profil = self.profil.clone();
        std::thread::spawn(move || {
            for _ in 0..60 {
                if std::fs::remove_dir_all(&profil).is_ok() || !profil.exists() {
                    eprintln!("profil supprime : {}", !profil.exists());
                    return;
                }
                std::thread::sleep(Duration::from_millis(250));
            }
            eprintln!("profil non supprime (balayage au prochain lancement) : {}", profil.display());
        });
    }
}

fn rect(z: [f32; 4]) -> Rect {
    Rect {
        position: PhysicalPosition::new(z[0].round() as i32, z[1].round() as i32).into(),
        size: PhysicalSize::new(z[2].round().max(1.0) as u32, z[3].round().max(1.0) as u32).into(),
    }
}

#[cfg(test)]
mod essais {
    use super::*;

    #[test]
    fn le_balayage_retire_les_profils_morts_et_laisse_les_vivants_et_le_reste() {
        let base = std::env::temp_dir().join(format!("vigie-essai-balayage-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        for nom in ["vigie-navigateur-111-0", "vigie-navigateur-222-3", "vigie-navigateur-abc-1", "autre-dossier", "vigie-navigateur-333-0"] {
            std::fs::create_dir_all(base.join(nom).join("EBWebView")).unwrap();
        }
        let retires = balayer_dans(&base, &|pid| pid == 222);
        // 111 et 333 sont morts ; 222 tourne ; « abc » n'est pas un numero ; « autre » n'est pas un profil.
        assert_eq!(retires, 2);
        assert!(!base.join("vigie-navigateur-111-0").exists());
        assert!(!base.join("vigie-navigateur-333-0").exists());
        assert!(base.join("vigie-navigateur-222-3").exists());
        assert!(base.join("vigie-navigateur-abc-1").exists());
        assert!(base.join("autre-dossier").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn le_processus_courant_est_vivant_et_un_numero_improbable_ne_l_est_pas() {
        assert!(vivant(std::process::id()));
        assert!(!vivant(0x7fff_fff0));
    }

    #[test]
    fn le_script_ne_recoit_que_le_passage_et_l_echappe() {
        let s = script(Some("il a dit \"non\" </script> \\ fin"));
        assert!(s.contains(r#"const T="il a dit \"non\" </script> \\ fin";"#), "{s}");
        // Sans passage, le script ne cherche rien mais garde l'echappement.
        let s = script(None);
        assert!(s.contains("const T=null;"));
        assert!(s.contains("__vigie_echap"));
        // Aucun pont de messages vers la vigie.
        assert!(!s.contains("postMessage") && !s.contains("ipc") && !s.contains("fetch(") && !s.contains("XMLHttpRequest"));
    }
}
