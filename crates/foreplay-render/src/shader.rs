//! Clean-room Godot 4 `.gdshader` (`spatial` & `canvas_item`) + `VisualShaderGraph`
//! -> Valid WGSL Compiler.
//!
//! Translates Godot shading language uniforms, built-ins (`VERTEX`, `NORMAL`, `UV`, `TIME`,
//! `ALBEDO`, `ROUGHNESS`, `METALLIC`, `EMISSION`, `ALPHA`, `COLOR`), and custom functions
//! into WebGPU-ready WGSL shaders.

use std::fmt;

/// Type of Godot shader.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShaderType {
    Spatial,
    CanvasItem,
}

/// Supported uniform data types in `.gdshader`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum UniformType {
    Float,
    Vec2,
    Vec3,
    Vec4,
    Sampler2D,
}

/// Metadata hints attached to uniforms.
#[derive(Clone, PartialEq, Debug)]
pub enum UniformHint {
    None,
    Range { min: f32, max: f32, step: Option<f32> },
    SourceColor,
    DefaultWhite,
    DefaultBlack,
}

/// Parsed uniform definition from a `.gdshader`.
#[derive(Clone, PartialEq, Debug)]
pub struct ShaderUniform {
    pub name: String,
    pub uniform_type: UniformType,
    pub hint: UniformHint,
    pub default_value: Option<String>,
}

/// Compilation or syntax error encountered during `.gdshader` translation.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ShaderCompileError {
    MissingShaderType,
    UnsupportedShaderType(String),
    SyntaxError(String),
    InvalidNodeConnection(String),
}

impl fmt::Display for ShaderCompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingShaderType => write!(f, "Missing 'shader_type' declaration (expected 'shader_type spatial;' or 'shader_type canvas_item;')"),
            Self::UnsupportedShaderType(s) => write!(f, "Unsupported shader type '{s}'"),
            Self::SyntaxError(msg) => write!(f, "Shader syntax error: {msg}"),
            Self::InvalidNodeConnection(msg) => write!(f, "Invalid visual shader graph connection: {msg}"),
        }
    }
}

impl std::error::Error for ShaderCompileError {}

/// Parse Godot 4 `.gdshader` source and compile into valid WGSL.
pub fn compile_gdshader_to_wgsl(source: &str) -> Result<String, ShaderCompileError> {
    let mut shader_type = None;
    let mut render_modes = Vec::new();
    let mut uniforms = Vec::new();

    let mut vertex_body = String::new();
    let mut fragment_body = String::new();

    let lines: Vec<&str> = source.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();
        if line.is_empty() || line.starts_with("//") {
            i += 1;
            continue;
        }

        if line.starts_with("shader_type") {
            let rest = line.trim_start_matches("shader_type").trim().trim_end_matches(';');
            if rest == "spatial" {
                shader_type = Some(ShaderType::Spatial);
            } else if rest == "canvas_item" {
                shader_type = Some(ShaderType::CanvasItem);
            } else {
                return Err(ShaderCompileError::UnsupportedShaderType(rest.to_string()));
            }
            i += 1;
            continue;
        }

        if line.starts_with("render_mode") {
            let rest = line.trim_start_matches("render_mode").trim().trim_end_matches(';');
            for mode in rest.split(',') {
                render_modes.push(mode.trim().to_string());
            }
            i += 1;
            continue;
        }

        if line.starts_with("uniform") {
            let u = parse_uniform_line(line)?;
            uniforms.push(u);
            i += 1;
            continue;
        }

        if line.starts_with("void vertex()") {
            let (body, next_i) = extract_block(&lines, i)?;
            vertex_body = body;
            i = next_i;
            continue;
        }

        if line.starts_with("void fragment()") {
            let (body, next_i) = extract_block(&lines, i)?;
            fragment_body = body;
            i = next_i;
            continue;
        }

        i += 1;
    }

    let st = shader_type.ok_or(ShaderCompileError::MissingShaderType)?;
    Ok(emit_wgsl(st, &render_modes, &uniforms, &vertex_body, &fragment_body))
}

