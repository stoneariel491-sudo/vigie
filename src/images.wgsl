// Les images : un rectangle texture, aux coins arrondis par la meme distance
// signee que les aplats, pour que le cadre et l'image tombent sur le meme bord.

struct Cadre {
    ecran: vec4<f32>,  // largeur, hauteur de l'ecran, en pixels
    boite: vec4<f32>,  // x, y, largeur, hauteur, en pixels
    rayon: vec4<f32>,  // x = rayon des coins
}

@group(0) @binding(0) var<uniform> cadre: Cadre;
@group(0) @binding(1) var image: texture_2d<f32>;
@group(0) @binding(2) var echantillon: sampler;

struct Sortie {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) local: vec2<f32>,
}

@vertex
fn sommet(@builtin(vertex_index) i: u32) -> Sortie {
    var table = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
    );
    let u = table[i];
    let px = cadre.boite.xy + u * cadre.boite.zw;

    var s: Sortie;
    s.position = vec4<f32>(
        px.x / cadre.ecran.x * 2.0 - 1.0,
        1.0 - px.y / cadre.ecran.y * 2.0,
        0.0,
        1.0,
    );
    s.uv = u;
    s.local = px - (cadre.boite.xy + cadre.boite.zw * 0.5);
    return s;
}

@fragment
fn fragment(e: Sortie) -> @location(0) vec4<f32> {
    // Grossie, l'image garde ses pixels nets : chaque texel reste un carre,
    // et seul son bord, large d'un pixel d'ecran, se fond dans le voisin.
    // Montree a sa taille ou plus petite, le pas est borne a un texel, et la
    // formule rend l'echantillonnage lineaire tel quel.
    // Les derivees sont celles de l'uv d'origine, pour que la carte choisisse
    // la bonne reduction sans saut a la couture des texels.
    let dims = vec2<f32>(textureDimensions(image));
    let tx = e.uv * dims;
    let couture = floor(tx + 0.5);
    let pas = clamp(fwidth(tx), vec2<f32>(1e-4), vec2<f32>(1.0));
    let net = (couture + clamp((tx - couture) / pas, vec2<f32>(-0.5), vec2<f32>(0.5))) / dims;
    let c = textureSampleGrad(image, echantillon, net, dpdx(e.uv), dpdy(e.uv));
    let demi = cadre.boite.zw * 0.5;
    let r = min(cadre.rayon.x, min(demi.x, demi.y));
    let q = abs(e.local) - demi + vec2<f32>(r);
    let d = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
    let dedans = 1.0 - smoothstep(-0.5, 0.5, d);
    return vec4<f32>(c.rgb, c.a * dedans);
}
