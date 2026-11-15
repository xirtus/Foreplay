//! Binary glTF 2.0 (`.glb`) and JSON (`.gltf`) Rigged Scene Importer.
//!
//! Mirrors Godot 4's `EditorSceneFormatImporterBlend` / `GLTFDocument` pipeline.
//! Resolves buffer views, accessors, PBR materials, skin inverse bind matrices,
//! and animation tracks into canonical `foreplay_ir::IrScene`.

use foreplay_ir::{IrNode, IrScene, IrValue, NodeId};
use std::collections::BTreeMap;
use std::num::NonZeroU32;
use thiserror::Error;

/// Errors that can occur when parsing or importing glTF 2.0 assets.
#[derive(Debug, Error)]
pub enum GlbImportError {
    #[error("glTF parse error: {0}")]
    Gltf(#[from] gltf::Error),
    #[error("GLB binary chunk error: {0}")]
    GlbBinary(String),
    #[error("missing default scene or nodes in glTF document")]
    MissingScene,
    #[error("buffer view index out of bounds: {0}")]
    BufferViewOutOfBounds(usize),
    #[error("accessor out of bounds: {0}")]
    AccessorOutOfBounds(usize),
    #[error("invalid accessor format: {0}")]
    InvalidAccessor(String),
}

/// Extracted PBR material properties.
#[derive(Clone, Debug, PartialEq)]
pub struct GlbMaterialData {
    pub name: Option<String>,
    pub base_color_factor: [f32; 4],
    pub metallic_factor: f32,
    pub roughness_factor: f32,
    pub emissive_factor: [f32; 3],
    pub alpha_mode: String,
    pub double_sided: bool,
    pub unlit: bool,
}

/// Extracted skin / skeleton data.
#[derive(Clone, Debug, PartialEq)]
pub struct GlbSkinData {
    pub name: Option<String>,
    pub skeleton_root: Option<usize>,
    pub joints: Vec<usize>,
    pub inverse_bind_matrices: Vec<[[f32; 4]; 4]>,
}

impl GlbSkinData {
    /// Converts stored 4x4 matrix arrays into `bevy_math::Mat4` transformations.
    pub fn inverse_bind_mat4s(&self) -> Vec<bevy_math::Mat4> {
        self.inverse_bind_matrices
            .iter()
            .map(bevy_math::Mat4::from_cols_array_2d)
            .collect()
    }
}

/// Keyframed animation clip property.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlbAnimationProperty {
    Translation,
    Rotation,
    Scale,
    Weights,
}

/// Interpolation curve type for animation channels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlbInterpolation {
    Linear,
    Step,
    CubicSpline,
}

/// Raw sampled keyframe values for a channel.
#[derive(Clone, Debug, PartialEq)]
pub enum GlbTrackValues {
    Translations(Vec<[f32; 3]>),
    Rotations(Vec<[f32; 4]>),
    Scales(Vec<[f32; 3]>),
    Weights(Vec<f32>),
}

/// A single animation channel track targeting a node property.
#[derive(Clone, Debug, PartialEq)]
pub struct GlbAnimationTrack {
    pub target_node: usize,
    pub target_property: GlbAnimationProperty,
    pub interpolation: GlbInterpolation,
    pub timestamps: Vec<f32>,
    pub values: GlbTrackValues,
}

/// Complete extracted animation clip.
#[derive(Clone, Debug, PartialEq)]
pub struct GlbAnimationClip {
    pub name: String,
    pub duration: f32,
    pub tracks: Vec<GlbAnimationTrack>,
}

/// Geometric vertex and index buffers for a mesh primitive.
#[derive(Clone, Debug, PartialEq)]
pub struct GlbPrimitiveData {
    pub material_index: Option<usize>,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub tangents: Vec<[f32; 4]>,
    pub texcoords_0: Vec<[f32; 2]>,
    pub joints_0: Vec<[u16; 4]>,
    pub weights_0: Vec<[f32; 4]>,
    pub indices: Option<Vec<u32>>,
    pub morph_targets_positions: Vec<Vec<[f32; 3]>>,
}

/// Container for all primitives belonging to a mesh.
#[derive(Clone, Debug, PartialEq)]
pub struct GlbMeshData {
    pub name: Option<String>,
    pub primitives: Vec<GlbPrimitiveData>,
}

