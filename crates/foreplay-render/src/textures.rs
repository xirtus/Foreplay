//! Texture pipeline: Multi-format decoding (PNG, JPEG, WebP, BMP, TGA, HDR),
//! CPU box & Lanczos mipmap generation, and real `wgpu` texture/sampler upload.

use thiserror::Error;

/// Texture loading and decoding errors.
#[derive(Debug, Error)]
pub enum TextureError {
    #[error("Image decode error: {0}")]
    ImageDecode(#[from] image::ImageError),

    #[error("Invalid dimensions: {0}x{1}")]
    InvalidDimensions(u32, u32),

    #[error("Empty image data")]
    EmptyData,

    #[error("Unsupported texture format: {0}")]
    UnsupportedFormat(String),

    #[error("Device upload error: {0}")]
    DeviceUpload(String),
}

/// Downsampling filter used for CPU mipmap generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MipFilter {
    /// 2x2 box area averaging (fastest, standard for linear textures).
    #[default]
    Box,
    /// High-quality Lanczos3 sinc reconstruction filter.
    Lanczos,
    /// Triangle / bilinear reconstruction filter.
    Bilinear,
    /// Nearest neighbor point sampling.
    Nearest,
}

/// In-memory decoded RGBA8 texture with complete mipmap pyramid.
#[derive(Clone, PartialEq, Debug)]
pub struct DecodedTexture {
    pub width: u32,
    pub height: u32,
    pub format: wgpu::TextureFormat,
    pub mip_levels: Vec<Vec<u8>>,
    pub has_alpha: bool,
}

impl DecodedTexture {
    /// Construct a decoded texture with a single mip level (base level).
    #[must_use]
    pub fn new(width: u32, height: u32, rgba8: Vec<u8>, format: wgpu::TextureFormat) -> Self {
        let has_alpha = rgba8.chunks_exact(4).any(|c| c[3] < 255);
        Self {
            width,
            height,
            format,
            mip_levels: vec![rgba8],
            has_alpha,
        }
    }

    /// Number of mip levels currently present.
    #[must_use]
    pub fn mip_level_count(&self) -> u32 {
        self.mip_levels.len() as u32
    }

    /// Base (level 0) tightly-packed RGBA8 pixel data.
    #[must_use]
    pub fn base_rgba8(&self) -> &[u8] {
        self.mip_levels.first().map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Read a pixel `(x, y)` at mip level 0. Returns `[r, g, b, a]`.
    #[must_use]
    pub fn get_pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let offset = ((y * self.width + x) * 4) as usize;
        let base = self.base_rgba8();
        if offset + 4 <= base.len() {
            Some([base[offset], base[offset + 1], base[offset + 2], base[offset + 3]])
        } else {
            None
        }
    }
}

/// GPU-resident texture resource paired with its default view and sampler.
#[derive(Debug)]
pub struct GpuTexture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pub width: u32,
    pub height: u32,
    pub mip_level_count: u32,
    pub format: wgpu::TextureFormat,
}

/// Asset loader for multi-format decoding, mipmap building, and GPU texture staging.
#[derive(Debug, Default)]
pub struct TextureAssetLoader;

impl TextureAssetLoader {
    /// Decode arbitrary image bytes (PNG, JPEG, WebP, BMP, TGA, HDR) into a `DecodedTexture`.
    ///
    /// If `generate_mips` is true, generates a complete mip pyramid down to 1x1 using `filter`.
    pub fn load_from_memory(
        bytes: &[u8],
        generate_mips: bool,
        filter: MipFilter,
    ) -> Result<DecodedTexture, TextureError> {
        if bytes.is_empty() {
            return Err(TextureError::EmptyData);
        }

        let dyn_img = match image::load_from_memory(bytes) {
            Ok(img) => img,
            Err(_) => {
                // TGA headers lack magic bytes; attempt explicit TGA decode
                image::load_from_memory_with_format(bytes, image::ImageFormat::Tga)?
            }
        };
        let width = dyn_img.width();
        let height = dyn_img.height();

        if width == 0 || height == 0 {
            return Err(TextureError::InvalidDimensions(width, height));
        }

        let rgba = dyn_img.to_rgba8();
        let raw_pixels = rgba.into_raw();
        let has_alpha = raw_pixels.chunks_exact(4).any(|c| c[3] < 255);

        let mip_levels = if generate_mips {
            Self::generate_mipmaps(&raw_pixels, width, height, filter)
        } else {
            vec![raw_pixels]
        };

        Ok(DecodedTexture {
            width,
            height,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            mip_levels,
            has_alpha,
        })
    }

