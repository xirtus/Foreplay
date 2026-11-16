//! Phase 19.3A — Live Runtime Parametric Polyamour Modifiers, Shape Keys & `grind_edge` Splines in Three.js.
//!
//! Evaluates Polyamour 3D modeling nodes (`polyops 2` transcripts, procedural primitives,
//! modifier stacks, pose-space shape keys, and semantic tags) into:
//! - Three.js `BufferGeometry` with positions, normals, UVs, and UV2 (`uv2` / `uv1`) lightmap attributes.
//! - Three.js `morphAttributes.position` vertex morph targets and `morphTargetDictionary`.
//! - Continuous 3D `THREE.CatmullRomCurve3` skate grind-rail splines from chained `grind_edge` tags.
//! - Clean, runnable TypeScript runtime (`src/polyamour_runtime.ts`) providing `ParametricPolyamourController`
//!   for 60–120 FPS live procedural tweaking in WebGPU / WebGL (morph target weights, solidify thickness,
//!   array count/offset, wave/displace strength, and `getPointAt(t)` rail grinding).

use bevy_math::Vec3;
use foreplay_ir::{IrNode, IrScene, IrValue, NodeId};
use foreplay_polyamour::{
    PolyMesh, PolyModifier, PolyamourSemanticAttributes, ShapeKeySet,
    compile_polyamour_game_bundle, evaluate_modifier_stack, parse_polyops_v2,
    primitive_cone, primitive_cube, primitive_cylinder, primitive_grid,
    primitive_ico_sphere, primitive_plane, primitive_quarterpipe, primitive_stair_set,
    primitive_torus, primitive_uv_sphere, replay_polyops_v2,
};
use serde::Deserialize;
use std::collections::BTreeMap;

/// Baked GPU geometry, morph attributes, and gameplay splines ready for Three.js emission.
#[derive(Clone, Debug, PartialEq)]
pub struct BakedPolyamourAsset {
    /// Flattened triangle vertex positions `[x0, y0, z0, x1, y1, z1, ...]`.
    pub positions: Vec<f32>,
    /// Flattened triangle vertex normals `[nx0, ny0, nz0, ...]`.
    pub normals: Vec<f32>,
    /// Flattened UV coordinates (Channel 0) `[u0, v0, u1, v1, ...]`.
    pub uvs: Vec<f32>,
    /// Flattened Smart UV2 Lightmap coordinates (Channel 1 / Channel 2) `[u0, v0, ...]`.
    pub uv2: Vec<f32>,
    /// Triangle index buffer `[i0, i1, i2, ...]`.
    pub indices: Vec<u32>,
    /// Named morph target vertex positions: `KeyName -> [x0, y0, z0, ...]`.
    pub morph_targets: BTreeMap<String, Vec<f32>>,
    /// Chained 3D skate grind-rail splines, each a list of `[x, y, z]` control points.
    pub grind_rail_splines: Vec<Vec<[f32; 3]>>,
    /// Point cloud of convex collision points extracted from tagged faces.
    pub convex_hull_points: Vec<[f32; 3]>,
    /// Walkable navigation face indices.
    pub walkable_nav_faces: Vec<u32>,
    /// Base material color in CSS hex format (e.g. `0x38bdf8`).
    pub color_hex: String,
    /// PBR metalness `[0.0, 1.0]`.
    pub metalness: f32,
    /// PBR roughness `[0.0, 1.0]`.
    pub roughness: f32,
    /// Whether any grind rail splines form closed loops.
    pub spline_closed_flags: Vec<bool>,
}

/// A scene node carrying Polyamour 3D modeling metadata and its baked Three.js representation.
#[derive(Clone, Debug, PartialEq)]
pub struct ThreeJsParametricPolyamourAsset {
    pub node_id: NodeId,
    pub node_name: String,
    pub primitive: String,
    pub baked: BakedPolyamourAsset,
}