/// Complete imported bundle returning canonical `IrScene` alongside raw engine buffers.
#[derive(Clone, Debug, PartialEq)]
pub struct GlbImportedBundle {
    pub scene: IrScene,
    pub meshes: Vec<GlbMeshData>,
    pub materials: Vec<GlbMaterialData>,
    pub skins: Vec<GlbSkinData>,
    pub animations: Vec<GlbAnimationClip>,
}

/// Imports glTF binary (`.glb`) or JSON (`.gltf`) slice into a complete `GlbImportedBundle`.
pub fn import_glb_bytes_to_ir(
    bytes: &[u8],
    scene_name_override: Option<&str>,
) -> Result<GlbImportedBundle, GlbImportError> {
    let gltf_res = gltf::Gltf::from_slice(bytes);
    let (doc, blob_data) = match gltf_res {
        Ok(g) => (g.document, g.blob),
        Err(e) => {
            // Check if gltf::binary::Glb can parse it
            if bytes.starts_with(b"glTF") {
                let glb = gltf::binary::Glb::from_slice(bytes)
                    .map_err(|err| GlbImportError::GlbBinary(err.to_string()))?;
                let inner_gltf = gltf::Gltf::from_slice(&glb.json)?;
                (inner_gltf.document, glb.bin.map(|b| b.into_owned()))
            } else {
                return Err(GlbImportError::Gltf(e));
            }
        }
    };

    let blob_ref = blob_data.as_deref();
    let get_buffer_data = |buffer: gltf::Buffer| -> Option<&[u8]> {
        if buffer.index() == 0 {
            blob_ref
        } else {
            None
        }
    };

    // 1. Extract materials
    let mut materials = Vec::new();
    for mat in doc.materials() {
        let pbr = mat.pbr_metallic_roughness();
        let base_color_factor = pbr.base_color_factor();
        let metallic_factor = pbr.metallic_factor();
        let roughness_factor = pbr.roughness_factor();
        let emissive_factor = mat.emissive_factor();
        let alpha_mode = match mat.alpha_mode() {
            gltf::material::AlphaMode::Opaque => "Opaque".to_string(),
            gltf::material::AlphaMode::Mask => "Mask".to_string(),
            gltf::material::AlphaMode::Blend => "Blend".to_string(),
        };
        let double_sided = mat.double_sided();
        let unlit = mat.unlit();

        materials.push(GlbMaterialData {
            name: mat.name().map(ToString::to_string),
            base_color_factor,
            metallic_factor,
            roughness_factor,
            emissive_factor,
            alpha_mode,
            double_sided,
            unlit,
        });
    }

    // 2. Extract meshes & primitives
    let mut meshes = Vec::new();
    for mesh in doc.meshes() {
        let mut primitives = Vec::new();
        for prim in mesh.primitives() {
            let material_index = prim.material().index();
            let reader = prim.reader(get_buffer_data);

            let positions = reader
                .read_positions()
                .map(|p| p.collect::<Vec<[f32; 3]>>())
                .unwrap_or_default();

            let normals = reader
                .read_normals()
                .map(|n| n.collect::<Vec<[f32; 3]>>())
                .unwrap_or_default();

            let tangents = reader
                .read_tangents()
                .map(|t| t.collect::<Vec<[f32; 4]>>())
                .unwrap_or_default();

            let texcoords_0 = reader
                .read_tex_coords(0)
                .map(|uv| uv.into_f32().collect::<Vec<[f32; 2]>>())
                .unwrap_or_default();

            let joints_0 = reader
                .read_joints(0)
                .map(|j| j.into_u16().collect::<Vec<[u16; 4]>>())
                .unwrap_or_default();

            let weights_0 = reader
                .read_weights(0)
                .map(|w| w.into_f32().collect::<Vec<[f32; 4]>>())
                .unwrap_or_default();

            let indices = reader
                .read_indices()
                .map(|idx| idx.into_u32().collect::<Vec<u32>>());

            let mut morph_targets_positions = Vec::new();
            for target in reader.read_morph_targets() {
                if let Some(pos_iter) = target.0 {
                    morph_targets_positions.push(pos_iter.collect::<Vec<[f32; 3]>>());
                }
            }

            primitives.push(GlbPrimitiveData {
                material_index,
                positions,
                normals,
                tangents,
                texcoords_0,
                joints_0,
                weights_0,
                indices,
                morph_targets_positions,
            });
        }

        meshes.push(GlbMeshData {
            name: mesh.name().map(ToString::to_string),
            primitives,
        });
    }

    // 3. Extract skins
    let mut skins = Vec::new();
    for skin in doc.skins() {
        let joints = skin.joints().map(|j| j.index()).collect::<Vec<usize>>();
        let skeleton_root = skin.skeleton().map(|s| s.index());
        let skin_reader = skin.reader(get_buffer_data);
        let inverse_bind_matrices = skin_reader
            .read_inverse_bind_matrices()
            .map(|m| m.collect::<Vec<[[f32; 4]; 4]>>())
            .unwrap_or_default();

        skins.push(GlbSkinData {
            name: skin.name().map(ToString::to_string),
            skeleton_root,
            joints,
            inverse_bind_matrices,
        });
    }

    // 4. Extract animations
    let mut animations = Vec::new();
    for anim in doc.animations() {
        let anim_name = anim
            .name()
            .map(ToString::to_string)
            .unwrap_or_else(|| format!("Animation_{}", anim.index()));

        let mut tracks = Vec::new();
        let mut max_time: f32 = 0.0;

        for channel in anim.channels() {
            let target_node = channel.target().node().index();
            let target_property = match channel.target().property() {
                gltf::animation::Property::Translation => GlbAnimationProperty::Translation,
                gltf::animation::Property::Rotation => GlbAnimationProperty::Rotation,
                gltf::animation::Property::Scale => GlbAnimationProperty::Scale,
                gltf::animation::Property::MorphTargetWeights => GlbAnimationProperty::Weights,
            };

            let sampler = channel.sampler();
            let interpolation = match sampler.interpolation() {
                gltf::animation::Interpolation::Linear => GlbInterpolation::Linear,
                gltf::animation::Interpolation::Step => GlbInterpolation::Step,
                gltf::animation::Interpolation::CubicSpline => GlbInterpolation::CubicSpline,
            };

            let channel_reader = channel.reader(get_buffer_data);
            let timestamps = channel_reader
                .read_inputs()
                .map(|times| {
                    let ts = times.collect::<Vec<f32>>();
                    for &t in &ts {
                        if t > max_time {
                            max_time = t;
                        }
                    }
                    ts
                })
                .unwrap_or_default();

            if let Some(outputs) = channel_reader.read_outputs() {
                let values = match outputs {
                    gltf::animation::util::ReadOutputs::Translations(t) => {
                        GlbTrackValues::Translations(t.collect())
                    }
                    gltf::animation::util::ReadOutputs::Rotations(r) => {
                        GlbTrackValues::Rotations(r.into_f32().collect())
                    }
                    gltf::animation::util::ReadOutputs::Scales(s) => {
                        GlbTrackValues::Scales(s.collect())
                    }
                    gltf::animation::util::ReadOutputs::MorphTargetWeights(w) => {
                        GlbTrackValues::Weights(w.into_f32().collect())
                    }
                };

                tracks.push(GlbAnimationTrack {
                    target_node,
                    target_property,
                    interpolation,
                    timestamps,
                    values,
                });
            }
        }

        animations.push(GlbAnimationClip {
            name: anim_name,
            duration: max_time,
            tracks,
        });
    }

    // 5. Build canonical `IrScene` hierarchy
    let scene_name = scene_name_override
        .map(ToString::to_string)
        .or_else(|| {
            doc.default_scene()
                .and_then(|s| s.name().map(ToString::to_string))
        })
        .unwrap_or_else(|| "RiggedGlbScene".to_string());

    let mut ir_scene = IrScene::new(scene_name.clone());

    // Allocate root scene Node3D
    let root_id = NodeId {
        index: 1,
        generation: NonZeroU32::new(1).unwrap(),
    };
    ir_scene.nodes.insert(
        root_id,
        IrNode {
            id: root_id,
            name: scene_name.clone(),
            class_name: "Node3D".to_string(),
            parent: None,
            instance_scene: None,
            properties: BTreeMap::new(),
            groups: vec!["gltf_root".to_string()],
        },
    );

    // Map glTF node indices to NodeIds
    // Root is 1; glTF nodes start at 2
    let mut gltf_to_node_id: BTreeMap<usize, NodeId> = BTreeMap::new();
    let mut next_id: u32 = 2;

    for node in doc.nodes() {
        let node_id = NodeId {
            index: next_id,
            generation: NonZeroU32::new(1).unwrap(),
        };
        next_id += 1;
        gltf_to_node_id.insert(node.index(), node_id);
    }

    // Determine parents: by default top-level scene nodes have parent = root_id
    let mut node_parents: BTreeMap<usize, NodeId> = BTreeMap::new();
    for node in doc.nodes() {
        let parent_id = gltf_to_node_id[&node.index()];
        for child in node.children() {
            node_parents.insert(child.index(), parent_id);
        }
    }

    // Collect skin roots to classify Skeleton3D nodes
    let mut skin_root_nodes: BTreeMap<usize, usize> = BTreeMap::new();
    for (skin_idx, skin) in doc.skins().enumerate() {
        if let Some(skel) = skin.skeleton() {
            skin_root_nodes.insert(skel.index(), skin_idx);
        } else if let Some(first_joint) = skin.joints().next() {
            skin_root_nodes.insert(first_joint.index(), skin_idx);
        }
    }

    // Populate IrNode for each glTF node
    for node in doc.nodes() {
        let node_id = gltf_to_node_id[&node.index()];
        let parent = node_parents.get(&node.index()).copied().or(Some(root_id));
        let name = node
            .name()
            .map(ToString::to_string)
            .unwrap_or_else(|| format!("Node_{}", node.index()));

        let (trans, rot, scale) = node.transform().decomposed();
        let mut properties = BTreeMap::new();
        properties.insert("position".to_string(), IrValue::Vec3(trans));
        properties.insert("rotation".to_string(), IrValue::Quat(rot));
        properties.insert("scale".to_string(), IrValue::Vec3(scale));

        // Class resolution
        let class_name = if let Some(cam) = node.camera() {
            match cam.projection() {
                gltf::camera::Projection::Perspective(p) => {
                    properties.insert("fov".to_string(), IrValue::F64(p.yfov() as f64));
                    properties.insert("near".to_string(), IrValue::F64(p.znear() as f64));
                    if let Some(far) = p.zfar() {
                        properties.insert("far".to_string(), IrValue::F64(far as f64));
                    }
                }
                gltf::camera::Projection::Orthographic(o) => {
                    properties.insert("size".to_string(), IrValue::F64(o.ymag() as f64));
                    properties.insert("near".to_string(), IrValue::F64(o.znear() as f64));
                    properties.insert("far".to_string(), IrValue::F64(o.zfar() as f64));
                }
            }
            "Camera3D"
        } else if let Some(light) = node.light() {
            let col = light.color();
            properties.insert("light_color".to_string(), IrValue::Color([col[0], col[1], col[2], 1.0]));
            properties.insert("light_energy".to_string(), IrValue::F64(light.intensity() as f64));
            match light.kind() {
                gltf::khr_lights_punctual::Kind::Directional => "DirectionalLight3D",
                gltf::khr_lights_punctual::Kind::Point => "OmniLight3D",
                gltf::khr_lights_punctual::Kind::Spot { .. } => "SpotLight3D",
            }
        } else if let Some(mesh) = node.mesh() {
            let mesh_name = mesh.name().unwrap_or("UnnamedMesh");
            properties.insert("mesh_name".to_string(), IrValue::String(mesh_name.to_string()));
            properties.insert("primitive_count".to_string(), IrValue::I64(mesh.primitives().count() as i64));
            if let Some(skin) = node.skin() {
                properties.insert("skin_index".to_string(), IrValue::I64(skin.index() as i64));
                let skel_name = skin.name().unwrap_or("UnnamedSkin");
                properties.insert("skin_name".to_string(), IrValue::String(skel_name.to_string()));
            }
            "MeshInstance3D"
        } else if let Some(&skin_idx) = skin_root_nodes.get(&node.index()) {
            let skin_data = &skins[skin_idx];
            properties.insert("bone_count".to_string(), IrValue::I64(skin_data.joints.len() as i64));
            let joint_indices = skin_data.joints.iter().map(|&j| IrValue::I64(j as i64)).collect();
            properties.insert("joints".to_string(), IrValue::Array(joint_indices));

            let ibm_values = skin_data
                .inverse_bind_matrices
                .iter()
                .map(|m| {
                    let mut vals = Vec::with_capacity(16);
                    for col in m {
                        for &v in col {
                            vals.push(IrValue::F64(v as f64));
                        }
                    }
                    IrValue::Array(vals)
                })
                .collect();
            properties.insert("inverse_bind_matrices".to_string(), IrValue::Array(ibm_values));
            "Skeleton3D"
        } else {
            "Node3D"
        };

        ir_scene.nodes.insert(
            node_id,
            IrNode {
                id: node_id,
                name,
                class_name: class_name.to_string(),
                parent,
                instance_scene: None,
                properties,
                groups: Vec::new(),
            },
        );
    }

    // If animations are present, add an AnimationPlayer node
    if !animations.is_empty() {
        let anim_player_id = NodeId {
            index: next_id,
            generation: NonZeroU32::new(1).unwrap(),
        };
        let mut anim_props = BTreeMap::new();
        anim_props.insert("animation_count".to_string(), IrValue::I64(animations.len() as i64));
        let anim_names = animations
            .iter()
            .map(|a| IrValue::String(a.name.clone()))
            .collect();
        anim_props.insert("animations".to_string(), IrValue::Array(anim_names));
        if let Some(first) = animations.first() {
            anim_props.insert("current_animation".to_string(), IrValue::String(first.name.clone()));
            anim_props.insert("autoplay".to_string(), IrValue::String(first.name.clone()));
        }

        ir_scene.nodes.insert(
            anim_player_id,
            IrNode {
                id: anim_player_id,
                name: "AnimationPlayer".to_string(),
                class_name: "AnimationPlayer".to_string(),
                parent: Some(root_id),
                instance_scene: None,
                properties: anim_props,
                groups: vec!["animation_controller".to_string()],
            },
        );
    }

    Ok(GlbImportedBundle {
        scene: ir_scene,
        meshes,
        materials,
        skins,
        animations,
    })
}

