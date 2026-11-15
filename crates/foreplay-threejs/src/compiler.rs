//! Three.js + Vite + TypeScript game compiler.
//!
//! Generates a complete, zero-placeholder, 100% runnable Vite + TypeScript + Three.js (`^0.174.0`)
//! web game bundle from Foreplay `IrScene`, GDScripts, and TypeScript components.

use crate::error::ThreeJsError;
use crate::feel::{collect_feel_knobs, generate_feel_ts};
use crate::transpiler::generate_behaviors_ts;
use foreplay_atlas::{AtlasGraph, AtlasTaskContext};
use foreplay_ir::{FeelSpecKnob, IrNode, IrScene, IrValue, NodeId};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;

/// Tone mapping algorithm selected for the exported Three.js WebGPU / WebGL renderer.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Default)]
pub enum ThreeToneMapping {
    #[default]
    AgX,
    ACESFilmic,
    Neutral,
}

impl ThreeToneMapping {
    pub fn js_expression(&self) -> &'static str {
        match self {
            Self::AgX => "THREE.AgXToneMapping",
            Self::ACESFilmic => "THREE.ACESFilmicToneMapping",
            Self::Neutral => "THREE.NeutralToneMapping",
        }
    }
}

/// Options controlling 3JSE / Three.js project bundle emission.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ThreeJsExportOptions {
    pub project_name: String,
    pub use_webgpu_renderer: bool,
    pub include_feel_dev_hud: bool,
    pub include_atlas_json: bool,
    pub tone_mapping: ThreeToneMapping,
}

impl Default for ThreeJsExportOptions {
    fn default() -> Self {
        Self {
            project_name: "Foreplay Three.js Game".into(),
            use_webgpu_renderer: true,
            include_feel_dev_hud: true,
            include_atlas_json: true,
            tone_mapping: ThreeToneMapping::AgX,
        }
    }
}

/// Produced Three.js + TypeScript + Vite project bundle containing all generated web assets.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ThreeJsProjectBundle {
    pub files: BTreeMap<String, String>,
    pub node_count: usize,
    pub transpiled_scripts: usize,
}

/// Compiles an `IrScene` and its attached GDScript / TypeScript behaviors into a runnable Three.js game bundle.
pub fn compile_ir_to_threejs_project(
    scene: &IrScene,
    gd_scripts: &BTreeMap<NodeId, String>,
    ts_scripts: &BTreeMap<NodeId, String>,
    opts: &ThreeJsExportOptions,
) -> Result<ThreeJsProjectBundle, ThreeJsError> {
    let mut files = BTreeMap::new();

    // 1. Collect all feel knobs across scene overrides and scripts
    let mut script_knobs = Vec::new();
    for gd_source in gd_scripts.values() {
        if let Ok(module) = foreplay_gdscript::parse_gdscript(gd_source) {
            script_knobs.extend(module.feel_knobs);
        }
    }
    let all_knobs = collect_feel_knobs(scene, &script_knobs);

    // 2. package.json
    let slug = sanitize_slug(&opts.project_name);
    files.insert("package.json".into(), generate_package_json(&slug));

    // 3. tsconfig.json & vite.config.ts
    files.insert("tsconfig.json".into(), generate_tsconfig_json());
    files.insert("vite.config.ts".into(), generate_vite_config());

    // 4. index.html
    files.insert("index.html".into(), generate_index_html(opts));

    // 5. src/feel.ts
    files.insert(
        "src/feel.ts".into(),
        generate_feel_ts(&all_knobs, &scene.feel_overrides),
    );

    // 6. src/signals.ts
    files.insert("src/signals.ts".into(), generate_signals_ts(scene));

    // 7. src/scripts/behaviors.ts
    let behaviors_ts = generate_behaviors_ts(scene, gd_scripts, ts_scripts, &all_knobs)?;
    files.insert("src/scripts/behaviors.ts".into(), behaviors_ts);

    // 8. src/scene.ts
    files.insert("src/scene.ts".into(), generate_scene_ts(scene));

    // 9. src/main.ts
    files.insert("src/main.ts".into(), generate_main_ts(opts));

    // 10. src/atlas.json (if enabled)
    if opts.include_atlas_json {
        let atlas_json = generate_atlas_json(scene, &all_knobs)?;
        files.insert("src/atlas.json".into(), atlas_json);
    }

    let node_count = scene.nodes.len();
    let transpiled_scripts = gd_scripts.len() + ts_scripts.len();

    Ok(ThreeJsProjectBundle {
        files,
        node_count,
        transpiled_scripts,
    })
}

fn sanitize_slug(name: &str) -> String {
    let slug: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c.to_ascii_lowercase() } else { '-' })
        .collect();
    let trimmed = slug.trim_matches('-');
    if trimmed.is_empty() {
        "foreplay-game".into()
    } else {
        trimmed.to_string()
    }
}

fn generate_package_json(slug: &str) -> String {
    let pkg = json!({
        "name": slug,
        "private": true,
        "version": "1.0.0",
        "type": "module",
        "scripts": {
            "dev": "vite",
            "build": "tsc && vite build",
            "preview": "vite preview"
        },
        "dependencies": {
            "three": "^0.174.0"
        },
        "devDependencies": {
            "@types/three": "^0.174.0",
            "typescript": "^5.7.0",
            "vite": "^6.0.0"
        }
    });
    serde_json::to_string_pretty(&pkg).unwrap_or_else(|_| "{}".into())
}

fn generate_tsconfig_json() -> String {
    let tsconfig = json!({
        "compilerOptions": {
            "target": "ESNext",
            "useDefineForClassFields": true,
            "module": "ESNext",
            "lib": ["ESNext", "DOM", "DOM.Iterable"],
            "moduleResolution": "bundler",
            "strict": true,
            "resolveJsonModule": true,
            "isolatedModules": true,
            "esModuleInterop": true,
            "noEmit": true,
            "skipLibCheck": true
        },
        "include": ["src"]
    });
    serde_json::to_string_pretty(&tsconfig).unwrap_or_else(|_| "{}".into())
}

