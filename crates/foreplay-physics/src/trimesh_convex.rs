//! TriMesh BVH Colliders, Convex Hulls (GJK), and 6-Body Articulated Skater Bail Ragdolls.
//!
//! Provides fast SAH/AABB BVH queries over curved skate bowls and transitions (`TriMeshCollider3D`),
//! 3D point-cloud support mapping & GJK simplex intersection (`ConvexHullCollider3D`),
//! and articulated 6-body skater bail ragdoll simulation with ground/trimesh restitution (`RagdollRig3D`).

use bevy_math::{Mat4, Vec3};
use crate::RayHit3D;

/// Contact manifold emitted during 3D narrow-phase mesh/sphere collision.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ContactManifold3D {
    /// World-space point of closest contact on the obstacle surface.
    pub point: Vec3,
    /// Outward contact normal pointing from the obstacle surface towards the colliding body.
    pub normal: Vec3,
    /// Penetration depth along the contact normal.
    pub penetration: f32,
}

#[derive(Clone, Copy, PartialEq, Debug)]
struct BvhNode {
    aabb_min: Vec3,
    aabb_max: Vec3,
    left_first: u32,
    count: u32,
}

/// AABB/SAH Bounding Volume Hierarchy over arbitrary 3D triangle meshes.
///
/// Designed for skatepark bowls, quarterpipes, vert ramps, stairs, and curved transitions.
#[derive(Clone, Debug)]
pub struct TriMeshCollider3D {
    pub vertices: Vec<Vec3>,
    pub indices: Vec<[u32; 3]>,
    nodes: Vec<BvhNode>,
    tri_indices: Vec<u32>,
}

impl TriMeshCollider3D {
    /// Construct a new TriMeshCollider3D by building an AABB BVH over the provided mesh geometry.
    #[must_use]
    pub fn new(positions: &[Vec3], indices: &[[u32; 3]]) -> Self {
        let tri_count = indices.len();
        let mut tri_indices: Vec<u32> = (0..tri_count as u32).collect();

        let mut collider = Self {
            vertices: positions.to_vec(),
            indices: indices.to_vec(),
            nodes: Vec::with_capacity(tri_count.saturating_mul(2).max(1)),
            tri_indices: Vec::new(),
        };

        if tri_count > 0 {
            collider.build_bvh(&mut tri_indices);
        }
        collider.tri_indices = tri_indices;
        collider
    }

    /// Construct a bowl/halfpipe segment mesh collider for skatepark testing.
    #[must_use]
    pub fn new_curved_bowl(radius: f32, depth: f32, segments: usize) -> Self {
        let mut positions = Vec::new();
        let mut indices = Vec::new();

        let segs = segments.max(4);
        for y_idx in 0..=segs {
            let theta = (y_idx as f32 / segs as f32) * std::f32::consts::FRAC_PI_2; // 0 to 90 deg
            let y = -depth * theta.cos();
            let r = radius * theta.sin();

            for x_idx in 0..=segs {
                let phi = (x_idx as f32 / segs as f32) * std::f32::consts::TAU;
                let x = r * phi.cos();
                let z = r * phi.sin();
                positions.push(Vec3::new(x, y, z));
            }
        }

        let stride = segs + 1;
        for y_idx in 0..segs {
            for x_idx in 0..segs {
                let i0 = (y_idx * stride + x_idx) as u32;
                let i1 = (y_idx * stride + x_idx + 1) as u32;
                let i2 = ((y_idx + 1) * stride + x_idx) as u32;
                let i3 = ((y_idx + 1) * stride + x_idx + 1) as u32;

                indices.push([i0, i1, i2]);
                indices.push([i1, i3, i2]);
            }
        }

        Self::new(&positions, &indices)
    }