impl ThreeJsParametricPolyamourAsset {
    /// Inspects an `IrNode` and extracts a `ThreeJsParametricPolyamourAsset` if Polyamour metadata is present.
    pub fn from_node(node: &IrNode) -> Option<Self> {
        let has_primitive = node.properties.contains_key("polyamour_primitive");
        let has_ops = node.properties.contains_key("polyamour_ops")
            || node.properties.contains_key("polyops")
            || node.properties.contains_key("polyops_v2");
        let has_subsurf = node.properties.contains_key("polyamour_subsurf");
        let has_modifiers = node.properties.contains_key("polyamour_modifiers")
            || node.properties.contains_key("polyamour_solidify")
            || node.properties.contains_key("polyamour_array")
            || node.properties.contains_key("polyamour_mirror")
            || node.properties.contains_key("polyamour_displace");
        let has_shape_keys = node.properties.contains_key("polyamour_shape_keys")
            || node.properties.contains_key("shape_key_deltas");
        let has_grind_edges = node.properties.contains_key("grind_edges")
            || node.properties.contains_key("grind_edge")
            || node.properties.contains_key("polyamour_semantic_tags");

        if !has_primitive && !has_ops && !has_subsurf && !has_modifiers && !has_shape_keys && !has_grind_edges {
            return None;
        }

        let prim_name = node
            .properties
            .get("polyamour_primitive")
            .and_then(|v| match v {
                IrValue::String(s) => Some(s.as_str()),
                _ => None,
            })
            .unwrap_or("Cube");

        // 1. Build initial PolyMesh primitive
        let mut mesh = build_primitive_mesh(prim_name, &node.properties);
        let mut attrs = PolyamourSemanticAttributes::new();

        // 2. Replay polyops 2 transcript if present
        let script_val = node
            .properties
            .get("polyamour_ops")
            .or_else(|| node.properties.get("polyops"))
            .or_else(|| node.properties.get("polyops_v2"));
        if let Some(IrValue::String(script)) = script_val
            && let Ok(steps) = parse_polyops_v2(script)
        {
            let _ = replay_polyops_v2(&mut mesh, &mut attrs, &steps);
        }

        // 3. Extract direct grind_edges property if present
        extract_semantic_properties(&node.properties, &mut attrs);

        // 4. Extract and evaluate modifier stack
        let modifiers = extract_modifiers(&node.properties);
        let evaluated_mesh = if !modifiers.is_empty() {
            evaluate_modifier_stack(&mesh, &modifiers)
        } else {
            mesh
        };

        // 5. Extract ShapeKeySet morph targets
        let shape_keys = extract_shape_keys(&node.properties, &evaluated_mesh);

        // 6. Compile game bundle (grind splines, UV2 lightmaps, colliders)
        let game_bundle = compile_polyamour_game_bundle(&evaluated_mesh, &attrs);

        // 7. Triangulate mesh for GPU BufferGeometry
        let tri = evaluated_mesh.triangulate();

        // 8. Map UV2 coordinates to triangulated vertices
        let mut uv2_flat = Vec::with_capacity(tri.positions.len() * 2);
        for face_idx in 0..evaluated_mesh.faces.len() {
            let face_uv2 = game_bundle.uv2_by_face.get(face_idx);
            let face = &evaluated_mesh.faces[face_idx];
            let n_verts = face.verts.len();
            if n_verts >= 3 {
                // Approximate UV2 projection for triangulated ear-clips in face
                for i in 1..(n_verts - 1) {
                    let uv0 = face_uv2.and_then(|uvs| uvs.first()).copied().unwrap_or([0.0, 0.0]);
                    let uv1 = face_uv2.and_then(|uvs| uvs.get(i)).copied().unwrap_or([0.0, 0.0]);
                    let uv2 = face_uv2.and_then(|uvs| uvs.get(i + 1)).copied().unwrap_or([0.0, 0.0]);
                    uv2_flat.extend_from_slice(&uv0);
                    uv2_flat.extend_from_slice(&uv1);
                    uv2_flat.extend_from_slice(&uv2);
                }
            }
        }
        // Pad or truncate to match positions count
        while uv2_flat.len() < tri.positions.len() * 2 {
            uv2_flat.push(0.0);
            uv2_flat.push(0.0);
        }
        uv2_flat.truncate(tri.positions.len() * 2);

        // 9. Map morph targets to triangulated vertex count
        let mut morph_targets = BTreeMap::new();
        for (target_name, deformed_pos) in &shape_keys.targets {
            if deformed_pos.len() == evaluated_mesh.positions.len() {
                let mut tri_target = Vec::with_capacity(tri.positions.len() * 3);
                for face in &evaluated_mesh.faces {
                    let n = face.verts.len();
                    if n >= 3 {
                        for i in 1..(n - 1) {
                            let v0 = face.verts[0] as usize;
                            let v1 = face.verts[i] as usize;
                            let v2 = face.verts[i + 1] as usize;
                            let p0 = deformed_pos.get(v0).copied().unwrap_or(evaluated_mesh.positions[v0]);
                            let p1 = deformed_pos.get(v1).copied().unwrap_or(evaluated_mesh.positions[v1]);
                            let p2 = deformed_pos.get(v2).copied().unwrap_or(evaluated_mesh.positions[v2]);
                            tri_target.extend_from_slice(&[p0.x, p0.y, p0.z, p1.x, p1.y, p1.z, p2.x, p2.y, p2.z]);
                        }
                    }
                }
                while tri_target.len() < tri.positions.len() * 3 {
                    tri_target.push(0.0);
                }
                tri_target.truncate(tri.positions.len() * 3);
                morph_targets.insert(target_name.clone(), tri_target);
            }
        }

        // 10. Grind rail splines to 3D float triplets & check closed loop
        let mut grind_rail_splines = Vec::new();
        let mut spline_closed_flags = Vec::new();
        for spline in game_bundle.grind_rail_splines {
            if spline.len() >= 2 {
                let is_closed = spline.first().unwrap().distance(*spline.last().unwrap()) < 1e-4;
                let pts: Vec<[f32; 3]> = spline.iter().map(|p| [p.x, p.y, p.z]).collect();
                grind_rail_splines.push(pts);
                spline_closed_flags.push(is_closed);
            }
        }

        let convex_hull_points = game_bundle
            .convex_hull_points
            .iter()
            .map(|p| [p.x, p.y, p.z])
            .collect();

        // Flatten positions, normals, uvs
        let mut positions = Vec::with_capacity(tri.positions.len() * 3);
        for p in &tri.positions {
            positions.extend_from_slice(&[p.x, p.y, p.z]);
        }
        let mut normals = Vec::with_capacity(tri.normals.len() * 3);
        for n in &tri.normals {
            normals.extend_from_slice(&[n.x, n.y, n.z]);
        }
        let mut uvs = Vec::with_capacity(tri.uvs.len() * 2);
        for uv in &tri.uvs {
            uvs.extend_from_slice(uv);
        }

        let color_hex = extract_color_hex(&node.properties);
        let metalness = extract_num_prop(&node.properties, "metalness", 0.1) as f32;
        let roughness = extract_num_prop(&node.properties, "roughness", 0.5) as f32;

        let baked = BakedPolyamourAsset {
            positions,
            normals,
            uvs,
            uv2: uv2_flat,
            indices: tri.indices,
            morph_targets,
            grind_rail_splines,
            convex_hull_points,
            walkable_nav_faces: game_bundle.walkable_nav_faces,
            color_hex,
            metalness,
            roughness,
            spline_closed_flags,
        };

        Some(Self {
            node_id: node.id,
            node_name: node.name.clone(),
            primitive: prim_name.to_string(),
            baked,
        })
    }
}

/// Collects all `ThreeJsParametricPolyamourAsset` instances defined across an `IrScene`.
pub fn extract_polyamour_assets_from_scene(scene: &IrScene) -> Vec<ThreeJsParametricPolyamourAsset> {
    scene
        .nodes
        .values()
        .filter_map(ThreeJsParametricPolyamourAsset::from_node)
        .collect()
}

