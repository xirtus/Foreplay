//! Bevy 0.20 `SceneTree` Relational & Rebuild Equivalence Fuzzer (`foreplay-verifier`).
//!
//! Verifies ECS entity bijectivity, `ChildOf` / `Children` hierarchy relations,
//! zero-orphaned entity guarantees on `rebuild_world_from_ir()`, and byte-deterministic
//! `.fscn` serialization stability.

use bevy_ecs::hierarchy::{ChildOf, Children};
use foreplay_core::{ForeplayNodeMeta, SceneTree};
use foreplay_ir::{IrNode, IrScene, IrValue, NodeId, SignalConnection};
use std::collections::{BTreeMap, HashSet};
use std::num::NonZeroU32;

/// Checks that `SceneTree::from_ir` spawns the exact bijective set of `bevy_ecs` entities.
pub fn verify_bijective_ecs_entities(tree: &mut SceneTree, expected_scene: &IrScene) -> Result<(), String> {
    if tree.ir_to_entity.len() != expected_scene.nodes.len() {
        return Err(format!(
            "Bijectivity error: tree.ir_to_entity has {} entries, but scene has {} nodes",
            tree.ir_to_entity.len(),
            expected_scene.nodes.len()
        ));
    }

    let mut visited_entities = HashSet::new();

    for (&node_id, ir_node) in &expected_scene.nodes {
        let Some(&entity) = tree.ir_to_entity.get(&node_id) else {
            return Err(format!("NodeId {node_id:?} missing from tree.ir_to_entity"));
        };

        if !visited_entities.insert(entity) {
            return Err(format!(
                "Bijectivity error: duplicate Entity {entity:?} mapped from multiple NodeIds!"
            ));
        }

        let Some(meta) = tree.world.get::<ForeplayNodeMeta>(entity) else {
            return Err(format!(
                "Entity {entity:?} for NodeId {node_id:?} is missing ForeplayNodeMeta component"
            ));
        };

        let expected_packed = (u64::from(node_id.generation.get()) << 32) | u64::from(node_id.index);
        if meta.ir_id != expected_packed {
            return Err(format!(
                "ForeplayNodeMeta.ir_id ({}) != expected packed id ({}) for NodeId {node_id:?}",
                meta.ir_id, expected_packed
            ));
        }
        if meta.name != ir_node.name {
            return Err(format!(
                "ForeplayNodeMeta.name ('{}') != expected node.name ('{}')",
                meta.name, ir_node.name
            ));
        }
        if meta.godot_class != ir_node.class_name {
            return Err(format!(
                "ForeplayNodeMeta.godot_class ('{}') != expected ('{}')",
                meta.godot_class, ir_node.class_name
            ));
        }
    }

    // Verify exactly expected_scene.nodes.len() entities have ForeplayNodeMeta in World (zero ghosts)
    let meta_count = tree
        .world
        .query::<&ForeplayNodeMeta>()
        .iter(&tree.world)
        .count();
    if meta_count != expected_scene.nodes.len() {
        return Err(format!(
            "Total ForeplayNodeMeta entities in World ({meta_count}) != expected ({})",
            expected_scene.nodes.len()
        ));
    }

    Ok(())
}

/// Checks that every `IrNode` with `parent: Some(p)` where `p` exists in `nodes`
/// produces a valid `ChildOf(parent_ent)` and `Children` contains `child_ent`.
pub fn verify_hierarchy_relations(tree: &SceneTree, scene: &IrScene) -> Result<(), String> {
    for (&node_id, ir_node) in &scene.nodes {
        let child_ent = tree.ir_to_entity[&node_id];

        if let Some(parent_id) = ir_node.parent {
            if let Some(&parent_ent) = tree.ir_to_entity.get(&parent_id) {
                // Must have ChildOf pointing to parent_ent
                let child_of = tree
                    .world
                    .get::<ChildOf>(child_ent)
                    .ok_or_else(|| format!("Node {node_id:?} missing ChildOf relation"))?;

                if child_of.parent() != parent_ent {
                    return Err(format!(
                        "Node {node_id:?} ChildOf points to {:?}, expected parent_ent {parent_ent:?}",
                        child_of.parent()
                    ));
                }

                // Parent must have Children containing child_ent
                let children = tree
                    .world
                    .get::<Children>(parent_ent)
                    .ok_or_else(|| format!("Parent {parent_id:?} missing Children component"))?;

                if !children.contains(&child_ent) {
                    return Err(format!(
                        "Parent {parent_id:?} Children does not contain child {node_id:?}"
                    ));
                }
            } else {
                // Parent does not exist in nodes: should not have ChildOf relation
                if let Some(child_of) = tree.world.get::<ChildOf>(child_ent) {
                    return Err(format!(
                        "Node {node_id:?} has ChildOf({:?}) but parent {parent_id:?} not in scene!",
                        child_of.parent()
                    ));
                }
            }
        } else {
            // Root nodes should not have ChildOf relation
            if let Some(child_of) = tree.world.get::<ChildOf>(child_ent) {
                return Err(format!(
                    "Root node {node_id:?} has unexpected ChildOf({:?})",
                    child_of.parent()
                ));
            }
        }
    }

    Ok(())
}