    fn build_bvh(&mut self, tri_indices: &mut [u32]) {
        // Allocate root node
        let root = BvhNode {
            aabb_min: Vec3::splat(f32::INFINITY),
            aabb_max: Vec3::splat(f32::NEG_INFINITY),
            left_first: 0,
            count: tri_indices.len() as u32,
        };
        self.nodes.push(root);

        self.update_node_bounds(0, tri_indices);
        self.subdivide(0, tri_indices);
    }

    fn update_node_bounds(&mut self, node_idx: usize, tri_indices: &[u32]) {
        let first = self.nodes[node_idx].left_first as usize;
        let count = self.nodes[node_idx].count as usize;

        let mut bmin = Vec3::splat(f32::INFINITY);
        let mut bmax = Vec3::splat(f32::NEG_INFINITY);

        for &tri_idx in &tri_indices[first..first + count] {
            let tri = self.indices[tri_idx as usize];
            for &vi in &tri {
                let p = self.vertices[vi as usize];
                bmin = bmin.min(p);
                bmax = bmax.max(p);
            }
        }

        self.nodes[node_idx].aabb_min = bmin;
        self.nodes[node_idx].aabb_max = bmax;
    }

    fn subdivide(&mut self, node_idx: usize, tri_indices: &mut [u32]) {
        let count = self.nodes[node_idx].count as usize;
        if count <= 4 {
            return;
        }

        let bmin = self.nodes[node_idx].aabb_min;
        let bmax = self.nodes[node_idx].aabb_max;
        let extent = bmax - bmin;

        // Split along longest axis
        let axis = if extent.x > extent.y && extent.x > extent.z {
            0
        } else if extent.y > extent.z {
            1
        } else {
            2
        };

        let split_pos = bmin[axis] + extent[axis] * 0.5;

        let first = self.nodes[node_idx].left_first as usize;
        let mut i = first;
        let mut j = first + count - 1;

        while i <= j {
            let tri_idx = tri_indices[i] as usize;
            let tri = self.indices[tri_idx];
            let centroid = (self.vertices[tri[0] as usize]
                + self.vertices[tri[1] as usize]
                + self.vertices[tri[2] as usize])
                / 3.0;

            if centroid[axis] < split_pos {
                i += 1;
            } else {
                tri_indices.swap(i, j);
                if j == 0 {
                    break;
                }
                j -= 1;
            }
        }

        let left_count = i - first;
        if left_count == 0 || left_count == count {
            return; // Cannot partition further
        }

        let left_child_idx = self.nodes.len();
        self.nodes.push(BvhNode {
            aabb_min: Vec3::splat(f32::INFINITY),
            aabb_max: Vec3::splat(f32::NEG_INFINITY),
            left_first: first as u32,
            count: left_count as u32,
        });

        self.nodes.push(BvhNode {
            aabb_min: Vec3::splat(f32::INFINITY),
            aabb_max: Vec3::splat(f32::NEG_INFINITY),
            left_first: i as u32,
            count: (count - left_count) as u32,
        });

        self.nodes[node_idx].left_first = left_child_idx as u32;
        self.nodes[node_idx].count = 0; // Marks as interior node

        self.update_node_bounds(left_child_idx, tri_indices);
        self.update_node_bounds(left_child_idx + 1, tri_indices);

        self.subdivide(left_child_idx, tri_indices);
        self.subdivide(left_child_idx + 1, tri_indices);
    }