/// Emits the complete runnable `src/polyamour_runtime.ts` TypeScript file for the compiled Vite project.
pub fn generate_polyamour_runtime_ts(assets: &[ThreeJsParametricPolyamourAsset]) -> String {
    let mut ts = String::new();
    ts.push_str(
        "// src/polyamour_runtime.ts - Foreplay Three.js Parametric Polyamour Runtime Controller\n\
        // Phase 19.3A: Live Parametric Modifiers, Shape Keys & Skate Grind Splines\n\n\
        import * as THREE from 'three';\n\n\
        export interface GrindSpline {\n\
            id: string;\n\
            nodeId: number;\n\
            curve: THREE.CatmullRomCurve3;\n\
            points: THREE.Vector3[];\n\
            length: number;\n\
            isClosed: boolean;\n\
            getPointAt(t: number): THREE.Vector3;\n\
            getTangentAt(t: number): THREE.Vector3;\n\
        }\n\n\
        export interface ParametricPolyamourAssetMeta {\n\
            nodeId: number;\n\
            name: string;\n\
            primitive: string;\n\
            shapeKeys: string[];\n\
            grindRailCount: number;\n\
            hasUv2: boolean;\n\
        }\n\n\
        export class ParametricPolyamourController {\n\
            public scene: THREE.Scene;\n\
            public meshes: Map<number, THREE.Mesh> = new Map();\n\
            public splines: GrindSpline[] = [];\n\
            public baseGeometries: Map<number, { positions: Float32Array; normals: Float32Array }> = new Map();\n\
            public assetMetas: Map<number, ParametricPolyamourAssetMeta> = new Map();\n\n\
            constructor(scene: THREE.Scene) {\n\
                this.scene = scene;\n\
            }\n\n\
            registerMesh(nodeId: number, mesh: THREE.Mesh, meta?: ParametricPolyamourAssetMeta): void {\n\
                this.meshes.set(nodeId, mesh);\n\
                if (meta) {\n\
                    this.assetMetas.set(nodeId, meta);\n\
                }\n\
                const geom = mesh.geometry;\n\
                if (geom && geom.attributes.position) {\n\
                    this.baseGeometries.set(nodeId, {\n\
                        positions: new Float32Array(geom.attributes.position.array),\n\
                        normals: geom.attributes.normal\n\
                            ? new Float32Array(geom.attributes.normal.array)\n\
                            : new Float32Array(0),\n\
                    });\n\
                }\n\
                if (mesh.userData && Array.isArray(mesh.userData.grindSplines)) {\n\
                    for (let i = 0; i < mesh.userData.grindSplines.length; i++) {\n\
                        const curve: THREE.CatmullRomCurve3 = mesh.userData.grindSplines[i];\n\
                        this.splines.push({\n\
                            id: `grind_${nodeId}_${i}`,\n\
                            nodeId,\n\
                            curve,\n\
                            points: curve.points,\n\
                            length: curve.getLength(),\n\
                            isClosed: curve.closed,\n\
                            getPointAt: (t: number) => curve.getPointAt(t),\n\
                            getTangentAt: (t: number) => curve.getTangentAt(t),\n\
                        });\n\
                    }\n\
                }\n\
            }\n\n\
            setShapeKeyWeight(nodeId: number, keyName: string, weight: number): boolean {\n\
                const mesh = this.meshes.get(nodeId);\n\
                if (!mesh || !mesh.morphTargetDictionary || !mesh.morphTargetInfluences) {\n\
                    return false;\n\
                }\n\
                const idx = mesh.morphTargetDictionary[keyName];\n\
                if (idx !== undefined) {\n\
                    mesh.morphTargetInfluences[idx] = Math.max(0, Math.min(1, weight));\n\
                    return true;\n\
                }\n\
                return false;\n\
            }\n\n\
            getShapeKeyWeight(nodeId: number, keyName: string): number {\n\
                const mesh = this.meshes.get(nodeId);\n\
                if (!mesh || !mesh.morphTargetDictionary || !mesh.morphTargetInfluences) {\n\
                    return 0;\n\
                }\n\
                const idx = mesh.morphTargetDictionary[keyName];\n\
                return idx !== undefined ? mesh.morphTargetInfluences[idx] : 0;\n\
            }\n\n\
            getGrindSplines(nodeId?: number): GrindSpline[] {\n\
                return nodeId !== undefined\n\
                    ? this.splines.filter((s) => s.nodeId === nodeId)\n\
                    : this.splines;\n\
            }\n\n\
            sampleGrindRail(splineIndex: number, t: number): { position: THREE.Vector3; tangent: THREE.Vector3 } {\n\
                const spline = this.splines[splineIndex];\n\
                if (!spline) {\n\
                    return { position: new THREE.Vector3(), tangent: new THREE.Vector3(0, 0, 1) };\n\
                }\n\
                const clampedT = Math.max(0, Math.min(1, t));\n\
                return {\n\
                    position: spline.getPointAt(clampedT),\n\
                    tangent: spline.getTangentAt(clampedT),\n\
                };\n\
            }\n\n\
            findNearestGrindRailPoint(\n\
                worldPos: THREE.Vector3,\n\
                maxDistance: number = 4.0\n\
            ): { spline: GrindSpline; t: number; point: THREE.Vector3; distance: number } | null {\n\
                let closest: { spline: GrindSpline; t: number; point: THREE.Vector3; distance: number } | null = null;\n\
                let minD = maxDistance;\n\
                for (const spline of this.splines) {\n\
                    const steps = 32;\n\
                    for (let i = 0; i <= steps; i++) {\n\
                        const t = i / steps;\n\
                        const pt = spline.getPointAt(t);\n\
                        const d = pt.distanceTo(worldPos);\n\
                        if (d < minD) {\n\
                            minD = d;\n\
                            closest = { spline, t, point: pt, distance: d };\n\
                        }\n\
                    }\n\
                }\n\
                return closest;\n\
            }\n\n\
            updateDisplaceModifier(nodeId: number, strength: number, frequency: number = 2.0, time: number = 0): void {\n\
                const mesh = this.meshes.get(nodeId);\n\
                const base = this.baseGeometries.get(nodeId);\n\
                if (!mesh || !base || base.normals.length === 0) return;\n\
                const posAttr = mesh.geometry.attributes.position;\n\
                const posArray = posAttr.array as Float32Array;\n\
                const basePos = base.positions;\n\
                const baseNorm = base.normals;\n\
                const count = posArray.length / 3;\n\
                for (let i = 0; i < count; i++) {\n\
                    const idx = i * 3;\n\
                    const phase = (basePos[idx] + basePos[idx + 1] + basePos[idx + 2]) * frequency + time;\n\
                    const disp = Math.sin(phase) * strength;\n\
                    posArray[idx] = basePos[idx] + baseNorm[idx] * disp;\n\
                    posArray[idx + 1] = basePos[idx + 1] + baseNorm[idx + 1] * disp;\n\
                    posArray[idx + 2] = basePos[idx + 2] + baseNorm[idx + 2] * disp;\n\
                }\n\
                posAttr.needsUpdate = true;\n\
                mesh.geometry.computeVertexNormals();\n\
            }\n\n\
            updateSolidifyModifier(nodeId: number, thickness: number): void {\n\
                const mesh = this.meshes.get(nodeId);\n\
                const base = this.baseGeometries.get(nodeId);\n\
                if (!mesh || !base || base.normals.length === 0) return;\n\
                const posAttr = mesh.geometry.attributes.position;\n\
                const posArray = posAttr.array as Float32Array;\n\
                const basePos = base.positions;\n\
                const baseNorm = base.normals;\n\
                const count = posArray.length / 3;\n\
                for (let i = 0; i < count; i++) {\n\
                    const idx = i * 3;\n\
                    posArray[idx] = basePos[idx] + baseNorm[idx] * thickness;\n\
                    posArray[idx + 1] = basePos[idx + 1] + baseNorm[idx + 1] * thickness;\n\
                    posArray[idx + 2] = basePos[idx + 2] + baseNorm[idx + 2] * thickness;\n\
                }\n\
                posAttr.needsUpdate = true;\n\
                mesh.geometry.computeVertexNormals();\n\
            }\n\n\
            updateArrayModifier(nodeId: number, count: number, offset: [number, number, number]): void {\n\
                const mesh = this.meshes.get(nodeId);\n\
                if (!mesh) return;\n\
                let arrayGroup = mesh.userData.arrayGroup as THREE.Group | undefined;\n\
                if (!arrayGroup) {\n\
                    arrayGroup = new THREE.Group();\n\
                    if (mesh.parent) {\n\
                        mesh.parent.add(arrayGroup);\n\
                    } else {\n\
                        this.scene.add(arrayGroup);\n\
                    }\n\
                    mesh.userData.arrayGroup = arrayGroup;\n\
                }\n\
                while (arrayGroup.children.length > 0) {\n\
                    arrayGroup.remove(arrayGroup.children[0]);\n\
                }\n\
                for (let k = 1; k < count; k++) {\n\
                    const clone = mesh.clone();\n\
                    clone.position.set(\n\
                        mesh.position.x + offset[0] * k,\n\
                        mesh.position.y + offset[1] * k,\n\
                        mesh.position.z + offset[2] * k\n\
                    );\n\
                    arrayGroup.add(clone);\n\
                }\n\
            }\n\n\
            bindFeelKnobs(feelStore: any): void {\n\
                if (!feelStore || typeof feelStore.subscribe !== 'function') return;\n\
                feelStore.subscribe((key: string, value: number) => {\n\
                    if (key.startsWith('morph_')) {\n\
                        const keyName = key.replace('morph_', '');\n\
                        for (const nodeId of this.meshes.keys()) {\n\
                            this.setShapeKeyWeight(nodeId, keyName, value);\n\
                        }\n\
                    } else if (key === 'displace_strength' || key === 'wave_strength') {\n\
                        for (const nodeId of this.meshes.keys()) {\n\
                            this.updateDisplaceModifier(nodeId, value, 2.0, performance.now() / 1000);\n\
                        }\n\
                    } else if (key === 'solidify_thickness') {\n\
                        for (const nodeId of this.meshes.keys()) {\n\
                            this.updateSolidifyModifier(nodeId, value);\n\
                        }\n\
                    }\n\
                });\n\
            }\n\n\
            tick(_dt: number, _elapsed: number): void {\n\
                // High-frequency procedural loop hook (60-120 FPS)\n\
            }\n\n\
            static initRuntime(\n\
                scene: THREE.Scene,\n\
                nodes: Map<number, THREE.Object3D | HTMLElement>\n\
            ): ParametricPolyamourController {\n\
                const controller = new ParametricPolyamourController(scene);\n\
                for (const [id, obj] of nodes) {\n\
                    if (\n\
                        obj instanceof THREE.Mesh &&\n\
                        obj.userData &&\n\
                        (obj.userData.polyamourAsset || obj.userData.grindSplines)\n\
                    ) {\n\
                        controller.registerMesh(id, obj, obj.userData.polyamourMeta);\n\
                    }\n\
                }\n\
                return controller;\n\
            }\n\
        }\n",
    );

    // Append manifest list of baked assets
    ts.push_str("\nexport const BAKED_POLYAMOUR_ASSET_METAS: ParametricPolyamourAssetMeta[] = [\n");
    for asset in assets {
        let keys_json = serde_json::to_string(
            &asset.baked.morph_targets.keys().cloned().collect::<Vec<_>>(),
        )
        .unwrap_or_else(|_| "[]".into());
        ts.push_str(&format!(
            "    {{\n\
            \x20       nodeId: {},\n\
            \x20       name: {:?},\n\
            \x20       primitive: {:?},\n\
            \x20       shapeKeys: {},\n\
            \x20       grindRailCount: {},\n\
            \x20       hasUv2: {},\n\
            \x20   }},\n",
            asset.node_id.index,
            asset.node_name,
            asset.primitive,
            keys_json,
            asset.baked.grind_rail_splines.len(),
            !asset.baked.uv2.is_empty()
        ));
    }
    ts.push_str("];\n");

    ts
}

