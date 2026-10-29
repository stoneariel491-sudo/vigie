//! Les aplats : filets, rails, pastille. Tout ce qui n'est pas du texte.
//!
//! Un seul tuyau, une seule fonction de forme — le rectangle a coins arrondis
//! par sa distance signee. Un filet d'un pixel est un rectangle de rayon nul ;
//! la pastille est un rectangle carre de rayon egal a sa demi-hauteur. Les
//! traiter separement multiplierait le code pour rien.

use wgpu::util::DeviceExt;

/// Une forme, telle qu'elle part vers la carte. L'ordre et le remplissage
/// suivent l'alignement de WGSL : deux vec4 et rien qui deborde.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Forme {
    /// x, y, largeur, hauteur, en pixels.
    pub boite: [f32; 4],
    /// rouge, vert, bleu, alpha, en 0..1 LINEAIRE.
    pub couleur: [f32; 4],
    /// rayon des coins ; puis le fondu horizontal (0 pour un aplat, +1 ou -1
    /// pour un voile, voir formes.wgsl) ; puis deux reserves pour tenir
    /// l'alignement.
    pub rayon: [f32; 4],
}

/// Les octets d'une tranche. `Forme` est `repr(C)` et n'est faite que de f32
/// contigus, donc sa representation en memoire est exactement ce que la carte
/// attend. Une caisse entiere pour trois lignes n'en vaut pas la peine.
fn octets<T: Copy>(v: &[T]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }
}

pub struct Formes {
    tuyau: wgpu::RenderPipeline,
    disposition: wgpu::BindGroupLayout,
}

impl Formes {
    pub fn nouveau(peripherique: &wgpu::Device, format: wgpu::TextureFormat) -> Formes {
        let module = peripherique.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("formes"),
            source: wgpu::ShaderSource::Wgsl(include_str!("formes.wgsl").into()),
        });
        let disposition =
            peripherique.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("formes"),
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
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });
        let agencement = peripherique.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("formes"),
            bind_group_layouts: &[Some(&disposition)],
            immediate_size: 0,
        });
        let tuyau = peripherique.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("formes"),
            layout: Some(&agencement),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("sommet"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fragment"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    // Alpha droit : les formes se posent sur le fond, jamais
                    // les unes sur les autres.
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        Formes {
            tuyau,
            disposition,
        }
    }

    pub fn dessiner(
        &self,
        peripherique: &wgpu::Device,
        passe: &mut wgpu::RenderPass<'_>,
        ecran: (f32, f32),
        formes: &[Forme],
    ) {
        if formes.is_empty() {
            return;
        }
        let taille = peripherique.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ecran"),
            contents: octets(&[[ecran.0, ecran.1, 0.0, 0.0]]),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let tampon = peripherique.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("formes"),
            contents: octets(formes),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let groupe = peripherique.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.disposition,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: taille.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: tampon.as_entire_binding(),
                },
            ],
        });
        passe.set_pipeline(&self.tuyau);
        passe.set_bind_group(0, &groupe, &[]);
        // Six sommets par forme : deux triangles, aucun tampon de sommets.
        passe.draw(0..6 * formes.len() as u32, 0..1);
    }
}