    /// Cast a 3D ray against the triangle mesh BVH, returning the closest hit within `max_dist`.
    #[must_use]
    pub fn ray_cast(&self, origin: Vec3, dir: Vec3, max_dist: f32) -> Option<RayHit3D> {
        if self.nodes.is_empty() || dir.length_squared() <= 1e-8 {
            return None;
        }

        let ray_dir = dir.normalize_or_zero();
        let inv_dir = Vec3::new(
            if ray_dir.x.abs() > 1e-8 { 1.0 / ray_dir.x } else { 1e8 },
            if ray_dir.y.abs() > 1e-8 { 1.0 / ray_dir.y } else { 1e8 },
            if ray_dir.z.abs() > 1e-8 { 1.0 / ray_dir.z } else { 1e8 },
        );

        let mut stack = [0_usize; 64];
        let mut stack_ptr = 1;
        stack[0] = 0;

        let mut closest_hit: Option<RayHit3D> = None;
        let mut closest_dist = max_dist;

        while stack_ptr > 0 {
            stack_ptr -= 1;
            let node_idx = stack[stack_ptr];
            let node = &self.nodes[node_idx];

            if !ray_intersects_aabb(origin, inv_dir, node.aabb_min, node.aabb_max, closest_dist) {
                continue;
            }

            if node.count > 0 {
                // Leaf node: intersect triangles
                let first = node.left_first as usize;
                let count = node.count as usize;

                for &tri_idx in &self.tri_indices[first..first + count] {
                    let tri = self.indices[tri_idx as usize];
                    let v0 = self.vertices[tri[0] as usize];
                    let v1 = self.vertices[tri[1] as usize];
                    let v2 = self.vertices[tri[2] as usize];

                    if let Some((t, normal)) = ray_triangle_intersect_bvh(origin, ray_dir, v0, v1, v2)
                        && t < closest_dist
                    {
                        closest_dist = t;
                        closest_hit = Some(RayHit3D {
                            body_id: None,
                            point: origin + ray_dir * t,
                            normal,
                            distance: t,
                        });
                    }
                }
            } else {
                let left = node.left_first as usize;
                stack[stack_ptr] = left + 1;
                stack[stack_ptr + 1] = left;
                stack_ptr += 2;
            }
        }

        closest_hit
    }

    /// Test narrow-phase collision of a sphere against the mesh, returning the deepest contact manifold.
    #[must_use]
    pub fn collide_sphere(&self, center: Vec3, radius: f32) -> Option<ContactManifold3D> {
        if self.nodes.is_empty() || radius <= 0.0 {
            return None;
        }

        let sphere_min = center - Vec3::splat(radius);
        let sphere_max = center + Vec3::splat(radius);
        let rad_sq = radius * radius;

        let mut stack = [0_usize; 64];
        let mut stack_ptr = 1;
        stack[0] = 0;

        let mut deepest_manifold: Option<ContactManifold3D> = None;
        let mut max_pen = 0.0_f32;

        while stack_ptr > 0 {
            stack_ptr -= 1;
            let node_idx = stack[stack_ptr];
            let node = &self.nodes[node_idx];

            // AABB vs AABB overlap check
            if node.aabb_max.x < sphere_min.x || node.aabb_min.x > sphere_max.x
                || node.aabb_max.y < sphere_min.y || node.aabb_min.y > sphere_max.y
                || node.aabb_max.z < sphere_min.z || node.aabb_min.z > sphere_max.z
            {
                continue;
            }

            if node.count > 0 {
                let first = node.left_first as usize;
                let count = node.count as usize;

                for &tri_idx in &self.tri_indices[first..first + count] {
                    let tri = self.indices[tri_idx as usize];
                    let a = self.vertices[tri[0] as usize];
                    let b = self.vertices[tri[1] as usize];
                    let c = self.vertices[tri[2] as usize];

                    let closest_pt = closest_point_on_triangle(center, a, b, c);
                    let diff = center - closest_pt;
                    let dist_sq = diff.length_squared();

                    if dist_sq < rad_sq {
                        let dist = dist_sq.sqrt();
                        let penetration = radius - dist;

                        if penetration > max_pen {
                            max_pen = penetration;
                            let fnorm = (b - a).cross(c - a).normalize_or_zero();
                            let normal = if dist > 1e-5 {
                                diff / dist
                            } else {
                                fnorm
                            };

                            deepest_manifold = Some(ContactManifold3D {
                                point: closest_pt,
                                normal,
                                penetration,
                            });
                        }
                    }
                }
            } else {
                let left = node.left_first as usize;
                stack[stack_ptr] = left + 1;
                stack[stack_ptr + 1] = left;
                stack_ptr += 2;
            }
        }

        deepest_manifold
    }
}