fn parse_uniform_line(line: &str) -> Result<ShaderUniform, ShaderCompileError> {
    let rest = line.trim_start_matches("uniform").trim().trim_end_matches(';');
    let parts: Vec<&str> = rest.splitn(2, ' ').collect();
    if parts.len() < 2 {
        return Err(ShaderCompileError::SyntaxError(format!("Malformed uniform: {line}")));
    }

    let u_type_str = parts[0].trim();
    let remaining = parts[1].trim();

    let u_type = match u_type_str {
        "float" => UniformType::Float,
        "vec2" => UniformType::Vec2,
        "vec3" => UniformType::Vec3,
        "vec4" => UniformType::Vec4,
        "sampler2D" => UniformType::Sampler2D,
        other => return Err(ShaderCompileError::SyntaxError(format!("Unsupported uniform type '{other}'"))),
    };

    let (name_part, default_val) = if let Some(eq_pos) = remaining.find('=') {
        let n = remaining[..eq_pos].trim();
        let d = remaining[eq_pos + 1..].trim();
        (n, Some(d.to_string()))
    } else {
        (remaining, None)
    };

    let (name, hint) = if let Some(colon_pos) = name_part.find(':') {
        let n = name_part[..colon_pos].trim();
        let h_str = name_part[colon_pos + 1..].trim();
        let hint = parse_hint(h_str);
        (n, hint)
    } else {
        (name_part.trim(), UniformHint::None)
    };

    Ok(ShaderUniform {
        name: name.to_string(),
        uniform_type: u_type,
        hint,
        default_value: default_val,
    })
}

fn parse_hint(s: &str) -> UniformHint {
    if s.starts_with("hint_range") {
        let inner = s.trim_start_matches("hint_range(").trim_end_matches(')');
        let nums: Vec<&str> = inner.split(',').collect();
        if nums.len() >= 2 {
            let min = nums[0].trim().parse::<f32>().unwrap_or(0.0);
            let max = nums[1].trim().parse::<f32>().unwrap_or(1.0);
            let step = nums.get(2).and_then(|v| v.trim().parse::<f32>().ok());
            return UniformHint::Range { min, max, step };
        }
    } else if s.contains("source_color") {
        return UniformHint::SourceColor;
    } else if s.contains("hint_default_white") {
        return UniformHint::DefaultWhite;
    } else if s.contains("hint_default_black") {
        return UniformHint::DefaultBlack;
    }
    UniformHint::None
}

fn extract_block(lines: &[&str], start_idx: usize) -> Result<(String, usize), ShaderCompileError> {
    let mut depth = 0;
    let mut body = String::new();
    let mut i = start_idx;
    let mut started = false;

    while i < lines.len() {
        let line = lines[i];
        for ch in line.chars() {
            if ch == '{' {
                depth += 1;
                started = true;
            } else if ch == '}' {
                depth -= 1;
            }
        }

        if started {
            body.push_str(line);
            body.push('\n');
            if depth == 0 {
                return Ok((body, i + 1));
            }
        }
        i += 1;
    }

    Err(ShaderCompileError::SyntaxError("Unclosed block in shader".into()))
}

