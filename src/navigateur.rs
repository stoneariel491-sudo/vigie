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
//! La regle de Stone pour le local est appliquee au RESEAU, pas a la page : tout
//! le trafic du moteur passe par le mandataire de la vigie (`mandataire.rs`), qui
//! resout lui-meme chaque nom et juge l'adresse obtenue. Les gardiens par
//! evenement du moteur (navigation, cadre, ressource) restent, en seconde ligne :
//! leur refus est mesure comme non tenu (`passage-b/troisieme/`, mecanisme), le
//! mandataire est la ligne qui tient.
//!
//! Le passage est cherche de deux manieres : le fragment de texte dans
//! l'adresse (`#:~:text=`), que le moteur applique au chargement, et un script
//! de la vigie qui, apres un delai, cherche le texte jusqu'a quinze secondes
//! (les pages remplies apres coup) et le surligne par l'API des surbrillances
//! CSS. La vigie ne dit « retrouve » que sur ce que ce script a verifie : le
//! texte est dans la page, affiche, a l'ecran. Une valeur que la page pourrait
//! ecrire elle-meme (une variable globale, un defilement) ne prouve rien. Le
//! script ne recoit que le texte a chercher ; son etat vit dans une fermeture, et
//! sa poignee porte un nom tire au sort que la page ne connait pas.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use webview2_com::Microsoft::Web::WebView2::Win32::{
    ICoreWebView2, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL, COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL,
};
use webview2_com::{take_pwstr, BasicAuthenticationRequestedEventHandler, HistoryChangedEventHandler, NavigationStartingEventHandler, WebResourceRequestedEventHandler};
use windows::core::{Interface, HSTRING, PCWSTR, PWSTR};
use wry::dpi::{PhysicalPosition, PhysicalSize};
use wry::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use wry::{NewWindowResponse, PageLoadEvent, PermissionResponse, Rect, WebContext, WebView, WebViewBuilder};
use wry::{WebViewBuilderExtWindows, WebViewExtWindows};

use crate::mandataire::{Accord, Mandataire};
use crate::passage::{self, classer, Classe};

/// Les arguments du moteur, hors mandataire. `--disable-quic` : QUIC (UDP) ne
/// passe pas par un mandataire HTTP ; `disable_non_proxied_udp` : WebRTC ne
/// sort pas en UDP direct, ce qui lui ferait joindre le reseau local.
const ARGUMENTS_DU_MOTEUR: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --lang=en-US --accept-lang=en-US,en --disable-quic --webrtc-ip-handling-policy=disable_non_proxied_udp";

/// Les arguments complets : le trafic du moteur passe par le mandataire, y
/// compris celui de la boucle locale (`<-loopback>` retire l'exception que le
/// moteur se donne pour `localhost`, `127.0.0.0/8` et `[::1]`).
fn arguments_du_moteur(port: u16) -> String {
    format!("{ARGUMENTS_DU_MOTEUR} --proxy-server=http://127.0.0.1:{port} --proxy-bypass-list=<-loopback>")
}

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
    echap: bool,
    /// Un geste d'historique fait au clavier dans la page (Alt + fleche), a rendre une fois.
    geste: Option<Geste>,
    /// L'historique du moteur lui-meme : `CanGoBack` et `CanGoForward`, relus a chaque changement.
    recul: bool,
    avance: bool,
    courante: String,
    refus: Vec<String>,
    /// Les refus du mandataire, comptes a part : les cadres refuses ne doivent pas
    /// user les vingt lignes que la sortie d'erreur se donne.
    refus_du_mandataire: usize,
}

/// Un geste de l'historique de la page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Geste {
    Reculer,
    Avancer,
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
    /// La fenetre de la vigie, proprietaire de toute boite que le moteur pose.
    hote: isize,
    prochain_balayage: Instant,
    boite_vue: bool,
    /// Le nom de la poignee du script dans la page, tire au sort a l'ouverture.
    cle: String,
    /// Le mandataire par lequel passe tout le trafic du moteur ; il tombe apres la vue.
    mandataire: Mandataire,
    /// La page a ete quittee faute d'avoir traverse le mandataire.
    coupee: bool,
}

/// Un nom que la page ne peut pas deviner : de l'aleatoire du systeme, deux fois.
fn cle_secrete() -> String {
    use std::hash::{BuildHasher, Hasher};
    let tirage = || std::collections::hash_map::RandomState::new().build_hasher().finish();
    format!("v{:016x}{:016x}", tirage(), tirage())
}

/// Ce que le moteur a le droit d'ouvrir, pour la page et pour chaque lien qu'elle
/// suit : le meme jugement que la lecture, donc les memes ecritures du local
/// (decimale, hexadecimale, IPv6 mappee, point final) et du reseau prive. Le
/// local ne s'ouvre que si Stone l'a accepte pour cette page.
pub fn navigation_permise(u: &str, local_ok: bool) -> bool {
    u.eq_ignore_ascii_case("about:blank")
        || match classer(u) {
            Classe::Web | Classe::NonChiffre => true,
            Classe::Local => local_ok,
            _ => false,
        }
}

/// L'hote de sa machine que Stone a accepte : celui de la page elle-meme, et
/// seulement quand il a confirme une adresse locale. C'est le seul hote local
/// que la page, ses cadres et ses ressources ont le droit de joindre.
pub fn hote_accepte(adresse: &str, local_ok: bool) -> Option<String> {
    (local_ok && classer(adresse) == Classe::Local).then(|| passage::adresse(adresse).hote)
}

/// Le jugement des cadres (`<iframe>`) : celui du gardien de navigation, mais
/// l'accord pour le local ne vaut que pour l'hote (et le port) que Stone a vu.
/// Un cadre vers un autre service de sa machine n'est pas ce qu'il a accepte.
pub fn cadre_permis(u: &str, accepte: Option<&str>) -> bool {
    if u.eq_ignore_ascii_case("about:blank") || u.eq_ignore_ascii_case("about:srcdoc") {
        return true;
    }
    match classer(u) {
        Classe::Web | Classe::NonChiffre => true,
        Classe::Local => accepte.is_some_and(|h| passage::adresse(u).hote == h),
        _ => false,
    }
}

/// Le jugement de toute ressource que la page demande (image, script, feuille de
/// style, `fetch`, cadre, connexion) : celui des cadres, plus ce qui n'a pas de
/// reseau (`data:`, `blob:`).
pub fn ressource_permise(u: &str, accepte: Option<&str>) -> bool {
    let bas = u.trim_start().to_ascii_lowercase();
    bas.starts_with("data:") || bas.starts_with("blob:") || cadre_permis(u, accepte)
}

