//! Cosmic-Text 0.19 Unicode Shaping, Font Rasterization, and Godot `RichTextLabel` BBCode Engine.
//!
//! Provides bidirectional text shaping, multi-line wrapping, cursor hit-testing,
//! hardware-ready RGBA8 font glyph rasterization via `SwashCache`, and complete
//! Godot 4 `RichTextLabel` BBCode parsing (`[b]`, `[i]`, `[u]`, `[s]`, `[code]`,
//! `[color]`, `[font_size]`, `[center]`, `[right]`, `[url]`, `[img]`, `[wave]`, `[shake]`).

use cosmic_text::{
    Align, Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Style, SwashCache, SwashContent,
    Weight, Wrap,
};
use std::f32::consts::PI;

// ============================================================================
// TextServer Core
// ============================================================================

/// Godot-parity `TextServer` wrapping `cosmic-text 0.19` shaping and `SwashCache` rasterization.
#[derive(Debug)]
pub struct TextServer {
    pub font_system: FontSystem,
    pub swash_cache: SwashCache,
}

impl Default for TextServer {
    fn default() -> Self {
        Self::new()
    }
}

impl TextServer {
    /// Creates a new `TextServer` with default system fonts and rasterization cache.
    pub fn new() -> Self {
        Self {
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
        }
    }

    /// Shapes a string into a multi-line `Buffer` with given font size, line height, and max width.
    pub fn shape_text(
        &mut self,
        text: &str,
        font_size: f32,
        line_height: f32,
        max_width: Option<f32>,
    ) -> Buffer {
        let metrics = Metrics::new(font_size, line_height);
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_size(max_width, None);
        buffer.set_wrap(if max_width.is_some() {
            Wrap::Word
        } else {
            Wrap::None
        });
        buffer.set_text(text, &Attrs::new(), Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.font_system, false);
        buffer
    }

    /// Tests cursor hit-testing at layout coordinate `(x, y)`.
    pub fn hit_test(&self, buffer: &Buffer, x: f32, y: f32) -> Option<cosmic_text::Cursor> {
        buffer.hit(x, y)
    }

    /// Shapes rich text parsed from BBCode into a formatted `Buffer`.
    pub fn layout_bbcode(
        &mut self,
        rich: &BbCodeRichText,
        font_size: f32,
        line_height: f32,
        max_width: Option<f32>,
    ) -> Buffer {
        let metrics = Metrics::new(font_size, line_height);
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_size(max_width, None);
        buffer.set_wrap(if max_width.is_some() {
            Wrap::Word
        } else {
            Wrap::None
        });

        let default_align = rich.alignment;
        let plain_text = rich.to_plain_text();

        // Build attrs for each span
        let mut span_runs: Vec<(&str, Attrs)> = Vec::new();
        for span in &rich.spans {
            let mut attrs = Attrs::new();
            if span.bold {
                attrs = attrs.weight(Weight::BOLD);
            }
            if span.italic {
                attrs = attrs.style(Style::Italic);
            }
            if span.monospace {
                attrs = attrs.family(Family::Monospace);
            }
            if let Some(col) = span.color {
                attrs = attrs.color(cosmic_text::Color::rgba(col[0], col[1], col[2], col[3]));
            }
            span_runs.push((span.text.as_str(), attrs));
        }

        if span_runs.is_empty() {
            buffer.set_text(&plain_text, &Attrs::new(), Shaping::Advanced, default_align);
        } else {
            buffer.set_rich_text(
                span_runs,
                &Attrs::new(),
                Shaping::Advanced,
                default_align,
            );
        }

        buffer.shape_until_scroll(&mut self.font_system, false);
        buffer
    }

