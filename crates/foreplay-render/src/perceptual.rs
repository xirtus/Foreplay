//! 3JSE Vision-in-the-Loop Perceptual Verification Gate.
//!
//! Provides deterministic Structural Similarity Index (SSIM) and Peak Signal-to-Noise Ratio (PSNR)
//! computation over RGBA8 viewports and golden-frame render buffers.

/// Compute Peak Signal-to-Noise Ratio (PSNR) in decibels between two RGBA8 images.
///
/// Returns `f64::INFINITY` when images are bit-identical, or a positive decibel value
/// ($10 \cdot \log_{10}(255^2 / \text{MSE})$).
#[must_use]
pub fn compute_psnr_rgba8(img_a: &[u8], img_b: &[u8], width: u32, height: u32) -> f64 {
    let pixel_count = (width as usize) * (height as usize);
    let expected_len = pixel_count * 4;
    if img_a.len() < expected_len || img_b.len() < expected_len || pixel_count == 0 {
        return 0.0;
    }

    let mut sum_sq_err = 0.0_f64;
    // Compare RGB channels (ignore alpha or include if needed; RGB is standard for visual PSNR)
    let sample_count = (pixel_count * 3) as f64;

    for i in 0..pixel_count {
        let base = i * 4;
        let dr = img_a[base] as f64 - img_b[base] as f64;
        let dg = img_a[base + 1] as f64 - img_b[base + 1] as f64;
        let db = img_a[base + 2] as f64 - img_b[base + 2] as f64;
        sum_sq_err += dr * dr + dg * dg + db * db;
    }

    let mse = sum_sq_err / sample_count;
    if mse <= 1e-12 {
        f64::INFINITY
    } else {
        10.0 * ((255.0 * 255.0) / mse).log10()
    }
}

/// Compute Mean Structural Similarity Index Measure (SSIM) between two RGBA8 images.
///
/// Uses standard 8x8 block decomposition with ITU-R BT.709 luminance weighting:
/// $Y = 0.2126 R + 0.7152 G + 0.0722 B$.
///
/// Returns $1.0$ for identical images, with values close to $1.0$ denoting high perceptual similarity.
#[must_use]
pub fn compute_ssim_rgba8(img_a: &[u8], img_b: &[u8], width: u32, height: u32) -> f64 {
    let w = width as usize;
    let h = height as usize;
    let expected_len = w * h * 4;
    if img_a.len() < expected_len || img_b.len() < expected_len || w == 0 || h == 0 {
        return 0.0;
    }

    // Convert RGBA8 to luminance float buffer
    let mut lum_a = vec![0.0_f64; w * h];
    let mut lum_b = vec![0.0_f64; w * h];

    for y in 0..h {
        for x in 0..w {
            let idx = (y * w + x) * 4;
            let ya = 0.2126 * (img_a[idx] as f64)
                + 0.7152 * (img_a[idx + 1] as f64)
                + 0.0722 * (img_a[idx + 2] as f64);
            let yb = 0.2126 * (img_b[idx] as f64)
                + 0.7152 * (img_b[idx + 1] as f64)
                + 0.0722 * (img_b[idx + 2] as f64);
            lum_a[y * w + x] = ya;
            lum_b[y * w + x] = yb;
        }
    }

    // SSIM stability constants for 8-bit dynamic range L = 255
    let c1 = (0.01 * 255.0) * (0.01 * 255.0); // 6.5025
    let c2 = (0.03 * 255.0) * (0.03 * 255.0); // 58.5225

    let block_size = 8_usize;
    let mut total_ssim = 0.0_f64;
    let mut block_count = 0_usize;

    let step = 4_usize; // Strided window for dense sampling
    let max_y = h.saturating_sub(block_size);
    let max_x = w.saturating_sub(block_size);

    if h < block_size || w < block_size {
        // Fallback for sub-8x8 images: single block
        return compute_single_window_ssim(
            &lum_a, &lum_b, w, SsimWindow { x0: 0, y0: 0, bw: w, bh: h, c1, c2 },
        );
    }

    let mut by = 0;
    while by <= max_y {
        let mut bx = 0;
        while bx <= max_x {
            let ssim = compute_single_window_ssim(
                &lum_a, &lum_b, w, SsimWindow { x0: bx, y0: by, bw: block_size, bh: block_size, c1, c2 },
            );
            total_ssim += ssim;
            block_count += 1;
            bx += step;
        }
        by += step;
    }

    if block_count > 0 {
        (total_ssim / block_count as f64).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

#[derive(Clone, Copy, Debug)]
struct SsimWindow {
    x0: usize,
    y0: usize,
    bw: usize,
    bh: usize,
    c1: f64,
    c2: f64,
}

fn compute_single_window_ssim(
    lum_a: &[f64],
    lum_b: &[f64],
    stride: usize,
    win: SsimWindow,
) -> f64 {
    let n = (win.bw * win.bh) as f64;
    let mut mean_a = 0.0_f64;
    let mut mean_b = 0.0_f64;

    for y in 0..win.bh {
        for x in 0..win.bw {
            let idx = (win.y0 + y) * stride + (win.x0 + x);
            mean_a += lum_a[idx];
            mean_b += lum_b[idx];
        }
    }
    mean_a /= n;
    mean_b /= n;

    let mut var_a = 0.0_f64;
    let mut var_b = 0.0_f64;
    let mut covar_ab = 0.0_f64;

    for y in 0..win.bh {
        for x in 0..win.bw {
            let idx = (win.y0 + y) * stride + (win.x0 + x);
            let da = lum_a[idx] - mean_a;
            let db = lum_b[idx] - mean_b;
            var_a += da * da;
            var_b += db * db;
            covar_ab += da * db;
        }
    }
    var_a /= n;
    var_b /= n;
    covar_ab /= n;

    let numerator = (2.0 * mean_a * mean_b + win.c1) * (2.0 * covar_ab + win.c2);
    let denominator = (mean_a * mean_a + mean_b * mean_b + win.c1) * (var_a + var_b + win.c2);

    if denominator.abs() > 1e-12 {
        numerator / denominator
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssim_and_psnr_self_identity() {
        let width = 64;
        let height = 64;
        let mut img = vec![0_u8; (width * height * 4) as usize];
        for (i, p) in img.chunks_exact_mut(4).enumerate() {
            let x = (i % width as usize) as u8;
            let y = (i / width as usize) as u8;
            p[0] = x.wrapping_mul(4);
            p[1] = y.wrapping_mul(4);
            p[2] = (x ^ y).wrapping_mul(4);
            p[3] = 255;
        }

        let ssim = compute_ssim_rgba8(&img, &img, width, height);
        assert!((ssim - 1.0).abs() < 1e-6, "SSIM of image with itself must be 1.0, got {ssim}");

        let psnr = compute_psnr_rgba8(&img, &img, width, height);
        assert!(psnr.is_infinite() || psnr > 99.0, "PSNR of identical images must be infinite, got {psnr}");
    }

    #[test]
    fn ssim_perceptual_difference() {
        let width = 32;
        let height = 32;
        let img_white = vec![255_u8; (width * height * 4) as usize];
        let img_black = vec![0_u8; (width * height * 4) as usize];

        let ssim = compute_ssim_rgba8(&img_white, &img_black, width, height);
        assert!(ssim < 0.1, "White vs Black SSIM must be near 0, got {ssim}");

        let psnr = compute_psnr_rgba8(&img_white, &img_black, width, height);
        assert!(psnr.is_finite() && psnr < 10.0, "White vs Black PSNR must be low, got {psnr}");
    }
}
