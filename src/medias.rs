//! Les medias que les agents produisent : ce qu'ils ecrivent sur le disque, et
//! pas seulement ce qu'ils ouvrent.
//!
//! Un agent de design rend une maquette en PNG, assemble une planche, tourne
//! une animation en MP4. La transcription n'en garde que la commande et ce
//! qu'elle a imprime ; l'image, elle, est sur le disque. La vigie la retrouve
//! par son nom : tout nom de fichier image ou video qui paraît dans un geste ou
//! dans son resultat, ramene au dossier ou la commande tournait.
//!
//! Un nom ne suffit pas a dire qu'un agent a PRODUIT le fichier : il peut lister
//! le travail d'un autre, ou une capture vieille d'un mois. La regle, et sa
//! raison, est dans `produit_par`.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::catalogue::normaliser;

pub const IMAGES: [&str; 6] = ["png", "jpg", "jpeg", "webp", "gif", "bmp"];
pub const VIDEOS: [&str; 4] = ["mp4", "webm", "mov", "mkv"];
/// Un son : la voix d'un atelier, une narration. Il s'ouvre dans la
/// visionneuse comme une video, sur son onde.
pub const AUDIOS: [&str; 7] = ["mp3", "wav", "m4a", "aac", "flac", "ogg", "opus"];
/// La taille de l'onde d'un son. C'est celle de la visionneuse ; la vignette
/// n'en est que la reduction, comme pour une image.
pub const ONDE: (u32, u32) = (1600, 400);

/// L'ecart toléré entre l'horloge des transcriptions et celle du disque, et
/// le temps qu'une ecriture met a se terminer apres que l'outil a rendu la
/// main.
const MARGE: Duration = Duration::from_secs(3);

pub fn extension(nom: &str) -> Option<String> {
    let (_, e) = nom.rsplit_once('.')?;
    let e = e.to_ascii_lowercase();
    (IMAGES.contains(&e.as_str()) || VIDEOS.contains(&e.as_str()) || AUDIOS.contains(&e.as_str())).then_some(e)
}

