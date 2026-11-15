//! Built-in Polyamour / Blender 3D Geometric Primitives (`foreplay-polyamour::primitives`).
//!
//! Provides the 8 canonical Blender primitives:
//! - Cube, Plane, Grid, UV Sphere, Ico Sphere, Cylinder, Cone, Torus.
//!
//! Plus level-design primitives:
//! - Quarterpipe (skate transition ramp), Stair Set.

use crate::mesh::PolyMesh;
use bevy_math::Vec3;

/// Creates a centered cube with side length `size`.
pub fn primitive_cube(size: f32) -> PolyMesh {
    let mut mesh = PolyMesh::new();
    let h = size * 0.5;

    // 8 vertices
    // 0: -X, -Y, -Z
    // 1: +X, -Y, -Z
    // 2: +X, +Y, -Z
    // 3: -X, +Y, -Z
    // 4: -X, -Y, +Z
    // 5: +X, -Y, +Z
    // 6: +X, +Y, +Z
    // 7: -X, +Y, +Z
    mesh.add_vertex(Vec3::new(-h, -h, -h));
    mesh.add_vertex(Vec3::new(h, -h, -h));
    mesh.add_vertex(Vec3::new(h, h, -h));
    mesh.add_vertex(Vec3::new(-h, h, -h));
    mesh.add_vertex(Vec3::new(-h, -h, h));
    mesh.add_vertex(Vec3::new(h, -h, h));
    mesh.add_vertex(Vec3::new(h, h, h));
    mesh.add_vertex(Vec3::new(-h, h, h));

    let uvs = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

    // 6 quad faces (counter-clockwise outward winding)
    mesh.add_face(vec![4, 5, 6, 7], uvs.clone(), false, 0); // Front (+Z)
    mesh.add_face(vec![1, 0, 3, 2], uvs.clone(), false, 0); // Back (-Z)
    mesh.add_face(vec![7, 6, 2, 3], uvs.clone(), false, 0); // Top (+Y)
    mesh.add_face(vec![0, 1, 5, 4], uvs.clone(), false, 0); // Bottom (-Y)
    mesh.add_face(vec![5, 1, 2, 6], uvs.clone(), false, 0); // Right (+X)
    mesh.add_face(vec![0, 4, 7, 3], uvs, false, 0); // Left (-X)

    mesh
}

/// Creates a horizontal plane of size `size` centered at origin in the XZ plane.
pub fn primitive_plane(size: f32) -> PolyMesh {
    let mut mesh = PolyMesh::new();
    let h = size * 0.5;

    mesh.add_vertex(Vec3::new(-h, 0.0, h));
    mesh.add_vertex(Vec3::new(h, 0.0, h));
    mesh.add_vertex(Vec3::new(h, 0.0, -h));
    mesh.add_vertex(Vec3::new(-h, 0.0, -h));

    let uvs = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    mesh.add_face(vec![0, 1, 2, 3], uvs, false, 0);

    mesh
}

/// Creates a subdivided grid in the XZ plane.
pub fn primitive_grid(
    size_x: f32,
    size_z: f32,
    subdivisions_x: u32,
    subdivisions_z: u32,
) -> PolyMesh {
    let mut mesh = PolyMesh::new();
    let sub_x = subdivisions_x.max(1);
    let sub_z = subdivisions_z.max(1);

    let hx = size_x * 0.5;
    let hz = size_z * 0.5;

    let cols = sub_x + 1;
    let rows = sub_z + 1;

    // Generate vertices
    for r in 0..rows {
        let tz = r as f32 / sub_z as f32;
        let z = -hz + tz * size_z;
        for c in 0..cols {
            let tx = c as f32 / sub_x as f32;
            let x = -hx + tx * size_x;
            mesh.add_vertex(Vec3::new(x, 0.0, z));
        }
    }

    // Generate quads
    for r in 0..sub_z {
        for c in 0..sub_x {
            let i00 = r * cols + c;
            let i10 = r * cols + (c + 1);
            let i11 = (r + 1) * cols + (c + 1);
            let i01 = (r + 1) * cols + c;

            let u0 = c as f32 / sub_x as f32;
            let u1 = (c + 1) as f32 / sub_x as f32;
            let v0 = r as f32 / sub_z as f32;
            let v1 = (r + 1) as f32 / sub_z as f32;

            let uvs = vec![[u0, v0], [u1, v0], [u1, v1], [u0, v1]];
            mesh.add_face(vec![i00, i10, i11, i01], uvs, false, 0);
        }
    }

    mesh
}