/// Generates Three.js TypeScript code defining a single baked Polyamour mesh node.
pub fn generate_node_scene_ts(asset: &ThreeJsParametricPolyamourAsset) -> String {
    let id_idx = asset.node_id.index;
    let node_var = format!("node_{id_idx}");
    let b = &asset.baked;

    let pos_str = format_float_array(&b.positions);
    let norm_str = format_float_array(&b.normals);
    let uv_str = format_float_array(&b.uvs);
    let uv2_str = format_float_array(&b.uv2);
    let idx_str = format_uint_array(&b.indices);

    let mut ts = String::new();
    ts.push_str(&format!(
        "    const geom_{id_idx} = new THREE.BufferGeometry();\n\
        \x20   const pos_{id_idx} = new Float32Array({pos_str});\n\
        \x20   const norm_{id_idx} = new Float32Array({norm_str});\n\
        \x20   const uv_{id_idx} = new Float32Array({uv_str});\n\
        \x20   const uv2_{id_idx} = new Float32Array({uv2_str});\n\
        \x20   const idx_{id_idx} = new Uint32Array({idx_str});\n\
        \x20   geom_{id_idx}.setAttribute('position', new THREE.BufferAttribute(pos_{id_idx}, 3));\n\
        \x20   geom_{id_idx}.setAttribute('normal', new THREE.BufferAttribute(norm_{id_idx}, 3));\n\
        \x20   geom_{id_idx}.setAttribute('uv', new THREE.BufferAttribute(uv_{id_idx}, 2));\n\
        \x20   geom_{id_idx}.setAttribute('uv2', new THREE.BufferAttribute(uv2_{id_idx}, 2));\n\
        \x20   geom_{id_idx}.setAttribute('uv1', new THREE.BufferAttribute(uv2_{id_idx}, 2));\n\
        \x20   geom_{id_idx}.setIndex(new THREE.BufferAttribute(idx_{id_idx}, 1));\n"
    ));

    // Morph target attributes
    if !b.morph_targets.is_empty() {
        ts.push_str(&format!(
            "    geom_{id_idx}.morphAttributes.position = [\n"
        ));
        for target_data in b.morph_targets.values() {
            let target_str = format_float_array(target_data);
            ts.push_str(&format!(
                "        new THREE.BufferAttribute(new Float32Array({target_str}), 3),\n"
            ));
        }
        ts.push_str("    ];\n");
    }

    // Material and Mesh
    ts.push_str(&format!(
        "    const mat_{id_idx} = new THREE.MeshStandardMaterial({{\n\
        \x20       color: {},\n\
        \x20       metalness: {},\n\
        \x20       roughness: {},\n\
        \x20   }});\n\
        \x20   const {node_var} = new THREE.Mesh(geom_{id_idx}, mat_{id_idx});\n\
        \x20   {node_var}.castShadow = true;\n\
        \x20   {node_var}.receiveShadow = true;\n\
        \x20   {node_var}.userData.polyamourAsset = true;\n",
        b.color_hex, b.metalness, b.roughness
    ));

    // Morph target dictionary & influences
    if !b.morph_targets.is_empty() {
        let mut dict_entries = Vec::new();
        let mut influences = Vec::new();
        for (idx, name) in b.morph_targets.keys().enumerate() {
            dict_entries.push(format!("{:?}: {idx}", name));
            influences.push("0.0".to_string());
        }
        ts.push_str(&format!(
            "    {node_var}.morphTargetDictionary = {{ {} }};\n\
            \x20   {node_var}.morphTargetInfluences = [ {} ];\n",
            dict_entries.join(", "),
            influences.join(", ")
        ));
    }

    // Skate grind splines
    if !b.grind_rail_splines.is_empty() {
        ts.push_str(&format!(
            "    {node_var}.userData.grindSplines = [\n"
        ));
        for (i, spline) in b.grind_rail_splines.iter().enumerate() {
            let closed = b.spline_closed_flags.get(i).copied().unwrap_or(false);
            let mut pt_strs = Vec::new();
            for pt in spline {
                pt_strs.push(format!("new THREE.Vector3({:.3}, {:.3}, {:.3})", pt[0], pt[1], pt[2]));
            }
            ts.push_str(&format!(
                "        new THREE.CatmullRomCurve3([\n\
                \x20           {}\n\
                \x20       ], {}, 'catmullrom', 0.5),\n",
                pt_strs.join(",\n            "),
                closed
            ));
        }
        ts.push_str("    ];\n");
    }

    ts
}

