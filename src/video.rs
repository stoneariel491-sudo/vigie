//! La lecture d'une video dans la vigie elle-meme : ses images dans une
//! texture de la fenetre, son son par la sortie audio de Windows. Aucun lecteur
//! ne s'ouvre a cote.
//!
//! ffmpeg decode, et rien d'autre : il rend des pixels RGBA bruts sur sa sortie
//! standard, a la taille d'affichage, et des echantillons PCM sur une autre.
//! La vigie les cadence, les affiche et les joue. Ecrire un decodeur H.264
//! n'aurait aucun sens ; en dependre d'une application a cote, c'est ce que
//! l'auteur ne veut pas.
//!
//! L'horloge est celle du mur : la position est le temps ecoule depuis la
//! lecture, et l'image affichee est celle de cette position. Une image en
//! retard est sautee plutot que montree tard ; un decodeur trop lent fait
//! donc sauter des images, jamais ralentir le temps.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::mpsc::{sync_channel, Receiver, TryRecvError};
use std::time::Instant;

/// La plus grande largeur decodee. Au-dela, la fenetre n'en montre pas plus,
/// et chaque image couterait plus a lire et a envoyer a la carte.
const LARGEUR_MAX: u32 = 1600;
/// Les images decodees d'avance. Le decodeur s'arrete quand elles sont
/// pleines : c'est ce qui le met en pause avec la lecture, sans signal.
const AVANCE: usize = 4;

fn outil(nom: &str) -> std::process::Command {
    let mut c = std::process::Command::new(nom);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Sans ce drapeau, chaque appel ferait clignoter une console noire.
        c.creation_flags(0x0800_0000);
    }
    c.stdin(Stdio::null()).stderr(Stdio::null());
    c
}

/// Ce que ffprobe dit d'une video.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Forme {
    pub largeur: u32,
    pub hauteur: u32,
    pub duree: f32,
    pub cadence: f32,
    pub son: bool,
}

/// « 30000/1001 » ou « 15/1 » : la cadence en images par seconde.
pub fn cadence(fraction: &str) -> Option<f32> {
    let (n, d) = fraction.trim().split_once('/').unwrap_or((fraction.trim(), "1"));
    let (n, d): (f32, f32) = (n.parse().ok()?, d.parse().ok()?);
    (n > 0.0 && d > 0.0).then_some(n / d)
}

/// Un son n'a pas d'image : sa forme a 0 pixel, et la lecture ne lance aucun
/// decodeur d'images. L'horloge et le son font tout.
pub fn sonder_son(chemin: &Path) -> Option<Forme> {
    let s = outil("ffprobe")
        .args(["-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0"])
        .arg(chemin)
        .output()
        .ok()?;
    let duree: f32 = String::from_utf8_lossy(&s.stdout).trim().parse().ok().filter(|d: &f32| *d > 0.0)?;
    Some(Forme { largeur: 0, hauteur: 0, duree, cadence: 1.0, son: true })
}

impl Forme {
    /// Un son seul, sans image.
    pub fn sans_image(&self) -> bool {
        self.largeur == 0
    }
}

pub fn sonder(chemin: &Path) -> Option<Forme> {
    // Un mp3 peut porter une pochette, que ffprobe compte comme un flux video :
    // c'est l'extension qui dit que le fichier est un son.
    if crate::medias::est_audio(chemin) {
        return sonder_son(chemin);
    }
    let s = outil("ffprobe")
        .args([
            "-v", "error", "-show_entries", "stream=codec_type,width,height,r_frame_rate:format=duration", "-of", "json",
        ])
        .arg(chemin)
        .output()
        .ok()?;
    let v: serde_json::Value = serde_json::from_slice(&s.stdout).ok()?;
    let flux = v.get("streams")?.as_array()?;
    let image = flux.iter().find(|f| f.get("codec_type").and_then(|x| x.as_str()) == Some("video"))?;
    let son = flux.iter().any(|f| f.get("codec_type").and_then(|x| x.as_str()) == Some("audio"));
    let n = |c: &str| image.get(c).and_then(|x| x.as_u64()).map(|x| x as u32);
    let duree = v.pointer("/format/duration").and_then(|x| x.as_str()).and_then(|x| x.parse().ok()).unwrap_or(0.0);
    Some(Forme {
        largeur: n("width")?,
        hauteur: n("height")?,
        duree,
        cadence: image.get("r_frame_rate").and_then(|x| x.as_str()).and_then(cadence).unwrap_or(25.0),
        son,
    })
}

