use glam::*;
use itertools::Itertools as _;

/// A single sprite.
pub struct Sprite<'a> {
    /// Texture to draw with.
    pub texture: &'a wgpu::Texture,

    /// Source offset from the texture.
    pub src_offset: IVec2,

    /// Source size.
    pub src_size: UVec2,

    /// Source layer.
    pub src_layer: u32,

    /// Target transform.
    pub transform: Affine2,

    /// Tint.
    pub tint: crate::Color,
}

/// Batches a flat list of [`Sprite`]s into groups with textures.
pub fn batch<'a>(sprites: &'a [Sprite]) -> Vec<crate::Group<'a>> {
    sprites
        .iter()
        .chunk_by(|s| s.texture)
        .into_iter()
        .map(|(_, chunk)| {
            let chunk = chunk.collect::<Vec<_>>();
            crate::Group {
                texture: chunk.first().unwrap().texture,
                items: chunk
                    .into_iter()
                    .map(|s| crate::Item {
                        src_offset: s.src_offset,
                        src_size: s.src_size,
                        src_layer: s.src_layer,
                        transform: s.transform,
                        tint: s.tint,
                    })
                    .collect::<Vec<_>>(),
            }
        })
        .collect::<Vec<_>>()
}