/// Ce que le moteur ne soumet pas au gardien de navigation de `wry` : les cadres
/// (`<iframe>`), toute ressource qu'une page demande, l'historique. Si l'un
/// d'eux ne se pose pas, la page ne s'ouvre pas : un gardien absent ne vaut rien.
fn poser_les_gardiens(vue: &WebView, etat: &Arc<Mutex<Etat>>, accepte: Option<String>, identifiants: (String, String)) -> Result<(), String> {
    let wv: ICoreWebView2 = vue.webview();
    let env = vue.environment();
    let mut jeton = 0i64;
    let e = |x: windows::core::Error| x.to_string();
    unsafe {
        let (e_cadre, a_cadre) = (etat.clone(), accepte.clone());
        wv.add_FrameNavigationStarting(
            &NavigationStartingEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                let mut uri = PWSTR::null();
                args.Uri(&mut uri)?;
                let u = take_pwstr(uri);
                if !cadre_permis(&u, a_cadre.as_deref()) {
                    args.SetCancel(true)?;
                    noter_refus(&e_cadre, "cadre", &u);
                }
                Ok(())
            })),
            &mut jeton,
        )
        .map_err(e)?;

        let filtre = HSTRING::from("*");
        match wv.cast::<webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_22>() {
            Ok(w22) => w22
                .AddWebResourceRequestedFilterWithRequestSourceKinds(&filtre, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL, COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL)
                .map_err(e)?,
            // Un moteur plus ancien : le filtre ne voit pas les cadres, qui restent
            // juges a leur navigation.
            Err(_) => wv.AddWebResourceRequestedFilter(&filtre, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL).map_err(e)?,
        }
        let (e_res, a_res) = (etat.clone(), accepte);
        wv.add_WebResourceRequested(
            &WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                let requete = args.Request()?;
                let mut uri = PWSTR::null();
                requete.Uri(&mut uri)?;
                let u = take_pwstr(uri);
                if !ressource_permise(&u, a_res.as_deref()) {
                    let refus = env.CreateWebResourceResponse(None::<&windows::Win32::System::Com::IStream>, 403, &HSTRING::from("Refuse par la vigie"), &HSTRING::from(""))?;
                    args.SetResponse(&refus)?;
                    noter_refus(&e_res, "ressource", &u);
                }
                Ok(())
            })),
            &mut jeton,
        )
        .map_err(e)?;

        // Le defi du mandataire (407) : le moteur y repond par le jeton que la vigie
        // lui a donne, et par rien d'autre ; tout autre defi reste sans reponse.
        let w10 = wv.cast::<webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_10>().map_err(e)?;
        w10.add_BasicAuthenticationRequested(
            &BasicAuthenticationRequestedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                let mut uri = PWSTR::null();
                args.Uri(&mut uri)?;
                let _ = take_pwstr(uri);
                let reponse = args.Response()?;
                reponse.SetUserName(PCWSTR(HSTRING::from(identifiants.0.as_str()).as_ptr()))?;
                reponse.SetPassword(PCWSTR(HSTRING::from(identifiants.1.as_str()).as_ptr()))?;
                Ok(())
            })),
            &mut jeton,
        )
        .map_err(e)?;

        let e_hist = etat.clone();
        wv.add_HistoryChanged(
            &HistoryChangedEventHandler::create(Box::new(move |wv, _| {
                let Some(wv) = wv else { return Ok(()) };
                let (mut arriere, mut avant) = (windows::core::BOOL(0), windows::core::BOOL(0));
                wv.CanGoBack(&mut arriere)?;
                wv.CanGoForward(&mut avant)?;
                let mut s = e_hist.lock().unwrap();
                s.recul = arriere.as_bool();
                s.avance = avant.as_bool();
                Ok(())
            })),
            &mut jeton,
        )
        .map_err(e)?;
    }
    Ok(())
}

/// Un refus des gardiens : dit sur la sortie d'erreur (vingt au plus, une page
/// peut en demander des milliers) et garde pour les essais.
fn noter_refus(etat: &Arc<Mutex<Etat>>, genre: &str, u: &str) {
    let mut s = etat.lock().unwrap();
    let du_mandataire = genre == "mandataire";
    if du_mandataire {
        s.refus_du_mandataire += 1;
    }
    if (du_mandataire && s.refus_du_mandataire <= 200) || s.refus.len() < 20 {
        eprintln!("{genre} refuse : {}", u.chars().take(80).collect::<String>());
    }
    if s.refus.len() < 1000 {
        s.refus.push(u.chars().take(80).collect());
    }
}

/// Referme les boites que d'autres processus ont posees sur la fenetre `hote`
/// (le moteur de page, pour un selecteur de fichiers) et rend la fenetre si elle
/// reste inactive sans qu'aucune boite ne la tienne. Rend le nombre de boites
/// refermees. `hors_pid` : le processus dont les fenetres ne sont pas touchees
/// (la vigie elle-meme ; 0 pour l'essai).
#[cfg(windows)]
pub fn liberer_la_fenetre_hors(hote: isize, hors_pid: u32) -> usize {
    use windows_sys::Win32::Foundation::{HWND, LPARAM};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, IsWindowEnabled};
    use windows_sys::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindow, GetWindowThreadProcessId, IsWindowVisible, PostMessageW, GW_OWNER, WM_CLOSE};

    struct Recherche {
        hote: isize,
        hors_pid: u32,
        trouvees: Vec<isize>,
    }
    unsafe extern "system" fn voir(h: HWND, p: LPARAM) -> i32 {
        let r = unsafe { &mut *(p as *mut Recherche) };
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(h, &mut pid) };
        if unsafe { GetWindow(h, GW_OWNER) } as isize == r.hote && unsafe { IsWindowVisible(h) } != 0 && pid != r.hors_pid {
            r.trouvees.push(h as isize);
        }
        1
    }
    let mut r = Recherche { hote, hors_pid, trouvees: Vec::new() };
    unsafe {
        EnumWindows(Some(voir), &mut r as *mut Recherche as LPARAM);
        for &h in &r.trouvees {
            PostMessageW(h as HWND, WM_CLOSE, 0, 0);
        }
        if r.trouvees.is_empty() && IsWindowEnabled(hote as HWND) == 0 {
            EnableWindow(hote as HWND, 1);
        }
    }
    r.trouvees.len()
}

#[cfg(not(windows))]
pub fn liberer_la_fenetre_hors(_hote: isize, _hors_pid: u32) -> usize {
    0
}