/// Constructs a valid, spec-compliant binary glTF 2.0 (`.glb`) containing:
/// - 2-bone skinned mesh (quad split between root and spine bone)
/// - PBR Metallic-Roughness material
/// - Keyframed rotation animation track
///
/// Guaranteed deterministic, 4-byte padded, with magic `0x46546C67` and version 2.
pub fn build_sample_rigged_character_glb() -> Vec<u8> {
    // 1. Prepare raw binary buffer data
    let mut bin = Vec::new();

    // 0: Positions (4 vertices, 3 floats each = 48 bytes)
    let pos_offset = bin.len();
    let positions: [[f32; 3]; 4] = [
        [-0.5, 0.0, 0.0],
        [0.5, 0.0, 0.0],
        [0.5, 2.0, 0.0],
        [-0.5, 2.0, 0.0],
    ];
    for p in &positions {
        for f in p {
            bin.extend_from_slice(&f.to_le_bytes());
        }
    }
    let pos_len = bin.len() - pos_offset;

    // 1: Normals (4 vertices, 3 floats each = 48 bytes)
    let norm_offset = bin.len();
    let normals: [[f32; 3]; 4] = [
        [0.0, 0.0, 1.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, 1.0],
    ];
    for n in &normals {
        for f in n {
            bin.extend_from_slice(&f.to_le_bytes());
        }
    }
    let norm_len = bin.len() - norm_offset;

    // 2: TexCoords_0 (4 vertices, 2 floats each = 32 bytes)
    let uv_offset = bin.len();
    let uvs: [[f32; 2]; 4] = [
        [0.0, 0.0],
        [1.0, 0.0],
        [1.0, 1.0],
        [0.0, 1.0],
    ];
    for uv in &uvs {
        for f in uv {
            bin.extend_from_slice(&f.to_le_bytes());
        }
    }
    let uv_len = bin.len() - uv_offset;

    // 3: Joints_0 (4 vertices, 4 u16 each = 32 bytes)
    // Vertices 0 & 1 weight on joint 0 (Bone_Hips), vertices 2 & 3 weight on joint 1 (Bone_Spine)
    let joints_offset = bin.len();
    let joints: [[u16; 4]; 4] = [
        [0, 1, 0, 0],
        [0, 1, 0, 0],
        [1, 0, 0, 0],
        [1, 0, 0, 0],
    ];
    for j in &joints {
        for u in j {
            bin.extend_from_slice(&u.to_le_bytes());
        }
    }
    let joints_len = bin.len() - joints_offset;

    // 4: Weights_0 (4 vertices, 4 floats each = 64 bytes)
    let weights_offset = bin.len();
    let weights: [[f32; 4]; 4] = [
        [1.0, 0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0, 0.0],
    ];
    for w in &weights {
        for f in w {
            bin.extend_from_slice(&f.to_le_bytes());
        }
    }
    let weights_len = bin.len() - weights_offset;

    // 5: Indices (6 u16 = 12 bytes)
    let idx_offset = bin.len();
    let indices: [u16; 6] = [0, 1, 2, 0, 2, 3];
    for idx in &indices {
        bin.extend_from_slice(&idx.to_le_bytes());
    }
    let idx_len = bin.len() - idx_offset;

    // 6: Inverse Bind Matrices (2 matrices of 16 f32 = 128 bytes)
    // Joint 0: Identity; Joint 1: translation by -1 on Y
    let ibm_offset = bin.len();
    let ibm_0: [f32; 16] = [
        1.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ];
    let ibm_1: [f32; 16] = [
        1.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        0.0, -1.0, 0.0, 1.0,
    ];
    for f in &ibm_0 {
        bin.extend_from_slice(&f.to_le_bytes());
    }
    for f in &ibm_1 {
        bin.extend_from_slice(&f.to_le_bytes());
    }
    let ibm_len = bin.len() - ibm_offset;

    // 7: Animation Timestamps (3 floats = 12 bytes)
    let anim_time_offset = bin.len();
    let anim_times: [f32; 3] = [0.0, 0.5, 1.0];
    for t in &anim_times {
        bin.extend_from_slice(&t.to_le_bytes());
    }
    let anim_time_len = bin.len() - anim_time_offset;

    // 8: Animation Rotations (3 quaternions of 4 floats = 48 bytes)
    let anim_rot_offset = bin.len();
    let anim_rots: [[f32; 4]; 3] = [
        [0.0, 0.0, 0.0, 1.0],
        [0.0, 0.0, 0.258819, 0.965926],
        [0.0, 0.0, 0.0, 1.0],
    ];
    for r in &anim_rots {
        for f in r {
            bin.extend_from_slice(&f.to_le_bytes());
        }
    }
    let anim_rot_len = bin.len() - anim_rot_offset;

    // Pad BIN to 4-byte boundary with 0x00
    while bin.len() % 4 != 0 {
        bin.push(0);
    }
    let total_bin_len = bin.len();

    // 2. Build JSON string describing views, accessors, materials, meshes, skins, animations, nodes
    let json_str = format!(
        r#"{{
  "asset": {{ "version": "2.0", "generator": "foreplay-porthost" }},
  "scene": 0,
  "scenes": [
    {{ "name": "RiggedCharacterScene", "nodes": [0] }}
  ],
  "nodes": [
    {{ "name": "CharacterRoot", "children": [1, 3] }},
    {{ "name": "Bone_Hips", "translation": [0.0, 0.0, 0.0], "children": [2] }},
    {{ "name": "Bone_Spine", "translation": [0.0, 1.0, 0.0] }},
    {{ "name": "SkinnedMeshInstance", "mesh": 0, "skin": 0 }}
  ],
  "meshes": [
    {{
      "name": "RiggedQuadMesh",
      "primitives": [
        {{
          "attributes": {{
            "POSITION": 0,
            "NORMAL": 1,
            "TEXCOORD_0": 2,
            "JOINTS_0": 3,
            "WEIGHTS_0": 4
          }},
          "indices": 5,
          "material": 0
        }}
      ]
    }}
  ],
  "materials": [
    {{
      "name": "RiggedPbrMaterial",
      "pbrMetallicRoughness": {{
        "baseColorFactor": [0.8, 0.2, 0.2, 1.0],
        "metallicFactor": 0.1,
        "roughnessFactor": 0.8
      }},
      "doubleSided": true
    }}
  ],
  "skins": [
    {{
      "name": "CharacterArmature",
      "inverseBindMatrices": 6,
      "joints": [1, 2],
      "skeleton": 1
    }}
  ],
  "animations": [
    {{
      "name": "SpineTwist",
      "channels": [
        {{
          "sampler": 0,
          "target": {{
            "node": 2,
            "path": "rotation"
          }}
        }}
      ],
      "samplers": [
        {{
          "input": 7,
          "interpolation": "LINEAR",
          "output": 8
        }}
      ]
    }}
  ],
  "buffers": [
    {{ "byteLength": {total_bin_len} }}
  ],
  "bufferViews": [
    {{ "buffer": 0, "byteOffset": {pos_offset}, "byteLength": {pos_len}, "target": 34962 }},
    {{ "buffer": 0, "byteOffset": {norm_offset}, "byteLength": {norm_len}, "target": 34962 }},
    {{ "buffer": 0, "byteOffset": {uv_offset}, "byteLength": {uv_len}, "target": 34962 }},
    {{ "buffer": 0, "byteOffset": {joints_offset}, "byteLength": {joints_len}, "target": 34962 }},
    {{ "buffer": 0, "byteOffset": {weights_offset}, "byteLength": {weights_len}, "target": 34962 }},
    {{ "buffer": 0, "byteOffset": {idx_offset}, "byteLength": {idx_len}, "target": 34963 }},
    {{ "buffer": 0, "byteOffset": {ibm_offset}, "byteLength": {ibm_len} }},
    {{ "buffer": 0, "byteOffset": {anim_time_offset}, "byteLength": {anim_time_len} }},
    {{ "buffer": 0, "byteOffset": {anim_rot_offset}, "byteLength": {anim_rot_len} }}
  ],
  "accessors": [
    {{ "bufferView": 0, "byteOffset": 0, "componentType": 5126, "count": 4, "type": "VEC3", "min": [-0.5, 0.0, 0.0], "max": [0.5, 2.0, 0.0] }},
    {{ "bufferView": 1, "byteOffset": 0, "componentType": 5126, "count": 4, "type": "VEC3" }},
    {{ "bufferView": 2, "byteOffset": 0, "componentType": 5126, "count": 4, "type": "VEC2" }},
    {{ "bufferView": 3, "byteOffset": 0, "componentType": 5123, "count": 4, "type": "VEC4" }},
    {{ "bufferView": 4, "byteOffset": 0, "componentType": 5126, "count": 4, "type": "VEC4" }},
    {{ "bufferView": 5, "byteOffset": 0, "componentType": 5123, "count": 6, "type": "SCALAR" }},
    {{ "bufferView": 6, "byteOffset": 0, "componentType": 5126, "count": 2, "type": "MAT4" }},
    {{ "bufferView": 7, "byteOffset": 0, "componentType": 5126, "count": 3, "type": "SCALAR", "min": [0.0], "max": [1.0] }},
    {{ "bufferView": 8, "byteOffset": 0, "componentType": 5126, "count": 3, "type": "VEC4" }}
  ]
}}"#
    );

    let mut json_bytes = json_str.into_bytes();
    // JSON chunk must be padded with 0x20 spaces to 4-byte alignment
    while json_bytes.len() % 4 != 0 {
        json_bytes.push(b' ');
    }

    let json_chunk_len = json_bytes.len() as u32;
    let bin_chunk_len = bin.len() as u32;
    let total_file_len = 12 + 8 + json_chunk_len + 8 + bin_chunk_len;

    let mut glb = Vec::with_capacity(total_file_len as usize);

    // 12-byte header
    glb.extend_from_slice(&0x46546C67u32.to_le_bytes()); // magic "glTF"
    glb.extend_from_slice(&2u32.to_le_bytes()); // version 2
    glb.extend_from_slice(&total_file_len.to_le_bytes());

    // JSON chunk header & data
    glb.extend_from_slice(&json_chunk_len.to_le_bytes());
    glb.extend_from_slice(&0x4E4F534Au32.to_le_bytes()); // "JSON"
    glb.extend_from_slice(&json_bytes);

    // BIN chunk header & data
    glb.extend_from_slice(&bin_chunk_len.to_le_bytes());
    glb.extend_from_slice(&0x004E4942u32.to_le_bytes()); // "BIN\0"
    glb.extend_from_slice(&bin);

    glb
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_rigged_character_glb_spec_compliance() {
        let glb_bytes = build_sample_rigged_character_glb();
        assert!(glb_bytes.len() >= 12);
        assert_eq!(&glb_bytes[0..4], b"glTF");
        let version = u32::from_le_bytes([glb_bytes[4], glb_bytes[5], glb_bytes[6], glb_bytes[7]]);
        assert_eq!(version, 2);
        let declared_len = u32::from_le_bytes([glb_bytes[8], glb_bytes[9], glb_bytes[10], glb_bytes[11]]);
        assert_eq!(declared_len as usize, glb_bytes.len());

        let imported = import_glb_bytes_to_ir(&glb_bytes, None)
            .expect("sample rigged character glb should import cleanly");

        assert_eq!(imported.scene.name, "RiggedCharacterScene");
        assert_eq!(imported.meshes.len(), 1);
        assert_eq!(imported.materials.len(), 1);
        assert_eq!(imported.skins.len(), 1);
        assert_eq!(imported.animations.len(), 1);

        // Verify skin data
        let skin = &imported.skins[0];
        assert_eq!(skin.joints.len(), 2);
        assert_eq!(skin.inverse_bind_matrices.len(), 2);

        // Verify animation
        let anim = &imported.animations[0];
        assert_eq!(anim.name, "SpineTwist");
        assert_eq!(anim.duration, 1.0);
        assert_eq!(anim.tracks.len(), 1);
        assert_eq!(anim.tracks[0].target_property, GlbAnimationProperty::Rotation);
        if let GlbTrackValues::Rotations(rots) = &anim.tracks[0].values {
            assert_eq!(rots.len(), 3);
        } else {
            panic!("expected rotation track values");
        }

        // Verify mesh primitive
        let prim = &imported.meshes[0].primitives[0];
        assert_eq!(prim.positions.len(), 4);
        assert_eq!(prim.normals.len(), 4);
        assert_eq!(prim.texcoords_0.len(), 4);
        assert_eq!(prim.joints_0.len(), 4);
        assert_eq!(prim.weights_0.len(), 4);
        assert_eq!(prim.indices.as_ref().map(|i| i.len()), Some(6));

        // Verify scene nodes
        let nodes = &imported.scene.nodes;
        assert!(nodes.values().any(|n| n.class_name == "Skeleton3D"));
        assert!(nodes.values().any(|n| n.class_name == "MeshInstance3D"));
        assert!(nodes.values().any(|n| n.class_name == "AnimationPlayer"));

        // Verify bevy_math::Mat4 conversion
        let mat4s = skin.inverse_bind_mat4s();
        assert_eq!(mat4s.len(), 2);
        assert_eq!(mat4s[0], bevy_math::Mat4::IDENTITY);
        assert_eq!(mat4s[1].col(3), bevy_math::Vec4::new(0.0, -1.0, 0.0, 1.0));
    }

    #[test]
    fn parses_gltf_with_cameras_and_punctual_lights() {
        let json = r#"{
  "asset": { "version": "2.0" },
  "extensionsUsed": ["KHR_lights_punctual"],
  "extensions": {
    "KHR_lights_punctual": {
      "lights": [
        {
          "name": "SunLight",
          "color": [1.0, 0.9, 0.8],
          "intensity": 2.5,
          "type": "directional"
        },
        {
          "name": "PointLamp",
          "color": [0.2, 0.5, 1.0],
          "intensity": 5.0,
          "type": "point"
        },
        {
          "name": "Flashlight",
          "color": [1.0, 1.0, 1.0],
          "intensity": 10.0,
          "type": "spot",
          "spot": {
            "innerConeAngle": 0.2,
            "outerConeAngle": 0.5
          }
        }
      ]
    }
  },
  "cameras": [
    {
      "name": "PlayerCam",
      "type": "perspective",
      "perspective": {
        "yfov": 1.0472,
        "znear": 0.1,
        "zfar": 1000.0
      }
    }
  ],
  "nodes": [
    { "name": "CamNode", "camera": 0 },
    {
      "name": "DirLightNode",
      "extensions": {
        "KHR_lights_punctual": { "light": 0 }
      }
    },
    {
      "name": "PointLightNode",
      "extensions": {
        "KHR_lights_punctual": { "light": 1 }
      }
    },
    {
      "name": "SpotLightNode",
      "extensions": {
        "KHR_lights_punctual": { "light": 2 }
      }
    }
  ]
}"#;

        let bundle = import_glb_bytes_to_ir(json.as_bytes(), Some("LightingRig"))
            .expect("lighting rig gltf should import");

        assert_eq!(bundle.scene.name, "LightingRig");
        let nodes = &bundle.scene.nodes;

        let cam_node = nodes.values().find(|n| n.name == "CamNode").expect("CamNode");
        assert_eq!(cam_node.class_name, "Camera3D");

        let dir_node = nodes.values().find(|n| n.name == "DirLightNode").expect("DirLightNode");
        assert_eq!(dir_node.class_name, "DirectionalLight3D");

        let point_node = nodes.values().find(|n| n.name == "PointLightNode").expect("PointLightNode");
        assert_eq!(point_node.class_name, "OmniLight3D");

        let spot_node = nodes.values().find(|n| n.name == "SpotLightNode").expect("SpotLightNode");
        assert_eq!(spot_node.class_name, "SpotLight3D");
    }

    #[test]
    fn invalid_or_truncated_glb_returns_err() {
        let bad_bytes = b"not a gltf or glb file at all";
        let res = import_glb_bytes_to_ir(bad_bytes, None);
        assert!(res.is_err());

        let truncated_header = b"glTF\x02\x00\x00\x00\xFF\x00\x00\x00";
        let res2 = import_glb_bytes_to_ir(truncated_header, None);
        assert!(res2.is_err());
    }
}