    /// Rasterizes shaped glyphs in `buffer` into an RGBA8 bitmap buffer of dimension `(width, height)`.
    pub fn rasterize_shaped_text_rgba(
        &mut self,
        buffer: &Buffer,
        width: u32,
        height: u32,
    ) -> Vec<u8> {
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        let w = width as i32;
        let h = height as i32;

        for run in buffer.layout_runs() {
            let line_y = run.line_y;

            for glyph in run.glyphs {
                let physical = glyph.physical((0.0, 0.0), 1.0);
                if let Some(img) = self
                    .swash_cache
                    .get_image(&mut self.font_system, physical.cache_key)
                {
                    let glyph_x = physical.x + img.placement.left;
                    let glyph_y = (line_y as i32) - img.placement.top;
                    let img_w = img.placement.width as i32;
                    let img_h = img.placement.height as i32;

                    let (r, g, b, a_base) = glyph
                        .color_opt
                        .map(|c| (c.r(), c.g(), c.b(), c.a()))
                        .unwrap_or((255, 255, 255, 255));

                    match img.content {
                        SwashContent::Mask => {
                            for row in 0..img_h {
                                let py = glyph_y + row;
                                if py < 0 || py >= h {
                                    continue;
                                }
                                for col in 0..img_w {
                                    let px = glyph_x + col;
                                    if px < 0 || px >= w {
                                        continue;
                                    }
                                    let mask_val = img.data[(row * img_w + col) as usize];
                                    if mask_val == 0 {
                                        continue;
                                    }

                                    let target_idx = ((py * w + px) * 4) as usize;
                                    let alpha = (u32::from(a_base) * u32::from(mask_val) / 255) as u8;

                                    // Alpha blend with target
                                    let inv_a = 255 - u32::from(alpha);
                                    let cur_r = u32::from(pixels[target_idx]);
                                    let cur_g = u32::from(pixels[target_idx + 1]);
                                    let cur_b = u32::from(pixels[target_idx + 2]);
                                    let cur_a = u32::from(pixels[target_idx + 3]);

                                    pixels[target_idx] = ((u32::from(r) * u32::from(alpha) + cur_r * inv_a) / 255) as u8;
                                    pixels[target_idx + 1] = ((u32::from(g) * u32::from(alpha) + cur_g * inv_a) / 255) as u8;
                                    pixels[target_idx + 2] = ((u32::from(b) * u32::from(alpha) + cur_b * inv_a) / 255) as u8;
                                    pixels[target_idx + 3] = (u32::from(alpha) + (cur_a * inv_a) / 255) as u8;
                                }
                            }
                        }
                        SwashContent::Color => {
                            for row in 0..img_h {
                                let py = glyph_y + row;
                                if py < 0 || py >= h {
                                    continue;
                                }
                                for col in 0..img_w {
                                    let px = glyph_x + col;
                                    if px < 0 || px >= w {
                                        continue;
                                    }
                                    let src_idx = ((row * img_w + col) * 4) as usize;
                                    let target_idx = ((py * w + px) * 4) as usize;

                                    pixels[target_idx..target_idx + 4]
                                        .copy_from_slice(&img.data[src_idx..src_idx + 4]);
                                }
                            }
                        }
                        SwashContent::SubpixelMask => {
                            for row in 0..img_h {
                                let py = glyph_y + row;
                                if py < 0 || py >= h {
                                    continue;
                                }
                                for col in 0..img_w {
                                    let px = glyph_x + col;
                                    if px < 0 || px >= w {
                                        continue;
                                    }
                                    let src_idx = ((row * img_w + col) * 3) as usize;
                                    let avg_mask = ((u32::from(img.data[src_idx])
                                        + u32::from(img.data[src_idx + 1])
                                        + u32::from(img.data[src_idx + 2]))
                                        / 3) as u8;
                                    if avg_mask == 0 {
                                        continue;
                                    }

                                    let target_idx = ((py * w + px) * 4) as usize;
                                    let alpha = (u32::from(a_base) * u32::from(avg_mask) / 255) as u8;
                                    let inv_a = 255 - u32::from(alpha);
                                    let cur_r = u32::from(pixels[target_idx]);
                                    let cur_g = u32::from(pixels[target_idx + 1]);
                                    let cur_b = u32::from(pixels[target_idx + 2]);
                                    let cur_a = u32::from(pixels[target_idx + 3]);

                                    pixels[target_idx] = ((u32::from(r) * u32::from(alpha) + cur_r * inv_a) / 255) as u8;
                                    pixels[target_idx + 1] = ((u32::from(g) * u32::from(alpha) + cur_g * inv_a) / 255) as u8;
                                    pixels[target_idx + 2] = ((u32::from(b) * u32::from(alpha) + cur_b * inv_a) / 255) as u8;
                                    pixels[target_idx + 3] = (u32::from(alpha) + (cur_a * inv_a) / 255) as u8;
                                }
                            }
                        }
                    }
                }
            }
        }

        pixels
    }
}