/// Creates a standard UV Sphere.
pub fn primitive_uv_sphere(radius: f32, segments: u32, rings: u32) -> PolyMesh {
    let mut mesh = PolyMesh::new();
    let segs = segments.max(3);
    let num_rings = rings.max(2);

    // North pole (+Y)
    let north_idx = mesh.add_vertex(Vec3::new(0.0, radius, 0.0));

    // Intermediate ring vertices
    for r in 1..num_rings {
        let phi = std::f32::consts::PI * (r as f32 / num_rings as f32);
        let y = radius * phi.cos();
        let ring_radius = radius * phi.sin();

        for s in 0..segs {
            let theta = std::f32::consts::TAU * (s as f32 / segs as f32);
            let x = ring_radius * theta.cos();
            let z = ring_radius * theta.sin();
            mesh.add_vertex(Vec3::new(x, y, z));
        }
    }

    // South pole (-Y)
    let south_idx = mesh.add_vertex(Vec3::new(0.0, -radius, 0.0));

    // North pole triangular cap
    for s in 0..segs {
        let next_s = (s + 1) % segs;
        let v1 = 1 + s;
        let v2 = 1 + next_s;
        let uvs = vec![[0.5, 1.0], [s as f32 / segs as f32, 1.0 - 1.0 / num_rings as f32], [next_s as f32 / segs as f32, 1.0 - 1.0 / num_rings as f32]];
        mesh.add_face(vec![north_idx, v1, v2], uvs, true, 0);
    }

    // Intermediate quad rings
    for r in 0..(num_rings - 2) {
        let r0 = 1 + r * segs;
        let r1 = 1 + (r + 1) * segs;
        for s in 0..segs {
            let next_s = (s + 1) % segs;
            let v00 = r0 + s;
            let v10 = r0 + next_s;
            let v11 = r1 + next_s;
            let v01 = r1 + s;

            let u0 = s as f32 / segs as f32;
            let u1 = (s + 1) as f32 / segs as f32;
            let v0 = 1.0 - (r + 1) as f32 / num_rings as f32;
            let v1 = 1.0 - (r + 2) as f32 / num_rings as f32;

            mesh.add_face(
                vec![v00, v10, v11, v01],
                vec![[u0, v0], [u1, v0], [u1, v1], [u0, v1]],
                true,
                0,
            );
        }
    }

    // South pole triangular cap
    let last_ring = 1 + (num_rings - 2) * segs;
    for s in 0..segs {
        let next_s = (s + 1) % segs;
        let v1 = last_ring + next_s;
        let v2 = last_ring + s;
        let uvs = vec![[0.5, 0.0], [next_s as f32 / segs as f32, 1.0 / num_rings as f32], [s as f32 / segs as f32, 1.0 / num_rings as f32]];
        mesh.add_face(vec![south_idx, v1, v2], uvs, true, 0);
    }

    mesh
}