/// Verifies that `rebuild_world_from_ir()` after arbitrary mutations produces an isomorphic
/// ECS hierarchy with zero orphaned entities.
pub fn verify_rebuild_world_equivalence(
    tree: &mut SceneTree,
    mutated_scene: IrScene,
) -> Result<(), String> {
    tree.active_scene = mutated_scene.clone();
    tree.rebuild_world_from_ir();

    verify_bijective_ecs_entities(tree, &mutated_scene)?;
    verify_hierarchy_relations(tree, &mutated_scene)?;

    // Total entities in World should match exactly active_scene.nodes.len()
    let total_entities_with_meta = tree
        .world
        .query::<&ForeplayNodeMeta>()
        .iter(&tree.world)
        .count();
    if total_entities_with_meta != mutated_scene.nodes.len() {
        return Err(format!(
            "Zero-orphan invariant violated: found {total_entities_with_meta} entities with ForeplayNodeMeta, expected {}",
            mutated_scene.nodes.len()
        ));
    }

    Ok(())
}

/// Verifies deterministic `.fscn` output stability:
/// `scene.to_fscn_string()` produces byte-for-byte identical output across repeated
/// invocations and regardless of insertion order into maps/sets.
pub fn verify_fscn_emission_stability(scene: &IrScene) -> Result<(), String> {
    let baseline = scene.to_fscn_string();

    // Invocations on the exact same instance must be byte-for-byte identical
    for i in 0..5 {
        let repeated = scene.to_fscn_string();
        if repeated != baseline {
            return Err(format!(
                "Repeated to_fscn_string() call #{i} differed from baseline"
            ));
        }
    }

    // Reconstruct scene with nodes and feel overrides inserted in reversed order
    let mut permuted = IrScene::new(&scene.name);

    // Reversed feel overrides
    let mut reversed_feels: Vec<_> = scene.feel_overrides.iter().collect();
    reversed_feels.reverse();
    for (k, v) in reversed_feels {
        permuted.feel_overrides.insert(k.clone(), *v);
    }

    // Reversed nodes with reversed properties
    let mut reversed_nodes: Vec<_> = scene.nodes.iter().collect();
    reversed_nodes.reverse();
    for (&id, node) in reversed_nodes {
        let mut perm_node = node.clone();
        let mut reversed_props: Vec<_> = node.properties.iter().collect();
        reversed_props.reverse();
        perm_node.properties.clear();
        for (pk, pv) in reversed_props {
            perm_node.properties.insert(pk.clone(), pv.clone());
        }
        permuted.nodes.insert(id, perm_node);
    }

    // Copy signal connections
    permuted.connections = scene.connections.clone();

    let permuted_emission = permuted.to_fscn_string();
    if permuted_emission != baseline {
        return Err(
            "FSCN emission stability failed: output differed when scene was constructed with reversed insertion order!"
                .into(),
        );
    }

    Ok(())
}