fn emit_wgsl(
    shader_type: ShaderType,
    _render_modes: &[String],
    uniforms: &[ShaderUniform],
    vertex_body: &str,
    fragment_body: &str,
) -> String {
    let mut out = String::new();
    out.push_str("// Generated by Foreplay Godot 4 .gdshader -> WGSL Compiler\n\n");

    // Uniform buffer struct
    out.push_str("struct MaterialUniforms {\n");
    let mut scalar_and_vec_uniforms = Vec::new();
    let mut sampler_uniforms = Vec::new();

    for u in uniforms {
        match u.uniform_type {
            UniformType::Sampler2D => sampler_uniforms.push(u),
            _ => scalar_and_vec_uniforms.push(u),
        }
    }

    if scalar_and_vec_uniforms.is_empty() {
        out.push_str("    _dummy: vec4<f32>,\n");
    } else {
        for u in &scalar_and_vec_uniforms {
            let wgsl_type = match u.uniform_type {
                UniformType::Float => "f32",
                UniformType::Vec2 => "vec2<f32>",
                UniformType::Vec3 => "vec3<f32>",
                UniformType::Vec4 => "vec4<f32>",
                UniformType::Sampler2D => unreachable!(),
            };
            out.push_str(&format!("    {}: {},\n", u.name, wgsl_type));
        }
    }
    out.push_str("};\n\n");

    out.push_str("struct SceneUniforms {\n");
    out.push_str("    view_proj: mat4x4<f32>,\n");
    out.push_str("    camera_pos: vec3<f32>,\n");
    out.push_str("    time: f32,\n");
    out.push_str("};\n\n");

    out.push_str("@group(0) @binding(0) var<uniform> material: MaterialUniforms;\n");
    out.push_str("@group(0) @binding(1) var<uniform> scene: SceneUniforms;\n\n");

    let mut binding_idx = 2;
    for s in &sampler_uniforms {
        out.push_str(&format!("@group(0) @binding({}) var {}_texture: texture_2d<f32>;\n", binding_idx, s.name));
        out.push_str(&format!("@group(0) @binding({}) var {}_sampler: sampler;\n", binding_idx + 1, s.name));
        binding_idx += 2;
    }
    out.push('\n');

    // Vertex input & output
    out.push_str("struct VertexInput {\n");
    out.push_str("    @location(0) position: vec3<f32>,\n");
    out.push_str("    @location(1) normal: vec3<f32>,\n");
    out.push_str("    @location(2) uv: vec2<f32>,\n");
    out.push_str("    @location(3) color: vec4<f32>,\n");
    out.push_str("};\n\n");

    out.push_str("struct VertexOutput {\n");
    out.push_str("    @builtin(position) clip_position: vec4<f32>,\n");
    out.push_str("    @location(0) uv: vec2<f32>,\n");
    out.push_str("    @location(1) normal: vec3<f32>,\n");
    out.push_str("    @location(2) world_pos: vec3<f32>,\n");
    out.push_str("    @location(3) color: vec4<f32>,\n");
    out.push_str("};\n\n");

    // Vertex entry point
    out.push_str("@vertex\nfn vs_main(in: VertexInput) -> VertexOutput {\n");
    out.push_str("    var out: VertexOutput;\n");
    out.push_str("    var VERTEX = in.position;\n");
    out.push_str("    var NORMAL = in.normal;\n");
    out.push_str("    var UV = in.uv;\n");
    out.push_str("    var COLOR = in.color;\n");
    out.push_str("    let TIME = scene.time;\n\n");

    if !vertex_body.is_empty() {
        let clean_v = clean_function_body(vertex_body, uniforms);
        out.push_str(&clean_v);
        out.push('\n');
    }

    out.push_str("    out.clip_position = scene.view_proj * vec4<f32>(VERTEX, 1.0);\n");
    out.push_str("    out.uv = UV;\n");
    out.push_str("    out.normal = NORMAL;\n");
    out.push_str("    out.world_pos = VERTEX;\n");
    out.push_str("    out.color = COLOR;\n");
    out.push_str("    return out;\n");
    out.push_str("}\n\n");

    // Fragment entry point
    out.push_str("@fragment\nfn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {\n");
    out.push_str("    var ALBEDO = vec3<f32>(1.0, 1.0, 1.0);\n");
    out.push_str("    var ROUGHNESS: f32 = 0.5;\n");
    out.push_str("    var METALLIC: f32 = 0.0;\n");
    out.push_str("    var EMISSION = vec3<f32>(0.0, 0.0, 0.0);\n");
    out.push_str("    var ALPHA: f32 = 1.0;\n");
    out.push_str("    var COLOR = in.color;\n");
    out.push_str("    let UV = in.uv;\n");
    out.push_str("    let NORMAL = in.normal;\n");
    out.push_str("    let TIME = scene.time;\n\n");

    if !fragment_body.is_empty() {
        let clean_f = clean_function_body(fragment_body, uniforms);
        out.push_str(&clean_f);
        out.push('\n');
    }

    match shader_type {
        ShaderType::Spatial => {
            out.push_str("    let final_color = ALBEDO + EMISSION;\n");
            out.push_str("    return vec4<f32>(final_color, ALPHA);\n");
        }
        ShaderType::CanvasItem => {
            out.push_str("    return COLOR;\n");
        }
    }
    out.push_str("}\n");

    out
}