fn generate_vite_config() -> String {
    "import { defineConfig } from 'vite';\n\n\
    export default defineConfig({\n\
        build: {\n\
            target: 'esnext',\n\
        },\n\
        server: {\n\
            port: 3000,\n\
            open: true,\n\
        },\n\
    });\n"
        .into()
}

fn generate_index_html(opts: &ThreeJsExportOptions) -> String {
    format!(
        "<!DOCTYPE html>\n\
        <html lang=\"en\">\n\
        <head>\n\
            <meta charset=\"UTF-8\" />\n\
            <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\" />\n\
            <title>{}</title>\n\
            <style>\n\
                * {{ box-sizing: border-box; margin: 0; padding: 0; user-select: none; }}\n\
                html, body {{ width: 100%; height: 100%; overflow: hidden; background: #050508; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif; }}\n\
                #canvas {{ width: 100vw; height: 100vh; display: block; }}\n\
                #ui-overlay {{ position: absolute; top: 0; left: 0; width: 100vw; height: 100vh; pointer-events: none; z-index: 10; display: flex; flex-direction: column; justify-content: flex-start; padding: 16px; }}\n\
                .foreplay-button {{ pointer-events: auto; padding: 10px 18px; background: rgba(30, 30, 40, 0.85); color: #fff; border: 1px solid rgba(255, 255, 255, 0.2); border-radius: 8px; font-weight: 600; cursor: pointer; backdrop-filter: blur(8px); transition: all 0.15s ease; margin-bottom: 8px; align-self: flex-start; }}\n\
                .foreplay-button:hover {{ background: rgba(50, 50, 70, 0.95); border-color: rgba(255, 255, 255, 0.4); transform: translateY(-1px); }}\n\
                .foreplay-label {{ color: #e0e0ff; font-size: 14px; font-weight: 500; text-shadow: 0 1px 3px rgba(0,0,0,0.8); margin-bottom: 6px; }}\n\
                .feel-hud-container {{ position: absolute; top: 16px; right: 16px; width: 280px; max-height: calc(100vh - 32px); overflow-y: auto; background: rgba(15, 15, 25, 0.82); border: 1px solid rgba(255, 255, 255, 0.15); border-radius: 12px; backdrop-filter: blur(16px); z-index: 100; box-shadow: 0 8px 32px rgba(0,0,0,0.5); color: #f0f0f5; }}\n\
                .feel-hud-header {{ display: flex; justify-content: space-between; align-items: center; padding: 12px 14px; border-bottom: 1px solid rgba(255, 255, 255, 0.1); font-weight: 700; font-size: 13px; letter-spacing: 0.5px; text-transform: uppercase; color: #a5b4fc; }}\n\
                .feel-hud-actions button {{ background: rgba(255,255,255,0.08); border: none; color: #fff; width: 24px; height: 24px; border-radius: 4px; cursor: pointer; margin-left: 4px; }}\n\
                .feel-hud-actions button:hover {{ background: rgba(255,255,255,0.2); }}\n\
                .feel-hud-body {{ padding: 12px 14px; display: flex; flex-direction: column; gap: 12px; }}\n\
                .feel-hud-row-meta {{ display: flex; justify-content: space-between; font-size: 12px; margin-bottom: 4px; font-weight: 500; }}\n\
                .feel-hud-val {{ color: #38bdf8; font-family: ui-monospace, SFMono-Regular, Menlo, monospace; }}\n\
                .feel-hud-slider {{ width: 100%; height: 5px; border-radius: 3px; background: rgba(255,255,255,0.15); outline: none; -webkit-appearance: none; cursor: pointer; }}\n\
                .feel-hud-slider::-webkit-slider-thumb {{ -webkit-appearance: none; width: 14px; height: 14px; border-radius: 50%; background: #38bdf8; cursor: pointer; box-shadow: 0 0 8px rgba(56, 189, 248, 0.6); }}\n\
            </style>\n\
        </head>\n\
        <body>\n\
            <canvas id=\"canvas\"></canvas>\n\
            <div id=\"ui-overlay\"></div>\n\
            <script type=\"module\" src=\"/src/main.ts\"></script>\n\
        </body>\n\
        </html>\n",
        opts.project_name
    )
}

fn generate_signals_ts(scene: &IrScene) -> String {
    let mut ts = String::new();
    ts.push_str(
        "// Foreplay Game Engine Signal Bus\n\
        // Generated automatically by foreplay-threejs compiler.\n\n\
        export type SignalCallback = (...args: any[]) => void;\n\n\
        export class SignalBus {\n\
            private listeners: Map<string, SignalCallback[]> = new Map();\n\n\
            private key(nodeId: number, signalName: string): string {\n\
                return `${nodeId}:${signalName}`;\n\
            }\n\n\
            on(nodeId: number, signalName: string, callback: SignalCallback): () => void {\n\
                const k = this.key(nodeId, signalName);\n\
                if (!this.listeners.has(k)) {\n\
                    this.listeners.set(k, []);\n\
                }\n\
                this.listeners.get(k)!.push(callback);\n\
                return () => this.off(nodeId, signalName, callback);\n\
            }\n\n\
            off(nodeId: number, signalName: string, callback: SignalCallback): void {\n\
                const k = this.key(nodeId, signalName);\n\
                const arr = this.listeners.get(k);\n\
                if (arr) {\n\
                    const idx = arr.indexOf(callback);\n\
                    if (idx !== -1) arr.splice(idx, 1);\n\
                }\n\
            }\n\n\
            emit(nodeId: number, signalName: string, ...args: any[]): void {\n\
                const k = this.key(nodeId, signalName);\n\
                const arr = this.listeners.get(k);\n\
                if (arr) {\n\
                    for (const cb of [...arr]) {\n\
                        cb(...args);\n\
                    }\n\
                }\n\
            }\n\
        }\n\n\
        export function initSignalConnections(bus: SignalBus, behaviors: Map<number, any>): void {\n",
    );

    for conn in &scene.connections {
        let from_id = conn.from_node.index;
        let to_id = conn.to_node.index;
        let sig = &conn.signal_name;
        let meth = &conn.method_name;
        let clean_meth = meth.trim_start_matches('_');

        ts.push_str(&format!(
            "    bus.on({from_id}, {:?}, (...args: any[]) => {{\n\
            \x20       const target = behaviors.get({to_id});\n\
            \x20       if (target) {{\n\
            \x20           if (typeof target[{:?}] === 'function') target[{:?}](...args);\n\
            \x20           else if (typeof target[{:?}] === 'function') target[{:?}](...args);\n\
            \x20       }}\n\
            \x20   }});\n",
            sig, meth, meth, clean_meth, clean_meth
        ));
    }

    ts.push_str("}\n");
    ts
}