/// Creates an Ico Sphere (subdivided regular icosahedron).
pub fn primitive_ico_sphere(radius: f32, subdivisions: u32) -> PolyMesh {
    let mut mesh = PolyMesh::new();

    // Golden ratio
    let phi = (1.0 + 5.0f32.sqrt()) * 0.5;

    // 12 vertices of regular icosahedron
    let mut initial_verts = vec![
        Vec3::new(-1.0, phi, 0.0),
        Vec3::new(1.0, phi, 0.0),
        Vec3::new(-1.0, -phi, 0.0),
        Vec3::new(1.0, -phi, 0.0),
        Vec3::new(0.0, -1.0, phi),
        Vec3::new(0.0, 1.0, phi),
        Vec3::new(0.0, -1.0, -phi),
        Vec3::new(0.0, 1.0, -phi),
        Vec3::new(phi, 0.0, -1.0),
        Vec3::new(phi, 0.0, 1.0),
        Vec3::new(-phi, 0.0, -1.0),
        Vec3::new(-phi, 0.0, 1.0),
    ];

    for p in &mut initial_verts {
        mesh.add_vertex(p.normalize() * radius);
    }

    // 20 initial triangular faces
    let mut faces: Vec<[u32; 3]> = vec![
        [0, 11, 5],
        [0, 5, 1],
        [0, 1, 7],
        [0, 7, 10],
        [0, 10, 11],
        [1, 5, 9],
        [5, 11, 4],
        [11, 10, 2],
        [10, 7, 6],
        [7, 1, 8],
        [3, 9, 4],
        [3, 4, 2],
        [3, 2, 6],
        [3, 6, 8],
        [3, 8, 9],
        [4, 9, 5],
        [2, 4, 11],
        [6, 2, 10],
        [8, 6, 7],
        [9, 8, 1],
    ];

    // Iterative subdivision
    let mut midpoint_cache = std::collections::BTreeMap::new();
    let get_midpoint = |mesh: &mut PolyMesh,
                        cache: &mut std::collections::BTreeMap<[u32; 2], u32>,
                        v0: u32,
                        v1: u32| {
        let key = if v0 < v1 { [v0, v1] } else { [v1, v0] };
        if let Some(&idx) = cache.get(&key) {
            return idx;
        }
        let p0 = mesh.positions[v0 as usize];
        let p1 = mesh.positions[v1 as usize];
        let mid = ((p0 + p1) * 0.5).normalize() * radius;
        let new_idx = mesh.add_vertex(mid);
        cache.insert(key, new_idx);
        new_idx
    };

    let iters = subdivisions.min(5);
    for _ in 0..iters {
        let mut new_faces = Vec::with_capacity(faces.len() * 4);
        for &[v0, v1, v2] in &faces {
            let m01 = get_midpoint(&mut mesh, &mut midpoint_cache, v0, v1);
            let m12 = get_midpoint(&mut mesh, &mut midpoint_cache, v1, v2);
            let m20 = get_midpoint(&mut mesh, &mut midpoint_cache, v2, v0);

            new_faces.push([v0, m01, m20]);
            new_faces.push([v1, m12, m01]);
            new_faces.push([v2, m20, m12]);
            new_faces.push([m01, m12, m20]);
        }
        faces = new_faces;
    }

    for tri in faces {
        mesh.add_face(
            vec![tri[0], tri[1], tri[2]],
            vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]],
            true,
            0,
        );
    }

    mesh
}

/// Creates a cylinder along the Y axis.
pub fn primitive_cylinder(radius: f32, depth: f32, segments: u32) -> PolyMesh {
    let mut mesh = PolyMesh::new();
    let segs = segments.max(3);
    let hh = depth * 0.5;

    // Top ring (0..segs) at +hh, Bottom ring (segs..2*segs) at -hh
    for s in 0..segs {
        let theta = std::f32::consts::TAU * (s as f32 / segs as f32);
        let x = radius * theta.cos();
        let z = radius * theta.sin();
        mesh.add_vertex(Vec3::new(x, hh, z));
    }
    for s in 0..segs {
        let theta = std::f32::consts::TAU * (s as f32 / segs as f32);
        let x = radius * theta.cos();
        let z = radius * theta.sin();
        mesh.add_vertex(Vec3::new(x, -hh, z));
    }

    // Top cap n-gon
    let top_verts: Vec<u32> = (0..segs).rev().collect();
    let top_uvs = vec![[0.5, 0.5]; segs as usize];
    mesh.add_face(top_verts, top_uvs, false, 0);

    // Bottom cap n-gon
    let bot_verts: Vec<u32> = (0..segs).map(|s| segs + s).collect();
    let bot_uvs = vec![[0.5, 0.5]; segs as usize];
    mesh.add_face(bot_verts, bot_uvs, false, 0);

    // Side quad faces
    for s in 0..segs {
        let next = (s + 1) % segs;
        let v0 = s;
        let v1 = next;
        let v2 = segs + next;
        let v3 = segs + s;

        let u0 = s as f32 / segs as f32;
        let u1 = (s + 1) as f32 / segs as f32;
        mesh.add_face(
            vec![v0, v1, v2, v3],
            vec![[u0, 1.0], [u1, 1.0], [u1, 0.0], [u0, 0.0]],
            true,
            0,
        );
    }

    mesh
}