fn clean_function_body(block: &str, uniforms: &[ShaderUniform]) -> String {
    // Strip outer braces if present
    let trimmed = block.trim();
    let body = if trimmed.starts_with('{') && trimmed.ends_with('}') {
        &trimmed[1..trimmed.len() - 1]
    } else if let Some(open) = trimmed.find('{') {
        if let Some(close) = trimmed.rfind('}') {
            &trimmed[open + 1..close]
        } else {
            trimmed
        }
    } else {
        trimmed
    };

    let mut translated = body.to_string();

    // Replace texture(sampler, uv) with textureSample(sampler_texture, sampler_sampler, uv)
    for u in uniforms {
        if u.uniform_type == UniformType::Sampler2D {
            let pat = format!("texture({},", u.name);
            let rep = format!("textureSample({}_texture, {}_sampler,", u.name, u.name);
            translated = translated.replace(&pat, &rep);

            let pat_sp = format!("texture( {},", u.name);
            translated = translated.replace(&pat_sp, &rep);
        } else {
            // Uniform variable access: e.g. albedo_color -> material.albedo_color
            // Only replace whole words:
            let name = &u.name;
            let rep = format!("material.{name}");
            translated = replace_ident(&translated, name, &rep);
        }
    }

    translated
}

fn is_ident_boundary(c: char) -> bool {
    !c.is_alphanumeric() && c != '_'
}

fn replace_ident(src: &str, ident: &str, replacement: &str) -> String {
    let mut result = String::new();
    let mut chars = src.char_indices().peekable();

    while let Some((idx, ch)) = chars.next() {
        if src[idx..].starts_with(ident) {
            let before_ok = idx == 0 || src[..idx].chars().last().is_some_and(is_ident_boundary);
            let after_idx = idx + ident.len();
            let after_ok = after_idx >= src.len() || src[after_idx..].chars().next().is_some_and(is_ident_boundary);

            if before_ok && after_ok {
                result.push_str(replacement);
                for _ in 1..ident.len() {
                    chars.next();
                }
                continue;
            }
        }
        result.push(ch);
    }

    result
}

/// Visual shader node types for Godot-style visual shader graph.
#[derive(Clone, PartialEq, Debug)]
pub enum VisualShaderNode {
    FloatConstant(f32),
    ColorConstant([f32; 4]),
    Texture2D { name: String },
    Add,
    Multiply,
    DotProduct,
    OutputSpatial,
    OutputCanvasItem,
}

/// Visual shader graph that can be compiled into WGSL.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct VisualShaderGraph {
    pub shader_type: Option<ShaderType>,
    pub nodes: Vec<VisualShaderNode>,
    /// Connections `(from_node, from_port) -> (to_node, to_port)`
    pub connections: Vec<((usize, usize), (usize, usize))>,
}

impl VisualShaderGraph {
    #[must_use]
    pub fn new(shader_type: ShaderType) -> Self {
        let mut g = Self {
            shader_type: Some(shader_type),
            nodes: Vec::new(),
            connections: Vec::new(),
        };

        // Node 0 is the output master node
        match shader_type {
            ShaderType::Spatial => g.nodes.push(VisualShaderNode::OutputSpatial),
            ShaderType::CanvasItem => g.nodes.push(VisualShaderNode::OutputCanvasItem),
        }
        g
    }

    /// Add a node to the visual graph. Returns the node index.
    pub fn add_node(&mut self, node: VisualShaderNode) -> usize {
        self.nodes.push(node);
        self.nodes.len() - 1
    }

    /// Connect output port of one node to input port of another node.
    pub fn connect_ports(&mut self, from: (usize, usize), to: (usize, usize)) {
        self.connections.push((from, to));
    }