// ----------------- Helpers -----------------

fn build_primitive_mesh(primitive_name: &str, props: &BTreeMap<String, IrValue>) -> PolyMesh {
    let clean = primitive_name.to_ascii_lowercase();
    match clean.as_str() {
        "quarterpipe" => {
            let radius = extract_num_prop(props, "radius", 2.0) as f32;
            let width = extract_num_prop(props, "width", 4.0) as f32;
            let segments = extract_num_prop(props, "segments", 6.0) as u32;
            primitive_quarterpipe(radius, width, segments.max(3))
        }
        "stairset" | "stair_set" | "stairs" => {
            let width = extract_num_prop(props, "width", 3.0) as f32;
            let height = extract_num_prop(props, "height", 2.0) as f32;
            let depth = extract_num_prop(props, "depth", 3.0) as f32;
            let count = extract_num_prop(props, "step_count", 4.0) as u32;
            primitive_stair_set(width, height, depth, count.max(2))
        }
        "torus" => {
            let major = extract_num_prop(props, "major_radius", 2.0) as f32;
            let minor = extract_num_prop(props, "minor_radius", 0.5) as f32;
            let major_seg = extract_num_prop(props, "major_segments", 12.0) as u32;
            let minor_seg = extract_num_prop(props, "minor_segments", 8.0) as u32;
            primitive_torus(major, minor, major_seg.max(4), minor_seg.max(3))
        }
        "cylinder" => {
            let radius = extract_num_prop(props, "radius", 1.0) as f32;
            let height = extract_num_prop(props, "height", 2.0) as f32;
            let seg = extract_num_prop(props, "segments", 12.0) as u32;
            primitive_cylinder(radius, height, seg.max(3))
        }
        "cone" => {
            let radius = extract_num_prop(props, "radius", 1.0) as f32;
            let height = extract_num_prop(props, "height", 2.0) as f32;
            let seg = extract_num_prop(props, "segments", 12.0) as u32;
            primitive_cone(radius, height, seg.max(3))
        }
        "icosphere" | "ico_sphere" => {
            let radius = extract_num_prop(props, "radius", 1.0) as f32;
            let sub = extract_num_prop(props, "subdivisions", 1.0) as u32;
            primitive_ico_sphere(radius, sub)
        }
        "uvsphere" | "uv_sphere" | "sphere" => {
            let radius = extract_num_prop(props, "radius", 1.0) as f32;
            let rings = extract_num_prop(props, "rings", 8.0) as u32;
            let seg = extract_num_prop(props, "segments", 12.0) as u32;
            primitive_uv_sphere(radius, rings.max(3), seg.max(3))
        }
        "plane" => {
            let size = extract_num_prop(props, "size", 4.0) as f32;
            primitive_plane(size)
        }
        "grid" => {
            let w = extract_num_prop(props, "width", 10.0) as f32;
            let d = extract_num_prop(props, "depth", 10.0) as f32;
            let sx = extract_num_prop(props, "subs_x", 4.0) as u32;
            let sz = extract_num_prop(props, "subs_z", 4.0) as u32;
            primitive_grid(w, d, sx.max(1), sz.max(1))
        }
        _ => {
            let size = extract_num_prop(props, "size", 2.0) as f32;
            primitive_cube(size)
        }
    }
}