// ============================================================================
// BBCode RichText Engine
// ============================================================================

/// Animated wave effect configuration `[wave amp=20.0 freq=2.0]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaveEffect {
    pub amp: f32,
    pub freq: f32,
}

impl Default for WaveEffect {
    fn default() -> Self {
        Self {
            amp: 20.0,
            freq: 2.0,
        }
    }
}

/// Animated shake effect configuration `[shake rate=24.0 level=5.0]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShakeEffect {
    pub rate: f32,
    pub level: f32,
}

impl Default for ShakeEffect {
    fn default() -> Self {
        Self {
            rate: 24.0,
            level: 5.0,
        }
    }
}

/// Styled formatting span in parsed BBCode rich text.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BbCodeSpan {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub monospace: bool,
    pub color: Option<[u8; 4]>,
    pub font_size: Option<f32>,
    pub url: Option<String>,
    pub image_path: Option<String>,
    pub wave: Option<WaveEffect>,
    pub shake: Option<ShakeEffect>,
}

/// Dynamic displacement offset calculated for an animated glyph at time `t`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlyphOffset {
    pub glyph_index: usize,
    pub dx: f32,
    pub dy: f32,
}

/// Godot-parity `RichTextLabel` BBCode document holding parsed styled spans.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BbCodeRichText {
    pub spans: Vec<BbCodeSpan>,
    pub alignment: Option<Align>,
}

impl BbCodeRichText {
    pub fn new() -> Self {
        Self::default()
    }