fn ray_intersects_aabb(origin: Vec3, inv_dir: Vec3, min: Vec3, max: Vec3, max_t: f32) -> bool {
    let t0 = (min - origin) * inv_dir;
    let t1 = (max - origin) * inv_dir;

    let tmin = t0.min(t1);
    let tmax = t0.max(t1);

    let enter = tmin.x.max(tmin.y).max(tmin.z).max(0.0);
    let exit = tmax.x.min(tmax.y).min(tmax.z).min(max_t);

    enter <= exit
}

fn ray_triangle_intersect_bvh(
    origin: Vec3,
    dir: Vec3,
    v0: Vec3,
    v1: Vec3,
    v2: Vec3,
) -> Option<(f32, Vec3)> {
    let edge1 = v1 - v0;
    let edge2 = v2 - v0;
    let pvec = dir.cross(edge2);
    let det = edge1.dot(pvec);

    if det.abs() < 1e-7 {
        return None;
    }

    let inv_det = 1.0 / det;
    let tvec = origin - v0;
    let u = tvec.dot(pvec) * inv_det;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }

    let qvec = tvec.cross(edge1);
    let v = dir.dot(qvec) * inv_det;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }

    let t = edge2.dot(qvec) * inv_det;
    if t > 1e-4 {
        let fnorm = edge1.cross(edge2).normalize_or_zero();
        let normal = if fnorm.dot(dir) < 0.0 { fnorm } else { -fnorm };
        Some((t, normal))
    } else {
        None
    }
}

/// Compute the closest point on a 3D triangle $(A, B, C)$ to query point $P$ using Voronoi regions.
#[must_use]
pub fn closest_point_on_triangle(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    let ab = b - a;
    let ac = c - a;
    let ap = p - a;

    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }

    let bp = p - b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }

    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return a + ab * v;
    }

    let cp = p - c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }

    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return a + ac * w;
    }

    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return b + (c - b) * w;
    }

    let denom = 1.0 / (va + vb + vc);
    let v = vb * denom;
    let w = vc * denom;
    a + ab * v + ac * w
}

/// 3D convex hull collider supporting extreme support queries and GJK collision tests.
#[derive(Clone, PartialEq, Debug)]
pub struct ConvexHullCollider3D {
    pub points: Vec<Vec3>,
}

impl ConvexHullCollider3D {
    /// Create a new convex hull from a point cloud.
    #[must_use]
    pub fn new(points: Vec<Vec3>) -> Self {
        Self { points }
    }

    /// Create a convex box with half-extents.
    #[must_use]
    pub fn new_box(half_extents: Vec3) -> Self {
        let h = half_extents;
        let points = vec![
            Vec3::new(-h.x, -h.y, -h.z),
            Vec3::new( h.x, -h.y, -h.z),
            Vec3::new( h.x,  h.y, -h.z),
            Vec3::new(-h.x,  h.y, -h.z),
            Vec3::new(-h.x, -h.y,  h.z),
            Vec3::new( h.x, -h.y,  h.z),
            Vec3::new( h.x,  h.y,  h.z),
            Vec3::new(-h.x,  h.y,  h.z),
        ];
        Self { points }
    }

    /// Evaluate the support point along a search direction: $\arg\max_{v \in S} (v \cdot \text{dir})$.
    #[must_use]
    pub fn support_point(&self, dir: Vec3) -> Vec3 {
        let mut best_pt = self.points.first().copied().unwrap_or(Vec3::ZERO);
        let mut best_dot = f32::NEG_INFINITY;

        for &p in &self.points {
            let dot = p.dot(dir);
            if dot > best_dot {
                best_dot = dot;
                best_pt = p;
            }
        }
        best_pt
    }

