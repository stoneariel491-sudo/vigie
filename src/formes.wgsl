// Les aplats : un rectangle a coins arrondis, et rien d'autre.
//
// Six sommets par forme, aucun tampon de sommets : l'indice suffit a retrouver
// a la fois quelle forme et quel coin. La forme est lue dans une memoire de
// stockage, ce qui evite un tampon d'instances et ses declarations.

struct Forme {
    boite: vec4<f32>,    // x, y, largeur, hauteur, en pixels
    couleur: vec4<f32>,  // rouge, vert, bleu, alpha, LINEAIRE
    rayon: vec4<f32>,    // x = rayon des coins, y = fondu (voir fragment)
}

@group(0) @binding(0) var<uniform> ecran: vec4<f32>;
@group(0) @binding(1) var<storage, read> formes: array<Forme>;

struct Sortie {
    @builtin(position) position: vec4<f32>,
    // La position DANS la forme, en pixels, pour la distance signee.
    @location(0) local: vec2<f32>,
    @location(1) demi: vec2<f32>,
    @location(2) rayon: f32,
    @location(3) couleur: vec4<f32>,
    @location(4) fondu: f32,
}

@vertex
fn sommet(@builtin(vertex_index) i: u32) -> Sortie {
    let n = i / 6u;
    let coin = i % 6u;
    let f = formes[n];

    // Deux triangles : 0,1,2 puis 2,1,3 en coordonnees de coin.
    var table = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
    );
    let u = table[coin];

    let taille = f.boite.zw;
    let px = f.boite.xy + u * taille;
    // Un demi-pixel de debord, pour que l'adoucissement du bord ait de la
    // place : sans lui la derniere rangee du contour serait coupee.
    let deborde = (u - vec2<f32>(0.5)) * 2.0;
    let px2 = px + deborde;

    var s: Sortie;
    s.position = vec4<f32>(
        px2.x / ecran.x * 2.0 - 1.0,
        1.0 - px2.y / ecran.y * 2.0,
        0.0,
        1.0,
    );
    s.local = px2 - (f.boite.xy + taille * 0.5);
    s.demi = taille * 0.5;
    s.rayon = min(f.rayon.x, min(s.demi.x, s.demi.y));
    s.couleur = f.couleur;
    s.fondu = f.rayon.y;
    return s;
}

@fragment
fn fragment(e: Sortie) -> @location(0) vec4<f32> {
    let q = abs(e.local) - e.demi + vec2<f32>(e.rayon);
    let d = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - e.rayon;
    // Un pixel de transition, centre sur le bord exact.
    let dedans = 1.0 - smoothstep(-0.5, 0.5, d);
    var a = e.couleur.a * dedans;
    // Le fondu horizontal des voiles qui bordent une bande d'onglets trop
    // longue : positif, la forme s'opacifie de gauche a droite ; negatif,
    // l'inverse. Nul, c'est un aplat ordinaire.
    if (e.fondu != 0.0) {
        let t = clamp((e.local.x + e.demi.x) / max(2.0 * e.demi.x, 1.0), 0.0, 1.0);
        a = a * select(1.0 - t, t, e.fondu > 0.0);
    }
    return vec4<f32>(e.couleur.rgb, a);
}