pub fn est_audio(chemin: &Path) -> bool {
    chemin
        .extension()
        .map(|e| AUDIOS.contains(&e.to_string_lossy().to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

pub fn est_video(chemin: &Path) -> bool {
    chemin
        .extension()
        .map(|e| VIDEOS.contains(&e.to_string_lossy().to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Un blanc peut etre dans un nom (« capture 1.png »), et rien dans le texte
/// ne dit ou ce nom commence : on remonte d'au plus tant de mots.
const MOTS_MAX: usize = 4;

/// Un mot sans la ponctuation qui le suit ni les barres doublees d'un echappement.
fn nettoyer(mot: &str) -> String {
    // « fig.png: », « fig.png. » : la ponctuation de fin n'est pas du nom.
    let t = mot.trim_end_matches(['.', ':', '!', '?']);
    // Le resultat d'un outil echappe parfois ses barres : « C:\\Users ».
    t.replace("\\\\", "\\")
}

fn recevable(t: &str) -> bool {
    t.len() >= 5 && !t.contains('*') && !t.starts_with("http") && !t.contains("://")
}

/// Les noms de medias d'un texte, dans l'ordre, sans doublon. Chacun vient avec
/// ses variantes, de la plus longue a la plus courte : « capture 1.png » dans
/// un `ls` peut etre ce fichier, ou « 1.png ». Seul le disque tranche, et
/// celui qui lit prend la premiere variante qui y existe.
///
/// Un nom ne traverse jamais un guillemet ni la ponctuation du shell : entre
/// guillemets, « "a b/fig 2.png" » est un nom entier, dont « fig 2.png » et
/// « 2.png » sont les variantes plus courtes.
pub fn noms(texte: &str) -> Vec<Vec<String>> {
    let mut sortie: Vec<Vec<String>> = Vec::new();
    for segment in texte.split(|c: char| {
        matches!(c, '\n' | '\r' | '"' | '\'' | '`' | '=' | '(' | ')' | ',' | ';' | '|' | '<' | '>' | '[' | ']' | '{' | '}')
    }) {
        let mots: Vec<&str> = segment.split_whitespace().collect();
        for (i, mot) in mots.iter().enumerate() {
            let fin = nettoyer(mot);
            if !recevable(&fin) || extension(&fin).is_none() {
                continue;
            }
            let premier = i.saturating_sub(MOTS_MAX - 1);
            let variantes: Vec<String> = (premier..=i)
                .map(|d| {
                    let mut v = mots[d..i].iter().map(|m| m.replace("\\\\", "\\")).collect::<Vec<_>>();
                    v.push(fin.clone());
                    v.join(" ")
                })
                .filter(|v| recevable(v))
                .collect();
            if !sortie.contains(&variantes) {
                sortie.push(variantes);
            }
        }
    }
    sortie
}

/// Le dossier ou tourne une commande : le premier « cd » qui l'ouvre, ou le
/// dossier de l'agent. « cd x && manim … » est la forme de presque toutes.
pub fn dossier_de(commande: &str, cwd: &str) -> String {
    let base = normaliser(cwd);
    let t = commande.trim_start();
    let reste = t
        .strip_prefix("cd ")
        .or_else(|| t.strip_prefix("Set-Location "))
        .or_else(|| t.strip_prefix("pushd "));
    let Some(reste) = reste else { return base };
    let reste = reste.trim_start();
    let d = if let Some(r) = reste.strip_prefix('"') {
        r.split('"').next().unwrap_or("")
    } else if let Some(r) = reste.strip_prefix('\'') {
        r.split('\'').next().unwrap_or("")
    } else {
        reste.split(|c: char| c.is_whitespace() || c == ';' || c == '&').next().unwrap_or("")
    };
    if d.is_empty() {
        return base;
    }
    resoudre(d, &base).to_string_lossy().to_string()
}

/// Un nom, ramene a un chemin absolu depuis `base`.
pub fn resoudre(nom: &str, base: &str) -> PathBuf {
    if !cfg!(windows) {
        return resoudre_unix(nom, base);
    }
    let nom = if let Some(r) = nom.strip_prefix("~/").or_else(|| nom.strip_prefix("~\\")) {
        let maison = std::env::var("USERPROFILE").unwrap_or_default();
        format!("{maison}\\{r}")
    } else {
        nom.to_string()
    };
    let n = normaliser(&nom);
    let b = n.as_bytes();
    let absolu = (b.len() >= 3 && b[1] == b':' && b[2] == b'\\') || n.starts_with("\\\\");
    if absolu {
        PathBuf::from(n)
    } else {
        Path::new(&normaliser(base)).join(n.trim_start_matches(".\\"))
    }
}

/// `resoudre` hors de Windows : un chemin absolu commence par « / », « ~/ » est
/// le dossier personnel, le reste part de `base`.
fn resoudre_unix(nom: &str, base: &str) -> PathBuf {
    if let Some(r) = nom.strip_prefix("~/") {
        let maison = std::env::var("HOME").unwrap_or_default();
        return Path::new(&maison).join(r);
    }
    if nom.starts_with('/') {
        return PathBuf::from(nom);
    }
    Path::new(&normaliser(base)).join(nom.trim_start_matches("./"))
}

/// Le fichier a-t-il ete produit par ce geste ? Deux cas, et seulement deux.
///
/// 1. Il a ete ecrit PENDANT le geste, entre son appel et son resultat : c'est
///    le geste qui l'a fait, qu'il le nomme ou que sa sortie le nomme.
/// 2. Le geste le nomme LUI-MEME, et il a ete ecrit depuis que l'agent a
///    commence : une video que manim range dans un sous-dossier n'est souvent
///    nommee qu'a la commande suivante, qui la sonde.
///
/// Ce qui est exclu : un nom qui ne parait que dans la SORTIE d'un geste et
/// qui a ete ecrit a un autre moment. C'est un « ls » qui liste le travail
/// d'un autre agent, ou une capture d'un ancien chantier.
pub fn produit_par(
    ecrit: SystemTime,
    appel: SystemTime,
    resultat: SystemTime,
    debut_agent: SystemTime,
    nomme_par_le_geste: bool,
) -> bool {
    let pendant = ecrit + MARGE >= appel && ecrit <= resultat + MARGE;
    let depuis_le_debut = ecrit + MARGE >= debut_agent;
    pendant || (nomme_par_le_geste && depuis_le_debut)
}

/// Une image du disque, decodee et reduite aux bornes. Une video rend son image
/// du milieu, frappee du signe de lecture, et sa duree.
pub fn charger(chemin: &Path, bornes: (u32, u32)) -> Option<(image::RgbaImage, Option<f32>)> {
    if est_audio(chemin) {
        let (img, duree) = onde(chemin)?;
        return Some((reduire(img, bornes), Some(duree)));
    }
    if est_video(chemin) {
        let (img, duree) = image_de_video(chemin)?;
        let mut img = reduire(img, bornes);
        frapper_lecture(&mut img);
        return Some((img, Some(duree)));
    }
    let img = image::open(chemin).ok()?.to_rgba8();
    Some((reduire(img, bornes), None))
}

pub fn reduire(img: image::RgbaImage, (lmax, hmax): (u32, u32)) -> image::RgbaImage {
    let (l, h) = (img.width().max(1), img.height().max(1));
    let r = (lmax as f64 / l as f64).min(hmax as f64 / h as f64).min(1.0);
    if r >= 1.0 {
        return img;
    }
    let nl = ((l as f64 * r).round() as u32).max(1);
    let nh = ((h as f64 * r).round() as u32).max(1);
    image::imageops::resize(&img, nl, nh, image::imageops::FilterType::Triangle)
}

/// ffmpeg et ffprobe, cherches dans le PATH. Sans eux, une video n'a pas
/// d'image et reste une ligne : on perd l'apercu, pas le fil.
fn outil(nom: &str) -> std::process::Command {
    let mut c = std::process::Command::new(nom);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Sans ce drapeau, chaque appel ferait clignoter une console noire
        // par-dessus la vigie.
        const SANS_FENETRE: u32 = 0x0800_0000;
        c.creation_flags(SANS_FENETRE);
    }
    c.stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    c
}

fn image_de_video(chemin: &Path) -> Option<(image::RgbaImage, f32)> {
    let sonde = outil("ffprobe")
        .args(["-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0"])
        .arg(chemin)
        .output()
        .ok()?;
    let duree: f32 = String::from_utf8_lossy(&sonde.stdout).trim().parse().unwrap_or(0.0);
    // Le milieu, et non la premiere image : une animation commence presque
    // toujours sur un fond vide.
    let instant = format!("{:.3}", duree * 0.5);
    let sortie = outil("ffmpeg")
        .args(["-v", "error", "-ss", &instant, "-i"])
        .arg(chemin)
        .args(["-frames:v", "1", "-f", "image2pipe", "-vcodec", "png", "-"])
        .output()
        .ok()?;
    let img = image::load_from_memory(&sortie.stdout).ok()?.to_rgba8();
    Some((img, duree))
}

/// Le son d'un fichier, ramene a un seul canal de 4 000 echantillons par
/// seconde : assez pour dessiner une onde, et une heure de voix tient dans
/// trente megaoctets.
const ONDE_HZ: u32 = 4000;

/// L'onde d'un son, peinte, et sa duree. ffmpeg decode ; la vigie dessine.
/// Sans ffmpeg, ou pour un fichier qu'il ne lit pas, rien : le son reste une
/// ligne du fil, sans onde.
pub fn onde(chemin: &Path) -> Option<(image::RgbaImage, f32)> {
    let sortie = outil("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(chemin)
        .args(["-vn", "-ac", "1", "-ar", &ONDE_HZ.to_string(), "-f", "s16le", "-"])
        .output()
        .ok()?;
    let echantillons: Vec<i16> = sortie.stdout.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
    if echantillons.is_empty() {
        return None;
    }
    let sonde = outil("ffprobe")
        .args(["-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0"])
        .arg(chemin)
        .output()
        .ok();
    let duree = sonde
        .and_then(|s| String::from_utf8_lossy(&s.stdout).trim().parse::<f32>().ok())
        .filter(|d| *d > 0.0)
        .unwrap_or(echantillons.len() as f32 / ONDE_HZ as f32);
    let (l, h) = ONDE;
    let barres = (l / PAS_BARRE) as usize;
    Some((dessiner_onde(&cretes(&echantillons, barres), l, h), duree))
}

/// Une barre de l'onde : trois pixels de large, deux de blanc, a 1 600 px.
const PAS_BARRE: u32 = 5;
const LARGEUR_BARRE: u32 = 3;

/// Pour chacune de `n` colonnes, la crete du son de ce morceau, entre 0 et 1.
/// La plus haute vaut 1, sauf dans un son presque muet : un souffle ne doit pas
/// se lire comme un cri (le plancher est un dixieme de la pleine echelle).
pub fn cretes(echantillons: &[i16], n: usize) -> Vec<f32> {
    if n == 0 || echantillons.is_empty() {
        return vec![0.0; n];
    }
    let mut v: Vec<f32> = (0..n)
        .map(|i| {
            let (a, b) = (i * echantillons.len() / n, ((i + 1) * echantillons.len() / n).max(i * echantillons.len() / n + 1));
            let morceau = &echantillons[a.min(echantillons.len() - 1)..b.min(echantillons.len())];
            morceau.iter().map(|s| (*s as i32).unsigned_abs() as f32 / 32768.0).fold(0.0, f32::max)
        })
        .collect();
    let max = v.iter().cloned().fold(0.0, f32::max).max(0.1);
    for x in v.iter_mut() {
        *x /= max;
    }
    v
}

/// L'onde, sur une plaque sombre : des barres claires, centrees, bornees par
/// l'anti-crenelage du haut et du bas, et un filet au milieu pour que le
/// silence se voie comme une ligne et non comme un trou.
pub fn dessiner_onde(cretes: &[f32], l: u32, h: u32) -> image::RgbaImage {
    let fond = [0x16u8, 0x17, 0x1a];
    let clair = [0xF2u8, 0xEF, 0xE9];
    let mut img = image::RgbaImage::from_pixel(l, h, image::Rgba([fond[0], fond[1], fond[2], 255]));
    let milieu = h as f32 * 0.5;
    let melanger = |img: &mut image::RgbaImage, x: u32, y: u32, a: f32| {
        let p = img.get_pixel_mut(x, y);
        for k in 0..3 {
            p[k] = (p[k] as f32 + (clair[k] as f32 - p[k] as f32) * a.clamp(0.0, 1.0)).round() as u8;
        }
    };
    // Le filet du milieu.
    for x in 0..l {
        for dy in 0..2u32 {
            let y = (milieu as u32).saturating_sub(1) + dy;
            if y < h {
                melanger(&mut img, x, y, 0.22);
            }
        }
    }
    for (i, c) in cretes.iter().enumerate() {
        let x0 = i as u32 * PAS_BARRE + (PAS_BARRE - LARGEUR_BARRE) / 2;
        let demi = (c.clamp(0.0, 1.0) * h as f32 * 0.44).max(2.0);
        let (haut, bas) = (milieu - demi, milieu + demi);
        for x in x0..(x0 + LARGEUR_BARRE).min(l) {
            // Les bords de la barre sont arrondis d'un pixel : la couverture
            // d'un bout tombe sur ses deux premieres et dernieres colonnes.
            let bord = if x == x0 || x + 1 == x0 + LARGEUR_BARRE { 0.55 } else { 1.0 };
            let (y0, y1) = (haut.floor() as u32, (bas.ceil() as u32).min(h));
            for y in y0..y1 {
                let couvert = ((y + 1) as f32).min(bas) - (y as f32).max(haut);
                melanger(&mut img, x, y, couvert.clamp(0.0, 1.0) * 0.9 * bord);
            }
        }
    }
    img
}

/// La taille d'une video, que son image reduite ne dit plus.
pub fn taille_video(chemin: &Path) -> Option<(u32, u32)> {
    let s = outil("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height", "-of", "csv=p=0"])
        .arg(chemin)
        .output()
        .ok()?;
    let t = String::from_utf8_lossy(&s.stdout);
    let (l, h) = t.trim().split_once(',')?;
    Some((l.trim().parse().ok()?, h.trim().parse().ok()?))
}

/// Le signe de lecture, peint dans les pixels : un disque sombre cerne d'un
/// anneau clair, et un triangle blanc, au centre, lisses a la main. L'anneau
/// est ce qui le fait voir sur une animation a fond noir, ou un disque sombre
/// seul disparait (vu le 2026-10-27 sur les rendus manim du cours). Le peindre ici plutot qu'a
/// l'ecran evite une passe de dessin de plus pour une seule sorte d'image.
fn frapper_lecture(img: &mut image::RgbaImage) {
    let (l, h) = (img.width() as f32, img.height() as f32);
    let r = (l.min(h) * 0.11).clamp(14.0, 44.0);
    let (cx, cy) = (l * 0.5, h * 0.5);
    // Le triangle, oriente vers la droite, recentre sur son centre de gravite.
    let s = r * 0.42;
    let (ax, ay) = (cx - s * 0.75, cy - s);
    let (bx, by) = (cx - s * 0.75, cy + s);
    let (qx, qy) = (cx + s * 1.25, cy);
    let x0 = (cx - r - 1.0).max(0.0) as u32;
    let x1 = ((cx + r + 1.0) as u32).min(img.width());
    let y0 = (cy - r - 1.0).max(0.0) as u32;
    let y1 = ((cy + r + 1.0) as u32).min(img.height());
    let cote = |px: f32, py: f32, x1: f32, y1: f32, x2: f32, y2: f32| {
        // Distance signee a l'arete, normalisee : positive a l'interieur.
        let (dx, dy) = (x2 - x1, y2 - y1);
        let n = (dx * dx + dy * dy).sqrt().max(1e-3);
        ((px - x1) * dy - (py - y1) * dx) / n
    };
    for y in y0..y1 {
        for x in x0..x1 {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let d = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
            let disque = (r - d + 0.5).clamp(0.0, 1.0);
            if disque <= 0.0 {
                continue;
            }
            let t = cote(px, py, ax, ay, bx, by)
                .min(cote(px, py, bx, by, qx, qy))
                .min(cote(px, py, qx, qy, ax, ay));
            let tri = (t + 0.5).clamp(0.0, 1.0);
            // L'anneau : un pixel et demi a l'interieur du bord du disque.
            let anneau = (1.0 - ((r - 1.25) - d).abs() / 0.9).clamp(0.0, 1.0) * 0.55;
            let blanc = (tri + anneau).min(1.0) * disque;
            let p = img.get_pixel_mut(x, y);
            for k in 0..3 {
                let fond = p[k] as f32 * (1.0 - 0.55 * disque);
                p[k] = (fond + (255.0 - fond) * blanc).round() as u8;
            }
        }
    }
}

#[cfg(test)]
mod essais {
    use super::*;

    /// La variante la plus courte de chaque nom, sans doublon : le nom sans
    /// blanc, celui que lisait la premiere version.
    fn courts(texte: &str) -> Vec<String> {
        let mut v: Vec<String> = Vec::new();
        for g in noms(texte) {
            let c = g.last().unwrap().clone();
            if !v.contains(&c) {
                v.push(c);
            }
        }
        v
    }

    /// Les formes relevees le 2026-10-27 dans le chantier de design du cours :
    /// une commande ffmpeg, la sortie d'un « ls -la », celle d'un « find ».
    #[test]
    fn les_noms_se_lisent_dans_les_commandes_et_les_sorties() {
        let commande = "cd /c/x/essais && ffmpeg -y -loglevel error -i clair_1000.png -i sombre_1000.png \
                        -filter_complex hstack=inputs=2 planche_clair_sombre.png && ls -la planche_clair_sombre.png";
        assert_eq!(courts(commande), vec!["clair_1000.png", "sombre_1000.png", "planche_clair_sombre.png"]);
        let sortie = "-rw-r--r-- 1 alex 197609 16982 Oct 27 09:52 clair_1000.png\nmedia/videos/secante/480p15/Secante.mp4";
        assert_eq!(courts(sortie), vec!["clair_1000.png", "media/videos/secante/480p15/Secante.mp4"]);
        // V=chemin, un motif, une adresse, un nom suivi d'un point final.
        assert_eq!(courts("V=media/a.mp4; find . -name '*.mp4'"), vec!["media/a.mp4"]);
        assert!(noms("https://exemple.org/logo.png").is_empty());
        assert_eq!(courts("Voir fig.PNG."), vec!["fig.PNG"]);
        assert!(noms("maquette.html secante.py").is_empty());
    }

    /// Un nom a blancs : entre guillemets, il est entier et ne deborde pas du
    /// guillemet ; dans une sortie sans guillemets, ses variantes remontent de
    /// quatre mots au plus, et la plus longue vient d'abord.
    #[test]
    fn un_nom_avec_des_blancs() {
        let g = noms("ffmpeg -i x.mp4 \"C:/Users/a b/capture 1.png\"");
        assert_eq!(g[1], vec!["C:/Users/a b/capture 1.png", "b/capture 1.png", "1.png"]);
        let g = noms("Oct 27 09:52 capture finale 2.png");
        assert_eq!(g[0], vec!["09:52 capture finale 2.png", "capture finale 2.png", "finale 2.png", "2.png"]);
    }

    /// La variante qui existe sur le disque l'emporte : « capture 1.png », et
    /// non « 1.png », qui existe aussi a cote.
    #[test]
    fn le_disque_tranche_entre_les_variantes() {
        let d = std::env::temp_dir().join(format!("vigie-blancs-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("capture 1.png"), b"x").unwrap();
        std::fs::write(d.join("1.png"), b"x").unwrap();
        let base = d.to_string_lossy().to_string();
        let g = &noms("-rw-r--r-- 1 alex 16982 Oct 27 09:52 capture 1.png")[0];
        let trouve = g.iter().map(|n| resoudre(n, &base)).find(|c| c.is_file());
        assert_eq!(trouve, Some(d.join("capture 1.png")));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[cfg(not(windows))]
    #[test]
    fn le_dossier_d_une_commande_et_un_nom_se_resolvent_sous_unix() {
        assert_eq!(dossier_de("cd /home/a/essais && ls", "/x"), "/home/a/essais");
        assert_eq!(dossier_de("cd \"/home/a b/e\" && ls", "/x"), "/home/a b/e");
        assert_eq!(dossier_de("cd media; ls", "/x"), "/x/media");
        assert_eq!(dossier_de("ls -la", "/home/a/"), "/home/a");
        assert_eq!(resoudre("planche.png", "/x/e"), PathBuf::from("/x/e/planche.png"));
        assert_eq!(resoudre("./m/a.mp4", "/x"), PathBuf::from("/x/m/a.mp4"));
        assert_eq!(resoudre("/y/b.png", "/x"), PathBuf::from("/y/b.png"));
    }

    #[cfg(windows)]
    #[test]
    fn le_dossier_d_une_commande() {
        assert_eq!(dossier_de("cd /c/Users/a/essais && ls", "C:\\x"), "C:\\Users\\a\\essais");
        assert_eq!(dossier_de("cd \"C:/Users/a b/e\" && ls", "C:\\x"), "C:\\Users\\a b\\e");
        assert_eq!(dossier_de("cd media; ls", "C:\\x"), "C:\\x\\media");
        assert_eq!(dossier_de("ls -la", "/c/Users/a"), "C:\\Users\\a");
    }

    #[cfg(windows)]
    #[test]
    fn un_nom_se_resout() {
        assert_eq!(resoudre("planche.png", "C:\\x\\e"), PathBuf::from("C:\\x\\e\\planche.png"));
        assert_eq!(resoudre("./m/a.mp4", "C:\\x"), PathBuf::from("C:\\x\\m\\a.mp4"));
        assert_eq!(resoudre("/c/y/b.png", "C:\\x"), PathBuf::from("C:\\y\\b.png"));
        assert_eq!(resoudre("C:/y/b.png", "C:\\x"), PathBuf::from("C:\\y\\b.png"));
    }

    /// Les deux cas de la regle, et ce qu'elle exclut.
    #[test]
    fn ce_qu_un_geste_a_produit() {
        let t = |s: u64| SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000 + s);
        let debut = t(0);
        // Ecrit pendant le geste (appel a 100, resultat a 110) : produit, qu'il
        // soit nomme par le geste ou seulement par sa sortie.
        assert!(produit_par(t(105), t(100), t(110), debut, false));
        // Nomme par le geste, ecrit plus tot par le meme agent : produit.
        assert!(produit_par(t(50), t(100), t(110), debut, true));
        // Seulement dans la sortie, ecrit a un autre moment : un « ls ».
        assert!(!produit_par(t(50), t(100), t(110), debut, false));
        // Nomme par le geste, mais plus vieux que l'agent : pas de lui.
        assert!(!produit_par(t(0) - Duration::from_secs(3600), t(100), t(110), debut, true));
    }

    /// Le signe de lecture assombrit le centre sans toucher les coins.
    #[test]
    fn le_signe_de_lecture_se_pose_au_centre() {
        let mut img = image::RgbaImage::from_pixel(200, 100, image::Rgba([200, 200, 200, 255]));
        frapper_lecture(&mut img);
        assert_eq!(img.get_pixel(2, 2).0, [200, 200, 200, 255]);
        // Le centre du triangle est blanc ; le bord gauche du disque, assombri.
        assert_eq!(img.get_pixel(100, 50).0[0], 255);
        assert!(img.get_pixel(88, 50).0[0] < 120);
        // L'anneau se voit sur du noir : r = 14, donc vers x = 100 - 12,75.
        let mut noir = image::RgbaImage::from_pixel(200, 100, image::Rgba([0, 0, 0, 255]));
        frapper_lecture(&mut noir);
        assert!(noir.get_pixel(87, 50).0[0] > 90);
        assert_eq!(noir.get_pixel(92, 50).0[0], 0);
    }

    #[test]
    fn un_son_se_reconnait_a_son_extension() {
        assert!(est_audio(Path::new("a/B.WAV")) && est_audio(Path::new("x.m4a")));
        assert!(!est_audio(Path::new("a.png")) && !est_audio(Path::new("a.mp4")));
        assert_eq!(extension("narration.mp3").as_deref(), Some("mp3"));
        assert_eq!(courts("python tts.py --sortie phrase-1/A.wav"), vec!["phrase-1/A.wav"]);
    }

    /// La crete la plus haute vaut 1 ; un son faible n'est pas gonfle a 1 : son
    /// plafond est le dixieme de la pleine echelle ; le silence reste a 0.
    #[test]
    fn les_cretes_se_normalisent_sans_gonfler_un_souffle() {
        let fort: Vec<i16> = (0..4000).map(|i| if i < 2000 { 30000 } else { 15000 }).collect();
        let c = cretes(&fort, 2);
        assert!((c[0] - 1.0).abs() < 1e-6 && (c[1] - 0.5).abs() < 0.01);
        let souffle: Vec<i16> = vec![100; 4000];
        assert!(cretes(&souffle, 4).iter().all(|x| *x < 0.05));
        assert!(cretes(&vec![0; 100], 5).iter().all(|x| *x == 0.0));
        assert_eq!(cretes(&[], 3), vec![0.0; 3]);
        assert!(cretes(&[1, 2, 3], 0).is_empty());
    }

    /// Une barre haute monte plus loin du milieu qu'une basse ; le silence est
    /// un filet, pas un trou ; les coins restent a la couleur de la plaque.
    #[test]
    fn l_onde_dessine_ses_barres_et_son_filet() {
        let img = dessiner_onde(&[1.0, 0.2, 0.0], 15, 100);
        let fond = [0x16u8, 0x17, 0x1a, 255];
        assert_eq!(img.get_pixel(0, 0).0, fond);
        assert_eq!(img.get_pixel(14, 99).0, fond);
        // Barre 0 (x = 1..4) : haute, claire tres haut ; barre 1 (x = 6..9) : courte.
        assert!(img.get_pixel(2, 10).0[0] > 150);
        assert_eq!(img.get_pixel(7, 10).0, fond);
        assert!(img.get_pixel(7, 45).0[0] > 150);
        // Le silence (barre 2, x = 11..14) garde une hauteur minimale.
        assert!(img.get_pixel(12, 49).0[0] > 0x17 + 20);
        assert_eq!(img.get_pixel(12, 30).0, fond);
        // Le filet traverse aussi les colonnes entre deux barres (x = 4 et 5) :
        // sans lui, le silence se lirait comme un trou.
        assert!(img.get_pixel(4, 49).0[0] > 0x16 + 20, "filet absent");
        assert_eq!(img.get_pixel(4, 30).0, fond);
    }
}