    /// Parses a Godot BBCode markup string into styled spans.
    pub fn parse(input: &str) -> Self {
        let mut spans = Vec::new();
        let mut alignment = None;

        // Active state stack
        let mut cur_bold: u32 = 0;
        let mut cur_italic: u32 = 0;
        let mut cur_underline: u32 = 0;
        let mut cur_strikethrough: u32 = 0;
        let mut cur_code: u32 = 0;
        let mut color_stack: Vec<[u8; 4]> = Vec::new();
        let mut font_size_stack: Vec<f32> = Vec::new();
        let mut url_stack: Vec<String> = Vec::new();
        let mut wave_stack: Vec<WaveEffect> = Vec::new();
        let mut shake_stack: Vec<ShakeEffect> = Vec::new();

        let mut current_text = String::new();

        let flush_current = |spans: &mut Vec<BbCodeSpan>,
                             text: &mut String,
                             b,
                             it,
                             u,
                             s,
                             c,
                             cols: &[u8; 4],
                             has_col: bool,
                             fs: Option<f32>,
                             url: Option<&String>,
                             wave: Option<WaveEffect>,
                             shake: Option<ShakeEffect>| {
            if !text.is_empty() {
                spans.push(BbCodeSpan {
                    text: std::mem::take(text),
                    bold: b > 0,
                    italic: it > 0,
                    underline: u > 0,
                    strikethrough: s > 0,
                    monospace: c > 0,
                    color: if has_col { Some(*cols) } else { None },
                    font_size: fs,
                    url: url.cloned(),
                    image_path: None,
                    wave,
                    shake,
                });
            }
        };

        let mut chars = input.char_indices().peekable();

        while let Some((idx, ch)) = chars.next() {
            if ch == '[' {
                // Find closing ']'
                let mut tag_content = String::new();
                let mut closed = false;
                for (_, next_ch) in chars.by_ref() {
                    if next_ch == ']' {
                        closed = true;
                        break;
                    }
                    tag_content.push(next_ch);
                }

                if !closed {
                    current_text.push('[');
                    current_text.push_str(&tag_content);
                    continue;
                }

                let cur_col = color_stack.last().copied().unwrap_or([255, 255, 255, 255]);
                let has_col = !color_stack.is_empty();
                let cur_fs = font_size_stack.last().copied();
                let cur_url = url_stack.last();
                let cur_wave = wave_stack.last().copied();
                let cur_shake = shake_stack.last().copied();

                flush_current(
                    &mut spans,
                    &mut current_text,
                    cur_bold,
                    cur_italic,
                    cur_underline,
                    cur_strikethrough,
                    cur_code,
                    &cur_col,
                    has_col,
                    cur_fs,
                    cur_url,
                    cur_wave,
                    cur_shake,
                );

                let tag = tag_content.trim();

                // Check tag
                if tag == "b" {
                    cur_bold += 1;
                } else if tag == "/b" {
                    cur_bold = cur_bold.saturating_sub(1);
                } else if tag == "i" {
                    cur_italic += 1;
                } else if tag == "/i" {
                    cur_italic = cur_italic.saturating_sub(1);
                } else if tag == "u" {
                    cur_underline += 1;
                } else if tag == "/u" {
                    cur_underline = cur_underline.saturating_sub(1);
                } else if tag == "s" {
                    cur_strikethrough += 1;
                } else if tag == "/s" {
                    cur_strikethrough = cur_strikethrough.saturating_sub(1);
                } else if tag == "code" {
                    cur_code += 1;
                } else if tag == "/code" {
                    cur_code = cur_code.saturating_sub(1);
                } else if tag == "center" {
                    alignment = Some(Align::Center);
                } else if tag == "right" {
                    alignment = Some(Align::Right);
                } else if tag == "/center" || tag == "/right" {
                    alignment = None;
                } else if let Some(val) = tag.strip_prefix("color=") {
                    if let Some(col) = parse_color(val) {
                        color_stack.push(col);
                    }
                } else if tag == "/color" {
                    color_stack.pop();
                } else if let Some(val) = tag.strip_prefix("font_size=") {
                    if let Ok(fs) = val.parse::<f32>() {
                        font_size_stack.push(fs);
                    }
                } else if tag == "/font_size" {
                    font_size_stack.pop();
                } else if let Some(val) = tag.strip_prefix("url=") {
                    url_stack.push(val.trim_matches('"').to_string());
                } else if tag == "url" {
                    url_stack.push(String::new());
                } else if tag == "/url" {
                    url_stack.pop();
                } else if tag.starts_with("img") {
                    // Inline image [img]res://...[/img]
                    let mut img_content = String::new();
                    while let Some((_, c)) = chars.next() {
                        if c == '[' {
                            let mut close_tag = String::new();
                            for (_, c2) in chars.by_ref() {
                                if c2 == ']' {
                                    break;
                                }
                                close_tag.push(c2);
                            }
                            if close_tag == "/img" {
                                break;
                            }
                        } else {
                            img_content.push(c);
                        }
                    }
                    spans.push(BbCodeSpan {
                        text: String::new(),
                        image_path: Some(img_content),
                        ..Default::default()
                    });
                } else if tag.starts_with("wave") {
                    let mut wave = WaveEffect::default();
                    for part in tag.split_whitespace().skip(1) {
                        if let Some(amp_str) = part.strip_prefix("amp=")
                            && let Ok(a) = amp_str.parse::<f32>()
                        {
                            wave.amp = a;
                        } else if let Some(freq_str) = part.strip_prefix("freq=")
                            && let Ok(f) = freq_str.parse::<f32>()
                        {
                            wave.freq = f;
                        }
                    }
                    wave_stack.push(wave);
                } else if tag == "/wave" {
                    wave_stack.pop();
                } else if tag.starts_with("shake") {
                    let mut shake = ShakeEffect::default();
                    for part in tag.split_whitespace().skip(1) {
                        if let Some(rate_str) = part.strip_prefix("rate=")
                            && let Ok(r) = rate_str.parse::<f32>()
                        {
                            shake.rate = r;
                        } else if let Some(level_str) = part.strip_prefix("level=")
                            && let Ok(l) = level_str.parse::<f32>()
                        {
                            shake.level = l;
                        }
                    }
                    shake_stack.push(shake);
                } else if tag == "/shake" {
                    shake_stack.pop();
                } else {
                    let _ = idx;
                }
            } else {
                current_text.push(ch);
            }
        }

        let cur_col = color_stack.last().copied().unwrap_or([255, 255, 255, 255]);
        let has_col = !color_stack.is_empty();
        let cur_fs = font_size_stack.last().copied();
        let cur_url = url_stack.last();
        let cur_wave = wave_stack.last().copied();
        let cur_shake = shake_stack.last().copied();

        flush_current(
            &mut spans,
            &mut current_text,
            cur_bold,
            cur_italic,
            cur_underline,
            cur_strikethrough,
            cur_code,
            &cur_col,
            has_col,
            cur_fs,
            cur_url,
            cur_wave,
            cur_shake,
        );

        Self { spans, alignment }
    }