/// Helper building a deterministic tree hierarchy for tests.
pub fn build_sample_tree_scene(chain_depth: usize, fan_out: usize) -> IrScene {
    let mut scene = IrScene::new("DeterministicSampleScene");

    // Add root
    let root_id = NodeId {
        index: 0,
        generation: NonZeroU32::new(1).unwrap(),
    };
    let mut root_props = BTreeMap::new();
    root_props.insert("world_gravity".into(), IrValue::F64(9.81));
    root_props.insert("tag".into(), IrValue::String("root_node".into()));

    scene.nodes.insert(
        root_id,
        IrNode {
            id: root_id,
            name: "WorldRoot".into(),
            class_name: "Node3D".into(),
            parent: None,
            instance_scene: None,
            properties: root_props,
            groups: vec!["roots".into()],
        },
    );

    // Build a deep chain: WorldRoot -> Chain1 -> Chain2 -> ...
    let mut prev_id = root_id;
    for i in 1..=chain_depth {
        let chain_id = NodeId {
            index: i as u32,
            generation: NonZeroU32::new(1).unwrap(),
        };
        scene.nodes.insert(
            chain_id,
            IrNode {
                id: chain_id,
                name: format!("ChainNode_{i}"),
                class_name: "Node3D".into(),
                parent: Some(prev_id),
                instance_scene: None,
                properties: BTreeMap::new(),
                groups: Vec::new(),
            },
        );
        prev_id = chain_id;
    }

    // Build high fan-out children directly under WorldRoot
    let start_fan = (chain_depth + 1) as u32;
    for f in 0..fan_out {
        let fan_id = NodeId {
            index: start_fan + f as u32,
            generation: NonZeroU32::new(1).unwrap(),
        };
        let mut props = BTreeMap::new();
        props.insert("collider_index".into(), IrValue::I64(f as i64));
        scene.nodes.insert(
            fan_id,
            IrNode {
                id: fan_id,
                name: format!("FanCollider_{f}"),
                class_name: "CollisionShape3D".into(),
                parent: Some(root_id),
                instance_scene: None,
                properties: props,
                groups: vec!["colliders".into()],
            },
        );
    }

    // Add arbitrary signal connections
    if chain_depth >= 1 && fan_out >= 1 {
        scene.connections.push(SignalConnection {
            from_node: root_id,
            signal_name: "tree_entered".into(),
            to_node: NodeId {
                index: 1,
                generation: NonZeroU32::new(1).unwrap(),
            },
            method_name: "_on_tree_entered".into(),
        });
        scene.connections.push(SignalConnection {
            from_node: NodeId {
                index: start_fan,
                generation: NonZeroU32::new(1).unwrap(),
            },
            signal_name: "body_entered".into(),
            to_node: root_id,
            method_name: "_on_body_entered".into(),
        });
    }

    scene.feel_overrides.insert("skater_speed_mult".into(), 1.25);
    scene.feel_overrides.insert("air_control_decay".into(), 0.05);

    scene
}

/// Proptest strategy generating arbitrary valid DAG / tree `IrScene` hierarchies
/// (forests, deep chains, high fan-out trees, arbitrary signal connections).
pub fn arb_tree_scene() -> impl proptest::strategy::Strategy<Value = IrScene> {
    use proptest::prelude::*;

    (1..=25usize).prop_flat_map(|node_count| {
        let parent_choices = (1..node_count)
            .map(|i| {
                prop_oneof![
                    Just(None),
                    (0..i).prop_map(move |p| {
                        Some(NodeId::new(p as u32, 1).expect("valid ID"))
                    })
                ]
            })
            .collect::<Vec<_>>();

        (
            Just(node_count),
            parent_choices,
            "[A-Z][a-zA-Z0-9_]{1,12}",
            prop::collection::btree_map(
                prop_oneof![
                    Just("ollie_pop".into()),
                    Just("air_drag".into()),
                    Just("rebound_damping".into()),
                ],
                any::<f64>(),
                0..3,
            ),
        )
            .prop_map(|(count, parents, scene_name, feel_overrides)| {
                let mut scene = IrScene::new(scene_name);
                scene.feel_overrides = feel_overrides;

                // Root node
                let root_id = NodeId::new(0, 1).expect("root");
                scene.nodes.insert(
                    root_id,
                    IrNode {
                        id: root_id,
                        name: "Root".into(),
                        class_name: "Node3D".into(),
                        parent: None,
                        instance_scene: None,
                        properties: BTreeMap::new(),
                        groups: vec!["roots".into()],
                    },
                );

                // Child / forest nodes
                for (idx, parent) in parents.into_iter().enumerate() {
                    let node_id = NodeId::new((idx + 1) as u32, 1).expect("valid");
                    let mut props = BTreeMap::new();
                    props.insert("node_index".into(), IrValue::I64(idx as i64));
                    scene.nodes.insert(
                        node_id,
                        IrNode {
                            id: node_id,
                            name: format!("Node_{}", idx + 1),
                            class_name: if idx % 3 == 0 {
                                "CharacterBody3D".into()
                            } else {
                                "Node3D".into()
                            },
                            parent,
                            instance_scene: None,
                            properties: props,
                            groups: Vec::new(),
                        },
                    );
                }

                // Add connections if multiple nodes exist
                if count > 1 {
                    scene.connections.push(SignalConnection {
                        from_node: root_id,
                        signal_name: "ready".into(),
                        to_node: NodeId::new(1, 1).expect("node 1"),
                        method_name: "_on_ready".into(),
                    });
                }

                scene
            })
    })
}

