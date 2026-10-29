//! Les images : ce que l'agent regarde, pose dans le fil a l'endroit ou il l'a
//! regarde.
//!
//! Un agent qui juge une maquette ouvre sa capture, et la transcription garde
//! l'image entiere. La montrer la, c'est voir ce qu'il voit au moment ou il le
//! pense : sans elle, « le contraste du titre est trop faible » ne se verifie
//! qu'en allant chercher le fichier a la main.
//!
//! Une texture par image, a la taille exacte de son affichage : un texel par
//! pixel, donc aucun flou d'echantillonnage et aucune moire sur le texte d'une
//! capture.

use wgpu::util::DeviceExt;

pub struct Texture {
    pub largeur: u32,
    pub hauteur: u32,
    vue: wgpu::TextureView,
    brute: wgpu::Texture,
}

impl Texture {
    /// Remplace ses pixels, a la meme taille : c'est ainsi qu'une video avance,
    /// une image apres l'autre, sans recreer de texture.
    pub fn ecrire(&self, file: &wgpu::Queue, pixels: &[u8]) {
        if pixels.len() != (4 * self.largeur * self.hauteur) as usize {
            return;
        }
        file.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.brute,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * self.largeur),
                rows_per_image: Some(self.hauteur),
            },
            wgpu::Extent3d { width: self.largeur, height: self.hauteur, depth_or_array_layers: 1 },
        );
    }
}

pub struct Images {
    tuyau: wgpu::RenderPipeline,
    disposition: wgpu::BindGroupLayout,
    echantillonneur: wgpu::Sampler,
}

/// Voir `formes::octets` : des f32 contigus, la representation que la carte
/// attend.
fn octets<T: Copy>(v: &[T]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }
}

impl Images {
    pub fn nouveau(peripherique: &wgpu::Device, format: wgpu::TextureFormat) -> Images {
        let module = peripherique.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("images"),
            source: wgpu::ShaderSource::Wgsl(include_str!("images.wgsl").into()),
        });
        let disposition =
            peripherique.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("images"),
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
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
        let agencement = peripherique.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("images"),
            bind_group_layouts: &[Some(&disposition)],
            immediate_size: 0,
        });
        let tuyau = peripherique.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("images"),
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
        // Lineaire : a taille exacte il ne sert qu'aux bords arrondis, et il
        // evite l'escalier si une texture est posee a une demi-unite pres.
        // Entre deux reductions aussi : l'image pleine de la visionneuse, montree
        // plus petite que sa vraie taille, passe de l'une a l'autre sans saut.
        let echantillonneur = peripherique.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("images"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        Images {
            tuyau,
            disposition,
            echantillonneur,
        }
    }

    /// Envoie des pixels RGBA (sRGB, 8 bits) vers la carte.
    pub fn texture(
        &self,
        peripherique: &wgpu::Device,
        file: &wgpu::Queue,
        pixels: &[u8],
        largeur: u32,
        hauteur: u32,
    ) -> Texture {
        self.texture_reduite(peripherique, file, pixels, largeur, hauteur, &[])
    }

    /// La meme, avec ses reductions, de la moitie a la plus petite : la carte
    /// lit celle qui convient a la taille d'affichage.
    pub fn texture_reduite(
        &self,
        peripherique: &wgpu::Device,
        file: &wgpu::Queue,
        pixels: &[u8],
        largeur: u32,
        hauteur: u32,
        niveaux: &[(u32, u32, Vec<u8>)],
    ) -> Texture {
        let taille = wgpu::Extent3d {
            width: largeur,
            height: hauteur,
            depth_or_array_layers: 1,
        };
        let t = peripherique.create_texture(&wgpu::TextureDescriptor {
            label: Some("image"),
            size: taille,
            mip_level_count: 1 + niveaux.len() as u32,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            // sRGB : la carte rend des valeurs lineaires a l'echantillonnage,
            // et la surface, sRGB elle aussi, les reconvertit. Les couleurs
            // de la capture ressortent donc telles qu'elles sont.
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        file.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &t,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * largeur),
                rows_per_image: Some(hauteur),
            },
            taille,
        );
        for (i, (l, h, p)) in niveaux.iter().enumerate() {
            file.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &t,
                    mip_level: 1 + i as u32,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                p,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * l),
                    rows_per_image: Some(*h),
                },
                wgpu::Extent3d { width: *l, height: *h, depth_or_array_layers: 1 },
            );
        }
        Texture {
            largeur,
            hauteur,
            vue: t.create_view(&wgpu::TextureViewDescriptor::default()),
            brute: t,
        }
    }

    /// Dessine chaque image dans sa boite. Rarement plus de deux ou trois a la
    /// fois a l'ecran : un groupe de liaison par image et par trame ne coute
    /// rien a cette echelle.
    pub fn dessiner(
        &self,
        peripherique: &wgpu::Device,
        passe: &mut wgpu::RenderPass<'_>,
        ecran: (f32, f32),
        liste: &[(&Texture, [f32; 4], f32)],
    ) {
        if liste.is_empty() {
            return;
        }
        passe.set_pipeline(&self.tuyau);
        for (texture, boite, rayon) in liste {
            let cadre = [[ecran.0, ecran.1, 0.0, 0.0], *boite, [*rayon, 0.0, 0.0, 0.0]];
            let uniforme = peripherique.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("cadre"),
                contents: octets(&cadre),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let groupe = peripherique.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.disposition,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniforme.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&texture.vue),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.echantillonneur),
                    },
                ],
            });
            passe.set_bind_group(0, &groupe, &[]);
            passe.draw(0..6, 0..1);
        }
    }
}