fn extract_semantic_properties(
    props: &BTreeMap<String, IrValue>,
    attrs: &mut PolyamourSemanticAttributes,
) {
    // If grind_edges property is set (e.g. JSON array of pairs `[[0, 1], [1, 2]]`)
    if let Some(val) = props.get("grind_edges").or_else(|| props.get("grind_edge")) {
        match val {
            IrValue::String(s) => {
                if let Ok(pairs) = serde_json::from_str::<Vec<[u32; 2]>>(s) {
                    for edge in pairs {
                        attrs.tag_edge(edge, "grind_edge");
                    }
                }
            }
            IrValue::Array(arr) => {
                for item in arr {
                    if let IrValue::Array(pair) = item
                        && pair.len() == 2
                    {
                        let a_opt = match &pair[0] {
                            IrValue::I64(i) => Some(*i as u32),
                            IrValue::F64(f) => Some(*f as u32),
                            _ => None,
                        };
                        let b_opt = match &pair[1] {
                            IrValue::I64(i) => Some(*i as u32),
                            IrValue::F64(f) => Some(*f as u32),
                            _ => None,
                        };
                        if let (Some(a), Some(b)) = (a_opt, b_opt) {
                            attrs.tag_edge([a, b], "grind_edge");
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // Direct semantic tags map (e.g. JSON object)
    if let Some(IrValue::String(json_str)) = props.get("polyamour_semantic_tags")
        && let Ok(deser) = serde_json::from_str::<PolyamourSemanticAttributes>(json_str)
    {
        for (e, tags) in deser.edge_tags {
            for t in tags {
                attrs.tag_edge(e, t);
            }
        }
        for (f, tags) in deser.face_tags {
            for t in tags {
                attrs.tag_face(f, t);
            }
        }
    }
}

fn extract_modifiers(props: &BTreeMap<String, IrValue>) -> Vec<PolyModifier> {
    let mut modifiers = Vec::new();

    // 1. Subsurf
    if let Some(subsurf) = props.get("polyamour_subsurf") {
        let levels = match subsurf {
            IrValue::I64(i) => *i as u8,
            IrValue::F64(f) => *f as u8,
            _ => 1,
        };
        if levels > 0 {
            modifiers.push(PolyModifier::Subdivision {
                levels: levels.min(3),
                simple: false,
            });
        }
    }

    // 2. Mirror
    if let Some(mirror) = props.get("polyamour_mirror") {
        let is_mirror = match mirror {
            IrValue::Bool(b) => *b,
            _ => true,
        };
        if is_mirror {
            modifiers.push(PolyModifier::Mirror {
                axis_x: true,
                axis_y: false,
                axis_z: false,
                merge_distance: 0.001,
            });
        }
    }

    // 3. Array
    if let Some(arr_val) = props.get("polyamour_array") {
        let count = match arr_val {
            IrValue::I64(i) => *i as u32,
            IrValue::F64(f) => *f as u32,
            _ => 2,
        };
        if count > 1 {
            let offset_x = extract_num_prop(props, "polyamour_array_offset_x", 1.0) as f32;
            let offset_y = extract_num_prop(props, "polyamour_array_offset_y", 0.0) as f32;
            let offset_z = extract_num_prop(props, "polyamour_array_offset_z", 0.0) as f32;
            modifiers.push(PolyModifier::Array {
                count,
                relative_offset: Vec3::new(offset_x, offset_y, offset_z),
                constant_offset: Vec3::ZERO,
            });
        }
    }

    // 4. Solidify
    if let Some(solid_val) = props.get("polyamour_solidify") {
        let thickness = match solid_val {
            IrValue::F64(f) => *f as f32,
            IrValue::I64(i) => *i as f32,
            _ => 0.1,
        };
        if thickness > 1e-4 {
            modifiers.push(PolyModifier::Solidify { thickness });
        }
    }

    // 5. Displace
    if let Some(disp_val) = props.get("polyamour_displace") {
        let strength = match disp_val {
            IrValue::F64(f) => *f as f32,
            IrValue::I64(i) => *i as f32,
            _ => 0.2,
        };
        let freq = extract_num_prop(props, "polyamour_displace_freq", 2.0) as f32;
        modifiers.push(PolyModifier::Displace {
            strength,
            frequency: freq,
        });
    }

    // 6. JSON modifier stack list
    if let Some(IrValue::String(json_str)) = props.get("polyamour_modifiers") {
        #[derive(Deserialize)]
        #[serde(tag = "type")]
        enum ModifierDto {
            Subdivision { levels: u8, simple: Option<bool> },
            Mirror { axis_x: Option<bool>, axis_y: Option<bool>, axis_z: Option<bool> },
            Array { count: u32, offset: Option<[f32; 3]> },
            Solidify { thickness: f32 },
            Displace { strength: f32, frequency: Option<f32> },
            Bevel { width: f32 },
            Smooth { factor: f32, iterations: Option<u32> },
        }

        if let Ok(dtos) = serde_json::from_str::<Vec<ModifierDto>>(json_str) {
            for dto in dtos {
                match dto {
                    ModifierDto::Subdivision { levels, simple } => {
                        modifiers.push(PolyModifier::Subdivision {
                            levels,
                            simple: simple.unwrap_or(false),
                        });
                    }
                    ModifierDto::Mirror { axis_x, axis_y, axis_z } => {
                        modifiers.push(PolyModifier::Mirror {
                            axis_x: axis_x.unwrap_or(true),
                            axis_y: axis_y.unwrap_or(false),
                            axis_z: axis_z.unwrap_or(false),
                            merge_distance: 0.001,
                        });
                    }
                    ModifierDto::Array { count, offset } => {
                        let off = offset.unwrap_or([1.0, 0.0, 0.0]);
                        modifiers.push(PolyModifier::Array {
                            count,
                            relative_offset: Vec3::new(off[0], off[1], off[2]),
                            constant_offset: Vec3::ZERO,
                        });
                    }
                    ModifierDto::Solidify { thickness } => {
                        modifiers.push(PolyModifier::Solidify { thickness });
                    }
                    ModifierDto::Displace { strength, frequency } => {
                        modifiers.push(PolyModifier::Displace {
                            strength,
                            frequency: frequency.unwrap_or(2.0),
                        });
                    }
                    ModifierDto::Bevel { width } => {
                        modifiers.push(PolyModifier::Bevel { width });
                    }
                    ModifierDto::Smooth { factor, iterations } => {
                        modifiers.push(PolyModifier::Smooth {
                            factor,
                            iterations: iterations.unwrap_or(2),
                        });
                    }
                }
            }
        }
    }

    modifiers
}

fn extract_shape_keys(props: &BTreeMap<String, IrValue>, mesh: &PolyMesh) -> ShapeKeySet {
    let mut shape_keys = ShapeKeySet::from_polymesh(mesh);

    if let Some(IrValue::String(json_str)) = props.get("polyamour_shape_keys")
        && let Ok(map) = serde_json::from_str::<BTreeMap<String, Vec<[f32; 3]>>>(json_str)
    {
        for (name, pts) in map {
            let mut vec_pts: Vec<Vec3> = pts.into_iter().map(|p| Vec3::new(p[0], p[1], p[2])).collect();
            while vec_pts.len() < mesh.positions.len() {
                vec_pts.push(mesh.positions[vec_pts.len()]);
            }
            vec_pts.truncate(mesh.positions.len());
            shape_keys.add_shape_key(name, vec_pts);
        }
    }

    if let Some(IrValue::String(json_str)) = props.get("shape_key_deltas")
        && let Ok(map) = serde_json::from_str::<BTreeMap<String, Vec<[f32; 3]>>>(json_str)
    {
        for (name, deltas) in map {
            let mut vec_deltas: Vec<Vec3> = deltas.into_iter().map(|p| Vec3::new(p[0], p[1], p[2])).collect();
            while vec_deltas.len() < mesh.positions.len() {
                vec_deltas.push(Vec3::ZERO);
            }
            vec_deltas.truncate(mesh.positions.len());
            shape_keys.sculpt_shape_key_delta(name, &vec_deltas);
        }
    }

    shape_keys
}

fn extract_num_prop(props: &BTreeMap<String, IrValue>, key: &str, default: f64) -> f64 {
    match props.get(key) {
        Some(IrValue::F64(f)) => *f,
        Some(IrValue::I64(i)) => *i as f64,
        _ => default,
    }
}

fn extract_color_hex(props: &BTreeMap<String, IrValue>) -> String {
    if let Some(IrValue::Color([r, g, b, _])) = props.get("color") {
        let ir = (r.clamp(0.0, 1.0) * 255.0).round() as u32;
        let ig = (g.clamp(0.0, 1.0) * 255.0).round() as u32;
        let ib = (b.clamp(0.0, 1.0) * 255.0).round() as u32;
        format!("0x{:02x}{:02x}{:02x}", ir, ig, ib)
    } else {
        "0x38bdf8".into()
    }
}

fn format_float_array(floats: &[f32]) -> String {
    let mut out = String::from("[");
    for (i, &f) in floats.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        if f.fract() == 0.0 {
            out.push_str(&format!("{f:.1}"));
        } else {
            out.push_str(&format!("{f:.4}"));
        }
    }
    out.push(']');
    out
}

fn format_uint_array(uints: &[u32]) -> String {
    let mut out = String::from("[");
    for (i, &u) in uints.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&u.to_string());
    }
    out.push(']');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_polyamour_asset_extraction_from_primitive() {
        let mut props = BTreeMap::new();
        props.insert("polyamour_primitive".into(), IrValue::String("Quarterpipe".into()));
        props.insert("radius".into(), IrValue::F64(3.0));
        props.insert("width".into(), IrValue::F64(5.0));

        let node = IrNode {
            id: NodeId::new(1, 1).unwrap(),
            name: "TestQuarterpipe".into(),
            class_name: "MeshInstance3D".into(),
            parent: None,
            instance_scene: None,
            properties: props,
            groups: vec![],
        };

        let asset = ThreeJsParametricPolyamourAsset::from_node(&node).expect("Asset extracted");
        assert_eq!(asset.primitive, "Quarterpipe");
        assert!(!asset.baked.positions.is_empty());
        assert_eq!(asset.baked.positions.len(), asset.baked.normals.len());
        assert_eq!(asset.baked.positions.len() / 3, asset.baked.uv2.len() / 2);
    }

    #[test]
    fn test_polyamour_uv2_lightmap_attribute_emission() {
        let mut props = BTreeMap::new();
        props.insert("polyamour_primitive".into(), IrValue::String("Cube".into()));
        let node = IrNode {
            id: NodeId::new(2, 1).unwrap(),
            name: "LightmapCube".into(),
            class_name: "MeshInstance3D".into(),
            parent: None,
            instance_scene: None,
            properties: props,
            groups: vec![],
        };

        let asset = ThreeJsParametricPolyamourAsset::from_node(&node).unwrap();
        assert!(!asset.baked.uv2.is_empty());
        let code = generate_node_scene_ts(&asset);
        assert!(code.contains("setAttribute('uv2'"));
        assert!(code.contains("setAttribute('uv1'"));
    }

    #[test]
    fn test_polyops_2_transcript_replay_in_threejs() {
        let mut props = BTreeMap::new();
        props.insert("polyamour_primitive".into(), IrValue::String("Cube".into()));
        props.insert(
            "polyops_v2".into(),
            IrValue::String(
                "polyops 2\n\
                select_face 4\n\
                extrude_faces 4 distance=1.5\n\
                tag_edge 0-1 tag=\"grind_edge\"\n"
                    .into(),
            ),
        );

        let node = IrNode {
            id: NodeId::new(3, 1).unwrap(),
            name: "TranscriptBuilding".into(),
            class_name: "MeshInstance3D".into(),
            parent: None,
            instance_scene: None,
            properties: props,
            groups: vec![],
        };

        let asset = ThreeJsParametricPolyamourAsset::from_node(&node).unwrap();
        assert!(!asset.baked.grind_rail_splines.is_empty());
        let code = generate_node_scene_ts(&asset);
        assert!(code.contains("userData.grindSplines"));
        assert!(code.contains("new THREE.CatmullRomCurve3"));
    }

    #[test]
    fn test_shape_keys_morph_attributes_emission() {
        let mut props = BTreeMap::new();
        props.insert("polyamour_primitive".into(), IrValue::String("Cube".into()));

        // Cube has 8 vertices. Provide 8 delta offsets for a "Smile" morph target
        let deltas = vec![[0.1, 0.2, 0.0]; 8];
        let mut deltas_map = BTreeMap::new();
        deltas_map.insert("Smile".to_string(), deltas);
        let json_str = serde_json::to_string(&deltas_map).unwrap();
        props.insert("shape_key_deltas".into(), IrValue::String(json_str));

        let node = IrNode {
            id: NodeId::new(4, 1).unwrap(),
            name: "MorphCube".into(),
            class_name: "MeshInstance3D".into(),
            parent: None,
            instance_scene: None,
            properties: props,
            groups: vec![],
        };

        let asset = ThreeJsParametricPolyamourAsset::from_node(&node).unwrap();
        assert!(asset.baked.morph_targets.contains_key("Smile"));

        let code = generate_node_scene_ts(&asset);
        assert!(code.contains("morphAttributes.position = ["));
        assert!(code.contains("morphTargetDictionary = { \"Smile\": 0 }"));
        assert!(code.contains("morphTargetInfluences = [ 0.0 ]"));
    }

    #[test]
    fn test_grind_edge_catmull_rom_spline_emission() {
        let mut props = BTreeMap::new();
        props.insert("polyamour_primitive".into(), IrValue::String("Cube".into()));
        props.insert("grind_edges".into(), IrValue::String("[[0, 1], [1, 2], [2, 3]]".into()));

        let node = IrNode {
            id: NodeId::new(5, 1).unwrap(),
            name: "SkateRail".into(),
            class_name: "MeshInstance3D".into(),
            parent: None,
            instance_scene: None,
            properties: props,
            groups: vec![],
        };

        let asset = ThreeJsParametricPolyamourAsset::from_node(&node).unwrap();
        assert!(!asset.baked.grind_rail_splines.is_empty());
        let spline = &asset.baked.grind_rail_splines[0];
        assert!(spline.len() >= 3);

        let code = generate_node_scene_ts(&asset);
        assert!(code.contains("new THREE.CatmullRomCurve3"));
    }

    #[test]
    fn test_closed_loop_grind_rail_spline() {
        let mut props = BTreeMap::new();
        props.insert("polyamour_primitive".into(), IrValue::String("Cube".into()));
        props.insert(
            "grind_edges".into(),
            IrValue::String("[[0, 1], [1, 2], [2, 3], [3, 0]]".into()),
        );

        let node = IrNode {
            id: NodeId::new(6, 1).unwrap(),
            name: "ClosedRail".into(),
            class_name: "MeshInstance3D".into(),
            parent: None,
            instance_scene: None,
            properties: props,
            groups: vec![],
        };

        let asset = ThreeJsParametricPolyamourAsset::from_node(&node).unwrap();
        assert!(!asset.baked.grind_rail_splines.is_empty());
        assert!(asset.baked.spline_closed_flags.contains(&true));

        let code = generate_node_scene_ts(&asset);
        assert!(code.contains("], true, 'catmullrom', 0.5)"));
    }

    #[test]
    fn test_modifier_stack_solidify_and_array_baking() {
        let mut props = BTreeMap::new();
        props.insert("polyamour_primitive".into(), IrValue::String("Plane".into()));
        props.insert("polyamour_solidify".into(), IrValue::F64(0.3));
        props.insert("polyamour_array".into(), IrValue::I64(3));

        let node = IrNode {
            id: NodeId::new(7, 1).unwrap(),
            name: "ModifierPlane".into(),
            class_name: "MeshInstance3D".into(),
            parent: None,
            instance_scene: None,
            properties: props,
            groups: vec![],
        };

        let asset = ThreeJsParametricPolyamourAsset::from_node(&node).unwrap();
        assert!(!asset.baked.positions.is_empty());
        // Solidified + arrayed plane should have significantly more vertices than single quad (4 * 3 = 12 floats)
        assert!(asset.baked.positions.len() > 12);
    }

    #[test]
    fn test_generate_polyamour_runtime_ts_content() {
        let mut props = BTreeMap::new();
        props.insert("polyamour_primitive".into(), IrValue::String("Quarterpipe".into()));
        let node = IrNode {
            id: NodeId::new(8, 1).unwrap(),
            name: "Halfpipe".into(),
            class_name: "MeshInstance3D".into(),
            parent: None,
            instance_scene: None,
            properties: props,
            groups: vec![],
        };

        let asset = ThreeJsParametricPolyamourAsset::from_node(&node).unwrap();
        let ts = generate_polyamour_runtime_ts(&[asset]);

        assert!(ts.contains("export class ParametricPolyamourController"));
        assert!(ts.contains("setShapeKeyWeight("));
        assert!(ts.contains("sampleGrindRail("));
        assert!(ts.contains("findNearestGrindRailPoint("));
        assert!(ts.contains("updateDisplaceModifier("));
        assert!(ts.contains("updateSolidifyModifier("));
        assert!(ts.contains("updateArrayModifier("));
        assert!(ts.contains("bindFeelKnobs("));
        assert!(ts.contains("BAKED_POLYAMOUR_ASSET_METAS"));
    }

    #[test]
    fn test_empty_polyamour_scene_still_compiles_runtime() {
        let ts = generate_polyamour_runtime_ts(&[]);
        assert!(ts.contains("export class ParametricPolyamourController"));
        assert!(ts.contains("BAKED_POLYAMOUR_ASSET_METAS: ParametricPolyamourAssetMeta[] = [\n];"));
    }

    #[test]
    fn test_multi_node_polyamour_assets_extraction() {
        let mut scene = IrScene::new("SkateHaven");
        let n1_id = NodeId::new(10, 1).unwrap();
        let n2_id = NodeId::new(11, 1).unwrap();

        let mut p1 = BTreeMap::new();
        p1.insert("polyamour_primitive".into(), IrValue::String("Cube".into()));
        scene.nodes.insert(
            n1_id,
            IrNode {
                id: n1_id,
                name: "Ramp1".into(),
                class_name: "MeshInstance3D".into(),
                parent: None,
                instance_scene: None,
                properties: p1,
                groups: vec![],
            },
        );

        let mut p2 = BTreeMap::new();
        p2.insert("polyamour_primitive".into(), IrValue::String("Torus".into()));
        scene.nodes.insert(
            n2_id,
            IrNode {
                id: n2_id,
                name: "Loop".into(),
                class_name: "MeshInstance3D".into(),
                parent: None,
                instance_scene: None,
                properties: p2,
                groups: vec![],
            },
        );

        let assets = extract_polyamour_assets_from_scene(&scene);
        assert_eq!(assets.len(), 2);
    }
}
