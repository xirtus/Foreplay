//! `foreplay-polyamour` — Native Blender / SexOS Polyamour 3D modeling, HalfEdgeMesh,
//! modifier stack, `.blend`/glTF/OBJ/STL/PLY I/O, `polyops 1` scripting, and SAH BVH AgX path tracer.

pub mod io;
pub mod mesh;
pub mod modifiers;
pub mod ops;
pub mod pathtracer;
pub mod polyops;
pub mod primitives;

pub use io::*;
pub use mesh::*;
pub use modifiers::*;
pub use ops::*;
pub use pathtracer::*;
pub use polyops::*;
pub use primitives::*;

/// Version of the `polyops` text command protocol.
pub const POLYOPS_VERSION: u32 = 1;

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_math::Vec3;

    #[test]
    fn primitives_generate_valid_polymesh_and_triangulate() {
        // Test all 8 canonical primitives + 2 level design primitives
        let cube = primitive_cube(2.0);
        assert_eq!(cube.positions.len(), 8);
        assert_eq!(cube.faces.len(), 6);
        let cube_tri = cube.triangulate();
        assert_eq!(cube_tri.indices.len(), 36); // 6 faces * 2 tris * 3 indices

        let plane = primitive_plane(4.0);
        assert_eq!(plane.positions.len(), 4);
        assert_eq!(plane.faces.len(), 1);

        let grid = primitive_grid(10.0, 10.0, 4, 4);
        assert_eq!(grid.faces.len(), 16);

        let uv_sphere = primitive_uv_sphere(1.0, 8, 4);
        assert!(!uv_sphere.faces.is_empty());

        let ico_sphere = primitive_ico_sphere(1.0, 1);
        assert_eq!(ico_sphere.faces.len(), 80); // 20 * 4

        let cylinder = primitive_cylinder(1.0, 2.0, 8);
        assert_eq!(cylinder.faces.len(), 10); // 2 caps + 8 sides

        let cone = primitive_cone(1.0, 2.0, 8);
        assert_eq!(cone.faces.len(), 9); // 1 cap + 8 sides

        let torus = primitive_torus(2.0, 0.5, 8, 6);
        assert_eq!(torus.faces.len(), 48); // 8 * 6

        let qpipe = primitive_quarterpipe(2.0, 4.0, 6);
        assert!(!qpipe.faces.is_empty());
        let qpipe_tri = qpipe.triangulate();
        assert!(!qpipe_tri.indices.is_empty());

        let stairs = primitive_stair_set(3.0, 2.0, 3.0, 4);
        assert!(!stairs.faces.is_empty());
        let stairs_tri = stairs.triangulate();
        assert!(!stairs_tri.indices.is_empty());
    }

    #[test]
    fn half_edge_mesh_topology_and_edge_loop_traversal() {
        let cube = primitive_cube(1.0);
        let he = HalfEdgeMesh::from_polymesh(&cube);

        assert!(he.is_manifold(), "cube must be a 2-manifold surface");
        assert!(he.boundary_edges().is_empty(), "closed cube has no boundary edges");

        // Vertex 0 neighbors in cube (connected to 3 other vertices)
        let neighbors = he.vertex_neighbors(0);
        assert_eq!(neighbors.len(), 3);

        // Quad edge loop
        let loop_edges = he.edge_loop([4, 5]);
        assert_eq!(loop_edges.len(), 4, "cube quad edge loop must contain 4 edges");

        // Face areas
        let areas = he.face_areas();
        assert_eq!(areas.len(), 6);
        for &a in areas {
            assert!((a - 1.0).abs() < 1e-4);
        }
    }

    #[test]
    fn edit_operations_extrude_inset_bisect_and_weld() {
        let mut cube = primitive_cube(2.0);

        // 1. Extrude top face (+Y)
        extrude_faces(&mut cube, &[2], 1.5);
        assert_eq!(cube.faces.len(), 10); // 6 original + 4 side walls

        // 2. Inset front face
        inset_faces(&mut cube, &[0], 0.2, 0.1);
        assert_eq!(cube.faces.len(), 14); // + 4 inset perimeter quads

        // 3. Knife bisect along plane X = 0
        knife_bisect(&mut cube, Vec3::ZERO, Vec3::X);
        assert!(cube.faces.len() > 14);

        // 4. Weld duplicate vertices
        let orig_vert_count = cube.positions.len();
        cube.weld_vertices(0.001);
        assert!(cube.positions.len() <= orig_vert_count);
    }

    #[test]
    fn boolean_csg_union_difference_intersect() {
        let a = primitive_cube(2.0);
        let mut b = primitive_cube(2.0);
        for p in &mut b.positions {
            *p += Vec3::new(1.0, 0.0, 0.0);
        }

        let union_mesh = boolean_csg(&a, &b, BooleanOp::Union);
        assert!(!union_mesh.faces.is_empty());

        let diff_mesh = boolean_csg(&a, &b, BooleanOp::Difference);
        assert!(!diff_mesh.faces.is_empty());

        let isect_mesh = boolean_csg(&a, &b, BooleanOp::Intersect);
        assert!(!isect_mesh.faces.is_empty());
    }

    #[test]
    fn modifier_stack_catmull_clark_mirror_array_solidify() {
        let base = primitive_cube(2.0);

        let stack = vec![
            PolyModifier::Subdivision {
                levels: 1,
                simple: false, // Catmull-Clark
            },
            PolyModifier::Mirror {
                axis_x: true,
                axis_y: false,
                axis_z: false,
                merge_distance: 0.01,
            },
            PolyModifier::Array {
                count: 3,
                relative_offset: Vec3::new(1.2, 0.0, 0.0),
                constant_offset: Vec3::ZERO,
            },
            PolyModifier::Solidify { thickness: 0.1 },
            PolyModifier::Triangulate,
        ];

        let result = evaluate_modifier_stack(&base, &stack);
        assert!(!result.positions.is_empty());
        assert!(!result.faces.is_empty());
        // After Triangulate, every face must be a triangle
        for face in &result.faces {
            assert_eq!(face.verts.len(), 3);
        }
    }

    #[test]
    fn io_roundtrip_obj_stl_ply_gltf() {
        let original = primitive_cube(1.0);

        // 1. OBJ
        let obj_str = export_obj(&original);
        assert!(obj_str.contains("v -0.5"));
        let obj_imported = import_obj(&obj_str).expect("import obj");
        assert_eq!(obj_imported.positions.len(), 8);
        assert_eq!(obj_imported.faces.len(), 6);

        // 2. STL ASCII & Binary
        let stl_ascii = export_stl_ascii(&original, "CubeSolid");
        assert!(stl_ascii.contains("solid CubeSolid"));
        let stl_imported = import_stl(stl_ascii.as_bytes()).expect("import stl ascii");
        assert_eq!(stl_imported.positions.len(), 8); // welded

        // 3. PLY
        let ply_str = export_ply(&original);
        assert!(ply_str.contains("element vertex 8"));
        let ply_imported = import_ply(&ply_str).expect("import ply");
        assert_eq!(ply_imported.positions.len(), 8);
        assert_eq!(ply_imported.faces.len(), 6);

        // 4. glTF JSON
        let gltf_json = export_gltf_json(&original);
        assert!(gltf_json.contains("data:application/octet-stream;base64,"));
        let gltf_imported = import_gltf_json(&gltf_json).expect("import gltf json");
        assert_eq!(gltf_imported.positions.len(), 8);
    }

    #[test]
    fn blend_file_cleanroom_header_and_dna_parser() {
        // Construct mock 64-bit little-endian Blender 4.0 header with a ME (Mesh) block and ENDB
        let mut mock_blend = Vec::new();
        // Magic + 64-bit pointer '-' + little-endian 'v' + version '400'
        mock_blend.extend_from_slice(b"BLENDER-v400");

        // Block 1: ME (Mesh block), size = 32 bytes, addr = 0x1000, sdna = 5, count = 1
        mock_blend.extend_from_slice(b"ME\0\0");
        mock_blend.extend_from_slice(&32u32.to_le_bytes());
        mock_blend.extend_from_slice(&0x1000u64.to_le_bytes());
        mock_blend.extend_from_slice(&5u32.to_le_bytes());
        mock_blend.extend_from_slice(&1u32.to_le_bytes());
        mock_blend.extend_from_slice(&[0u8; 32]); // payload

        // Block 2: DNA1, size = 16 bytes
        mock_blend.extend_from_slice(b"DNA1");
        mock_blend.extend_from_slice(&16u32.to_le_bytes());
        mock_blend.extend_from_slice(&0x2000u64.to_le_bytes());
        mock_blend.extend_from_slice(&0u32.to_le_bytes());
        mock_blend.extend_from_slice(&1u32.to_le_bytes());
        mock_blend.extend_from_slice(&[0u8; 16]); // payload

        // Block 3: ENDB
        mock_blend.extend_from_slice(b"ENDB");
        mock_blend.extend_from_slice(&0u32.to_le_bytes());
        mock_blend.extend_from_slice(&0u64.to_le_bytes());
        mock_blend.extend_from_slice(&0u32.to_le_bytes());
        mock_blend.extend_from_slice(&0u32.to_le_bytes());

        let summary = BlendFileSummary::parse_bytes(&mock_blend).expect("parse mock blend");
        assert_eq!(summary.header_magic, "BLENDER");
        assert_eq!(summary.version, "400");
        assert_eq!(summary.pointer_size, 8);
        assert!(summary.is_little_endian);
        assert_eq!(summary.mesh_block_count, 1);
        assert!(summary.has_dna);
        assert_eq!(summary.block_count, 3);

        // Hostile truncated input must return error gracefully without panic
        let truncated = &mock_blend[0..10];
        assert!(matches!(
            BlendFileSummary::parse_bytes(truncated),
            Err(BlendParseError::HeaderTooShort(_))
        ));
    }

    #[test]
    fn polyops_1_atomic_script_and_rollback_on_abort() {
        let mut scene = PolyScene::new();

        let script = r#"
            polyops 1
            begin
            material_new "WoodPlank" 0.6 0.4 0.2 1.0 0.1 0.7 as $wood
            add_primitive cube 2.0 as $box
            set $box position 0.0 1.0 0.0
            modifier_add $box subdivision 1
            material_assign $box $wood
            end
        "#;

        let report = run_polyops(&mut scene, script).expect("run polyops");
        assert_eq!(scene.objects.len(), 1);
        assert_eq!(scene.materials.len(), 1);
        assert!(scene.handles.contains_key("$box"));
        assert!(scene.handles.contains_key("$wood"));
        assert_eq!(report.created_ids.len(), 2);

        // Roundtrip serialization
        let emitted = scene_to_polyops(&scene);
        assert!(emitted.starts_with("polyops 1"));
        assert!(emitted.contains("material_new"));
        assert!(emitted.contains("add_mesh"));

        // Test atomic rollback on abort
        let abort_script = r#"
            polyops 1
            begin
            add_primitive cylinder 1.0 2.0 16 as $bad_cyl
            abort "simulated network cancellation"
            end
        "#;

        let res = run_polyops(&mut scene, abort_script);
        assert!(matches!(res, Err(PolyopsError::Aborted(_))));
        // State must be completely untouched by aborted script
        assert_eq!(scene.objects.len(), 1);
        assert!(!scene.handles.contains_key("$bad_cyl"));
    }

    #[test]
    fn pathtracer_bakes_deterministic_rgba8_with_agx_and_sah_bvh() {
        let cube = primitive_cube(2.0);
        let tri_mesh = cube.triangulate();

        let config = PathTracerConfig {
            width: 16,
            height: 16,
            samples_per_pixel: 2,
            max_bounces: 2,
            seed: 0x42,
            enable_denoiser: true,
            view_transform: ViewTransform::AgX,
            camera: PathTracerCamera {
                eye: Vec3::new(0.0, 1.5, 4.0),
                target: Vec3::ZERO,
                up: Vec3::Y,
                fov_degrees: 60.0,
            },
            default_material: PathTracerMaterial {
                base_color: Vec3::new(0.9, 0.4, 0.2),
                metallic: 0.2,
                roughness: 0.3,
            },
        };

        // Render pass 1
        let pixels1 = bake_pathtraced_rgba8(&tri_mesh, &config);
        assert_eq!(pixels1.len(), 16 * 16 * 4);

        // Render pass 2 (must be 100% bit-exact identical)
        let pixels2 = bake_pathtraced_rgba8(&tri_mesh, &config);
        assert_eq!(pixels1, pixels2, "Deterministic path tracer must produce bit-exact output");

        // Verify alpha channel is fully opaque
        for i in 0..(16 * 16) {
            assert_eq!(pixels1[i * 4 + 3], 255);
        }
    }
}
