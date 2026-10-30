//! Le verre de la visionneuse : la vue de la vigie floutee sous l'image
//! ouverte, et les plaques de verre ou se posent son titre et ses commandes.
//!
//! La matiere est celle de vitrine, arbitree par l'auteur
//! (`vitrine/DECISION.md`, « Pour l'integrateur ») : flou de 18 px en
//! lineaire, vibrance 1,35, teinte noire posee sur les octets encodes, verre
//! N-BK7 d'indice 1,5168, conge et epaisseur de 12 px pour une hauteur de 148,
//! reflexion de Fresnel exacte moyennee sur huit prises, lampe d'ecran au
//! niveau 85 sur le chant (`vitrine/src/galet.rs`, `NIVEAU_DEPLOYE`).
//!
//! Le chemin d'une trame, la visionneuse ouverte : la vue est dessinee dans
//! `scene` au lieu de l'ecran ; deux passes de flou la portent a
//! demi-resolution ; le fond la fond dans son verre a mesure de l'ouverture ;
//! les plaques se posent par-dessus ; l'image et les textes viennent apres.

use wgpu::util::DeviceExt;

/// Le flou du fond, en pixels de la scene a l'echelle 1 (vitrine : 18).
pub const SIGMA: f32 = 18.0;
/// La teinte du fond : celle du verre arbitre, qui pose l'image ouverte sur
/// une page qu'on devine sans la lire.
pub const TEINTE_FOND: f32 = 0.80;
/// La teinte d'une plaque, plus legere que le fond : elle se lit comme une
/// lame plus claire posee dessus. Choisie, non mesuree : voir LISEZ-MOI.md.
pub const TEINTE_PLAQUE: f32 = 0.62;
/// Au theme clair, la page floutee est claire, et les memes teintes la
/// laissaient grise : sous les plaques, l'encre de leurs textes tombait a
/// 2,60:1 (passage B du tour 00, mesure sur la capture). Le verre reste sombre
/// dans les deux themes, comme Coup d'oeil : la teinte monte pour que les
/// plaques retrouvent le fond qu'elles ont sur la page sombre.
pub const TEINTE_FOND_CLAIR: f32 = 0.95;
pub const TEINTE_PLAQUE_CLAIR: f32 = 0.88;

fn teintes() -> (f32, f32) {
    if crate::palette::clair() {
        (TEINTE_FOND_CLAIR, TEINTE_PLAQUE_CLAIR)
    } else {
        (TEINTE_FOND, TEINTE_PLAQUE)
    }
}
pub const SATURATION: f32 = 1.35;
pub const INDICE: f32 = 1.5168;
/// Le conge et l'epaisseur, rapportes a la hauteur comme dans vitrine.
pub const CONGE_SUR_HAUTEUR: f32 = 12.0 / 148.0;
/// La force de la reflexion a l'echelle 1,25 de ce poste (vitrine : 0,24).
pub const LISERE: f32 = 0.24;
/// Le niveau du chant sous la lampe, sur 255, galet deploye.
pub const NIVEAU_LAMPE: f32 = 85.0;
/// L'exposant de la superellipse des coins : 2 est l'arc de cercle ; au-dela,
/// le coin continu des plaques d'Apple.
pub const EXPOSANT: f32 = 4.0;