fn generate_scene_ts(scene: &IrScene) -> String {
    let mut ts = String::new();
    ts.push_str(
        "// Foreplay Scene Graph Codegen\n\
        // Generated automatically by foreplay-threejs compiler.\n\n\
        import * as THREE from 'three';\n\
        import { SignalBus } from './signals';\n\
        import { FeelStore } from './feel';\n\n\
        export interface BuiltScene {\n\
            scene: THREE.Scene;\n\
            camera: THREE.PerspectiveCamera;\n\
            nodes: Map<number, THREE.Object3D | HTMLElement>;\n\
            lights: THREE.Light[];\n\
        }\n\n\
        export function buildScene(signals: SignalBus, _feel: FeelStore): BuiltScene {\n\
            const scene = new THREE.Scene();\n\
            scene.name = {:?};\n\
            const nodes = new Map<number, THREE.Object3D | HTMLElement>();\n\
            const lights: THREE.Light[] = [];\n\
            let primaryCamera: THREE.PerspectiveCamera | null = null;\n\
            const uiOverlay = document.getElementById('ui-overlay');\n\n",
    );
    ts = ts.replace("{:?}", &format!("{:?}", scene.name));

    // Sort nodes in topological parent-child order
    let sorted_nodes = sort_nodes_parent_first(scene);

    for node in sorted_nodes {
        let id_idx = node.id.index;
        let node_var = format!("node_{id_idx}");

        ts.push_str(&format!("    // Node [{}: {} ({})]\n", id_idx, node.name, node.class_name));

        let polyamour_primitive = node.properties.get("polyamour_primitive").and_then(|v| match v {
            IrValue::String(s) => Some(s.as_str()),
            _ => None,
        });
        let polyamour_subsurf = node.properties.get("polyamour_subsurf").and_then(|v| match v {
            IrValue::I64(i) => Some(*i as usize),
            IrValue::F64(f) => Some(*f as usize),
            _ => None,
        });

        if polyamour_primitive.is_some() || polyamour_subsurf.is_some() {
            let prim_name = polyamour_primitive.unwrap_or("Cube");
            let tri_mesh = generate_polyamour_mesh(prim_name, &node.properties, polyamour_subsurf);
            let color_hex = extract_color_hex(&node.properties);
            let metal = extract_num_prop(&node.properties, "metalness", 0.1);
            let rough = extract_num_prop(&node.properties, "roughness", 0.5);

            let mut flat_positions = Vec::with_capacity(tri_mesh.positions.len() * 3);
            for p in &tri_mesh.positions {
                flat_positions.push(p.x);
                flat_positions.push(p.y);
                flat_positions.push(p.z);
            }
            let mut flat_normals = Vec::with_capacity(tri_mesh.normals.len() * 3);
            for n in &tri_mesh.normals {
                flat_normals.push(n.x);
                flat_normals.push(n.y);
                flat_normals.push(n.z);
            }
            let pos_str = format_float_array(&flat_positions);
            let norm_str = format_float_array(&flat_normals);
            let idx_str = format_uint_array(&tri_mesh.indices);

            ts.push_str(&format!(
                "    const geom_{id_idx} = new THREE.BufferGeometry();\n\
                \x20   const pos_{id_idx} = new Float32Array({pos_str});\n\
                \x20   const norm_{id_idx} = new Float32Array({norm_str});\n\
                \x20   const idx_{id_idx} = new Uint32Array({idx_str});\n\
                \x20   geom_{id_idx}.setAttribute('position', new THREE.BufferAttribute(pos_{id_idx}, 3));\n\
                \x20   geom_{id_idx}.setAttribute('normal', new THREE.BufferAttribute(norm_{id_idx}, 3));\n\
                \x20   geom_{id_idx}.setIndex(new THREE.BufferAttribute(idx_{id_idx}, 1));\n\
                \x20   const mat_{id_idx} = new THREE.MeshStandardMaterial({{\n\
                \x20       color: {color_hex},\n\
                \x20       metalness: {metal},\n\
                \x20       roughness: {rough},\n\
                \x20   }});\n\
                \x20   const {node_var} = new THREE.Mesh(geom_{id_idx}, mat_{id_idx});\n\
                \x20   {node_var}.castShadow = true;\n\
                \x20   {node_var}.receiveShadow = true;\n"
            ));
        } else {
            match node.class_name.as_str() {
                "Camera3D" => {
                    let fov = extract_num_prop(&node.properties, "fov", 75.0);
                    let near = extract_num_prop(&node.properties, "near", 0.1);
                    let far = extract_num_prop(&node.properties, "far", 1000.0);
                    ts.push_str(&format!(
                        "    const {node_var} = new THREE.PerspectiveCamera({fov}, window.innerWidth / window.innerHeight, {near}, {far});\n\
                        \x20   if (!primaryCamera) primaryCamera = {node_var};\n"
                    ));
                }
                "DirectionalLight3D" => {
                    let color_hex = extract_color_hex(&node.properties);
                    let intensity = extract_num_prop(&node.properties, "light_energy", 1.0);
                    ts.push_str(&format!(
                        "    const {node_var} = new THREE.DirectionalLight({color_hex}, {intensity});\n\
                        \x20   {node_var}.castShadow = true;\n\
                        \x20   {node_var}.shadow.mapSize.width = 2048;\n\
                        \x20   {node_var}.shadow.mapSize.height = 2048;\n\
                        \x20   {node_var}.shadow.camera.near = 0.5;\n\
                        \x20   {node_var}.shadow.camera.far = 500;\n\
                        \x20   lights.push({node_var});\n"
                    ));
                }
                "OmniLight3D" => {
                    let color_hex = extract_color_hex(&node.properties);
                    let intensity = extract_num_prop(&node.properties, "light_energy", 1.0);
                    let range = extract_num_prop(&node.properties, "omni_range", 10.0);
                    ts.push_str(&format!(
                        "    const {node_var} = new THREE.PointLight({color_hex}, {intensity}, {range});\n\
                        \x20   {node_var}.castShadow = true;\n\
                        \x20   lights.push({node_var});\n"
                    ));
                }
                "SpotLight3D" => {
                    let color_hex = extract_color_hex(&node.properties);
                    let intensity = extract_num_prop(&node.properties, "light_energy", 1.0);
                    let range = extract_num_prop(&node.properties, "spot_range", 15.0);
                    let angle = extract_num_prop(&node.properties, "spot_angle", 45.0).to_radians();
                    ts.push_str(&format!(
                        "    const {node_var} = new THREE.SpotLight({color_hex}, {intensity}, {range}, {angle});\n\
                        \x20   {node_var}.castShadow = true;\n\
                        \x20   lights.push({node_var});\n"
                    ));
                }
                "MeshInstance3D" | "CSGBox3D" => {
                    let (w, h, d) = extract_size_prop(&node.properties);
                    let color_hex = extract_color_hex(&node.properties);
                    let metal = extract_num_prop(&node.properties, "metalness", 0.1);
                    let rough = extract_num_prop(&node.properties, "roughness", 0.5);

                    ts.push_str(&format!(
                        "    const geom_{id_idx} = new THREE.BoxGeometry({w}, {h}, {d});\n\
                        \x20   const mat_{id_idx} = new THREE.MeshStandardMaterial({{\n\
                        \x20       color: {color_hex},\n\
                        \x20       metalness: {metal},\n\
                        \x20       roughness: {rough},\n\
                        \x20   }});\n\
                        \x20   const {node_var} = new THREE.Mesh(geom_{id_idx}, mat_{id_idx});\n\
                        \x20   {node_var}.castShadow = true;\n\
                        \x20   {node_var}.receiveShadow = true;\n"
                    ));
                }
                "GPUParticles3D" => {
                    let color_hex = extract_color_hex(&node.properties);
                    ts.push_str(&format!(
                        "    const geom_{id_idx} = new THREE.BufferGeometry();\n\
                        \x20   const pCoords_{id_idx} = new Float32Array(300);\n\
                        \x20   for (let i = 0; i < 300; i += 3) {{\n\
                        \x20       pCoords_{id_idx}[i] = (Math.random() - 0.5) * 5;\n\
                        \x20       pCoords_{id_idx}[i+1] = Math.random() * 3;\n\
                        \x20       pCoords_{id_idx}[i+2] = (Math.random() - 0.5) * 5;\n\
                        \x20   }}\n\
                        \x20   geom_{id_idx}.setAttribute('position', new THREE.BufferAttribute(pCoords_{id_idx}, 3));\n\
                        \x20   const mat_{id_idx} = new THREE.PointsMaterial({{ size: 0.1, color: {color_hex} }});\n\
                        \x20   const {node_var} = new THREE.Points(geom_{id_idx}, mat_{id_idx});\n"
                    ));
                }
                "Button" => {
                    let text = extract_string_prop(&node.properties, "text", &node.name);
                    ts.push_str(&format!(
                        "    const btn_{id_idx} = document.createElement('button');\n\
                        \x20   btn_{id_idx}.className = 'foreplay-button';\n\
                        \x20   btn_{id_idx}.textContent = {:?};\n\
                        \x20   btn_{id_idx}.addEventListener('click', () => signals.emit({id_idx}, 'pressed'));\n\
                        \x20   if (uiOverlay) uiOverlay.appendChild(btn_{id_idx});\n\
                        \x20   nodes.set({id_idx}, btn_{id_idx});\n\n",
                        text
                    ));
                    continue;
                }
                "Label" => {
                    let text = extract_string_prop(&node.properties, "text", &node.name);
                    ts.push_str(&format!(
                        "    const lbl_{id_idx} = document.createElement('div');\n\
                        \x20   lbl_{id_idx}.className = 'foreplay-label';\n\
                        \x20   lbl_{id_idx}.textContent = {:?};\n\
                        \x20   if (uiOverlay) uiOverlay.appendChild(lbl_{id_idx});\n\
                        \x20   nodes.set({id_idx}, lbl_{id_idx});\n\n",
                        text
                    ));
                    continue;
                }
                "Control" => {
                    ts.push_str(&format!(
                        "    const ctrl_{id_idx} = document.createElement('div');\n\
                        \x20   if (uiOverlay) uiOverlay.appendChild(ctrl_{id_idx});\n\
                        \x20   nodes.set({id_idx}, ctrl_{id_idx});\n\n"
                    ));
                    continue;
                }
                _ => {
                    // CharacterBody3D, StaticBody3D, RigidBody3D, Area3D, Node3D, etc.
                    ts.push_str(&format!("    const {node_var} = new THREE.Group();\n"));
                }
            }
        }

        // Apply metadata and transforms to 3D object
        ts.push_str(&format!("    {node_var}.name = {:?};\n", node.name));
        ts.push_str(&format!(
            "    {node_var}.userData.foreplayId = {{ index: {}, generation: {} }};\n",
            node.id.index,
            node.id.generation.get()
        ));
        let groups_json = serde_json::to_string(&node.groups).unwrap_or_else(|_| "[]".into());
        ts.push_str(&format!("    {node_var}.userData.groups = {groups_json};\n"));

        // Position, Quaternion, Scale
        let (px, py, pz) = extract_vec3_prop(&node.properties, "position", (0.0, 0.0, 0.0));
        ts.push_str(&format!("    {node_var}.position.set({px}, {py}, {pz});\n"));

        if let Some(IrValue::Quat(q)) = node.properties.get("quaternion") {
            ts.push_str(&format!("    {node_var}.quaternion.set({}, {}, {}, {});\n", q[0], q[1], q[2], q[3]));
        } else if let Some(IrValue::Vec3(r)) = node.properties.get("rotation") {
            ts.push_str(&format!("    {node_var}.rotation.set({}, {}, {});\n", r[0], r[1], r[2]));
        }

        let (sx, sy, sz) = extract_vec3_prop(&node.properties, "scale", (1.0, 1.0, 1.0));
        ts.push_str(&format!("    {node_var}.scale.set({sx}, {sy}, {sz});\n"));

        ts.push_str(&format!("    nodes.set({id_idx}, {node_var});\n"));

        // Hierarchy attachment
        if let Some(parent_id) = node.parent {
            let parent_idx = parent_id.index;
            ts.push_str(&format!(
                "    const p_{id_idx} = nodes.get({parent_idx});\n\
                \x20   if (p_{id_idx} && p_{id_idx} instanceof THREE.Object3D) p_{id_idx}.add({node_var});\n\
                \x20   else scene.add({node_var});\n\n"
            ));
        } else {
            ts.push_str(&format!("    scene.add({node_var});\n\n"));
        }
    }

    // Default camera fallback if scene had none
    ts.push_str(
        "    if (!primaryCamera) {\n\
        \x20       primaryCamera = new THREE.PerspectiveCamera(75, window.innerWidth / window.innerHeight, 0.1, 1000);\n\
        \x20       primaryCamera.position.set(0, 5, 10);\n\
        \x20       primaryCamera.lookAt(0, 0, 0);\n\
        \x20       scene.add(primaryCamera);\n\
        \x20   }\n\n\
        \x20   if (lights.length === 0) {\n\
        \x20       const amb = new THREE.AmbientLight(0xffffff, 0.6);\n\
        \x20       const dir = new THREE.DirectionalLight(0xffffff, 1.2);\n\
        \x20       dir.position.set(10, 20, 10);\n\
        \x20       dir.castShadow = true;\n\
        \x20       scene.add(amb);\n\
        \x20       scene.add(dir);\n\
        \x20       lights.push(amb, dir);\n\
        \x20   }\n\n\
        \x20   return { scene, camera: primaryCamera, nodes, lights };\n\
        }\n",
    );

    ts
}