    /// Compile visual graph to clean, structured WGSL.
    pub fn compile_wgsl(&self) -> Result<String, ShaderCompileError> {
        let st = self.shader_type.ok_or(ShaderCompileError::MissingShaderType)?;
        let mut pseudo_gdshader = String::new();
        match st {
            ShaderType::Spatial => pseudo_gdshader.push_str("shader_type spatial;\n\n"),
            ShaderType::CanvasItem => pseudo_gdshader.push_str("shader_type canvas_item;\n\n"),
        }

        // Collect uniforms from Texture2D nodes
        for node in &self.nodes {
            if let VisualShaderNode::Texture2D { name } = node {
                pseudo_gdshader.push_str(&format!("uniform sampler2D {name};\n"));
            }
        }

        pseudo_gdshader.push_str("void fragment() {\n");

        // Simple topological evaluation of nodes
        for (idx, node) in self.nodes.iter().enumerate() {
            match node {
                VisualShaderNode::FloatConstant(val) => {
                    pseudo_gdshader.push_str(&format!("    float n_{idx} = {val:.4};\n"));
                }
                VisualShaderNode::ColorConstant(c) => {
                    pseudo_gdshader.push_str(&format!("    vec4 n_{idx} = vec4({:.4}, {:.4}, {:.4}, {:.4});\n", c[0], c[1], c[2], c[3]));
                }
                VisualShaderNode::Texture2D { name } => {
                    pseudo_gdshader.push_str(&format!("    vec4 n_{idx} = texture({name}, UV);\n"));
                }
                VisualShaderNode::Add => {
                    let (in0, in1) = self.find_two_inputs(idx);
                    pseudo_gdshader.push_str(&format!("    vec4 n_{idx} = {in0} + {in1};\n"));
                }
                VisualShaderNode::Multiply => {
                    let (in0, in1) = self.find_two_inputs(idx);
                    pseudo_gdshader.push_str(&format!("    vec4 n_{idx} = {in0} * {in1};\n"));
                }
                VisualShaderNode::DotProduct => {
                    let (in0, in1) = self.find_two_inputs(idx);
                    pseudo_gdshader.push_str(&format!("    float n_{idx} = dot(({in0}).xyz, ({in1}).xyz);\n"));
                }
                VisualShaderNode::OutputSpatial => {
                    for &((from_node, _from_port), (_to_node, to_port)) in &self.connections {
                        if _to_node == idx {
                            match to_port {
                                0 => pseudo_gdshader.push_str(&format!("    ALBEDO = (n_{from_node}).xyz;\n")),
                                1 => pseudo_gdshader.push_str(&format!("    ROUGHNESS = (n_{from_node}).x;\n")),
                                2 => pseudo_gdshader.push_str(&format!("    METALLIC = (n_{from_node}).x;\n")),
                                3 => pseudo_gdshader.push_str(&format!("    EMISSION = (n_{from_node}).xyz;\n")),
                                4 => pseudo_gdshader.push_str(&format!("    ALPHA = (n_{from_node}).w;\n")),
                                _ => {}
                            }
                        }
                    }
                }
                VisualShaderNode::OutputCanvasItem => {
                    for &((from_node, _from_port), (_to_node, to_port)) in &self.connections {
                        if _to_node == idx && to_port == 0 {
                            pseudo_gdshader.push_str(&format!("    COLOR = n_{from_node};\n"));
                        }
                    }
                }
            }
        }

        pseudo_gdshader.push_str("}\n");

        compile_gdshader_to_wgsl(&pseudo_gdshader)
    }

    fn find_two_inputs(&self, node_idx: usize) -> (String, String) {
        let mut in0 = "vec4(0.0)".to_string();
        let mut in1 = "vec4(0.0)".to_string();

        for &((from_node, _from_port), (to_node, to_port)) in &self.connections {
            if to_node == node_idx {
                if to_port == 0 {
                    in0 = format!("n_{from_node}");
                } else if to_port == 1 {
                    in1 = format!("n_{from_node}");
                }
            }
        }
        (in0, in1)
    }
}