/// La taille de decodage : celle de la video, ramenee a LARGEUR_MAX, en
/// nombres pairs (ce que demandent la plupart des filtres de ffmpeg).
pub fn taille_decodee(l: u32, h: u32) -> (u32, u32) {
    let r = (LARGEUR_MAX as f32 / l.max(1) as f32).min(1.0);
    let pair = |x: f32| ((x.round() as u32) / 2 * 2).max(2);
    (pair(l as f32 * r), pair(h as f32 * r))
}

/// L'image a montrer a la position t : la derniere dont l'instant est passe.
pub fn image_a(t: f32, cadence: f32) -> i64 {
    (t.max(0.0) * cadence + 1e-4).floor() as i64
}

/// « 0:07 / 1:00 ».
pub fn temps(s: f32) -> String {
    let s = s.max(0.0).floor() as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

pub struct Lecture {
    pub chemin: PathBuf,
    pub forme: Forme,
    /// La taille des images rendues, celle de la texture.
    pub largeur: u32,
    pub hauteur: u32,
    images: Receiver<Vec<u8>>,
    processus: Option<Child>,
    son: Option<son::Son>,
    /// Le moment ou la position valait `origine`, si l'on joue.
    depart: Option<Instant>,
    origine: f32,
    /// La position ou le decodeur a commence : ses images se comptent de la,
    /// quelles que soient les pauses.
    base: f32,
    /// Le rang de la derniere image prise au decodeur, depuis `base`.
    prise: i64,
    epuise: bool,
}

impl Lecture {
    /// Ouvre la video a la position `depuis`, en pause ou non.
    pub fn ouvrir(chemin: &Path, depuis: f32, joue: bool) -> Option<Lecture> {
        let forme = sonder(chemin)?;
        Lecture::avec(chemin, forme, depuis, joue)
    }

    fn avec(chemin: &Path, forme: Forme, depuis: f32, joue: bool) -> Option<Lecture> {
        let (l, h) = if forme.sans_image() { (0, 0) } else { taille_decodee(forme.largeur, forme.hauteur) };
        let depuis = depuis.clamp(0.0, forme.duree.max(0.0));
        let (envoi, images) = sync_channel::<Vec<u8>>(AVANCE);
        let processus = if forme.sans_image() {
            // Rien a decoder : l'emetteur tombe ici, la file se dit epuisee.
            drop(envoi);
            None
        } else {
            let mut p = outil("ffmpeg")
                .args(["-v", "error", "-ss", &format!("{depuis:.3}"), "-i"])
                .arg(chemin)
                .args(["-an", "-f", "rawvideo", "-pix_fmt", "rgba", "-vf", &format!("scale={l}:{h}"), "-"])
                .stdout(Stdio::piped())
                .spawn()
                .ok()?;
            let mut sortie = p.stdout.take()?;
            let taille = (4 * l * h) as usize;
            std::thread::spawn(move || loop {
                let mut tampon = vec![0u8; taille];
                if sortie.read_exact(&mut tampon).is_err() || envoi.send(tampon).is_err() {
                    return;
                }
            });
            Some(p)
        };
        let son = if forme.son && joue { son::Son::ouvrir(chemin, depuis) } else { None };
        Some(Lecture {
            chemin: chemin.to_path_buf(),
            forme,
            largeur: l,
            hauteur: h,
            images,
            processus,
            son,
            depart: joue.then(Instant::now),
            origine: depuis,
            base: depuis,
            prise: -1,
            epuise: false,
        })
    }

    pub fn joue(&self) -> bool {
        self.depart.is_some()
    }

    pub fn position(&self) -> f32 {
        let p = match self.depart {
            Some(d) => self.origine + d.elapsed().as_secs_f32(),
            None => self.origine,
        };
        p.min(self.forme.duree.max(0.0))
    }

    /// La video est-elle arrivee au bout ? Elle s'arrete alors sur sa
    /// derniere image, et la relancer la reprend au debut.
    pub fn finie(&self) -> bool {
        self.forme.duree > 0.0 && self.position() >= self.forme.duree - 1e-3
    }

    /// Les pixels de l'image due, s'il en est arrive une nouvelle. Les images
    /// en retard sont jetees.
    pub fn image_due(&mut self) -> Option<Vec<u8>> {
        let voulue = image_a(self.position() - self.base, self.forme.cadence);
        let mut derniere = None;
        while self.prise < voulue && !self.epuise {
            match self.images.try_recv() {
                Ok(p) => {
                    self.prise += 1;
                    derniere = Some(p);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => self.epuise = true,
            }
        }
        if self.finie() && self.joue() {
            self.origine = self.forme.duree;
            self.depart = None;
            if let Some(s) = self.son.as_mut() {
                s.pause();
            }
        }
        derniere
    }

    /// Lecture ou pause. Au bout, la lecture repart du debut.
    pub fn basculer(self) -> Option<Lecture> {
        if self.joue() {
            let mut s = self;
            s.origine = s.position();
            s.depart = None;
            if let Some(son) = s.son.as_mut() {
                son.pause();
            }
            return Some(s);
        }
        if self.finie() {
            return self.aller(0.0, true);
        }
        // Reprendre la ou l'on s'est arrete : le decodeur, bloque sur sa file
        // pleine, repart de lui-meme ; le son, lui, se rouvre a la position.
        let mut s = self;
        s.depart = Some(Instant::now());
        if s.forme.son {
            match s.son.as_mut() {
                Some(son) => son.reprendre(),
                None => s.son = son::Son::ouvrir(&s.chemin, s.origine),
            }
        }
        Some(s)
    }

    /// Aller a la position t : le decodeur repart de la, le son aussi.
    pub fn aller(self, t: f32, joue: bool) -> Option<Lecture> {
        let (chemin, forme) = (self.chemin.clone(), self.forme);
        drop(self);
        Lecture::avec(&chemin, forme, t, joue)
    }
}

impl Drop for Lecture {
    fn drop(&mut self) {
        if let Some(p) = self.processus.as_mut() {
            let _ = p.kill();
            let _ = p.wait();
        }
    }
}

/// Le son, par waveOut : l'interface audio la plus simple de Windows, sans
/// aucune caisse. ffmpeg rend du PCM 16 bits stereo a 48 kHz ; un fil le verse
/// dans huit tampons de 100 ms qui tournent.
#[cfg(windows)]
mod son {
    use std::io::Read;
    use std::path::Path;
    use std::process::{Child, Stdio};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    const FREQUENCE: u32 = 48_000;
    const CANAUX: u16 = 2;
    const TAMPONS: usize = 8;
    /// 100 ms de son par tampon.
    const OCTETS: usize = (FREQUENCE as usize / 10) * CANAUX as usize * 2;
    const WAVE_MAPPER: u32 = 0xFFFF_FFFF;
    const WHDR_DONE: u32 = 1;

    #[repr(C, packed)]
    struct Format {
        etiquette: u16,
        canaux: u16,
        frequence: u32,
        octets_par_seconde: u32,
        alignement: u16,
        bits: u16,
        taille: u16,
    }

    #[repr(C)]
    struct Entete {
        donnees: *mut u8,
        longueur: u32,
        enregistres: u32,
        utilisateur: usize,
        drapeaux: u32,
        boucles: u32,
        suivant: *mut Entete,
        reserve: usize,
    }

    type Poignee = *mut core::ffi::c_void;

    #[link(name = "winmm")]
    extern "system" {
        fn waveOutOpen(p: *mut Poignee, dispositif: u32, f: *const Format, rappel: usize, instance: usize, drapeaux: u32) -> u32;
        fn waveOutPrepareHeader(p: Poignee, e: *mut Entete, taille: u32) -> u32;
        fn waveOutUnprepareHeader(p: Poignee, e: *mut Entete, taille: u32) -> u32;
        fn waveOutWrite(p: Poignee, e: *mut Entete, taille: u32) -> u32;
        fn waveOutPause(p: Poignee) -> u32;
        fn waveOutRestart(p: Poignee) -> u32;
        fn waveOutReset(p: Poignee) -> u32;
        fn waveOutClose(p: Poignee) -> u32;
    }

    /// La poignee passe d'un fil a l'autre ; waveOut l'accepte.
    #[derive(Clone, Copy)]
    struct Envoyable(usize);

    pub struct Son {
        poignee: Envoyable,
        arret: Arc<AtomicBool>,
        fil: Option<std::thread::JoinHandle<()>>,
        processus: Child,
    }

    impl Son {
        pub fn ouvrir(chemin: &Path, depuis: f32) -> Option<Son> {
            // VIGIE_MUET : les captures et les essais ne jouent rien dans les
            // haut-parleurs de la machine (une capture de la visionneuse sur un
            // son avait lu une seconde de voix, le 2026-10-29).
            if std::env::var_os("VIGIE_MUET").is_some() {
                return None;
            }
            let mut processus = super::outil("ffmpeg")
                .args(["-v", "error", "-ss", &format!("{depuis:.3}"), "-i"])
                .arg(chemin)
                .args(["-vn", "-f", "s16le", "-ac", "2", "-ar", "48000", "-"])
                .stdout(Stdio::piped())
                .spawn()
                .ok()?;
            let mut sortie = processus.stdout.take()?;
            let format = Format {
                etiquette: 1,
                canaux: CANAUX,
                frequence: FREQUENCE,
                octets_par_seconde: FREQUENCE * CANAUX as u32 * 2,
                alignement: CANAUX * 2,
                bits: 16,
                taille: 0,
            };
            let mut p: Poignee = std::ptr::null_mut();
            if unsafe { waveOutOpen(&mut p, WAVE_MAPPER, &format, 0, 0, 0) } != 0 {
                let _ = processus.kill();
                return None;
            }
            let poignee = Envoyable(p as usize);
            let arret = Arc::new(AtomicBool::new(false));
            let arret_fil = arret.clone();
            let fil = std::thread::spawn(move || {
                let p = poignee.0 as Poignee;
                let taille = std::mem::size_of::<Entete>() as u32;
                let mut memoire: Vec<Vec<u8>> = (0..TAMPONS).map(|_| vec![0u8; OCTETS]).collect();
                let mut entetes: Vec<Entete> = memoire
                    .iter_mut()
                    .map(|m| Entete {
                        donnees: m.as_mut_ptr(),
                        longueur: 0,
                        enregistres: 0,
                        utilisateur: 0,
                        drapeaux: WHDR_DONE,
                        boucles: 0,
                        suivant: std::ptr::null_mut(),
                        reserve: 0,
                    })
                    .collect();
                let mut prepares = [false; TAMPONS];
                let mut i = 0;
                'lecture: while !arret_fil.load(Ordering::Relaxed) {
                    // Attendre que ce tampon ait ete joue avant de le remplir.
                    while entetes[i].drapeaux & WHDR_DONE == 0 {
                        if arret_fil.load(Ordering::Relaxed) {
                            break 'lecture;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                    if prepares[i] {
                        unsafe { waveOutUnprepareHeader(p, &mut entetes[i], taille) };
                        prepares[i] = false;
                    }
                    let mut lus = 0;
                    while lus < OCTETS {
                        match sortie.read(&mut memoire[i][lus..]) {
                            Ok(0) | Err(_) => break,
                            Ok(n) => lus += n,
                        }
                    }
                    if lus == 0 {
                        break;
                    }
                    let e = &mut entetes[i];
                    e.longueur = (lus - lus % 4) as u32;
                    e.drapeaux = 0;
                    unsafe {
                        waveOutPrepareHeader(p, e, taille);
                        waveOutWrite(p, e, taille);
                    }
                    prepares[i] = true;
                    i = (i + 1) % TAMPONS;
                }
                // Le son coupe net, les tampons rendus, puis la poignee fermee :
                // les tampons ne doivent pas mourir avant que waveOut les lache.
                unsafe { waveOutReset(p) };
                for (k, e) in entetes.iter_mut().enumerate() {
                    if prepares[k] {
                        unsafe { waveOutUnprepareHeader(p, e, taille) };
                    }
                }
                unsafe { waveOutClose(p) };
                drop(memoire);
            });
            Some(Son { poignee, arret, fil: Some(fil), processus })
        }

        pub fn pause(&mut self) {
            unsafe { waveOutPause(self.poignee.0 as Poignee) };
        }

        pub fn reprendre(&mut self) {
            unsafe { waveOutRestart(self.poignee.0 as Poignee) };
        }
    }

    impl Drop for Son {
        fn drop(&mut self) {
            self.arret.store(true, Ordering::Relaxed);
            let _ = self.processus.kill();
            let _ = self.processus.wait();
            if let Some(f) = self.fil.take() {
                let _ = f.join();
            }
        }
    }
}

/// Le son hors de Windows : `ffplay`, que fournit ffmpeg, joue le fichier a
/// partir d'une position. Une pause arrete le processus et note ou l'on en
/// etait ; reprendre le relance de la. Sans ffplay sur le PATH, la vigie reste
/// muette (l'onde se dessine quand meme), comme avec `VIGIE_MUET`.
#[cfg(not(windows))]
mod son {
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, Stdio};
    use std::time::Instant;

    pub struct Son {
        chemin: PathBuf,
        /// Ou l'on en est, en secondes, quand le processus ne tourne pas.
        position: f32,
        joue: Option<(Child, Instant)>,
    }

    fn lancer(chemin: &Path, depuis: f32) -> Option<Child> {
        Command::new("ffplay")
            .args(["-nodisp", "-autoexit", "-loglevel", "quiet", "-vn", "-ss", &format!("{depuis:.3}")])
            .arg(chemin)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()
    }

    impl Son {
        pub fn ouvrir(chemin: &Path, depuis: f32) -> Option<Son> {
            // VIGIE_MUET : les captures et les essais ne jouent rien.
            if std::env::var_os("VIGIE_MUET").is_some() {
                return None;
            }
            let enfant = lancer(chemin, depuis)?;
            Some(Son { chemin: chemin.to_path_buf(), position: depuis, joue: Some((enfant, Instant::now())) })
        }

        pub fn pause(&mut self) {
            if let Some((mut enfant, depuis)) = self.joue.take() {
                self.position += depuis.elapsed().as_secs_f32();
                let _ = enfant.kill();
                let _ = enfant.wait();
            }
        }

        pub fn reprendre(&mut self) {
            if self.joue.is_none() {
                if let Some(enfant) = lancer(&self.chemin, self.position) {
                    self.joue = Some((enfant, Instant::now()));
                }
            }
        }
    }

    impl Drop for Son {
        fn drop(&mut self) {
            self.pause();
        }
    }
}

#[cfg(test)]
mod essais {
    use super::*;

    #[test]
    fn la_cadence_se_lit() {
        assert_eq!(cadence("15/1"), Some(15.0));
        assert!((cadence("30000/1001").unwrap() - 29.97).abs() < 0.01);
        assert_eq!(cadence("0/0"), None);
        assert_eq!(cadence("25"), Some(25.0));
    }

    #[test]
    fn la_taille_decodee_est_paire_et_bornee() {
        assert_eq!(taille_decodee(854, 480), (854, 480));
        assert_eq!(taille_decodee(1920, 1080), (1600, 900));
        assert_eq!(taille_decodee(601, 357), (600, 356));
    }

    /// A 15 images par seconde, l'image de 0,5 s est la huitieme (rang 7), et
    /// celle de 0 est la premiere ; une position negative ne recule pas.
    #[test]
    fn l_image_d_une_position() {
        assert_eq!(image_a(0.0, 15.0), 0);
        assert_eq!(image_a(0.5, 15.0), 7);
        assert_eq!(image_a(1.0, 15.0), 15);
        assert_eq!(image_a(-1.0, 15.0), 0);
        assert_eq!(temps(67.9), "1:07");
    }

    /// Une vraie video, faite ici par ffmpeg : une mire de 2 s a 10 images par
    /// seconde, 320 × 240, avec un son. La lecture en rend les images a la
    /// bonne taille et en nombre, et elle s'arrete au bout.
    #[test]
    fn une_vraie_video_se_lit() {
        let d = std::env::temp_dir().join(format!("vigie-video-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("mire.mp4");
        let ok = outil("ffmpeg")
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc=size=320x240:rate=10:duration=2"])
            .args(["-f", "lavfi", "-i", "sine=frequency=440:duration=2", "-shortest", "-pix_fmt", "yuv420p"])
            .arg(&f)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            eprintln!("ffmpeg absent : essai saute");
            return;
        }
        let forme = sonder(&f).unwrap();
        assert_eq!((forme.largeur, forme.hauteur, forme.cadence), (320, 240, 10.0));
        assert!(forme.son);
        assert!((forme.duree - 2.0).abs() < 0.1, "duree {}", forme.duree);
        // En pause, la premiere image vient, et elle seule. L'essai est muet :
        // il ne doit pas jouer un la a 440 Hz dans les haut-parleurs de la
        // machine ; le son se verifie dans la fenetre.
        let mut l = Lecture::avec(&f, Forme { son: false, ..forme }, 0.0, false).unwrap();
        let debut = Instant::now();
        let mut premiere = None;
        while premiere.is_none() && debut.elapsed().as_secs() < 5 {
            premiere = l.image_due();
        }
        assert_eq!(premiere.unwrap().len(), 320 * 240 * 4);
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(l.image_due().is_none());
        // En lecture, le temps avance et la video s'arrete au bout.
        let mut l = l.basculer().unwrap();
        let mut vues = 0;
        while debut.elapsed().as_secs() < 8 && !l.finie() {
            if l.image_due().is_some() {
                vues += 1;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        l.image_due();
        assert!(l.finie() && !l.joue());
        assert!(vues >= 15, "seulement {vues} images vues sur 20");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Un vrai son, fait ici par ffmpeg : 2 s de la a 440 Hz. Il se sonde sans
    /// image, se lit sans decodeur, avance avec l'horloge, saute a une position,
    /// et s'arrete au bout. Muet, comme l'essai de la video.
    #[test]
    fn un_vrai_son_se_lit_sans_image() {
        let d = std::env::temp_dir().join(format!("vigie-son-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("la.wav");
        let ok = outil("ffmpeg")
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", "sine=frequency=440:duration=2"])
            .arg(&f)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            eprintln!("ffmpeg absent : essai saute");
            return;
        }
        let forme = sonder(&f).unwrap();
        assert!(forme.sans_image() && forme.son);
        assert!((forme.duree - 2.0).abs() < 0.1, "duree {}", forme.duree);
        // Une video, elle, n'est pas un son : le contre-temoin.
        assert!(!Forme { largeur: 320, ..forme }.sans_image());
        let mut l = Lecture::avec(&f, Forme { son: false, ..forme }, 0.0, false).unwrap();
        assert!(l.processus.is_none());
        assert_eq!(l.position(), 0.0);
        assert!(l.image_due().is_none());
        // Le saut : en pause, la position est celle qu'on a visee.
        let l = l.aller(1.5, false).unwrap();
        assert!((l.position() - 1.5).abs() < 1e-3 && !l.joue());
        // En lecture, le temps avance, et au bout elle s'arrete.
        let mut l = l.basculer().unwrap();
        let debut = Instant::now();
        while debut.elapsed().as_secs() < 5 && !l.finie() {
            l.image_due();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        l.image_due();
        assert!(l.finie() && !l.joue());
        assert!((l.position() - forme.duree).abs() < 0.05);
        let _ = std::fs::remove_dir_all(&d);
    }
}