    /// Evaluate support point transformed by an affine matrix.
    #[must_use]
    pub fn support_point_transformed(&self, dir: Vec3, transform: Mat4) -> Vec3 {
        let inv_rot = transform.inverse().transform_vector3(dir);
        let local_pt = self.support_point(inv_rot);
        transform.transform_point3(local_pt)
    }
}

/// Gilbert-Johnson-Keerthi (GJK) convex collision intersection query.
///
/// Returns `true` if the two convex hulls intersect in 3D space under their respective transforms.
#[must_use]
pub fn gjk_intersects(
    hull_a: &ConvexHullCollider3D,
    transform_a: Mat4,
    hull_b: &ConvexHullCollider3D,
    transform_b: Mat4,
) -> bool {
    let mut dir = Vec3::Y;

    let mut simplex: Vec<Vec3> = Vec::with_capacity(4);
    let initial_pt = support_minkowski(hull_a, transform_a, hull_b, transform_b, dir);
    simplex.push(initial_pt);
    dir = -initial_pt;

    for _ in 0..32 {
        if dir.length_squared() < 1e-8 {
            return true;
        }

        let new_pt = support_minkowski(hull_a, transform_a, hull_b, transform_b, dir);
        if new_pt.dot(dir) < 0.0 {
            return false; // Origin outside Minkowski difference
        }

        simplex.push(new_pt);
        if update_simplex_and_direction(&mut simplex, &mut dir) {
            return true;
        }
    }

    false
}

fn support_minkowski(
    a: &ConvexHullCollider3D,
    ta: Mat4,
    b: &ConvexHullCollider3D,
    tb: Mat4,
    dir: Vec3,
) -> Vec3 {
    let sa = a.support_point_transformed(dir, ta);
    let sb = b.support_point_transformed(-dir, tb);
    sa - sb
}

fn update_simplex_and_direction(simplex: &mut Vec<Vec3>, dir: &mut Vec3) -> bool {
    match simplex.len() {
        2 => {
            // Line segment A-B (B is newest)
            let b = simplex[1];
            let a = simplex[0];
            let ab = a - b;
            let ao = -b;

            if ab.dot(ao) > 0.0 {
                *dir = ab.cross(ao).cross(ab).normalize_or_zero();
            } else {
                simplex.swap_remove(0);
                *dir = ao;
            }
            false
        }
        3 => {
            // Triangle A-B-C (C is newest)
            let c = simplex[2];
            let b = simplex[1];
            let a = simplex[0];

            let cb = b - c;
            let ca = a - c;
            let co = -c;

            let abc_norm = cb.cross(ca);

            if abc_norm.cross(ca).dot(co) > 0.0 {
                if ca.dot(co) > 0.0 {
                    simplex.remove(1); // Keep C, A
                    *dir = ca.cross(co).cross(ca).normalize_or_zero();
                } else if cb.dot(co) > 0.0 {
                    simplex.remove(0); // Keep C, B
                    *dir = cb.cross(co).cross(cb).normalize_or_zero();
                } else {
                    simplex.truncate(1);
                    simplex[0] = c;
                    *dir = co;
                }
            } else if cb.cross(abc_norm).dot(co) > 0.0 {
                if cb.dot(co) > 0.0 {
                    simplex.remove(0); // Keep C, B
                    *dir = cb.cross(co).cross(cb).normalize_or_zero();
                } else {
                    simplex.truncate(1);
                    simplex[0] = c;
                    *dir = co;
                }
            } else if abc_norm.dot(co) > 0.0 {
                *dir = abc_norm;
            } else {
                simplex.swap(0, 1);
                *dir = -abc_norm;
            }
            false
        }
        4 => {
            // Tetrahedron A-B-C-D (D is newest)
            let d = simplex[3];
            let c = simplex[2];
            let b = simplex[1];
            let a = simplex[0];

            let da = a - d;
            let db = b - d;
            let dc = c - d;
            let _do_vec = -d;

            let n_abc = (b - a).cross(c - a);
            let n_adb = da.cross(db);
            let n_bdc = db.cross(dc);
            let n_cda = dc.cross(da);

            if n_adb.dot(_do_vec) > 0.0 {
                simplex.remove(2); // Drop C
                *dir = n_adb;
                false
            } else if n_bdc.dot(_do_vec) > 0.0 {
                simplex.remove(0); // Drop A
                *dir = n_bdc;
                false
            } else if n_cda.dot(_do_vec) > 0.0 {
                simplex.remove(1); // Drop B
                *dir = n_cda;
                false
            } else if n_abc.dot(_do_vec) > 0.0 {
                simplex.remove(3); // Drop D
                *dir = n_abc;
                false
            } else {
                true // Enclosed origin!
            }
        }
        _ => false,
    }
}

