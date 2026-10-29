// Le verre de la visionneuse : le fond flou et les plaques.
//
// La recette est celle du verre ARBITREE de veilleuse (veilleuse/DECISION.md,
// « Pour l'integrateur », et src/galet.wgsl, `couleur_verre`) : une couche
// floutee en lineaire, sa vibrance, une teinte noire posee sur les octets
// ENCODES, et au biseau la refraction de Snell sur la couche nette, la
// reflexion de Fresnel exacte moyennee sur l'empreinte du pixel, et la lampe
// d'ecran sur le chant. Ce qui change ici est dit la ou cela change.
//
// Les textures de la scene sont vues en sRGB : l'echantillonnage decode et
// l'ecriture reencode, et tout le calcul se fait en lineaire.

struct Uniformes {
    // xy = taille de la scene (px), zw = taille de la couche floutee (px).
    taille: vec4<f32>,
    // x = sigma du flou en pixels de la couche lue, y = rayon en prises,
    // zw = direction d'une prise, en pixels de la couche lue.
    flou: vec4<f32>,
    // x = ouverture t (0 a 1), y = teinte du fond, z = saturation,
    // w = pas de lecture de la scene (2 : demi-resolution).
    fond: vec4<f32>,
    // La plaque : xy = centre, zw = demi-taille (px).
    plaque: vec4<f32>,
    // x = rayon du coin, y = exposant de la superellipse, z = conge b,
    // w = epaisseur e.
    forme: vec4<f32>,
    // x = indice, y = teinte de la plaque, z = force de la reflexion,
    // w = opacite de la plaque.
    matiere: vec4<f32>,
    // x = intensite L de la lampe (luminance lineaire), y = reflectance du
    // pixel du bord, zw = inutilises.
    lampe: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniformes;
@group(0) @binding(1) var ech: sampler;
// La scene nette (la vue de la vigie sous la visionneuse), et la couche lue par
// la passe : la scene pour la premiere passe de flou, la demi-couche pour la
// seconde, la couche floutee finale pour le fond et les plaques.
@group(0) @binding(2) var nette: texture_2d<f32>;
@group(0) @binding(3) var couche: texture_2d<f32>;

const LUMA = vec3<f32>(0.2126, 0.7152, 0.0722);
// Les prises sur l'empreinte d'un pixel du biseau, comme le verre ARBITREE.
const PRISES: i32 = 8;

@vertex
fn sommet(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    // Un triangle qui couvre l'ecran : chaque passe est plein ecran, et la
    // plaque se decoupe elle-meme par sa distance.
    let x = f32((i << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(i & 2u) * 2.0 - 1.0;
    return vec4<f32>(x, -y, 0.0, 1.0);
}

fn vers_srgb(c: vec3<f32>) -> vec3<f32> {
    let bas = c * 12.92;
    let haut = 1.055 * pow(max(c, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(bas, haut, c > vec3<f32>(0.0031308));
}

fn depuis_srgb(c: vec3<f32>) -> vec3<f32> {
    let bas = c / 12.92;
    let haut = pow((max(c, vec3<f32>(0.0)) + 0.055) / 1.055, vec3<f32>(2.4));
    return select(bas, haut, c > vec3<f32>(0.04045));
}

// --- Le flou gaussien separable, en lineaire --------------------------------
//
// Premiere passe : horizontale, elle lit la scene un pixel sur `fond.w` et
// ecrit la demi-couche. Seconde passe : verticale, sur la demi-couche. Le flou
// a demi-resolution est celui du givre de veilleuse (RECETTES.md de veilleuse :
// « divise son cout par huit sans rien changer de visible »).
@fragment
fn flou(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let dims = vec2<f32>(textureDimensions(couche));
    let pas = u.fond.w;
    let centre = pos.xy * pas;
    let sigma = u.flou.x;
    let r = i32(u.flou.y);
    var somme = vec3<f32>(0.0);
    var poids = 0.0;
    for (var i = -r; i <= r; i = i + 1) {
        let d = f32(i);
        let w = exp(-0.5 * d * d / (sigma * sigma));
        let p = clamp(centre + u.flou.zw * d * pas, vec2<f32>(0.5), dims - vec2<f32>(0.5));
        somme = somme + textureSampleLevel(couche, ech, p / dims, 0.0).rgb * w;
        poids = poids + w;
    }
    return vec4<f32>(somme / poids, 1.0);
}

// La couche floutee en un point de la scene, vibrance et teinte comprises.
fn floutee(p: vec2<f32>, teinte: f32) -> vec3<f32> {
    let c = textureSampleLevel(couche, ech, clamp(p / u.taille.xy, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0).rgb;
    let gris = dot(c, LUMA);
    let sature = max(mix(vec3<f32>(gris), c, u.fond.z), vec3<f32>(0.0));
    // La teinte est noire : mix(s, 0, tau) sur les octets encodes.
    return depuis_srgb(vers_srgb(sature) * (1.0 - teinte));
}

// --- Le fond ----------------------------------------------------------------
//
// La scene nette se fond dans le verre a mesure que la visionneuse s'ouvre :
// a t = 0 c'est la vue telle quelle, a t = 1 son verre. C'est l'ecart avec
// Apple, qui anime le rayon du flou : ici le flou est fixe et c'est son
// melange qui monte, ce qui coute une passe de moins par trame.
@fragment
fn fond(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let n = textureLoad(nette, vec2<i32>(pos.xy), 0).rgb;
    let v = floutee(pos.xy, u.fond.y);
    return vec4<f32>(mix(n, v, clamp(u.fond.x, 0.0, 1.0)), 1.0);
}

// --- Les plaques ------------------------------------------------------------

struct Forme {
    d: f32,
    dir: vec2<f32>,
};

// La distance signee a la superellipse et la direction de son gradient, vers
// l'exterieur : `forme` de galet.wgsl, a la lettre.
fn forme(p: vec2<f32>, demi: vec2<f32>, rayon: f32, n: f32) -> Forme {
    let s = sign(p);
    let q = abs(p) - demi + vec2<f32>(rayon);
    var d: f32;
    var g: vec2<f32>;
    if (q.x > 0.0 && q.y > 0.0) {
        let f = pow(pow(q.x, n) + pow(q.y, n), 1.0 / n);
        let brut = vec2<f32>(pow(q.x, n - 1.0), pow(q.y, n - 1.0)) / max(pow(f, n - 1.0), 1e-6);
        let ln = max(length(brut), 1e-4);
        d = (f - rayon) / ln;
        g = brut / ln;
    } else {
        let m = max(q, vec2<f32>(0.0));
        d = length(m) + min(max(q.x, q.y), 0.0) - rayon;
        if (m.x + m.y > 1e-5) {
            g = normalize(m);
        } else if (q.x > q.y) {
            g = vec2<f32>(1.0, 0.0);
        } else {
            g = vec2<f32>(0.0, 1.0);
        }
    }
    return Forme(d, g * s);
}

struct Bord {
    deplacement: f32,
    reflectance: f32,
    portee: f32,
};

// Le biseau en un point du conge : le deplacement de Snell, la reflectance de
// Fresnel exacte non polarisee, et la portee du rayon reflechi. `bord_en` de
// galet.wgsl, a la lettre.
fn bord_en(d: f32, b: f32, n: f32, e: f32) -> Bord {
    let sinp = clamp(1.0 + d / b, 0.0, 1.0);
    let cosp = sqrt(max(1.0 - sinp * sinp, 0.0));
    let z = max(e - b + b * cosp, 0.0);
    let sint = sinp / n;
    let cost = sqrt(max(1.0 - sint * sint, 0.0));
    let num = sinp * cost - cosp * sint;
    let den = max(cosp * cost + sinp * sint, 1e-6);
    let rs = (cosp - n * cost) / max(cosp + n * cost, 1e-6);
    let rp = (n * cosp - cost) / max(n * cosp + cost, 1e-6);
    let c2 = 1.0 - 2.0 * sinp * sinp;
    let s2 = 2.0 * sinp * cosp;
    var portee = -1.0;
    if (c2 < 0.0) {
        portee = z * s2 / (-c2);
    }
    return Bord(z * num / den, 0.5 * (rs * rs + rp * rp), portee);
}

fn nette_en(p: vec2<f32>) -> vec3<f32> {
    let q = clamp(p, vec2<f32>(0.5), u.taille.xy - vec2<f32>(0.5));
    return textureLoad(nette, vec2<i32>(q), 0).rgb;
}

// Ce que montre le fond en un point : la scene nette fondue dans son verre.
// C'est ce qu'une plaque reflete. Lire la scene nette, comme l'ilot de
// veilleuse lit le bureau, faisait apparaitre sous la plaque du titre une bande
// de texte presque net, alors qu'autour on ne voit que du flou (vu le
// 2026-10-27 sur la premiere capture du verre).
fn fond_en(p: vec2<f32>) -> vec3<f32> {
    return mix(nette_en(p), floutee(p, u.fond.y), clamp(u.fond.x, 0.0, 1.0));
}

// Une plaque de verre posee sur la visionneuse. Ce qu'elle laisse voir est la
// page sous la visionneuse, floutee, avec une teinte plus legere que celle du
// fond : posee sur un fond deja assombri, elle se lit comme une lame plus
// claire, et son chant comme une arete de lumiere. C'est l'autre ecart avec
// l'ilot de veilleuse, qui se pose sur le bureau lui-meme.
@fragment
fn plaque(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let f = forme(pos.xy - u.plaque.xy, u.plaque.zw, u.forme.x, u.forme.y);
    let couverture = clamp(0.5 - f.d, 0.0, 1.0);
    if (couverture <= 0.0) {
        discard;
    }
    let b = u.forme.z;
    let e = max(u.forme.w, 1e-3);
    let n = max(u.matiere.x, 1.0);
    let r0 = pow((n - 1.0) / (n + 1.0), 2.0);
    let dans_conge = b > 0.0 && f.d > -b - 1.0;

    // Fresnel, moyenne sur l'empreinte du pixel ; la reflexion lit le fond a
    // la portee du rayon reflechi.
    var transmis = 1.0 - r0;
    var reflet = vec3<f32>(0.0);
    var deplacement = 0.0;
    if (dans_conge) {
        var s_t = 0.0;
        var s_r = vec3<f32>(0.0);
        var s_d = 0.0;
        for (var k = 0; k < PRISES; k = k + 1) {
            let dd = f.d - 0.5 + (f32(k) + 0.5) / f32(PRISES);
            let g = bord_en(min(dd, 0.0), b, n, e);
            s_t = s_t + (1.0 - g.reflectance);
            s_d = s_d + g.deplacement;
            if (g.portee >= 0.0) {
                s_r = s_r + fond_en(pos.xy + f.dir * min(g.portee, 4.0 * e)) * g.reflectance;
            }
        }
        let inv = 1.0 / f32(PRISES);
        transmis = s_t * inv;
        reflet = s_r * inv;
        deplacement = s_d * inv;
    }

    // La face : la couche floutee, lue a travers le biseau qui la deplace vers
    // l'interieur (Snell).
    var verre = floutee(pos.xy - f.dir * deplacement, u.matiere.y);
    verre = verre * transmis + reflet * u.matiere.z;

    // La lampe d'ecran sur le chant : son poids suit la normale sortante (1 en
    // haut, 1/2 aux cotes, 0 en bas), son profil celui de la reflectance au-
    // dessus de r0, et elle l'emporte sur le reste par le plus grand en
    // luminance, jamais par la somme (galet.wgsl, E2).
    if (u.lampe.x > 0.0 && dans_conge) {
        let profil = clamp(((1.0 - transmis) - r0) / max(u.lampe.y - r0, 1e-6), 0.0, 1.0);
        let lueur = u.lampe.x * (1.0 - f.dir.y) * 0.5 * profil;
        if (lueur > dot(verre, LUMA)) {
            verre = vec3<f32>(lueur);
        }
    }
    return vec4<f32>(verre, couverture * u.matiere.w);
}