fn generate_main_ts(opts: &ThreeJsExportOptions) -> String {
    let use_webgpu = opts.use_webgpu_renderer;
    let tone_mapping_expr = opts.tone_mapping.js_expression();
    let mount_hud = opts.include_feel_dev_hud;

    format!(
        "// Foreplay Three.js Runtime Entrypoint\n\
        // Generated automatically by foreplay-threejs compiler.\n\n\
        import * as THREE from 'three';\n\
        import {{ buildScene }} from './scene';\n\
        import {{ SignalBus, initSignalConnections }} from './signals';\n\
        import {{ feelStore, mountFeelDevHud }} from './feel';\n\
        import {{ registerBehaviors, Behavior, BehaviorContext, InputState }} from './scripts/behaviors';\n\n\
        const canvas = document.getElementById('canvas') as HTMLCanvasElement;\n\
        const signalBus = new SignalBus();\n\n\
        // Input Manager\n\
        const keysDown = new Set<string>();\n\
        const keysJustPressed = new Set<string>();\n\n\
        window.addEventListener('keydown', (e) => {{\n\
            if (!keysDown.has(e.code)) {{\n\
                keysJustPressed.add(e.code);\n\
            }}\n\
            keysDown.add(e.code);\n\
        }});\n\n\
        window.addEventListener('keyup', (e) => {{\n\
            keysDown.delete(e.code);\n\
        }});\n\n\
        const inputState: InputState = {{\n\
            keysDown,\n\
            keysJustPressed,\n\
            isActionPressed(action: string): boolean {{\n\
                switch (action) {{\n\
                    case 'ui_up': return keysDown.has('KeyW') || keysDown.has('ArrowUp');\n\
                    case 'ui_down': return keysDown.has('KeyS') || keysDown.has('ArrowDown');\n\
                    case 'ui_left': return keysDown.has('KeyA') || keysDown.has('ArrowLeft');\n\
                    case 'ui_right': return keysDown.has('KeyD') || keysDown.has('ArrowRight');\n\
                    case 'ui_accept': return keysDown.has('Space') || keysDown.has('Enter');\n\
                    default: return keysDown.has(action);\n\
                }}\n\
            }},\n\
            isActionJustPressed(action: string): boolean {{\n\
                switch (action) {{\n\
                    case 'ui_up': return keysJustPressed.has('KeyW') || keysJustPressed.has('ArrowUp');\n\
                    case 'ui_down': return keysJustPressed.has('KeyS') || keysJustPressed.has('ArrowDown');\n\
                    case 'ui_left': return keysJustPressed.has('KeyA') || keysJustPressed.has('ArrowLeft');\n\
                    case 'ui_right': return keysJustPressed.has('KeyD') || keysJustPressed.has('ArrowRight');\n\
                    case 'ui_accept': return keysJustPressed.has('Space') || keysJustPressed.has('Enter');\n\
                    default: return keysJustPressed.has(action);\n\
                }}\n\
            }},\n\
        }};\n\n\
        async function bootstrap() {{\n\
            let renderer: THREE.WebGLRenderer;\n\
            const tryWebGpu = {use_webgpu};\n\n\
            if (tryWebGpu && 'gpu' in navigator) {{\n\
                try {{\n\
                    // Dynamic import Three.js WebGPU renderer\n\
                    const webgpuMod = await import('three/webgpu');\n\
                    const gpuRenderer = new (webgpuMod as any).WebGPURenderer({{ canvas, antialias: true }});\n\
                    await gpuRenderer.init();\n\
                    renderer = gpuRenderer as any;\n\
                    console.log('⚡ Initialized Three.js WebGPURenderer');\n\
                }} catch (err) {{\n\
                    console.warn('WebGPU initialization failed, falling back to WebGLRenderer:', err);\n\
                    renderer = new THREE.WebGLRenderer({{ canvas, antialias: true }});\n\
                }}\n\
            }} else {{\n\
                renderer = new THREE.WebGLRenderer({{ canvas, antialias: true }});\n\
                console.log('⚡ Initialized Three.js WebGLRenderer');\n\
            }}\n\n\
            renderer.setSize(window.innerWidth, window.innerHeight);\n\
            renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));\n\
            renderer.toneMapping = {tone_mapping_expr};\n\
            renderer.toneMappingExposure = 1.0;\n\
            renderer.shadowMap.enabled = true;\n\
            renderer.shadowMap.type = THREE.PCFSoftShadowMap;\n\n\
            const {{ scene, camera, nodes }} = buildScene(signalBus, feelStore);\n\
            const behaviors = registerBehaviors(nodes, signalBus, feelStore);\n\
            const behaviorMap = new Map<number, Behavior>();\n\
            for (const b of behaviors) behaviorMap.set(b.nodeId, b);\n\n\
            initSignalConnections(signalBus, behaviorMap);\n\n\
            if ({mount_hud}) {{\n\
                mountFeelDevHud(feelStore);\n\
            }}\n\n\
            // Lifecycle: ready\n\
            for (const b of behaviors) {{\n\
                const obj = nodes.get(b.nodeId) as THREE.Object3D;\n\
                const ctx: BehaviorContext = {{\n\
                    nodeId: b.nodeId,\n\
                    object3d: obj || new THREE.Object3D(),\n\
                    scene,\n\
                    camera,\n\
                    signals: signalBus,\n\
                    feel: feelStore,\n\
                    input: inputState,\n\
                    nodes,\n\
                }};\n\
                b.ctx = ctx;\n\
                if (typeof b.ready === 'function') b.ready(ctx);\n\
            }}\n\n\
            window.addEventListener('resize', () => {{\n\
                camera.aspect = window.innerWidth / window.innerHeight;\n\
                camera.updateProjectionMatrix();\n\
                renderer.setSize(window.innerWidth, window.innerHeight);\n\
            }});\n\n\
            // Fixed 60Hz physics accumulator loop\n\
            const fixedDt = 1.0 / 60.0;\n\
            let lastTime = performance.now();\n\
            let accumulator = 0;\n\n\
            function tick(currentTime: number) {{\n\
                const frameDelta = Math.min((currentTime - lastTime) / 1000.0, 0.1);\n\
                lastTime = currentTime;\n\
                accumulator += frameDelta;\n\n\
                while (accumulator >= fixedDt) {{\n\
                    for (const b of behaviors) {{\n\
                        if (typeof b.physicsProcess === 'function' && b.ctx) {{\n\
                            b.physicsProcess(fixedDt, b.ctx);\n\
                        }}\n\
                    }}\n\
                    keysJustPressed.clear();\n\
                    accumulator -= fixedDt;\n\
                }}\n\n\
                for (const b of behaviors) {{\n\
                    if (typeof b.process === 'function' && b.ctx) {{\n\
                        b.process(frameDelta, b.ctx);\n\
                    }}\n\
                }}\n\n\
                if ('renderAsync' in renderer) {{\n\
                    (renderer as any).renderAsync(scene, camera);\n\
                }} else {{\n\
                    renderer.render(scene, camera);\n\
                }}\n\n\
                requestAnimationFrame(tick);\n\
            }}\n\n\
            requestAnimationFrame(tick);\n\
        }}\n\n\
        bootstrap().catch(console.error);\n"
    )
}