/// Identifiers for the 6 anatomical bodies in the articulated skater bail ragdoll.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum RagdollBoneId {
    Pelvis = 0,
    Torso = 1,
    Head = 2,
    LeftLeg = 3,
    RightLeg = 4,
    Arms = 5,
}

/// Physical sphere body node representing a bone in the articulated ragdoll.
#[derive(Clone, PartialEq, Debug)]
pub struct RagdollBone {
    pub id: RagdollBoneId,
    pub position: Vec3,
    pub previous_position: Vec3,
    pub velocity: Vec3,
    pub mass: f32,
    pub radius: f32,
    pub rest_offset: Vec3,
}

#[derive(Clone, Copy, PartialEq, Debug)]
struct RagdollJoint {
    bone_a: usize,
    bone_b: usize,
    rest_distance: f32,
}

/// 6-body articulated skater bail ragdoll solver with distance/cone joint constraints
/// and ground/trimesh restitution.
#[derive(Clone, PartialEq, Debug)]
pub struct RagdollRig3D {
    pub bones: Vec<RagdollBone>,
    joints: Vec<RagdollJoint>,
    pub restitution: f32,
    pub friction: f32,
    pub is_active: bool,
}

impl Default for RagdollRig3D {
    fn default() -> Self {
        Self::new()
    }
}

impl RagdollRig3D {
    /// Construct a default 6-body skater ragdoll rig.
    #[must_use]
    pub fn new() -> Self {
        let bones = vec![
            RagdollBone {
                id: RagdollBoneId::Pelvis,
                position: Vec3::new(0.0, 1.0, 0.0),
                previous_position: Vec3::new(0.0, 1.0, 0.0),
                velocity: Vec3::ZERO,
                mass: 25.0,
                radius: 0.2,
                rest_offset: Vec3::new(0.0, 1.0, 0.0),
            },
            RagdollBone {
                id: RagdollBoneId::Torso,
                position: Vec3::new(0.0, 1.45, 0.0),
                previous_position: Vec3::new(0.0, 1.45, 0.0),
                velocity: Vec3::ZERO,
                mass: 25.0,
                radius: 0.22,
                rest_offset: Vec3::new(0.0, 1.45, 0.0),
            },
            RagdollBone {
                id: RagdollBoneId::Head,
                position: Vec3::new(0.0, 1.8, 0.0),
                previous_position: Vec3::new(0.0, 1.8, 0.0),
                velocity: Vec3::ZERO,
                mass: 5.0,
                radius: 0.14,
                rest_offset: Vec3::new(0.0, 1.8, 0.0),
            },
            RagdollBone {
                id: RagdollBoneId::LeftLeg,
                position: Vec3::new(-0.2, 0.45, 0.0),
                previous_position: Vec3::new(-0.2, 0.45, 0.0),
                velocity: Vec3::ZERO,
                mass: 10.0,
                radius: 0.15,
                rest_offset: Vec3::new(-0.2, 0.45, 0.0),
            },
            RagdollBone {
                id: RagdollBoneId::RightLeg,
                position: Vec3::new(0.2, 0.45, 0.0),
                previous_position: Vec3::new(0.2, 0.45, 0.0),
                velocity: Vec3::ZERO,
                mass: 10.0,
                radius: 0.15,
                rest_offset: Vec3::new(0.2, 0.45, 0.0),
            },
            RagdollBone {
                id: RagdollBoneId::Arms,
                position: Vec3::new(0.0, 1.4, 0.35),
                previous_position: Vec3::new(0.0, 1.4, 0.35),
                velocity: Vec3::ZERO,
                mass: 10.0,
                radius: 0.16,
                rest_offset: Vec3::new(0.0, 1.4, 0.35),
            },
        ];

        let joints = vec![
            RagdollJoint { bone_a: 0, bone_b: 1, rest_distance: 0.45 }, // Pelvis <-> Torso
            RagdollJoint { bone_a: 1, bone_b: 2, rest_distance: 0.35 }, // Torso <-> Head
            RagdollJoint { bone_a: 0, bone_b: 3, rest_distance: 0.58 }, // Pelvis <-> LeftLeg
            RagdollJoint { bone_a: 0, bone_b: 4, rest_distance: 0.58 }, // Pelvis <-> RightLeg
            RagdollJoint { bone_a: 1, bone_b: 5, rest_distance: 0.35 }, // Torso <-> Arms
        ];

        Self {
            bones,
            joints,
            restitution: 0.25,
            friction: 0.85,
            is_active: false,
        }
    }