/// Creates a cone along the Y axis.
pub fn primitive_cone(radius: f32, depth: f32, segments: u32) -> PolyMesh {
    let mut mesh = PolyMesh::new();
    let segs = segments.max(3);
    let hh = depth * 0.5;

    // Apex at +hh
    let apex_idx = mesh.add_vertex(Vec3::new(0.0, hh, 0.0));

    // Base ring at -hh
    for s in 0..segs {
        let theta = std::f32::consts::TAU * (s as f32 / segs as f32);
        let x = radius * theta.cos();
        let z = radius * theta.sin();
        mesh.add_vertex(Vec3::new(x, -hh, z));
    }

    // Base cap n-gon
    let base_verts: Vec<u32> = (1..=segs).collect();
    let base_uvs = vec![[0.5, 0.5]; segs as usize];
    mesh.add_face(base_verts, base_uvs, false, 0);

    // Side triangles
    for s in 0..segs {
        let next = (s + 1) % segs;
        let v1 = 1 + next;
        let v2 = 1 + s;
        let u0 = s as f32 / segs as f32;
        let u1 = (s + 1) as f32 / segs as f32;
        mesh.add_face(
            vec![apex_idx, v1, v2],
            vec![[0.5, 1.0], [u1, 0.0], [u0, 0.0]],
            true,
            0,
        );
    }

    mesh
}

/// Creates a Torus (donut).
pub fn primitive_torus(
    major_radius: f32,
    minor_radius: f32,
    major_segments: u32,
    minor_segments: u32,
) -> PolyMesh {
    let mut mesh = PolyMesh::new();
    let m_segs = major_segments.max(3);
    let n_segs = minor_segments.max(3);

    for i in 0..m_segs {
        let u = std::f32::consts::TAU * (i as f32 / m_segs as f32);
        let cos_u = u.cos();
        let sin_u = u.sin();

        for j in 0..n_segs {
            let v = std::f32::consts::TAU * (j as f32 / n_segs as f32);
            let cos_v = v.cos();
            let sin_v = v.sin();

            let x = (major_radius + minor_radius * cos_v) * cos_u;
            let y = minor_radius * sin_v;
            let z = (major_radius + minor_radius * cos_v) * sin_u;

            mesh.add_vertex(Vec3::new(x, y, z));
        }
    }

    // Quads connecting adjacent torus rings
    for i in 0..m_segs {
        let next_i = (i + 1) % m_segs;
        for j in 0..n_segs {
            let next_j = (j + 1) % n_segs;

            let v00 = i * n_segs + j;
            let v10 = next_i * n_segs + j;
            let v11 = next_i * n_segs + next_j;
            let v01 = i * n_segs + next_j;

            let u0 = i as f32 / m_segs as f32;
            let u1 = (i + 1) as f32 / m_segs as f32;
            let v0 = j as f32 / n_segs as f32;
            let v1 = (j + 1) as f32 / n_segs as f32;

            mesh.add_face(
                vec![v00, v10, v11, v01],
                vec![[u0, v0], [u1, v0], [u1, v1], [u0, v1]],
                true,
                0,
            );
        }
    }

    mesh
}

/// Creates a solid skate quarterpipe level-design ramp.
pub fn primitive_quarterpipe(radius: f32, width: f32, segments: u32) -> PolyMesh {
    let mut mesh = PolyMesh::new();
    let segs = segments.max(4);
    let hw = width * 0.5;

    // Arc from flat bottom (z = radius, y = 0) to vertical lip (z = 0, y = radius)
    // Left profile vertices: x = -hw
    // Right profile vertices: x = +hw
    let mut left_arc = Vec::with_capacity((segs + 1) as usize);
    let mut right_arc = Vec::with_capacity((segs + 1) as usize);

    for i in 0..=segs {
        let theta = (std::f32::consts::FRAC_PI_2) * (i as f32 / segs as f32);
        let y = radius * (1.0 - theta.cos());
        let z = radius * (1.0 - theta.sin());

        let vl = mesh.add_vertex(Vec3::new(-hw, y, z));
        let vr = mesh.add_vertex(Vec3::new(hw, y, z));

        left_arc.push(vl);
        right_arc.push(vr);
    }

    // Back-bottom corner vertices (z = 0, y = 0)
    let b_left = mesh.add_vertex(Vec3::new(-hw, 0.0, 0.0));
    let b_right = mesh.add_vertex(Vec3::new(hw, 0.0, 0.0));

    // 1. Curved riding surface (quads)
    for i in 0..segs as usize {
        let vl0 = left_arc[i];
        let vr0 = right_arc[i];
        let vr1 = right_arc[i + 1];
        let vl1 = left_arc[i + 1];

        let v0 = i as f32 / segs as f32;
        let v1 = (i + 1) as f32 / segs as f32;
        mesh.add_face(
            vec![vl0, vr0, vr1, vl1],
            vec![[0.0, v0], [1.0, v0], [1.0, v1], [0.0, v1]],
            true,
            0,
        );
    }

    // 2. Flat bottom base quad: from back-bottom (z=0, y=0) to arc bottom (z=radius, y=0)
    let arc_bot_l = left_arc[0];
    let arc_bot_r = right_arc[0];
    mesh.add_face(
        vec![b_left, b_right, arc_bot_r, arc_bot_l],
        vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        false,
        0,
    );

    // 3. Vertical back wall quad: from back-bottom (z=0, y=0) to arc top (z=0, y=radius)
    let arc_top_l = *left_arc.last().unwrap();
    let arc_top_r = *right_arc.last().unwrap();
    mesh.add_face(
        vec![b_right, b_left, arc_top_l, arc_top_r],
        vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        false,
        0,
    );

    // 4. Left side wall (n-gon)
    let mut left_side = vec![b_left];
    left_side.extend(left_arc.iter().copied());
    let left_uvs = vec![[0.0, 0.0]; left_side.len()];
    mesh.add_face(left_side, left_uvs, false, 0);

    // 5. Right side wall (n-gon)
    let mut right_side = vec![b_right];
    right_side.extend(right_arc.iter().rev().copied());
    let right_uvs = vec![[0.0, 0.0]; right_side.len()];
    mesh.add_face(right_side, right_uvs, false, 0);

    mesh
}

