//! Texture helpers.

use image::GenericImageView as _;
use wgpu::util::DeviceExt;

/// Loads a texture from an image.
pub fn load(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    img: &image::DynamicImage,
) -> wgpu::Texture {
    let (width, height) = img.dimensions();

    device.create_texture_with_data(
        queue,
        &wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::default(),
        &img.to_rgba8(),
    )
}
