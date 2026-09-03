//! Pictures the 2D pass can draw: RGBA images uploaded once, referenced
//! by a small [`ImageHandle`] the UI carries around. Textures are sRGB, so
//! sampling yields linear light like everything else in the pass.

use lntrn_image::Image;

use crate::gpu::Gpu;

/// Which image; `0` is reserved for "none".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageId(pub u32);

/// An uploaded image and its size in pixels. Cheap to copy; the UI keeps
/// these, the [`Images`] store keeps the textures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageHandle {
    pub id: ImageId,
    pub width: u32,
    pub height: u32,
}

impl ImageHandle {
    pub fn aspect(&self) -> f64 {
        self.width as f64 / self.height.max(1) as f64
    }
}

struct Slot {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
}

/// Every image the pass can bind, plus a white 1×1 fallback bound when a
/// run of vertices draws no image at all.
pub struct Images {
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    slots: Vec<Option<Slot>>,
    fallback: wgpu::BindGroup,
}

impl Images {
    pub fn new(gpu: &Gpu) -> Self {
        let layout = gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("lntrn image bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            ],
        });
        let sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("lntrn image sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let white = Image::solid(1, 1, [255, 255, 255, 255]);
        let (texture, _) = upload(gpu, &white);
        let fallback = bind(gpu, &layout, &sampler, &texture);
        Self { layout, sampler, slots: vec![None], fallback }
    }

    pub fn layout(&self) -> &wgpu::BindGroupLayout {
        &self.layout
    }

    /// Upload `image` (with mipmaps, so it shrinks cleanly).
    pub fn add(&mut self, gpu: &Gpu, image: &Image) -> ImageHandle {
        let (texture, _) = upload(gpu, image);
        let bind_group = bind(gpu, &self.layout, &self.sampler, &texture);
        let slot = Slot { texture, bind_group, width: image.width, height: image.height };
        let id = match self.slots.iter().skip(1).position(Option::is_none) {
            Some(i) => {
                self.slots[i + 1] = Some(slot);
                i as u32 + 1
            }
            None => {
                self.slots.push(Some(slot));
                self.slots.len() as u32 - 1
            }
        };
        ImageHandle { id: ImageId(id), width: image.width, height: image.height }
    }

    /// Put new pixels behind an existing handle. The handle's size stays
    /// what it was unless the image's size changed, in which case a new
    /// handle comes back (the old id keeps working).
    pub fn replace(&mut self, gpu: &Gpu, handle: ImageHandle, image: &Image) -> ImageHandle {
        let i = handle.id.0 as usize;
        let same_size = self.slots.get(i).and_then(Option::as_ref).is_some_and(|s| s.width == image.width && s.height == image.height);
        if same_size && let Some(slot) = self.slots[i].as_mut() {
            write_pixels(gpu, &slot.texture, image);
            return handle;
        }
        let (texture, _) = upload(gpu, image);
        let bind_group = bind(gpu, &self.layout, &self.sampler, &texture);
        let slot = Slot { texture, bind_group, width: image.width, height: image.height };
        if i < self.slots.len() && i > 0 {
            self.slots[i] = Some(slot);
            ImageHandle { id: handle.id, width: image.width, height: image.height }
        } else {
            self.add(gpu, image)
        }
    }

    pub fn remove(&mut self, id: ImageId) {
        if let Some(s) = self.slots.get_mut(id.0 as usize) {
            *s = None;
        }
    }

    pub fn handle(&self, id: ImageId) -> Option<ImageHandle> {
        self.slots.get(id.0 as usize).and_then(Option::as_ref).map(|s| ImageHandle { id, width: s.width, height: s.height })
    }

    /// The bind group for `id`, or the white fallback for `None` and
    /// removed images.
    pub fn bind_group(&self, id: Option<ImageId>) -> &wgpu::BindGroup {
        id.and_then(|id| self.slots.get(id.0 as usize)).and_then(Option::as_ref).map_or(&self.fallback, |s| &s.bind_group)
    }
}

fn mip_levels(w: u32, h: u32) -> u32 {
    32 - w.max(h).max(1).leading_zeros()
}

fn upload(gpu: &Gpu, image: &Image) -> (wgpu::Texture, wgpu::TextureView) {
    let levels = mip_levels(image.width, image.height);
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("lntrn image"),
        size: wgpu::Extent3d { width: image.width.max(1), height: image.height.max(1), depth_or_array_layers: 1 },
        mip_level_count: levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    write_pixels(gpu, &texture, image);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn write_pixels(gpu: &Gpu, texture: &wgpu::Texture, image: &Image) {
    for (level, mip) in image.mip_chain().iter().enumerate() {
        if level as u32 >= texture.mip_level_count() {
            break;
        }
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture, mip_level: level as u32, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            &mip.rgba,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(mip.width * 4), rows_per_image: Some(mip.height) },
            wgpu::Extent3d { width: mip.width, height: mip.height, depth_or_array_layers: 1 },
        );
    }
}

fn bind(gpu: &Gpu, layout: &wgpu::BindGroupLayout, sampler: &wgpu::Sampler, texture: &wgpu::Texture) -> wgpu::BindGroup {
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("lntrn image bind group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mip_counts() {
        assert_eq!(mip_levels(1, 1), 1);
        assert_eq!(mip_levels(2, 1), 2);
        assert_eq!(mip_levels(256, 128), 9);
        assert_eq!(mip_levels(300, 200), 9);
        assert_eq!(ImageHandle { id: ImageId(1), width: 200, height: 100 }.aspect(), 2.0);
    }
}