    /// Compute a complete chain of mipmaps down to 1x1 from base RGBA8 pixel data.
    #[must_use]
    pub fn generate_mipmaps(
        base_rgba8: &[u8],
        width: u32,
        height: u32,
        filter: MipFilter,
    ) -> Vec<Vec<u8>> {
        let mut mips = Vec::new();
        mips.push(base_rgba8.to_vec());

        let mut curr_w = width;
        let mut curr_h = height;
        let mut curr_data = base_rgba8.to_vec();

        while curr_w > 1 || curr_h > 1 {
            let next_w = (curr_w / 2).max(1);
            let next_h = (curr_h / 2).max(1);

            let next_data = match filter {
                MipFilter::Box => Self::downsample_box_2x2(&curr_data, curr_w, curr_h, next_w, next_h),
                MipFilter::Nearest => {
                    Self::downsample_nearest(&curr_data, curr_w, curr_h, next_w, next_h)
                }
                MipFilter::Lanczos => {
                    Self::downsample_imageops(&curr_data, curr_w, curr_h, next_w, next_h, image::imageops::FilterType::Lanczos3)
                }
                MipFilter::Bilinear => {
                    Self::downsample_imageops(&curr_data, curr_w, curr_h, next_w, next_h, image::imageops::FilterType::Triangle)
                }
            };

            mips.push(next_data.clone());
            curr_data = next_data;
            curr_w = next_w;
            curr_h = next_h;
        }

        mips
    }

    /// Fast 2x2 box filter area averaging.
    fn downsample_box_2x2(
        src: &[u8],
        src_w: u32,
        src_h: u32,
        dst_w: u32,
        dst_h: u32,
    ) -> Vec<u8> {
        let mut dst = vec![0u8; (dst_w * dst_h * 4) as usize];

        for dy in 0..dst_h {
            for dx in 0..dst_w {
                let sx0 = (dx * 2).min(src_w - 1);
                let sx1 = (dx * 2 + 1).min(src_w - 1);
                let sy0 = (dy * 2).min(src_h - 1);
                let sy1 = (dy * 2 + 1).min(src_h - 1);

                let p00 = Self::sample_pixel(src, src_w, sx0, sy0);
                let p10 = Self::sample_pixel(src, src_w, sx1, sy0);
                let p01 = Self::sample_pixel(src, src_w, sx0, sy1);
                let p11 = Self::sample_pixel(src, src_w, sx1, sy1);

                let r = ((p00[0] as u32 + p10[0] as u32 + p01[0] as u32 + p11[0] as u32 + 2) / 4) as u8;
                let g = ((p00[1] as u32 + p10[1] as u32 + p01[1] as u32 + p11[1] as u32 + 2) / 4) as u8;
                let b = ((p00[2] as u32 + p10[2] as u32 + p01[2] as u32 + p11[2] as u32 + 2) / 4) as u8;
                let a = ((p00[3] as u32 + p10[3] as u32 + p01[3] as u32 + p11[3] as u32 + 2) / 4) as u8;

                let dst_offset = ((dy * dst_w + dx) * 4) as usize;
                dst[dst_offset..dst_offset + 4].copy_from_slice(&[r, g, b, a]);
            }
        }

        dst
    }

    /// Nearest-neighbor downsampling.
    fn downsample_nearest(
        src: &[u8],
        src_w: u32,
        _src_h: u32,
        dst_w: u32,
        dst_h: u32,
    ) -> Vec<u8> {
        let mut dst = vec![0u8; (dst_w * dst_h * 4) as usize];
        for dy in 0..dst_h {
            for dx in 0..dst_w {
                let sx = dx * 2;
                let sy = dy * 2;
                let pixel = Self::sample_pixel(src, src_w, sx, sy);
                let dst_offset = ((dy * dst_w + dx) * 4) as usize;
                dst[dst_offset..dst_offset + 4].copy_from_slice(&pixel);
            }
        }
        dst
    }

    /// Downsampling using `image` crate filter types (Lanczos3, Triangle).
    fn downsample_imageops(
        src: &[u8],
        src_w: u32,
        src_h: u32,
        dst_w: u32,
        dst_h: u32,
        filter: image::imageops::FilterType,
    ) -> Vec<u8> {
        if let Some(img) = image::RgbaImage::from_raw(src_w, src_h, src.to_vec()) {
            let resized = image::imageops::resize(&img, dst_w, dst_h, filter);
            resized.into_raw()
        } else {
            Self::downsample_box_2x2(src, src_w, src_h, dst_w, dst_h)
        }
    }