fn liberer_la_fenetre(hote: isize) -> usize {
    liberer_la_fenetre_hors(hote, std::process::id())
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

/// Ce que la vigie pose dans TOUS les cadres de la page, avant elle : ce que la
/// page n'a pas le droit de faire et que le moteur laisse faire. Le partage
/// d'ecran ouvre un selecteur sans passer par le gestionnaire de permissions ;
/// l'ecriture du presse-papiers reussit sans geste ; `alert`, `confirm`,
/// `prompt` ouvrent des boites natives qui figent la page ; les selecteurs de
/// fichiers (`showOpenFilePicker`, `<input type=file>`) ouvrent une boite modale
/// dont la fenetre de la vigie est proprietaire : tant qu'elle est ouverte, la
/// vigie est desactivee et ne se ferme plus.
const BLINDAGE: &str = r#"(()=>{
const fige=(o,k,v)=>{try{Object.defineProperty(o,k,{value:v,writable:false,configurable:false,enumerable:false});}catch(e){}};
const refus=()=>Promise.reject(new DOMException('Refusé par la vigie','NotAllowedError'));
if(typeof MediaDevices==='function')fige(MediaDevices.prototype,'getDisplayMedia',refus);
if(typeof Clipboard==='function'){fige(Clipboard.prototype,'writeText',refus);fige(Clipboard.prototype,'write',refus);}
if(typeof Document==='function'){const ex=Document.prototype.execCommand;fige(Document.prototype,'execCommand',function(c,...a){const k=String(c).toLowerCase();return(k==='copy'||k==='cut')?false:Reflect.apply(ex,this,[c,...a]);});}
for(const n of ['showOpenFilePicker','showSaveFilePicker','showDirectoryPicker'])fige(window,n,refus);
for(const [c,m] of [['HID','requestDevice'],['USB','requestDevice'],['Bluetooth','requestDevice'],['Bluetooth','requestLEScan'],['Serial','requestPort'],['CredentialsContainer','get'],['CredentialsContainer','create'],['PaymentRequest','show']])if(typeof window[c]==='function')fige(window[c].prototype,m,refus);
if(typeof HTMLInputElement==='function')fige(HTMLInputElement.prototype,'showPicker',function(){throw new DOMException('Refusé par la vigie','NotAllowedError');});
window.addEventListener('click',e=>{const c=e.composedPath&&e.composedPath();const t=c&&c.length?c[0]:e.target;if(t&&t.tagName==='INPUT'&&String(t.type).toLowerCase()==='file'){e.preventDefault();e.stopImmediatePropagation();}},true);
for(const n of ['WebSocket','WebTransport','RTCPeerConnection','webkitRTCPeerConnection'])if(typeof window[n]==='function')fige(window,n,function(){throw new DOMException('Refusé par la vigie','SecurityError');});
fige(window,'alert',function(){});
fige(window,'confirm',function(){return false;});
fige(window,'prompt',function(){return null;});
})();"#;

/// Le script de la vigie, dans la page principale seulement. `passage` est la
/// seule donnee de la page qui y entre, en chaine JSON ; `cle` est le nom de la
/// poignee, tire au sort a chaque ouverture. Tout ce qu'il emploie du moteur est
/// pris ici, avant que la page ne s'execute, et non retrouve plus tard.
fn script(passage: Option<&str>, cle: &str) -> String {
    let texte = passage.map(|p| serde_json::to_string(p).unwrap_or_else(|_| "null".into())).unwrap_or_else(|| "null".into());
    SCRIPT.replace("@@CLE@@", cle).replace("@@TEXTE@@", &texte)
}

const SCRIPT: &str = r#"(()=>{
const K='@@CLE@@';
if(Object.getOwnPropertyDescriptor(window,K))return;
const T=@@TEXTE@@;
const ap=Reflect.apply,D=Object.getOwnPropertyDescriptor;
const U=f=>(o,...a)=>ap(f,o,a);
const lecteur=(p,n)=>{const d=D(p,n);return d&&d.get?o=>ap(d.get,o,[]):()=>undefined;};
const nodeValue=lecteur(Node.prototype,'nodeValue'),parent=lecteur(Node.prototype,'parentElement'),balise=lecteur(Element.prototype,'tagName');
const racineDoc=lecteur(Document.prototype,'documentElement'),corpsDoc=lecteur(Document.prototype,'body'),etatDoc=lecteur(Document.prototype,'readyState');
const haut=lecteur(DOMRectReadOnly.prototype,'top'),bas=lecteur(DOMRectReadOnly.prototype,'bottom'),large=lecteur(DOMRectReadOnly.prototype,'width'),grand=lecteur(DOMRectReadOnly.prototype,'height');
const touche=lecteur(KeyboardEvent.prototype,'key'),alt=lecteur(KeyboardEvent.prototype,'altKey'),ctrl=lecteur(KeyboardEvent.prototype,'ctrlKey'),maj=lecteur(KeyboardEvent.prototype,'shiftKey'),meta=lecteur(KeyboardEvent.prototype,'metaKey');
// `isTrusted` est un attribut inalterable de chaque evenement (propre a l'instance, non
// configurable), pas du prototype : c'est sur l'instance qu'on le lit.
const fiable=e=>{const d=D(e,'isTrusted');return d?(d.get?ap(d.get,e,[])===true:d.value===true):false;};
const genre=lecteur(Node.prototype,'nodeType'),ancetre=lecteur(Range.prototype,'commonAncestorContainer');
const styleDe=window.getComputedStyle,valeur=U(CSSStyleDeclaration.prototype.getPropertyValue),flottant=window.parseFloat;
const marcheur=U(Document.prototype.createTreeWalker),suivant=U(TreeWalker.prototype.nextNode),nouveauRange=U(Document.prototype.createRange);
const debutR=U(Range.prototype.setStart),finR=U(Range.prototype.setEnd),rectR=U(Range.prototype.getBoundingClientRect);
const indexDe=U(String.prototype.indexOf),minuscule=U(String.prototype.toLowerCase),codeDe=U(String.prototype.charCodeAt);
const dh=D(window,'innerHeight'),hauteur=()=>dh&&dh.get?ap(dh.get,window,[]):(dh?dh.value:0);
const defiler=window.scrollBy,minuteur=window.setInterval,arret=window.clearInterval,ecoute=EventTarget.prototype.addEventListener;
const Surbrillance=window.Highlight,registre=(window.CSS&&CSS.highlights)||null,poser=registre?U(HighlightRegistry.prototype.set):null;
let etat='',echap=false,geste='',iv=0;
ap(ecoute,window,['keydown',e=>{
  if(fiable(e)!==true)return;
  const k=touche(e);
  if(k==='Escape')echap=true;
  else if(alt(e)&&!ctrl(e)&&!maj(e)&&!meta(e)){if(k==='ArrowLeft')geste='b';else if(k==='ArrowRight')geste='f';}
},true]);
try{
  const CSSFeuille=window.CSSStyleSheet,f=new CSSFeuille();
  f.replaceSync('::highlight(vigie),::target-text{background-color:#ffc857;color:#1a1a1a;text-decoration:underline;text-decoration-thickness:2px;text-decoration-color:#1a1a1a;}');
  document.adoptedStyleSheets=[f];
}catch(e){}
const plie=c=>{const k=codeDe(c,0);
  if(k===0x2018||k===0x2019)return"'";
  if(k===0x201c||k===0x201d||k===0xab||k===0xbb)return'"';
  if(k===0x2013||k===0x2014)return'-';
  return minuscule(c);};