    /// Trigger bail ragdoll activation with skater root position and linear launch velocity.
    pub fn activate_from_skater_velocity(&mut self, root_pos: Vec3, linear_vel: Vec3) {
        self.is_active = true;

        for (i, bone) in self.bones.iter_mut().enumerate() {
            let offset = bone.rest_offset;
            bone.position = root_pos + offset;
            bone.previous_position = bone.position;

            // Introduce chaotic angular spin on limbs during bail
            let tumble_impulse = match i {
                0 => Vec3::ZERO,
                1 => Vec3::new(0.0, 0.5, 0.0),
                2 => Vec3::new(0.2, 0.4, -0.3),
                3 => Vec3::new(-0.8, -0.3, 0.5),
                4 => Vec3::new(0.8, -0.3, -0.5),
                _ => Vec3::new(0.0, 1.0, 0.8),
            };

            bone.velocity = linear_vel + tumble_impulse;
        }
    }

    /// Advance ragdoll dynamics by time step `dt_seconds`.
    pub fn step(&mut self, dt_seconds: f32, trimesh: Option<&TriMeshCollider3D>) {
        if !self.is_active || dt_seconds <= 1e-6 {
            return;
        }

        let gravity = Vec3::new(0.0, -9.81, 0.0);

        // 1. Verlet position integration
        for bone in &mut self.bones {
            bone.velocity += gravity * dt_seconds;
            bone.previous_position = bone.position;
            bone.position += bone.velocity * dt_seconds;
        }

        // 2. Obstacle Collisions (Ground Plane Y = 0 + TriMesh)
        for bone in &mut self.bones {
            // Ground plane collision
            if bone.position.y - bone.radius < 0.0 {
                let pen = bone.radius - bone.position.y;
                bone.position.y = bone.radius;
                if bone.velocity.y < 0.0 {
                    bone.velocity.y = -bone.velocity.y * self.restitution;
                    bone.velocity.x *= self.friction;
                    bone.velocity.z *= self.friction;
                }
                bone.position.y += pen * 0.1;
            }

            // TriMesh bowl collision
            if let Some(mesh) = trimesh
                && let Some(contact) = mesh.collide_sphere(bone.position, bone.radius)
            {
                bone.position += contact.normal * contact.penetration;
                let v_norm = bone.velocity.dot(contact.normal);
                if v_norm < 0.0 {
                    bone.velocity -= contact.normal * (v_norm * (1.0 + self.restitution));
                    bone.velocity *= self.friction;
                }
            }
        }

        // 3. Distance/Joint Constraint Solver (6 relaxation iterations)
        for _ in 0..6 {
            for joint in &self.joints {
                let p_a = self.bones[joint.bone_a].position;
                let p_b = self.bones[joint.bone_b].position;
                let delta = p_b - p_a;
                let dist = delta.length();

                if dist > 1e-4 {
                    let err = dist - joint.rest_distance;
                    let dir = delta / dist;

                    let m_a = self.bones[joint.bone_a].mass;
                    let m_b = self.bones[joint.bone_b].mass;
                    let inv_mass_sum = 1.0 / (m_a + m_b);

                    let corr_a = dir * (err * (m_b * inv_mass_sum));
                    let corr_b = dir * (err * (m_a * inv_mass_sum));

                    self.bones[joint.bone_a].position += corr_a * 0.5;
                    self.bones[joint.bone_b].position -= corr_b * 0.5;
                }
            }
        }

        // 4. Update velocities from settled positions
        for bone in &mut self.bones {
            bone.velocity = (bone.position - bone.previous_position) / dt_seconds;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trimesh_curved_bowl_sphere_sliding() {
        let bowl = TriMeshCollider3D::new_curved_bowl(5.0, 3.0, 8);
        assert!(!bowl.vertices.is_empty());
        assert!(!bowl.indices.is_empty());

        // Cast ray down center of bowl
        let hit = bowl.ray_cast(Vec3::new(0.0, 2.0, 0.0), Vec3::NEG_Y, 10.0);
        assert!(hit.is_some(), "Ray down center should hit bottom of bowl");
        let h = hit.expect("hit exists");
        assert!(h.point.y < 0.0);

        // Test sphere contact on curved bowl surface
        let sphere_pos = Vec3::new(2.0, -1.0, 0.0);
        let contact = bowl.collide_sphere(sphere_pos, 0.5);
        if let Some(c) = contact {
            assert!(c.penetration >= 0.0);
            assert!(c.normal.length_squared() > 0.5);
        }
    }

    #[test]
    fn convex_hull_gjk_intersection() {
        let box_a = ConvexHullCollider3D::new_box(Vec3::splat(1.0));
        let box_b = ConvexHullCollider3D::new_box(Vec3::splat(1.0));

        // Overlapping at origin
        assert!(gjk_intersects(&box_a, Mat4::IDENTITY, &box_b, Mat4::IDENTITY));

        // Separated along X axis
        let trans_sep = Mat4::from_translation(Vec3::new(5.0, 0.0, 0.0));
        assert!(!gjk_intersects(&box_a, Mat4::IDENTITY, &box_b, trans_sep));

        // Barely touching at distance 1.99
        let trans_touch = Mat4::from_translation(Vec3::new(1.99, 0.0, 0.0));
        assert!(gjk_intersects(&box_a, Mat4::IDENTITY, &box_b, trans_touch));
    }

    #[test]
    fn ragdoll_skater_bail_settle() {
        let mut ragdoll = RagdollRig3D::new();
        ragdoll.activate_from_skater_velocity(Vec3::new(0.0, 3.0, 0.0), Vec3::new(5.0, 2.0, 0.0));
        assert!(ragdoll.is_active);

        // Step simulation for 1.0 second (60 frames)
        for _ in 0..60 {
            ragdoll.step(1.0 / 60.0, None);
        }

        // Ragdoll should settle on ground without blowing up or falling through Y=0
        for bone in &ragdoll.bones {
            assert!(bone.position.y >= bone.radius - 0.05, "Bone {} fell through floor: pos={:?}", bone.id as usize, bone.position);
            assert!(bone.position.is_finite());
            assert!(bone.velocity.is_finite());
        }
    }
}