    /// Strips all BBCode markup tags and returns the underlying plain text.
    pub fn to_plain_text(&self) -> String {
        let mut out = String::new();
        for span in &self.spans {
            out.push_str(&span.text);
        }
        out
    }

    /// Evaluates per-glyph offset vectors for animated `[wave]` and `[shake]` effects at time `t`.
    pub fn evaluate_effects(&self, time: f32) -> Vec<GlyphOffset> {
        let mut offsets = Vec::new();
        let mut global_glyph_idx = 0;

        for span in &self.spans {
            for _ in span.text.chars() {
                let mut dx = 0.0;
                let mut dy = 0.0;

                if let Some(ref wave) = span.wave {
                    dy += wave.amp * (2.0 * PI * wave.freq * time + (global_glyph_idx as f32) * 0.5).sin();
                }

                if let Some(ref shake) = span.shake {
                    let step = (time * shake.rate).floor() as u64;
                    let hx = pseudo_hash(step, global_glyph_idx as u64, 0x1234);
                    let hy = pseudo_hash(step, global_glyph_idx as u64, 0x5678);
                    dx += ((hx as f32 / u32::MAX as f32) * 2.0 - 1.0) * shake.level;
                    dy += ((hy as f32 / u32::MAX as f32) * 2.0 - 1.0) * shake.level;
                }

                if dx.abs() > 1e-4 || dy.abs() > 1e-4 {
                    offsets.push(GlyphOffset {
                        glyph_index: global_glyph_idx,
                        dx,
                        dy,
                    });
                }

                global_glyph_idx += 1;
            }
        }

        offsets
    }
}

fn parse_color(s: &str) -> Option<[u8; 4]> {
    let clean = s.trim_matches('"').trim_matches('\'');
    if let Some(hex) = clean.strip_prefix('#') {
        match hex.len() {
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                Some([r, g, b, 255])
            }
            8 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
                Some([r, g, b, a])
            }
            _ => None,
        }
    } else {
        match clean.to_lowercase().as_str() {
            "red" => Some([255, 0, 0, 255]),
            "green" => Some([0, 255, 0, 255]),
            "blue" => Some([0, 0, 255, 255]),
            "yellow" => Some([255, 255, 0, 255]),
            "cyan" => Some([0, 255, 255, 255]),
            "magenta" => Some([255, 0, 255, 255]),
            "white" => Some([255, 255, 255, 255]),
            "black" => Some([0, 0, 0, 255]),
            "gray" | "grey" => Some([128, 128, 128, 255]),
            "orange" => Some([255, 165, 0, 255]),
            "purple" => Some([128, 0, 128, 255]),
            _ => None,
        }
    }
}

fn pseudo_hash(step: u64, idx: u64, seed: u64) -> u32 {
    let mut h = 0xcbf29ce484222325u64;
    h ^= step;
    h = h.wrapping_mul(0x100000001b3);
    h ^= idx;
    h = h.wrapping_mul(0x100000001b3);
    h ^= seed;
    h = h.wrapping_mul(0x100000001b3);
    (h ^ (h >> 32)) as u32
}