const blanc=k=>k<=32||k===160||k===0x1680||(k>=0x2000&&k<=0x200a)||k===0x2028||k===0x2029||k===0x202f||k===0x205f||k===0x3000||k===0xfeff;
function aiguille(){let q='';for(let i=0;i<T.length;i++){const c=T[i];if(blanc(codeDe(c,0)))continue;q+=plie(c);}return q;}
function lisible(r){
  let n=ancetre(r);if(!n)return false;
  if(genre(n)!==1)n=parent(n);
  let premier=true;
  for(let e=n;e;e=parent(e)){
    const cs=ap(styleDe,window,[e]);
    if(flottant(valeur(cs,'opacity'))<0.1||valeur(cs,'visibility')==='hidden'||valeur(cs,'display')==='none')return false;
    if(premier){const c=valeur(cs,'color');if(indexDe(c,'rgba(')===0&&indexDe(c,', 0)')===c.length-4)return false;premier=false;}
  }
  return true;
}
function visible(r){const rc=rectR(r);return large(rc)>0&&grand(rc)>0&&lisible(r);}
function trouver(){
  const racine=corpsDoc(document)||racineDoc(document);if(!racine)return null;
  const w=marcheur(document,racine,4,{acceptNode(n){const p=parent(n);if(!p)return 2;const t=balise(p);return(t==='SCRIPT'||t==='STYLE'||t==='NOSCRIPT'||t==='TEMPLATE')?2:1;}});
  let texte='';const noeud=[],place=[];
  for(let n=suivant(w);n;n=suivant(w)){const s=nodeValue(n);for(let i=0;i<s.length;i++){const c=s[i];if(blanc(codeDe(c,0)))continue;const m=plie(c);for(let k=0;k<m.length;k++){noeud[texte.length]=n;place[texte.length]=i;texte+=m[k];}}}
  const q=aiguille();if(!q)return null;
  let depart=0,essais=0;
  while(essais++<50){
    const j=indexDe(texte,q,depart);if(j<0)break;
    const a=j,b=j+q.length-1;
    const r=nouveauRange(document);debutR(r,noeud[a],place[a]);finR(r,noeud[b],place[b]+1);
    if(visible(r))return r;
    depart=j+1;
  }
  return null;
}
function surligner(r){
  try{if(registre&&Surbrillance)poser(registre,'vigie',new Surbrillance(r));else{const s=getSelection();s.removeAllRanges();s.addRange(r);}}catch(e){}
}
function essayer(){
  if(etatDoc(document)==='loading')return false;
  const r=trouver();if(!r)return false;
  const rc=rectR(r),h=hauteur(),t=haut(rc),b=bas(rc);
  if(b<=0||t>=h){ap(defiler,window,[0,t-h/3]);return false;}
  surligner(r);return true;
}
function lancer(){
  if(etat==='cherche'||etat==='trouve'||T===null)return;
  etat='cherche';let n=0;
  iv=ap(minuteur,window,[()=>{
    n++;
    if(essayer()){etat='trouve';ap(arret,window,[iv]);return;}
    if(n>60){etat='non';ap(arret,window,[iv]);}
  },250]);
}
Object.defineProperty(window,K,{value:function(c){
  if(c==='c'){lancer();return'';}
  if(c==='z'){echap=false;geste='';return'';}
  return'{"k":"'+geste+'","e":'+(echap?'true':'false')+',"s":"'+etat+'"}';
},writable:false,configurable:false,enumerable:false});
})();"#;