    #[inline]
    fn sample_pixel(src: &[u8], stride_w: u32, x: u32, y: u32) -> [u8; 4] {
        let offset = ((y * stride_w + x) * 4) as usize;
        if offset + 4 <= src.len() {
            [src[offset], src[offset + 1], src[offset + 2], src[offset + 3]]
        } else {
            [0, 0, 0, 0]
        }
    }

    /// Upload a `DecodedTexture` to a real hardware `wgpu::Device`, populating all mip levels
    /// and instantiating a default trilinear clamp-to-edge `wgpu::Sampler`.
    pub fn upload_to_wgpu(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        decoded: &DecodedTexture,
        label: Option<&str>,
    ) -> GpuTexture {
        let mip_level_count = decoded.mip_levels.len().max(1) as u32;

        let texture_desc = wgpu::TextureDescriptor {
            label,
            size: wgpu::Extent3d {
                width: decoded.width,
                height: decoded.height,
                depth_or_array_layers: 1,
            },
            mip_level_count,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: decoded.format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        };

        let texture = device.create_texture(&texture_desc);

        // Upload each mip level
        let mut w = decoded.width;
        let mut h = decoded.height;
        for (mip_idx, mip_data) in decoded.mip_levels.iter().enumerate() {
            let bytes_per_row = w * 4;
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: mip_idx as u32,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                mip_data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: Some(h),
                },
                wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
            );