pub fn lineaire(octet: f32) -> f32 {
    let c = octet / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// La reflectance de Fresnel non polarisee au point `d` du conge (`d` <= 0,
/// mesure depuis le bord, vers l'interieur). La formule de `bord_en` du
/// nuanceur : la meme ici pour calculer une fois par trame celle du pixel du
/// bord, qui normalise le profil de la lampe.
pub fn reflectance(d: f32, b: f32, n: f32) -> f32 {
    let sinp = (1.0 + d / b).clamp(0.0, 1.0);
    let cosp = (1.0 - sinp * sinp).max(0.0).sqrt();
    let sint = sinp / n;
    let cost = (1.0 - sint * sint).max(0.0).sqrt();
    let rs = (cosp - n * cost) / (cosp + n * cost).max(1e-6);
    let rp = (n * cosp - cost) / (n * cosp + cost).max(1e-6);
    0.5 * (rs * rs + rp * rp)
}

/// Celle du pixel du bord, moyennee sur son empreinte comme dans le nuanceur.
pub fn reflectance_du_bord(b: f32, n: f32) -> f32 {
    (0..8)
        .map(|k| reflectance((-0.5 - 0.5 + (k as f32 + 0.5) / 8.0).min(0.0), b, n))
        .sum::<f32>()
        / 8.0
}

/// Une plaque : sa boite en pixels, son rayon, et son opacite.
#[derive(Clone, Copy, Debug)]
pub struct Plaque {
    pub boite: [f32; 4],
    pub rayon: f32,
    pub opacite: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Uniformes {
    taille: [f32; 4],
    flou: [f32; 4],
    fond: [f32; 4],
    plaque: [f32; 4],
    forme: [f32; 4],
    matiere: [f32; 4],
    lampe: [f32; 4],
}

fn octets<T: Copy>(v: &[T]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }
}

struct Couches {
    taille: (u32, u32),
    /// La vue garde sa texture en vie.
    scene_vue: wgpu::TextureView,
    demi_a: wgpu::TextureView,
    demi_b: wgpu::TextureView,
}

pub struct Verre {
    disposition: wgpu::BindGroupLayout,
    flou: wgpu::RenderPipeline,
    fond: wgpu::RenderPipeline,
    plaque: wgpu::RenderPipeline,
    echantillonneur: wgpu::Sampler,
    format: wgpu::TextureFormat,
    couches: Option<Couches>,
}

/// Les demi-couches du flou, en flottants de 16 bits : un flou large de fonds
/// sombres, stocke sur huit bits, ferait des marches.
const FORMAT_FLOU: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

impl Verre {
    pub fn nouveau(p: &wgpu::Device, format: wgpu::TextureFormat) -> Verre {
        let module = p.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("verre"),
            source: wgpu::ShaderSource::Wgsl(include_str!("verre.wgsl").into()),
        });
        let texture = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let disposition = p.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("verre"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                texture(2),
                texture(3),
            ],
        });
        let agencement = p.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("verre"),
            bind_group_layouts: &[Some(&disposition)],
            immediate_size: 0,
        });
        let tuyau = |entree: &str, cible: wgpu::TextureFormat, melange: Option<wgpu::BlendState>| {
            p.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entree),
                layout: Some(&agencement),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("sommet"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(entree),
                    targets: &[Some(wgpu::ColorTargetState { format: cible, blend: melange, write_mask: wgpu::ColorWrites::ALL })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let flou = tuyau("flou", FORMAT_FLOU, None);
        let fond = tuyau("fond", format, None);
        let plaque = tuyau("plaque", format, Some(wgpu::BlendState::ALPHA_BLENDING));
        let echantillonneur = p.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("verre"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        Verre { disposition, flou, fond, plaque, echantillonneur, format, couches: None }
    }

    /// La texture ou dessiner la vue, a la taille de l'ecran, refaite si la
    /// fenetre a change.
    pub fn scene(&mut self, p: &wgpu::Device, l: u32, h: u32) -> &wgpu::TextureView {
        if self.couches.as_ref().is_none_or(|c| c.taille != (l, h)) {
            let tex = |nom, l: u32, h: u32, format| {
                p.create_texture(&wgpu::TextureDescriptor {
                    label: Some(nom),
                    size: wgpu::Extent3d { width: l.max(1), height: h.max(1), depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
            };
            let scene = tex("scene", l, h, self.format);
            let (dl, dh) = (l.div_ceil(2), h.div_ceil(2));
            let vue = |t: &wgpu::Texture| t.create_view(&wgpu::TextureViewDescriptor::default());
            self.couches = Some(Couches {
                taille: (l, h),
                scene_vue: vue(&scene),
                demi_a: vue(&tex("demi_a", dl, dh, FORMAT_FLOU)),
                demi_b: vue(&tex("demi_b", dl, dh, FORMAT_FLOU)),
            });
        }
        &self.couches.as_ref().expect("posees").scene_vue
    }

    /// La scene posee, si elle l'est.
    pub fn vue_scene(&self) -> Option<&wgpu::TextureView> {
        self.couches.as_ref().map(|c| &c.scene_vue)
    }

    fn groupe(&self, p: &wgpu::Device, u: &Uniformes, couche: &wgpu::TextureView) -> wgpu::BindGroup {
        let c = self.couches.as_ref().expect("posees");
        let tampon = p.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("verre"),
            contents: octets(std::slice::from_ref(u)),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        p.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.disposition,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: tampon.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.echantillonneur) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&c.scene_vue) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(couche) },
            ],
        })
    }

    fn passe(enc: &mut wgpu::CommandEncoder, cible: &wgpu::TextureView, charger: bool) -> wgpu::RenderPass<'static> {
        enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("verre"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: cible,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: if charger { wgpu::LoadOp::Load } else { wgpu::LoadOp::Clear(wgpu::Color::BLACK) },
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            multiview_mask: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        })
        .forget_lifetime()
    }

    /// Le flou de la scene, puis le fond et les plaques dans `cible`, pour une
    /// ouverture `t` et une echelle `e`.
    pub fn rendre(
        &self,
        p: &wgpu::Device,
        enc: &mut wgpu::CommandEncoder,
        cible: &wgpu::TextureView,
        t: f32,
        e: f32,
        plaques: &[Plaque],
    ) {
        let Some(c) = self.couches.as_ref() else { return };
        let (l, h) = (c.taille.0 as f32, c.taille.1 as f32);
        // Le sigma est celui de vitrine, a l'echelle de l'ecran, puis divise
        // par deux : la seconde passe le lit a demi-resolution.
        let sigma = SIGMA * e * 0.5;
        let rayon = (3.0 * sigma).ceil();
        let base = Uniformes {
            taille: [l, h, 0.0, 0.0],
            flou: [sigma, rayon, 1.0, 0.0],
            fond: [t, teintes().0, SATURATION, 2.0],
            plaque: [0.0; 4],
            forme: [0.0; 4],
            matiere: [INDICE, teintes().1, LISERE, 1.0],
            lampe: [0.0; 4],
        };
        // Horizontale : la scene, lue un pixel sur deux, vers la demi-couche A.
        {
            let g = self.groupe(p, &base, &c.scene_vue);
            let mut passe = Verre::passe(enc, &c.demi_a, false);
            passe.set_pipeline(&self.flou);
            passe.set_bind_group(0, &g, &[]);
            passe.draw(0..3, 0..1);
        }
        // Verticale : la demi-couche A, a son pas, vers B.
        {
            let u = Uniformes { flou: [sigma, rayon, 0.0, 1.0], fond: [t, teintes().0, SATURATION, 1.0], ..base };
            let g = self.groupe(p, &u, &c.demi_a);
            let mut passe = Verre::passe(enc, &c.demi_b, false);
            passe.set_pipeline(&self.flou);
            passe.set_bind_group(0, &g, &[]);
            passe.draw(0..3, 0..1);
        }
        // Le fond, puis les plaques, dans la cible.
        let mut passe = Verre::passe(enc, cible, false);
        let g = self.groupe(p, &base, &c.demi_b);
        passe.set_pipeline(&self.fond);
        passe.set_bind_group(0, &g, &[]);
        passe.draw(0..3, 0..1);
        if t <= 0.0 {
            return;
        }
        passe.set_pipeline(&self.plaque);
        let lampe = lineaire(NIVEAU_LAMPE);
        for pl in plaques {
            let [x, y, pl_l, pl_h] = pl.boite;
            let b = (CONGE_SUR_HAUTEUR * pl_h).max(1.0);
            let u = Uniformes {
                plaque: [x + pl_l * 0.5, y + pl_h * 0.5, pl_l * 0.5, pl_h * 0.5],
                forme: [pl.rayon, EXPOSANT, b, b],
                matiere: [INDICE, teintes().1, LISERE, pl.opacite],
                lampe: [lampe, reflectance_du_bord(b, INDICE), 0.0, 0.0],
                ..base
            };
            let g = self.groupe(p, &u, &c.demi_b);
            passe.set_bind_group(0, &g, &[]);
            passe.draw(0..3, 0..1);
        }
    }
}

#[cfg(test)]
mod essais {
    use super::*;

    /// A incidence normale, la reflectance d'un verre d'indice n est
    /// ((n - 1) / (n + 1))^2 : 0,0421 pour le N-BK7. Au bord, en incidence
    /// rasante, elle tend vers 1.
    #[test]
    fn la_reflectance_de_fresnel() {
        let r0 = ((INDICE - 1.0) / (INDICE + 1.0)).powi(2);
        assert!((reflectance(-4.0, 4.0, INDICE) - r0).abs() < 1e-6);
        assert!((r0 - 0.0421).abs() < 1e-4);
        assert!(reflectance(0.0, 4.0, INDICE) > 0.99);
        let bord = reflectance_du_bord(4.0, INDICE);
        assert!(bord > r0 && bord < 1.0);
    }

    /// Le niveau 85 de la lampe, en lineaire : la valeur que vitrine pose.
    #[test]
    fn la_lampe_en_lineaire() {
        assert!((lineaire(85.0) - 0.0908).abs() < 1e-3);
        assert_eq!(lineaire(0.0), 0.0);
        assert!((lineaire(255.0) - 1.0).abs() < 1e-6);
    }
}