fn generate_atlas_json(
    scene: &IrScene,
    knobs: &[FeelSpecKnob],
) -> Result<String, ThreeJsError> {
    let task_context = AtlasTaskContext::from_scene(scene);
    let atlas_graph = AtlasGraph::from_scene(scene);

    let edges_json: Vec<_> = task_context
        .edges
        .iter()
        .map(|e| {
            json!({
                "from": { "index": e.from.index, "generation": e.from.generation.get() },
                "to": { "index": e.to.index, "generation": e.to.generation.get() },
                "relation": e.relation
            })
        })
        .collect();

    let layers_json: BTreeMap<String, usize> = atlas_graph
        .layers
        .iter()
        .map(|(k, v)| (format!("{}:{}", k.index, k.generation.get()), *v))
        .collect();

    let doc = json!({
        "scene_name": scene.name,
        "node_count": scene.nodes.len(),
        "affected_nodes": task_context.affected_nodes.iter().map(|id| format!("{}:{}", id.index, id.generation.get())).collect::<Vec<_>>(),
        "edges": edges_json,
        "layers": layers_json,
        "max_layer": atlas_graph.max_layer,
        "feel_knobs": knobs,
        "feel_overrides": scene.feel_overrides
    });

    serde_json::to_string_pretty(&doc).map_err(ThreeJsError::Json)
}