            w = (w / 2).max(1);
            h = (h / 2).max(1);
        }

        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: label.map(|l| format!("{l}_view")).as_deref(),
            format: Some(decoded.format),
            dimension: Some(wgpu::TextureViewDimension::D2),
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            mip_level_count: Some(mip_level_count),
            base_array_layer: 0,
            array_layer_count: Some(1),
            usage: None,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: label.map(|l| format!("{l}_sampler")).as_deref(),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            lod_min_clamp: 0.0,
            lod_max_clamp: mip_level_count as f32,
            compare: None,
            anisotropy_clamp: 1,
            border_color: None,
        });

        GpuTexture {
            texture,
            view,
            sampler,
            width: decoded.width,
            height: decoded.height,
            mip_level_count,
            format: decoded.format,
        }
    }

    /// Generate an in-memory PNG encoded byte buffer of solid color.
    pub fn create_solid_png(
        width: u32,
        height: u32,
        color: [u8; 4],
    ) -> Result<Vec<u8>, TextureError> {
        let count = (width * height) as usize;
        let mut pixels = Vec::with_capacity(count * 4);
        for _ in 0..count {
            pixels.extend_from_slice(&color);
        }

        let mut png_bytes = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(&mut png_bytes);
        image::ImageEncoder::write_image(
            encoder,
            &pixels,
            width,
            height,
            image::ExtendedColorType::Rgba8,
        )?;

        Ok(png_bytes)
    }

    /// Generate an in-memory PNG encoded byte buffer with an alternating 2D checkerboard pattern.
    pub fn create_checkerboard_png(
        width: u32,
        height: u32,
        cell_size: u32,
        color_a: [u8; 4],
        color_b: [u8; 4],
    ) -> Result<Vec<u8>, TextureError> {
        let cell = cell_size.max(1);
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);

        for y in 0..height {
            for x in 0..width {
                let cx = x / cell;
                let cy = y / cell;
                if (cx + cy).is_multiple_of(2) {
                    pixels.extend_from_slice(&color_a);
                } else {
                    pixels.extend_from_slice(&color_b);
                }
            }
        }

        let mut png_bytes = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(&mut png_bytes);
        image::ImageEncoder::write_image(
            encoder,
            &pixels,
            width,
            height,
            image::ExtendedColorType::Rgba8,
        )?;

        Ok(png_bytes)
    }

    /// Generate an in-memory PNG encoded byte buffer of a continuous horizontal/vertical color gradient.
    pub fn create_gradient_png(width: u32, height: u32) -> Result<Vec<u8>, TextureError> {
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);

        for y in 0..height {
            let v = (y as f32) / (height.max(1) as f32);
            for x in 0..width {
                let u = (x as f32) / (width.max(1) as f32);
                let r = (u * 255.0).round() as u8;
                let g = (v * 255.0).round() as u8;
                let b = ((1.0 - u) * 255.0).round() as u8;
                pixels.extend_from_slice(&[r, g, b, 255]);
            }
        }

        let mut png_bytes = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(&mut png_bytes);
        image::ImageEncoder::write_image(
            encoder,
            &pixels,
            width,
            height,
            image::ExtendedColorType::Rgba8,
        )?;

        Ok(png_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_procedural_generation_and_decode() {
        let png = TextureAssetLoader::create_checkerboard_png(
            64,
            64,
            8,
            [255, 0, 0, 255],
            [0, 255, 0, 255],
        )
        .expect("checkerboard png generated");

        let decoded = TextureAssetLoader::load_from_memory(&png, true, MipFilter::Box)
            .expect("must decode valid png");

        assert_eq!(decoded.width, 64);
        assert_eq!(decoded.height, 64);
        assert_eq!(decoded.mip_level_count(), 7); // 64, 32, 16, 8, 4, 2, 1

        // Check top-left pixel (cell 0,0 is color_a)
        let p0 = decoded.get_pixel(0, 0).expect("pixel");
        assert_eq!(p0, [255, 0, 0, 255]);

        // Check cell (1, 0) which is x=8, y=0 (color_b)
        let p1 = decoded.get_pixel(8, 0).expect("pixel");
        assert_eq!(p1, [0, 255, 0, 255]);
    }

    #[test]
    fn mipmap_chain_dimensions_and_downsampling() {
        let base = vec![200u8; 16 * 8 * 4];
        let mips = TextureAssetLoader::generate_mipmaps(&base, 16, 8, MipFilter::Box);

        // 16x8 -> 8x4 -> 4x2 -> 2x1 -> 1x1 = 5 levels
        assert_eq!(mips.len(), 5);
        assert_eq!(mips[0].len(), 16 * 8 * 4);
        assert_eq!(mips[1].len(), 8 * 4 * 4);
        assert_eq!(mips[2].len(), 4 * 2 * 4);
        assert_eq!(mips[3].len(), 2 * 4);
        assert_eq!(mips[4].len(), 4);

        // Constant input should remain constant through box filtering
        assert_eq!(mips[4], vec![200, 200, 200, 200]);
    }

    #[test]
    fn multi_format_decoding_png_jpeg_bmp_tga_webp_hdr() {
        // 1. BMP
        let mut bmp_bytes = Vec::new();
        let bmp_encoder = image::codecs::bmp::BmpEncoder::new(&mut bmp_bytes);
        image::ImageEncoder::write_image(
            bmp_encoder,
            &[255, 128, 0, 255, 0, 255, 128, 255, 128, 0, 255, 255, 255, 255, 255, 255],
            2,
            2,
            image::ExtendedColorType::Rgba8,
        )
        .expect("bmp encode");
        let dec_bmp = TextureAssetLoader::load_from_memory(&bmp_bytes, false, MipFilter::Box)
            .expect("bmp decode");
        assert_eq!(dec_bmp.width, 2);
        assert_eq!(dec_bmp.height, 2);

        // 2. JPEG (RGB8)
        let mut jpeg_bytes = Vec::new();
        let jpeg_encoder = image::codecs::jpeg::JpegEncoder::new(&mut jpeg_bytes);
        image::ImageEncoder::write_image(
            jpeg_encoder,
            &[255, 100, 50, 50, 100, 255, 100, 255, 50, 255, 255, 255],
            2,
            2,
            image::ExtendedColorType::Rgb8,
        )
        .expect("jpeg encode");
        let dec_jpeg = TextureAssetLoader::load_from_memory(&jpeg_bytes, false, MipFilter::Box)
            .expect("jpeg decode");
        assert_eq!(dec_jpeg.width, 2);
        assert_eq!(dec_jpeg.height, 2);

        // 3. TGA
        let mut tga_bytes = Vec::new();
        let tga_encoder = image::codecs::tga::TgaEncoder::new(&mut tga_bytes);
        image::ImageEncoder::write_image(
            tga_encoder,
            &[10, 20, 30, 255, 40, 50, 60, 255, 70, 80, 90, 255, 100, 110, 120, 255],
            2,
            2,
            image::ExtendedColorType::Rgba8,
        )
        .expect("tga encode");
        let dec_tga = TextureAssetLoader::load_from_memory(&tga_bytes, false, MipFilter::Box)
            .expect("tga decode");
        assert_eq!(dec_tga.width, 2);
        assert_eq!(dec_tga.height, 2);

        // 4. WebP
        let mut webp_bytes = Vec::new();
        let webp_encoder = image::codecs::webp::WebPEncoder::new_lossless(&mut webp_bytes);
        image::ImageEncoder::write_image(
            webp_encoder,
            &[10, 20, 30, 255, 40, 50, 60, 255, 70, 80, 90, 255, 100, 110, 120, 255],
            2,
            2,
            image::ExtendedColorType::Rgba8,
        )
        .expect("webp encode");
        let dec_webp = TextureAssetLoader::load_from_memory(&webp_bytes, false, MipFilter::Box)
            .expect("webp decode");
        assert_eq!(dec_webp.width, 2);
        assert_eq!(dec_webp.height, 2);

        // 5. Radiance HDR
        let mut hdr_raw = Vec::new();
        hdr_raw.extend_from_slice(b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n-Y 2 +X 2\n");
        // 4 pixels uncompressed RGBE: [R, G, B, E]
        // E = 129 -> 2^(129-128)/256 = 2/256
        for _ in 0..4 {
            hdr_raw.extend_from_slice(&[128, 64, 32, 129]);
        }
        let dec_hdr = TextureAssetLoader::load_from_memory(&hdr_raw, false, MipFilter::Box)
            .expect("hdr decode");
        assert_eq!(dec_hdr.width, 2);
        assert_eq!(dec_hdr.height, 2);
    }

    #[test]
    fn texture_upload_to_hardware_wgpu() {
        let device = crate::WgpuRenderDevice::new_or_fallback();
        if let (Some(wgpu_dev), Some(wgpu_queue)) = (device.device(), device.queue()) {
            let png = TextureAssetLoader::create_solid_png(64, 64, [50, 100, 150, 255])
                .expect("solid png");
            let decoded = TextureAssetLoader::load_from_memory(&png, true, MipFilter::Box)
                .expect("decode");

            let gpu_tex = TextureAssetLoader::upload_to_wgpu(
                wgpu_dev,
                wgpu_queue,
                &decoded,
                Some("test_solid_texture"),
            );

            assert_eq!(gpu_tex.width, 64);
            assert_eq!(gpu_tex.height, 64);
            assert_eq!(gpu_tex.mip_level_count, 7);
        }
    }

    #[test]
    fn box_downsampling_odd_dimensions() {
        let base = vec![128u8; 15 * 7 * 4];
        let mips = TextureAssetLoader::generate_mipmaps(&base, 15, 7, MipFilter::Box);
        // 15x7 -> 7x3 -> 3x1 -> 1x1
        assert_eq!(mips.len(), 4);
        assert_eq!(mips[0].len(), 15 * 7 * 4);
        assert_eq!(mips[1].len(), 7 * 3 * 4);
        assert_eq!(mips[2].len(), 3 * 4);
        assert_eq!(mips[3].len(), 4);
    }

    #[test]
    fn empty_image_error_handling() {
        let res = TextureAssetLoader::load_from_memory(&[], false, MipFilter::Box);
        assert!(matches!(res, Err(TextureError::EmptyData)));
    }

    #[test]
    fn solid_png_generation_and_verification() {
        let color = [42, 84, 126, 200];
        let bytes = TextureAssetLoader::create_solid_png(8, 8, color).expect("generate solid");
        let decoded = TextureAssetLoader::load_from_memory(&bytes, false, MipFilter::Nearest)
            .expect("decode solid");
        assert_eq!(decoded.width, 8);
        assert_eq!(decoded.height, 8);
        assert_eq!(decoded.get_pixel(4, 4), Some(color));
        assert!(decoded.has_alpha);
    }

    #[test]
    fn gradient_png_generation_and_verification() {
        let bytes = TextureAssetLoader::create_gradient_png(16, 16).expect("gradient png");
        let decoded = TextureAssetLoader::load_from_memory(&bytes, false, MipFilter::Box)
            .expect("decode gradient");
        assert_eq!(decoded.width, 16);
        assert_eq!(decoded.height, 16);
        // Top-left: u=0, v=0 -> r=0, g=0, b=255
        let p_tl = decoded.get_pixel(0, 0).expect("pixel tl");
        assert_eq!(p_tl[0], 0);
        assert_eq!(p_tl[1], 0);
        assert_eq!(p_tl[2], 255);
    }
}