/// La page d'un lien s'ouvre dans la vigie, sauf si `VIGIE_SANS_PAGE` est posee.
///
/// Le 2026-10-29, elle a ete livree desactivee : le juge de la seconde
/// correction (`passage-b/juge-correction-2.md`) avait rendu « pas fini », car
/// une page pouvait encore faire joindre un service local par une redirection,
/// un `<object>`, un `<video>`, un `EventSource` ou un `prefetch`. Le meme jour,
/// la troisieme passe a applique la regle de Stone au reseau, par un mandataire
/// (`mandataire.rs`), et son juge (`juge-correction-3.md`) a rendu « fini » :
/// 0 requete vers le service interdit sur 32 lancements, contre 20 sur 20 pour
/// l'executable d'avant. Elle est donc activee par defaut ; `VIGIE_SANS_PAGE=1`
/// la coupe (un interrupteur de secours, pas un reglage).
pub fn page_activee() -> bool {
    std::env::var_os("VIGIE_SANS_PAGE").is_none()
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
        let cle = cle_secrete();
        let mut contexte = WebContext::new(Some(profil.clone()));
        let (e_nav, e_charge) = (etat.clone(), etat.clone());
        let accepte = hote_accepte(adresse, local_ok);
        // Tout le trafic du moteur passe par le mandataire de la vigie, qui juge
        // chaque connexion apres resolution. Sans lui, la page ne s'ouvre pas.
        let dire: crate::mandataire::Dire = {
            let e = etat.clone();
            Arc::new(move |cible, motif| noter_refus(&e, "mandataire", &format!("{cible} ({motif})")))
        };
        let mandataire = Mandataire::demarrer(accepte.as_ref().and_then(|_| Accord::de(adresse)), cle_secrete(), dire).map_err(|e| format!("le mandataire ne demarre pas : {e}"))?;
        eprintln!("mandataire demarre sur 127.0.0.1:{}", mandataire.port());
        let arguments = arguments_du_moteur(mandataire.port());
        let hote = match fenetre.window_handle().map(|h| h.as_raw()) {
            Ok(RawWindowHandle::Win32(h)) => h.hwnd.get(),
            _ => 0,
        };
        let b = WebViewBuilder::new_with_web_context(&mut contexte)
            .with_incognito(true)
            // La langue de la page est celle que l'agent a lue (WebFetch lit en
            // anglais) : sans elle, un site qui negocie la langue envoie sa
            // version francaise, ou le passage cite n'est plus. Les deux
            // premiers arguments sont ceux que wry passe d'ordinaire.
            // SmartScreen est eteint : il enverrait a Microsoft chaque adresse que
            // Stone ouvre. Le prix est son absence contre les sites pieges ; la page
            // n'a ni compte, ni session, ni droit (profil InPrivate, tout refuse).
            .with_additional_browser_args(&arguments)
            // Pas de raccourcis de navigateur (F5, impression, recul au clavier) :
            // la vigie a ses propres commandes.
            .with_browser_accelerator_keys(false)
            .with_clipboard(false)
            .with_visible(true)
            .with_bounds(rect(zone))
            .with_initialization_script(BLINDAGE)
            .with_initialization_script_for_main_only(script(passage, &cle), true)
            .with_permission_handler(|k| {
                eprintln!("permission refusee : {k:?}");
                PermissionResponse::Deny
            })
            .with_new_window_req_handler(|u, _| {
                eprintln!("fenetre fille refusee : {}", u.chars().take(80).collect::<String>());
                NewWindowResponse::Deny
            })
            .with_download_started_handler(move |u, _| {
                eprintln!("telechargement refuse : {u}");
                false
            })
            .with_navigation_handler(move |u| {
                let ouvrable = navigation_permise(&u, local_ok);
                let mut e = e_nav.lock().unwrap();
                if ouvrable {
                    if !u.eq_ignore_ascii_case("about:blank") {
                        e.courante = u.split('#').next().unwrap_or("").to_string();
                    }
                } else {
                    eprintln!("navigation refusee : {}", u.chars().take(80).collect::<String>());
                    e.refus.push(u.chars().take(80).collect());
                }
                ouvrable
            })
            .with_on_page_load_handler(move |ev, u| {
                if matches!(ev, PageLoadEvent::Finished) && !u.eq_ignore_ascii_case("about:blank") {
                    let mut e = e_charge.lock().unwrap();
                    e.charge = Some(Instant::now());
                    e.script_lance = false;
                }
            })
            ;
        let debut = Instant::now();
        // Rien n'est charge a la construction : les gardiens des cadres, des
        // ressources et de l'historique se posent d'abord, la premiere requete
        // ne leur echappe pas.
        let vue = b.build_as_child(fenetre).map_err(|e| e.to_string())?;
        poser_les_gardiens(&vue, &etat, accepte, mandataire.identifiants()).map_err(|e| format!("les gardiens de la page ne se posent pas : {e}"))?;
        vue.load_url(adresse).map_err(|e| e.to_string())?;
        eprintln!("moteur de page demarre en {} ms", debut.elapsed().as_millis());
        Ok(Navigateur {
            vue,
            _contexte: contexte,
            profil,
            etat,
            cherche: passage.is_some(),
            prochaine: Instant::now() + PAS_SONDE,
            derniere: [zone[0] as i32, zone[1] as i32, zone[2] as i32, zone[3] as i32],
            ouverte: Instant::now(),
            hote,
            prochain_balayage: Instant::now(),
            boite_vue: false,
            cle,
            mandataire,
            coupee: false,
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
        // Echec ferme : une page chargee sans que le mandataire ait vu une seule
        // requete, c'est un moteur qui n'a pas pris l'argument. Elle est quittee.
        let chargee = self.etat.lock().unwrap().charge.is_some();
        if chargee && !self.coupee && self.mandataire.vues() == 0 {
            self.coupee = true;
            let _ = self.vue.load_url("about:blank");
            self.etat.lock().unwrap().sonde = Sonde::Indisponible;
            eprintln!("le moteur n'est pas passe par le mandataire : page quittee");
        }
        let e = self.etat.clone();
        let _ = self.vue.evaluate_script_with_callback(
            &format!("(()=>{{try{{return window['{}']('e')}}catch(x){{return '{{}}'}}}})()", self.cle),
            move |r| {
                // Du JSON dans du JSON ; seuls un nombre, un booleen et l'un de
                // trois mots en sortent, rien qui s'affiche ou s'execute. Ce que
                // la poignee rend ne vient pas d'une valeur que la page ecrit.
                let interne: String = serde_json::from_str(&r).unwrap_or_default();
                let Ok(v) = serde_json::from_str::<serde_json::Value>(&interne) else { return };
                let mut e = e.lock().unwrap();
                if v.get("e").and_then(|x| x.as_bool()) == Some(true) {
                    e.echap = true;
                }
                match v.get("k").and_then(|x| x.as_str()) {
                    Some("b") => e.geste = Some(Geste::Reculer),
                    Some("f") => e.geste = Some(Geste::Avancer),
                    _ => {}
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
                // Le fragment a eu le temps de faire son travail ; le script verifie
                // lui-meme que le texte est dans la page et a l'ecran.
                (Some(t), false) if t.elapsed() >= DELAI_SONDE => {
                    e.script_lance = true;
                    lancer = true;
                }
                (_, true) if self.ouverte.elapsed() >= DELAI_ABANDON => e.sonde = Sonde::NonRetrouve,
                _ => {}
            }
        }
        if lancer {
            let _ = self.vue.evaluate_script(&format!("window['{}']('c')", self.cle));
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
            let _ = self.vue.evaluate_script(&format!("window['{}']('z')", self.cle));
            return true;
        }
        false
    }

    /// Alt + fleche a ete presse dans la page (une seule fois).
    pub fn geste_demande(&self) -> Option<Geste> {
        let mut e = self.etat.lock().unwrap();
        let g = e.geste.take();
        if g.is_some() {
            let _ = self.vue.evaluate_script(&format!("window['{}']('z')", self.cle));
        }
        g
    }

    /// Y a-t-il une page avant celle-ci dans l'historique du moteur ?
    pub fn peut_reculer(&self) -> bool {
        self.etat.lock().unwrap().recul
    }

    /// Y a-t-il une page apres celle-ci : on a recule et le moteur la garde.
    pub fn peut_avancer(&self) -> bool {
        self.etat.lock().unwrap().avance
    }

    pub fn reculer(&self) {
        let _ = unsafe { self.vue.webview().GoBack() };
    }

    pub fn avancer(&self) {
        let _ = unsafe { self.vue.webview().GoForward() };
    }

    /// Une boite que le moteur a posee sur la fenetre de la vigie (selecteur de
    /// fichiers, invite d'un peripherique) la rend inactive : plus de clic, plus
    /// de fermeture. Elle est refermee, et la fenetre rendue, a chaque trame.
    pub fn liberer(&mut self) {
        if self.hote == 0 || Instant::now() < self.prochain_balayage {
            return;
        }
        self.prochain_balayage = Instant::now() + Duration::from_millis(100);
        let n = liberer_la_fenetre(self.hote);
        // Dite une fois par boite : elle met quelques trames a disparaitre.
        if n > 0 && !self.boite_vue {
            eprintln!("boite native du moteur refermee : {n}");
        }
        self.boite_vue = n > 0;
    }

    /// L'adresse reelle de la page affichee, sans son fragment.
    pub fn courante(&self) -> String {
        self.etat.lock().unwrap().courante.clone()
    }
}

/// Les suppressions de profil en cours : la sortie de la vigie les attend un peu.
static EN_COURS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static RESTANTS: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// A la sortie de la vigie : laisse aux suppressions de profil le temps de finir
/// (six secondes au plus). Celles qui ne le sont pas, le moteur tenant encore ses
/// fichiers, sont confiees a un petit processus qui les finit apres la sortie ;
/// ce qui resterait malgre tout est balaye au lancement suivant.
pub fn attendre_les_suppressions() {
    let debut = Instant::now();
    while EN_COURS.load(std::sync::atomic::Ordering::SeqCst) > 0 && debut.elapsed() < Duration::from_secs(6) {
        std::thread::sleep(Duration::from_millis(50));
    }
    let restants: Vec<PathBuf> = RESTANTS.lock().map(|r| r.iter().filter(|p| p.exists()).cloned().collect()).unwrap_or_default();
    for p in restants {
        // Le dossier est celui que `dossier_de_profil` a nomme : ni espace ni guillemet.
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // Une vingtaine de secondes de recul : le moteur a fini de lacher ses fichiers.
            let recul = if cfg!(test) { 3 } else { 20 };
            let ordre = format!("ping -n {recul} 127.0.0.1 >nul & rmdir /S /Q \"{}\"", p.display());
            // raw_arg : cmd lit sa ligne de commande telle quelle, les guillemets de Rust la casseraient.
            let _ = std::process::Command::new("cmd").arg("/C").raw_arg(&ordre).creation_flags(0x0800_0000).spawn();
            eprintln!("profil confie a un processus de nettoyage : {}", p.display());
        }
    }
}

impl Drop for Navigateur {
    fn drop(&mut self) {
        eprintln!("mandataire : {} requetes vues", self.mandataire.vues());
        // Le profil disparait avec la page : le moteur lache ses fichiers un
        // peu apres sa fermeture, d'ou les reprises.
        let profil = self.profil.clone();
        if let Ok(mut r) = RESTANTS.lock() {
            r.push(profil.clone());
        }
        EN_COURS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        std::thread::spawn(move || {
            for _ in 0..60 {
                if std::fs::remove_dir_all(&profil).is_ok() || !profil.exists() {
                    eprintln!("profil supprime : {}", !profil.exists());
                    EN_COURS.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                    return;
                }
                std::thread::sleep(Duration::from_millis(250));
            }
            eprintln!("profil non supprime (balayage au prochain lancement) : {}", profil.display());
            EN_COURS.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
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
        // Un processus fini dont quelqu'un tient encore la poignee (ici le `Child`
        // gardé en vie) n'est pas vivant : son code de sortie le dit.
        #[cfg(windows)]
        {
            let mut fini = std::process::Command::new("cmd").args(["/C", "exit 0"]).spawn().unwrap();
            fini.wait().unwrap();
            assert!(!vivant(fini.id()), "un processus termine, poignee tenue, passe pour vivant");
        }
    }

    /// Objection T4 du juge technique : le gardien ouvrait ces ecritures, meme
    /// quand Stone avait refuse le local.
    #[test]
    fn le_gardien_refuse_le_local_sous_toutes_ses_ecritures_quand_stone_a_refuse() {
        for u in [
            "http://127.0.0.1:8792/", "http://localhost./", "http://2130706433/", "http://0x7f.0.0.1/", "http://0177.0.0.1:80/", "http://[::ffff:127.0.0.1]/",
            "http://[0:0:0:0:0:0:0:1]/", "http://10.0.0.1/", "http://192.168.1.1/", "http://169.254.169.254/latest/meta-data/",
        ] {
            assert!(!navigation_permise(u, false), "ouvert sans accord : {u}");
            assert!(navigation_permise(u, true), "refuse malgre l'accord : {u}");
        }
        for u in ["https://example.org/", "http://example.org/x", "about:blank", "ABOUT:BLANK"] {
            assert!(navigation_permise(u, false), "{u}");
        }
        for u in ["file:///C:/Windows/win.ini", "javascript:alert(1)", "data:text/html,x", "ms-settings:", "mailto:a@b.c", "foo://x", "blob:https://a/b", "ftp://x/"] {
            assert!(!navigation_permise(u, true), "ouvert : {u}");
        }
    }

    #[test]
    fn le_script_ne_recoit_que_le_passage_et_l_echappe() {
        let s = script(Some("il a dit \"non\" </script> \\ fin"), "vcle");
        assert!(s.contains(r#"const T="il a dit \"non\" </script> \\ fin";"#), "{s}");
        // Sans passage, le script ne cherche rien mais garde l'echappement.
        let s = script(None, "vcle");
        assert!(s.contains("const T=null;"));
        assert!(s.contains("Escape"));
        // Aucun pont de messages vers la vigie.
        assert!(!s.contains("postMessage") && !s.contains("ipc") && !s.contains("fetch(") && !s.contains("XMLHttpRequest"));
    }

    /// La page ne doit pas pouvoir fabriquer ce que la vigie affiche : aucun
    /// etat n'est une variable de la page, la poignee porte le nom tire au sort,
    /// et un evenement clavier fabrique par la page ne ferme rien.
    #[test]
    fn l_etat_du_passage_n_est_pas_une_variable_que_la_page_ecrit() {
        let s = script(Some("x"), "vabcdef");
        assert!(s.contains("const K='vabcdef';"), "{s}");
        assert!(!s.contains("__vigie"), "une variable globale predictible subsiste : {s}");
        assert!(s.contains("isTrusted"), "un evenement fabrique par la page ferait fermer la page");
        // Le script prend ses outils avant la page, et ne cherche aucun ecran : le defilement
        // seul n'est plus une preuve.
        assert!(!s.contains("scrollY"), "le defilement de la page prouverait le passage");
        assert_ne!(cle_secrete(), cle_secrete());
        assert!(cle_secrete().len() >= 30 && cle_secrete().starts_with('v'));
    }

    #[test]
    fn le_blindage_refuse_ce_que_le_moteur_laisse_passer() {
        for besoin in [
            "MediaDevices.prototype,'getDisplayMedia',refus",
            "Clipboard.prototype,'writeText',refus",
            "Clipboard.prototype,'write',refus",
            "Document.prototype,'execCommand'",
            "window,'alert'",
            "window,'confirm'",
            "window,'prompt'",
            "'showOpenFilePicker','showSaveFilePicker','showDirectoryPicker'",
            "['HID','requestDevice'],['USB','requestDevice'],['Bluetooth','requestDevice']",
            "['Serial','requestPort'],['CredentialsContainer','get'],['CredentialsContainer','create']",
            "['WebSocket','WebTransport','RTCPeerConnection','webkitRTCPeerConnection']",
            "HTMLInputElement.prototype,'showPicker'",
            "String(t.type).toLowerCase()==='file'",
            "configurable:false",
        ] {
            assert!(BLINDAGE.contains(besoin), "le blindage ne dit rien de {besoin}");
        }
        assert!(!BLINDAGE.contains("fetch(") && !BLINDAGE.contains("postMessage"));
    }

    /// Le moteur tient encore le dossier a la sortie : la suppression est confiee
    /// a un processus qui finit le travail apres, et le dossier disparait.
    #[cfg(windows)]
    #[test]
    fn un_profil_que_le_moteur_tient_encore_est_confie_au_nettoyage() {
        let dossier = std::env::temp_dir().join(format!("vigie-essai-nettoyage-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dossier);
        std::fs::create_dir_all(dossier.join("EBWebView")).unwrap();
        // Un processus dont le dossier courant est le profil : Windows ne le laisse
        // pas supprimer, comme les processus du moteur qui s'eteignent.
        let mut tenant = std::process::Command::new("ping").args(["-n", "30", "127.0.0.1"]).current_dir(&dossier).stdout(std::process::Stdio::null()).spawn().unwrap();
        std::thread::sleep(Duration::from_millis(300));
        assert!(std::fs::remove_dir_all(&dossier).is_err() && dossier.exists(), "le dossier tenu a pu etre supprime : le test ne tient rien");
        RESTANTS.lock().unwrap().push(dossier.clone());
        attendre_les_suppressions();
        assert!(dossier.exists(), "le dossier ne devait pas disparaitre tant qu'il est tenu");
        let _ = tenant.kill();
        let _ = tenant.wait();
        let debut = Instant::now();
        while dossier.exists() && debut.elapsed() < Duration::from_secs(20) {
            std::thread::sleep(Duration::from_millis(200));
        }
        assert!(!dossier.exists(), "le processus de nettoyage n'a pas supprime le dossier");
        RESTANTS.lock().unwrap().clear();
    }


    /// Objection T4 (cadres) du juge de la correction : le gardien de `wry` ne voit
    /// que la navigation de la page ; un `<iframe>` vers une adresse de la machine
    /// ou vers `foo:`, `mailto:`, `ms-settings:` n'etait jamais refuse.
    #[test]
    fn les_cadres_et_les_ressources_sont_juges_comme_la_navigation() {
        let accepte = Some("127.0.0.1:8791");
        for u in ["foo://x", "mailto:a@b.c", "ms-settings:", "file:///C:/Windows/win.ini", "javascript:alert(1)", "data:text/html,x", "blob:https://a/b", "ftp://x/", "view-source:https://a/"] {
            assert!(!cadre_permis(u, accepte), "cadre ouvert : {u}");
            assert!(!cadre_permis(u, None), "cadre ouvert sans accord : {u}");
        }
        for u in ["about:blank", "ABOUT:SRCDOC", "https://example.org/", "http://example.org/x"] {
            assert!(cadre_permis(u, None), "cadre refuse : {u}");
        }
        // Sans accord : le local, sous toutes ses ecritures, et le reseau prive.
        for u in [
            "http://127.0.0.1:8792/cadre-local", "http://localhost./", "http://2130706433/", "http://0x7f.0.0.1/", "http://[::ffff:127.0.0.1]/",
            "http://10.0.0.1/", "http://192.168.1.1/", "http://169.254.169.254/latest/meta-data/", "ws://127.0.0.1:9/", "ws://10.0.0.1/",
        ] {
            assert!(!cadre_permis(u, None), "cadre local ouvert sans accord : {u}");
            assert!(!ressource_permise(u, None), "ressource locale ouverte sans accord : {u}");
        }
        // Avec l'accord de Stone pour 127.0.0.1:8791, ce service-la et nul autre.
        assert!(cadre_permis("http://127.0.0.1:8791/page.css", accepte));
        assert!(cadre_permis("HTTP://127.0.0.1:8791/", accepte));
        for u in ["http://127.0.0.1:8792/secret", "http://127.0.0.1/", "http://localhost:8791/", "http://10.0.0.1:8791/", "ws://127.0.0.1:8792/"] {
            assert!(!cadre_permis(u, accepte), "un autre service de la machine est ouvert : {u}");
            assert!(!ressource_permise(u, accepte), "une autre ressource de la machine est ouverte : {u}");
        }
        // Une ressource peut etre inline (rien ne passe par le reseau).
        for u in ["data:image/png;base64,AAAA", "blob:https://example.org/uuid", "DATA:text/css,a{}"] {
            assert!(ressource_permise(u, None), "ressource inline refusee : {u}");
        }
        // `ws:` n'arrive pas au filtre du moteur (vu), et le script refuse WebSocket ; si un
        // moteur le livrait un jour, il serait refuse plutot que juge comme du web.
        for u in ["file:///C:/Windows/win.ini", "ftp://x/", "foo://x", "ms-settings:", "mailto:a@b.c", "wss://example.org/s"] {
            assert!(!ressource_permise(u, accepte), "ressource ouverte : {u}");
        }
        assert_eq!(hote_accepte("http://127.0.0.1:8791/p", true).as_deref(), Some("127.0.0.1:8791"));
        assert_eq!(hote_accepte("http://127.0.0.1:8791/p", false), None);
        assert_eq!(hote_accepte("https://example.org/", true), None);
    }

    /// Objection N1 du juge de la correction : une boite modale posee par le moteur
    /// rend la fenetre de la vigie inactive, et plus rien ne la ferme. La boite est
    /// refermee, la fenetre rendue ; celles de la vigie elle-meme ne sont pas touchees.
    #[cfg(windows)]
    #[test]
    fn une_boite_posee_sur_la_fenetre_est_refermee_et_la_fenetre_rendue() {
        use std::sync::mpsc;
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled;
        use windows_sys::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, MessageBoxW, MB_OK, WS_POPUP, WS_VISIBLE};

        let (envoi, recu) = mpsc::channel::<isize>();
        // La fenetre et sa boite vivent dans un meme fil, qui pompe ses messages
        // pendant que la boite est ouverte (comme le fait la vigie).
        let fil = std::thread::spawn(move || unsafe {
            let classe: Vec<u16> = "STATIC\0".encode_utf16().collect();
            let titre: Vec<u16> = "essai\0".encode_utf16().collect();
            let hote = CreateWindowExW(0, classe.as_ptr(), titre.as_ptr(), WS_POPUP | WS_VISIBLE, 0, 0, 200, 100, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null());
            envoi.send(hote as isize).unwrap();
            let texte: Vec<u16> = "Select a file this site can read\0".encode_utf16().collect();
            MessageBoxW(hote, texte.as_ptr(), titre.as_ptr(), MB_OK);
            let actif = IsWindowEnabled(hote) != 0;
            DestroyWindow(hote);
            actif
        });
        let hote = recu.recv_timeout(Duration::from_secs(5)).expect("la fenetre n'est pas nee");
        let debut = Instant::now();
        while unsafe { IsWindowEnabled(hote as HWND) } != 0 && debut.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(unsafe { IsWindowEnabled(hote as HWND) }, 0, "la boite ne s'est pas ouverte : l'essai ne tient rien");
        // Les fenetres du processus meme ne sont pas touchees.
        // La fenetre est desactivee un instant avant que la boite ne soit montree.
        std::thread::sleep(Duration::from_millis(400));
        assert_eq!(liberer_la_fenetre_hors(hote, std::process::id()), 0);
        std::thread::sleep(Duration::from_millis(300));
        assert!(!fil.is_finished(), "une boite de la vigie a ete fermee");
        // Celles d'un autre processus le sont (ici, 0 : tout processus compte).
        assert_eq!(liberer_la_fenetre_hors(hote, 0), 1);
        let debut = Instant::now();
        while !fil.is_finished() && debut.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(fil.is_finished(), "la boite est restee ouverte");
        assert!(fil.join().unwrap(), "la fenetre est restee inactive apres la boite");
    }

    /// Une fenetre laissee inactive sans qu'aucune boite ne la tienne (la boite a
    /// disparu avec son processus) est rendue.
    #[cfg(windows)]
    #[test]
    fn une_fenetre_inactive_sans_boite_est_rendue() {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, IsWindowEnabled};
        use windows_sys::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, WS_POPUP, WS_VISIBLE};
        unsafe {
            let classe: Vec<u16> = "STATIC\0".encode_utf16().collect();
            let hote = CreateWindowExW(0, classe.as_ptr(), classe.as_ptr(), WS_POPUP | WS_VISIBLE, 0, 0, 100, 50, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null());
            EnableWindow(hote, 0);
            assert_eq!(IsWindowEnabled(hote), 0, "l'essai ne tient rien");
            assert_eq!(liberer_la_fenetre_hors(hote as isize, 0), 0);
            assert_ne!(IsWindowEnabled(hote), 0);
            DestroyWindow(hote);
        }
    }

    /// Alt + fleche est lu dans la page par la sonde, sur un evenement de confiance
    /// seulement, et rend la meme forme que l'echappement.
    #[test]
    fn alt_fleche_est_lu_dans_la_page_sur_un_evenement_de_confiance() {
        let s = script(Some("x"), "vabcdef");
        for besoin in ["'ArrowLeft'", "'ArrowRight'", "altKey", "ctrlKey", "shiftKey", "metaKey", "geste='b'", "geste='f'", "\"k\":\""] {
            assert!(s.contains(besoin), "le script ne dit rien de {besoin}");
        }
        assert!(s.contains("if(fiable(e)!==true)return;"), "un Alt+fleche fabrique par la page ferait reculer");
        assert!(!s.contains("history.length") && !s.contains("histLong"), "l'historique se lit au moteur, pas a la page");
        // `isTrusted` est propre a l'instance de l'evenement : le chercher sur le prototype ne
        // trouve rien et la sonde ne verrait plus jamais ni Echap ni Alt + fleche (vu en vrai).
        assert!(s.contains("D(e,'isTrusted')"), "isTrusted doit se lire sur l'evenement");
        assert!(!s.contains("Event.prototype,'isTrusted'"), "isTrusted n'est pas un attribut du prototype");
    }

    /// Objection N2 du juge de la correction : une citation a `opacity:0` etait
    /// « retrouvee ». La sonde regarde ce que le moteur calcule, pas la boite.
    #[test]
    fn la_sonde_regarde_le_style_calcule_du_texte() {
        let s = script(Some("x"), "vabcdef");
        for besoin in ["getComputedStyle", "'opacity'", "'visibility'", "'display'", "'color'", "function lisible(r)", "large(rc)>0&&grand(rc)>0&&lisible(r)"] {
            assert!(s.contains(besoin), "le script ne dit rien de {besoin}");
        }
    }

    #[test]
    fn le_moteur_est_lance_derriere_le_mandataire_sans_exception_ni_voie_directe() {
        // Un essai de chaines, et dit tel : ce qu'il garde, c'est que ces quatre
        // arguments ne disparaissent pas. Qu'ils FONT ce qu'ils disent, seul le
        // rejeu en vrai le prouve (`passage-b/troisieme/`).
        let a = arguments_du_moteur(4242);
        assert!(a.contains("--proxy-server=http://127.0.0.1:4242"), "{a}");
        assert!(a.contains("--proxy-bypass-list=<-loopback>"), "{a}");
        assert!(a.contains("--disable-quic"), "{a}");
        assert!(a.contains("--webrtc-ip-handling-policy=disable_non_proxied_udp"), "{a}");
        // Le moteur ne recoit pas deux fois la meme option (la derniere gagnerait).
        assert_eq!(a.matches("--proxy-server").count(), 1);
        assert_eq!(a.matches("--proxy-bypass-list").count(), 1);
    }
}

#[cfg(test)]
mod essais_de_la_page_desactivee {
    use super::*;

    /// La page est activee par defaut : seul `VIGIE_SANS_PAGE` la coupe.
    #[test]
    fn la_page_est_activee_par_defaut() {
        if std::env::var_os("VIGIE_SANS_PAGE").is_none() {
            assert!(page_activee());
        } else {
            assert!(!page_activee());
        }
    }
}