fn sort_nodes_parent_first(scene: &IrScene) -> Vec<&IrNode> {
    let mut result = Vec::new();
    let mut visited = std::collections::BTreeSet::new();

    fn visit<'a>(
        node_id: NodeId,
        scene: &'a IrScene,
        visited: &mut std::collections::BTreeSet<NodeId>,
        result: &mut Vec<&'a IrNode>,
    ) {
        if visited.contains(&node_id) {
            return;
        }
        if let Some(node) = scene.nodes.get(&node_id) {
            if let Some(parent) = node.parent
                && scene.nodes.contains_key(&parent)
            {
                visit(parent, scene, visited, result);
            }
            if visited.insert(node_id) {
                result.push(node);
            }
        }
    }

    for &id in scene.nodes.keys() {
        visit(id, scene, &mut visited, &mut result);
    }

    result
}

fn extract_num_prop(props: &BTreeMap<String, IrValue>, key: &str, default: f64) -> f64 {
    match props.get(key) {
        Some(IrValue::F64(v)) => *v,
        Some(IrValue::I64(v)) => *v as f64,
        _ => default,
    }
}

fn extract_string_prop<'a>(
    props: &'a BTreeMap<String, IrValue>,
    key: &str,
    default: &'a str,
) -> &'a str {
    match props.get(key) {
        Some(IrValue::String(s)) => s.as_str(),
        _ => default,
    }
}