/// Creates a solid skate stair set level-design block with `step_count` steps.
pub fn primitive_stair_set(
    width: f32,
    total_height: f32,
    total_depth: f32,
    step_count: u32,
) -> PolyMesh {
    let mut mesh = PolyMesh::new();
    let n = step_count.max(1);
    let hw = width * 0.5;

    let dy = total_height / n as f32;
    let dz = total_depth / n as f32;

    // Profile points along staircase from bottom to top
    // For each step i:
    // - riser: from (y = i*dy, z = i*dz) to (y = (i+1)*dy, z = i*dz)
    // - tread: from (y = (i+1)*dy, z = i*dz) to (y = (i+1)*dy, z = (i+1)*dz)
    let mut left_profile = Vec::new();
    let mut right_profile = Vec::new();

    for i in 0..n {
        let y0 = i as f32 * dy;
        let y1 = (i + 1) as f32 * dy;
        let z0 = i as f32 * dz;
        let z1 = (i + 1) as f32 * dz;

        // Riser points
        let rl0 = mesh.add_vertex(Vec3::new(-hw, y0, z0));
        let rr0 = mesh.add_vertex(Vec3::new(hw, y0, z0));
        let rl1 = mesh.add_vertex(Vec3::new(-hw, y1, z0));
        let rr1 = mesh.add_vertex(Vec3::new(hw, y1, z0));

        // Tread points
        let tl1 = mesh.add_vertex(Vec3::new(-hw, y1, z1));
        let tr1 = mesh.add_vertex(Vec3::new(hw, y1, z1));

        // Riser quad face
        mesh.add_face(
            vec![rl0, rr0, rr1, rl1],
            vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            false,
            0,
        );

        // Tread quad face
        mesh.add_face(
            vec![rl1, rr1, tr1, tl1],
            vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            false,
            0,
        );

        left_profile.push(rl0);
        left_profile.push(rl1);
        left_profile.push(tl1);

        right_profile.push(rr0);
        right_profile.push(rr1);
        right_profile.push(tr1);
    }

    // Bottom and back corner points
    let bot_left = mesh.add_vertex(Vec3::new(-hw, 0.0, total_depth));
    let bot_right = mesh.add_vertex(Vec3::new(hw, 0.0, total_depth));

    // Bottom base quad
    let start_l = left_profile[0];
    let start_r = right_profile[0];
    mesh.add_face(
        vec![start_r, start_l, bot_left, bot_right],
        vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        false,
        0,
    );

    // Back wall quad
    let top_l = *left_profile.last().unwrap();
    let top_r = *right_profile.last().unwrap();
    mesh.add_face(
        vec![bot_right, bot_left, top_l, top_r],
        vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        false,
        0,
    );

    // Left side polygon
    let mut left_poly = vec![bot_left];
    left_poly.extend(left_profile.iter().copied());
    let left_uvs = vec![[0.0, 0.0]; left_poly.len()];
    mesh.add_face(left_poly, left_uvs, false, 0);

    // Right side polygon
    let mut right_poly = vec![bot_right];
    right_poly.extend(right_profile.iter().rev().copied());
    let right_uvs = vec![[0.0, 0.0]; right_poly.len()];
    mesh.add_face(right_poly, right_uvs, false, 0);

    mesh
}