fn extract_size_prop(props: &BTreeMap<String, IrValue>) -> (f32, f32, f32) {
    if let Some(IrValue::Vec3(s)) = props.get("size") {
        (s[0], s[1], s[2])
    } else {
        (1.0, 1.0, 1.0)
    }
}

fn extract_vec3_prop(
    props: &BTreeMap<String, IrValue>,
    key: &str,
    default: (f32, f32, f32),
) -> (f32, f32, f32) {
    if let Some(IrValue::Vec3(v)) = props.get(key) {
        (v[0], v[1], v[2])
    } else {
        default
    }
}

fn extract_color_hex(props: &BTreeMap<String, IrValue>) -> String {
    if let Some(IrValue::Color(c)) = props.get("color").or_else(|| props.get("material_color")) {
        let r = (c[0].clamp(0.0, 1.0) * 255.0) as u32;
        let g = (c[1].clamp(0.0, 1.0) * 255.0) as u32;
        let b = (c[2].clamp(0.0, 1.0) * 255.0) as u32;
        format!("0x{:02x}{:02x}{:02x}", r, g, b)
    } else {
        "0xffffff".into()
    }
}

fn generate_polyamour_mesh(
    primitive_name: &str,
    props: &BTreeMap<String, IrValue>,
    subsurf_level: Option<usize>,
) -> foreplay_polyamour::TriangulatedMesh {
    let clean_name = primitive_name.to_ascii_lowercase();
    let base_mesh = match clean_name.as_str() {
        "quarterpipe" => {
            let radius = extract_num_prop(props, "radius", 2.0) as f32;
            let width = extract_num_prop(props, "width", 4.0) as f32;
            let segments = extract_num_prop(props, "segments", 6.0) as u32;
            foreplay_polyamour::primitive_quarterpipe(radius, width, segments.max(3))
        }
        "stairset" | "stair_set" | "stairs" => {
            let width = extract_num_prop(props, "width", 3.0) as f32;
            let height = extract_num_prop(props, "height", 2.0) as f32;
            let depth = extract_num_prop(props, "depth", 3.0) as f32;
            let count = extract_num_prop(props, "step_count", 4.0) as u32;
            foreplay_polyamour::primitive_stair_set(width, height, depth, count.max(2))
        }
        "torus" => {
            let major = extract_num_prop(props, "major_radius", 2.0) as f32;
            let minor = extract_num_prop(props, "minor_radius", 0.5) as f32;
            let major_seg = extract_num_prop(props, "major_segments", 12.0) as u32;
            let minor_seg = extract_num_prop(props, "minor_segments", 8.0) as u32;
            foreplay_polyamour::primitive_torus(major, minor, major_seg.max(4), minor_seg.max(3))
        }
        "cylinder" => {
            let radius = extract_num_prop(props, "radius", 1.0) as f32;
            let height = extract_num_prop(props, "height", 2.0) as f32;
            let seg = extract_num_prop(props, "segments", 12.0) as u32;
            foreplay_polyamour::primitive_cylinder(radius, height, seg.max(3))
        }
        "cone" => {
            let radius = extract_num_prop(props, "radius", 1.0) as f32;
            let height = extract_num_prop(props, "height", 2.0) as f32;
            let seg = extract_num_prop(props, "segments", 12.0) as u32;
            foreplay_polyamour::primitive_cone(radius, height, seg.max(3))
        }
        "icosphere" | "ico_sphere" => {
            let radius = extract_num_prop(props, "radius", 1.0) as f32;
            let sub = extract_num_prop(props, "subdivisions", 1.0) as u32;
            foreplay_polyamour::primitive_ico_sphere(radius, sub)
        }
        "uvsphere" | "uv_sphere" | "sphere" => {
            let radius = extract_num_prop(props, "radius", 1.0) as f32;
            let rings = extract_num_prop(props, "rings", 8.0) as u32;
            let seg = extract_num_prop(props, "segments", 12.0) as u32;
            foreplay_polyamour::primitive_uv_sphere(radius, rings.max(3), seg.max(3))
        }
        "plane" => {
            let size = extract_num_prop(props, "size", 4.0) as f32;
            foreplay_polyamour::primitive_plane(size)
        }
        "grid" => {
            let w = extract_num_prop(props, "width", 10.0) as f32;
            let d = extract_num_prop(props, "depth", 10.0) as f32;
            let sx = extract_num_prop(props, "subs_x", 4.0) as u32;
            let sz = extract_num_prop(props, "subs_z", 4.0) as u32;
            foreplay_polyamour::primitive_grid(w, d, sx.max(1), sz.max(1))
        }
        _ => {
            let size = extract_num_prop(props, "size", 2.0) as f32;
            foreplay_polyamour::primitive_cube(size)
        }
    };

    let poly_mesh = if let Some(lvl) = subsurf_level {
        if lvl > 0 {
            let stack = vec![foreplay_polyamour::PolyModifier::Subdivision {
                levels: lvl.min(3) as u8,
                simple: false,
            }];
            foreplay_polyamour::evaluate_modifier_stack(&base_mesh, &stack)
        } else {
            base_mesh
        }
    } else {
        base_mesh
    };

    poly_mesh.triangulate()
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
    fn compiles_full_threejs_project_bundle() {
        let mut scene = IrScene::new("CyberCity");
        scene.feel_overrides.insert("skate_speed".into(), 16.5);

        let p_id = NodeId::new(1, 1).unwrap();
        let c_id = NodeId::new(2, 1).unwrap();
        let l_id = NodeId::new(3, 1).unwrap();

        let mut p_props = BTreeMap::new();
        p_props.insert("position".into(), IrValue::Vec3([0.0, 1.0, 0.0]));
        scene.nodes.insert(
            p_id,
            IrNode {
                id: p_id,
                name: "Skater".into(),
                class_name: "CharacterBody3D".into(),
                parent: None,
                instance_scene: None,
                properties: p_props,
                groups: vec!["player".into()],
            },
        );

        let mut c_props = BTreeMap::new();
        c_props.insert("position".into(), IrValue::Vec3([0.0, 3.0, 6.0]));
        scene.nodes.insert(
            c_id,
            IrNode {
                id: c_id,
                name: "Camera".into(),
                class_name: "Camera3D".into(),
                parent: Some(p_id),
                instance_scene: None,
                properties: c_props,
                groups: vec![],
            },
        );

        scene.nodes.insert(
            l_id,
            IrNode {
                id: l_id,
                name: "Sun".into(),
                class_name: "DirectionalLight3D".into(),
                parent: None,
                instance_scene: None,
                properties: BTreeMap::new(),
                groups: vec![],
            },
        );

        let mut gd_scripts = BTreeMap::new();
        gd_scripts.insert(
            p_id,
            include_str!("../../../templates/godot-migration/skater_controller.gd").to_string(),
        );

        let opts = ThreeJsExportOptions {
            project_name: "Cyber City Skater".into(),
            use_webgpu_renderer: true,
            include_feel_dev_hud: true,
            include_atlas_json: true,
            tone_mapping: ThreeToneMapping::AgX,
        };

        let bundle = compile_ir_to_threejs_project(&scene, &gd_scripts, &BTreeMap::new(), &opts)
            .expect("Compilation succeeds");

        assert_eq!(bundle.node_count, 3);
        assert_eq!(bundle.transpiled_scripts, 1);

        assert!(bundle.files.contains_key("package.json"));
        assert!(bundle.files.contains_key("tsconfig.json"));
        assert!(bundle.files.contains_key("vite.config.ts"));
        assert!(bundle.files.contains_key("index.html"));
        assert!(bundle.files.contains_key("src/feel.ts"));
        assert!(bundle.files.contains_key("src/signals.ts"));
        assert!(bundle.files.contains_key("src/scripts/behaviors.ts"));
        assert!(bundle.files.contains_key("src/scene.ts"));
        assert!(bundle.files.contains_key("src/main.ts"));
        assert!(bundle.files.contains_key("src/atlas.json"));

        let pkg = &bundle.files["package.json"];
        assert!(pkg.contains("\"three\": \"^0.174.0\""));
        assert!(pkg.contains("\"name\": \"cyber-city-skater\""));

        let scene_ts = &bundle.files["src/scene.ts"];
        assert!(scene_ts.contains("new THREE.PerspectiveCamera"));
        assert!(scene_ts.contains("new THREE.DirectionalLight"));

        let main_ts = &bundle.files["src/main.ts"];
        assert!(main_ts.contains("three/webgpu"));
        assert!(main_ts.contains("THREE.AgXToneMapping"));

        let atlas_json = &bundle.files["src/atlas.json"];
        assert!(atlas_json.contains("CyberCity"));
    }

    #[test]
    fn compiles_polyamour_quarterpipe_and_modifiers_to_threejs_buffer_geometry() {
        let mut scene = IrScene::new("SkateParkQuarterpipe");
        let qpipe_id = NodeId::new(1, 1).unwrap();
        let mut props = BTreeMap::new();
        props.insert("polyamour_primitive".into(), IrValue::String("Quarterpipe".into()));
        props.insert("polyamour_subsurf".into(), IrValue::I64(1));
        props.insert("radius".into(), IrValue::F64(3.0));
        props.insert("width".into(), IrValue::F64(6.0));
        props.insert("segments".into(), IrValue::I64(4));
        props.insert("position".into(), IrValue::Vec3([0.0, 0.0, -5.0]));
        props.insert("color".into(), IrValue::Color([0.2, 0.8, 1.0, 1.0]));

        scene.nodes.insert(
            qpipe_id,
            IrNode {
                id: qpipe_id,
                name: "MegaQuarterpipe".into(),
                class_name: "MeshInstance3D".into(),
                parent: None,
                instance_scene: None,
                properties: props,
                groups: vec!["obstacle".into(), "skate_ramp".into()],
            },
        );

        let opts = ThreeJsExportOptions::default();
        let bundle = compile_ir_to_threejs_project(&scene, &BTreeMap::new(), &BTreeMap::new(), &opts)
            .expect("Compilation must succeed");

        let scene_ts = &bundle.files["src/scene.ts"];
        assert!(scene_ts.contains("new THREE.BufferGeometry()"));
        assert!(scene_ts.contains("new Float32Array("));
        assert!(scene_ts.contains("new Uint32Array("));
        assert!(scene_ts.contains("setAttribute('position'"));
        assert!(scene_ts.contains("setAttribute('normal'"));
        assert!(scene_ts.contains("setIndex("));
        assert!(scene_ts.contains("MegaQuarterpipe"));
    }
}
