//! Godot 4.4 Built-In Node Class Registry & Metadata Catalog.

use crate::IrValue;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// High-level engine category matching Godot's ClassDB taxonomy and badge coloring.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum NodeCategory {
    Plain,
    Node2D,
    Node3D,
    ControlUi,
    Animation,
    Audio,
}

impl NodeCategory {
    /// Returns the canonical Godot badge accent color [R, G, B, A] in 0.0..1.0 range.
    pub const fn badge_color(self) -> [f32; 4] {
        match self {
            Self::Plain => [0.65, 0.65, 0.65, 1.0],      // Godot base gray
            Self::Node2D => [0.35, 0.65, 0.95, 1.0],     // Godot 2D blue
            Self::Node3D => [0.95, 0.45, 0.40, 1.0],     // Godot 3D red/coral
            Self::ControlUi => [0.40, 0.85, 0.45, 1.0],  // Godot UI green
            Self::Animation => [0.85, 0.45, 0.85, 1.0],  // Godot Animation magenta
            Self::Audio => [0.95, 0.75, 0.35, 1.0],      // Godot Audio amber
        }
    }
}

/// Metadata describing a signal exposed by a built-in node class.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SignalSpec {
    pub name: String,
    pub parameters: Vec<(String, String)>,
    pub doc: String,
}

impl SignalSpec {
    pub fn new(name: impl Into<String>, doc: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            parameters: Vec::new(),
            doc: doc.into(),
        }
    }

    pub fn with_param(mut self, name: impl Into<String>, type_name: impl Into<String>) -> Self {
        self.parameters.push((name.into(), type_name.into()));
        self
    }
}

/// Full specification for a built-in Godot 4.4 engine node class.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct NodeClassSpec {
    pub name: String,
    pub parent_class: Option<String>,
    pub category: NodeCategory,
    pub badge_color: [f32; 4],
    pub doc: String,
    pub built_in_signals: Vec<SignalSpec>,
    pub default_properties: BTreeMap<String, IrValue>,
}

impl NodeClassSpec {
    pub fn new(
        name: impl Into<String>,
        parent_class: Option<&str>,
        category: NodeCategory,
        doc: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            parent_class: parent_class.map(str::to_string),
            category,
            badge_color: category.badge_color(),
            doc: doc.into(),
            built_in_signals: Vec::new(),
            default_properties: BTreeMap::new(),
        }
    }

    pub fn with_signal(mut self, signal: SignalSpec) -> Self {
        self.built_in_signals.push(signal);
        self
    }

    pub fn with_property(mut self, key: impl Into<String>, val: IrValue) -> Self {
        self.default_properties.insert(key.into(), val);
        self
    }
}

/// Returns the global static catalog containing 48+ standard Godot 4.4 node classes.
pub fn builtin_node_catalog() -> &'static BTreeMap<String, NodeClassSpec> {
    static CATALOG: OnceLock<BTreeMap<String, NodeClassSpec>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let specs = build_catalog_specs();
        let mut map = BTreeMap::new();
        for spec in specs {
            map.insert(spec.name.clone(), spec);
        }
        map
    })
}

/// Looks up a built-in node class by name.
pub fn get_node_class(name: &str) -> Option<&'static NodeClassSpec> {
    builtin_node_catalog().get(name)
}

/// Verifies whether `derived` inherits from `ancestor` in the built-in class taxonomy.
pub fn is_subclass_of(derived: &str, ancestor: &str) -> bool {
    if derived == ancestor {
        return true;
    }
    let catalog = builtin_node_catalog();
    let mut current = derived;
    while let Some(spec) = catalog.get(current) {
        if let Some(parent) = &spec.parent_class {
            if parent == ancestor {
                return true;
            }
            current = parent;
        } else {
            break;
        }
    }
    false
}

fn build_catalog_specs() -> Vec<NodeClassSpec> {
    vec![
        // -------------------------------------------------------------
        // 1. BASE / SYSTEM NODES (5 classes)
        // -------------------------------------------------------------
        NodeClassSpec::new("Node", None, NodeCategory::Plain, "Base class for all scene tree nodes.")
            .with_signal(SignalSpec::new("ready", "Emitted when the node is ready."))
            .with_signal(SignalSpec::new("tree_entered", "Emitted when the node enters the scene tree."))
            .with_signal(SignalSpec::new("tree_exited", "Emitted when the node exits the scene tree."))
            .with_property("process_mode", IrValue::I64(0)),

        NodeClassSpec::new("Timer", Some("Node"), NodeCategory::Plain, "A countdown timer.")
            .with_signal(SignalSpec::new("timeout", "Emitted when the timer reaches 0."))
            .with_property("wait_time", IrValue::F64(1.0))
            .with_property("one_shot", IrValue::Bool(false))
            .with_property("autostart", IrValue::Bool(false)),

        NodeClassSpec::new("CanvasLayer", Some("Node"), NodeCategory::Plain, "2D rendering layer.")
            .with_property("layer", IrValue::I64(1))
            .with_property("visible", IrValue::Bool(true)),

        NodeClassSpec::new("SubViewport", Some("Node"), NodeCategory::Plain, "Creates an isolated rendering target.")
            .with_signal(SignalSpec::new("size_changed", "Emitted when the viewport size changes."))
            .with_property("size", IrValue::Vec2([512.0, 512.0])),

        NodeClassSpec::new("WorldEnvironment", Some("Node"), NodeCategory::Plain, "Applies global rendering and post-processing environment.")
            .with_property("environment", IrValue::ResourcePath("res://default_env.tres".into())),

        // -------------------------------------------------------------
        // 2. 2D NODES (17 classes)
        // -------------------------------------------------------------
        NodeClassSpec::new("Node2D", Some("Node"), NodeCategory::Node2D, "2D game object with position, rotation, and scale.")
            .with_property("position", IrValue::Vec2([0.0, 0.0]))
            .with_property("rotation", IrValue::F64(0.0))
            .with_property("scale", IrValue::Vec2([1.0, 1.0])),

        NodeClassSpec::new("Sprite2D", Some("Node2D"), NodeCategory::Node2D, "Displays a 2D texture.")
            .with_property("centered", IrValue::Bool(true))
            .with_property("flip_h", IrValue::Bool(false))
            .with_property("flip_v", IrValue::Bool(false)),

        NodeClassSpec::new("AnimatedSprite2D", Some("Node2D"), NodeCategory::Node2D, "Displays frame-based 2D animations.")
            .with_signal(SignalSpec::new("animation_finished", "Emitted when playback reaches the end of the animation."))
            .with_signal(SignalSpec::new("frame_changed", "Emitted when the current frame changes.")),

        NodeClassSpec::new("Camera2D", Some("Node2D"), NodeCategory::Node2D, "Camera node for 2D scenes.")
            .with_property("zoom", IrValue::Vec2([1.0, 1.0]))
            .with_property("enabled", IrValue::Bool(true)),

        NodeClassSpec::new("TileMapLayer", Some("Node2D"), NodeCategory::Node2D, "Displays tiles from a TileSet in 2D grid.")
            .with_property("enabled", IrValue::Bool(true)),

        NodeClassSpec::new("CollisionShape2D", Some("Node2D"), NodeCategory::Node2D, "Provides 2D collision geometry.")
            .with_property("disabled", IrValue::Bool(false)),

        NodeClassSpec::new("StaticBody2D", Some("Node2D"), NodeCategory::Node2D, "A 2D body that cannot be moved by physics.")
            .with_property("constant_linear_velocity", IrValue::Vec2([0.0, 0.0])),

        NodeClassSpec::new("RigidBody2D", Some("Node2D"), NodeCategory::Node2D, "A 2D body driven by the physics simulation.")
            .with_signal(SignalSpec::new("body_entered", "Emitted when a body collides with this rigid body."))
            .with_signal(SignalSpec::new("body_exited", "Emitted when a body exits contact."))
            .with_property("mass", IrValue::F64(1.0))
            .with_property("gravity_scale", IrValue::F64(1.0)),

        NodeClassSpec::new("CharacterBody2D", Some("Node2D"), NodeCategory::Node2D, "A 2D body controlled by user gameplay code.")
            .with_property("motion_mode", IrValue::I64(0)),

        NodeClassSpec::new("Area2D", Some("Node2D"), NodeCategory::Node2D, "2D region that detects overlap with physics bodies and areas.")
            .with_signal(SignalSpec::new("body_entered", "Emitted when a physics body enters this area."))
            .with_signal(SignalSpec::new("body_exited", "Emitted when a physics body exits this area."))
            .with_signal(SignalSpec::new("area_entered", "Emitted when another area enters."))
            .with_signal(SignalSpec::new("area_exited", "Emitted when another area exits.")),

        NodeClassSpec::new("RayCast2D", Some("Node2D"), NodeCategory::Node2D, "Queries the 2D physics world along a ray.")
            .with_property("target_position", IrValue::Vec2([0.0, 50.0]))
            .with_property("enabled", IrValue::Bool(true)),

        NodeClassSpec::new("PointLight2D", Some("Node2D"), NodeCategory::Node2D, "Omnidirectional 2D light source.")
            .with_property("energy", IrValue::F64(1.0))
            .with_property("color", IrValue::Color([1.0, 1.0, 1.0, 1.0])),

        NodeClassSpec::new("CPUParticles2D", Some("Node2D"), NodeCategory::Node2D, "CPU-driven 2D particle emitter.")
            .with_property("emitting", IrValue::Bool(true))
            .with_property("amount", IrValue::I64(8))
            .with_property("lifetime", IrValue::F64(1.0)),

        NodeClassSpec::new("Line2D", Some("Node2D"), NodeCategory::Node2D, "Draws 2D polyline strips.")
            .with_property("width", IrValue::F64(10.0))
            .with_property("default_color", IrValue::Color([1.0, 1.0, 1.0, 1.0])),

        NodeClassSpec::new("Polygon2D", Some("Node2D"), NodeCategory::Node2D, "2D textured or colored polygon.")
            .with_property("color", IrValue::Color([1.0, 1.0, 1.0, 1.0])),

        NodeClassSpec::new("NavigationRegion2D", Some("Node2D"), NodeCategory::Node2D, "Defines a 2D navigation mesh region.")
            .with_property("enabled", IrValue::Bool(true)),

        NodeClassSpec::new("NavigationAgent2D", Some("Node"), NodeCategory::Node2D, "Calculates paths and avoidance in 2D navigation.")
            .with_signal(SignalSpec::new("navigation_finished", "Emitted when target reached."))
            .with_signal(SignalSpec::new("velocity_computed", "Emitted when avoidance velocity is computed.")),

        // -------------------------------------------------------------
        // 3. 3D NODES (19 classes)
        // -------------------------------------------------------------
        NodeClassSpec::new("Node3D", Some("Node"), NodeCategory::Node3D, "3D spatial node with transform matrix.")
            .with_property("position", IrValue::Vec3([0.0, 0.0, 0.0]))
            .with_property("rotation", IrValue::Quat([0.0, 0.0, 0.0, 1.0]))
            .with_property("scale", IrValue::Vec3([1.0, 1.0, 1.0])),

        NodeClassSpec::new("MeshInstance3D", Some("Node3D"), NodeCategory::Node3D, "Renders a 3D geometry mesh.")
            .with_property("cast_shadow", IrValue::I64(1)),

        NodeClassSpec::new("CSGBox3D", Some("Node3D"), NodeCategory::Node3D, "Constructive Solid Geometry box primitive.")
            .with_property("size", IrValue::Vec3([2.0, 2.0, 2.0])),

        NodeClassSpec::new("Camera3D", Some("Node3D"), NodeCategory::Node3D, "View camera in 3D space.")
            .with_property("fov", IrValue::F64(75.0))
            .with_property("near", IrValue::F64(0.05))
            .with_property("far", IrValue::F64(4000.0))
            .with_property("current", IrValue::Bool(true)),

        NodeClassSpec::new("DirectionalLight3D", Some("Node3D"), NodeCategory::Node3D, "Sun/directional parallel 3D light.")
            .with_property("light_energy", IrValue::F64(1.0))
            .with_property("light_color", IrValue::Color([1.0, 1.0, 1.0, 1.0])),

        NodeClassSpec::new("OmniLight3D", Some("Node3D"), NodeCategory::Node3D, "Omnidirectional 3D point light.")
            .with_property("omni_range", IrValue::F64(5.0))
            .with_property("light_energy", IrValue::F64(1.0)),

        NodeClassSpec::new("SpotLight3D", Some("Node3D"), NodeCategory::Node3D, "Cone-shaped 3D spot light.")
            .with_property("spot_range", IrValue::F64(5.0))
            .with_property("spot_angle", IrValue::F64(45.0)),

        NodeClassSpec::new("StaticBody3D", Some("Node3D"), NodeCategory::Node3D, "Immovable 3D physical surface.")
            .with_property("constant_linear_velocity", IrValue::Vec3([0.0, 0.0, 0.0])),

        NodeClassSpec::new("RigidBody3D", Some("Node3D"), NodeCategory::Node3D, "Physics-simulated 3D body.")
            .with_signal(SignalSpec::new("body_entered", "Emitted upon contact."))
            .with_signal(SignalSpec::new("body_exited", "Emitted when contact ceases."))
            .with_property("mass", IrValue::F64(1.0))
            .with_property("gravity_scale", IrValue::F64(1.0)),

        NodeClassSpec::new("CharacterBody3D", Some("Node3D"), NodeCategory::Node3D, "User-controlled kinematic 3D actor.")
            .with_property("floor_stop_on_slope", IrValue::Bool(true))
            .with_property("max_slides", IrValue::I64(4)),

        NodeClassSpec::new("Area3D", Some("Node3D"), NodeCategory::Node3D, "3D volume detector for physics bodies and areas.")
            .with_signal(SignalSpec::new("body_entered", "Emitted when a body enters the 3D volume."))
            .with_signal(SignalSpec::new("body_exited", "Emitted when a body exits."))
            .with_signal(SignalSpec::new("area_entered", "Emitted when another area enters."))
            .with_signal(SignalSpec::new("area_exited", "Emitted when another area exits.")),

        NodeClassSpec::new("CollisionShape3D", Some("Node3D"), NodeCategory::Node3D, "Provides 3D collision geometry.")
            .with_property("disabled", IrValue::Bool(false)),

        NodeClassSpec::new("RayCast3D", Some("Node3D"), NodeCategory::Node3D, "3D physics ray inquiry.")
            .with_property("target_position", IrValue::Vec3([0.0, -1.0, 0.0]))
            .with_property("enabled", IrValue::Bool(true)),

        NodeClassSpec::new("GridMap", Some("Node3D"), NodeCategory::Node3D, "3D tile/block grid placement map.")
            .with_property("cell_size", IrValue::Vec3([2.0, 2.0, 2.0])),

        NodeClassSpec::new("NavigationRegion3D", Some("Node3D"), NodeCategory::Node3D, "Defines a 3D navigation mesh volume.")
            .with_property("enabled", IrValue::Bool(true)),

        NodeClassSpec::new("NavigationAgent3D", Some("Node"), NodeCategory::Node3D, "Pathfinding and obstacle avoidance agent in 3D.")
            .with_signal(SignalSpec::new("navigation_finished", "Emitted when target reached."))
            .with_signal(SignalSpec::new("velocity_computed", "Avoidance velocity computed.")),

        NodeClassSpec::new("Skeleton3D", Some("Node3D"), NodeCategory::Node3D, "Hierarchical bone system for skinning meshes.")
            .with_property("motion_scale", IrValue::F64(1.0)),

        NodeClassSpec::new("BoneAttachment3D", Some("Node3D"), NodeCategory::Node3D, "Attaches spatial nodes to a specific Skeleton3D bone.")
            .with_property("bone_name", IrValue::String("".into()))
            .with_property("bone_idx", IrValue::I64(-1)),

        NodeClassSpec::new("GPUParticles3D", Some("Node3D"), NodeCategory::Node3D, "GPU-accelerated 3D particle system.")
            .with_property("emitting", IrValue::Bool(true))
            .with_property("amount", IrValue::I64(64))
            .with_property("lifetime", IrValue::F64(1.0)),

        // -------------------------------------------------------------
        // 4. UI / CONTROL NODES (17 classes)
        // -------------------------------------------------------------
        NodeClassSpec::new("Control", Some("Node"), NodeCategory::ControlUi, "Base class for all UI elements.")
            .with_signal(SignalSpec::new("resized", "Emitted when control dimensions change."))
            .with_property("size", IrValue::Vec2([0.0, 0.0]))
            .with_property("position", IrValue::Vec2([0.0, 0.0])),

        NodeClassSpec::new("Label", Some("Control"), NodeCategory::ControlUi, "Displays plain text.")
            .with_property("text", IrValue::String("".into())),

        NodeClassSpec::new("Button", Some("Control"), NodeCategory::ControlUi, "Clickable button widget.")
            .with_signal(SignalSpec::new("pressed", "Emitted when button is clicked."))
            .with_signal(SignalSpec::new("button_down", "Emitted on pointer down."))
            .with_signal(SignalSpec::new("button_up", "Emitted on pointer release."))
            .with_signal(SignalSpec::new("toggled", "Emitted when toggle state changes."))
            .with_property("text", IrValue::String("Button".into()))
            .with_property("disabled", IrValue::Bool(false)),

        NodeClassSpec::new("CheckBox", Some("Button"), NodeCategory::ControlUi, "Checkbox button with checkmark icon.")
            .with_signal(SignalSpec::new("toggled", "Emitted when checked state flips."))
            .with_property("button_pressed", IrValue::Bool(false)),

        NodeClassSpec::new("HSlider", Some("Control"), NodeCategory::ControlUi, "Horizontal numeric range slider.")
            .with_signal(SignalSpec::new("value_changed", "Emitted when slider value changes."))
            .with_property("min_value", IrValue::F64(0.0))
            .with_property("max_value", IrValue::F64(100.0))
            .with_property("value", IrValue::F64(0.0))
            .with_property("step", IrValue::F64(1.0)),

        NodeClassSpec::new("LineEdit", Some("Control"), NodeCategory::ControlUi, "Single-line text input field.")
            .with_signal(SignalSpec::new("text_changed", "Emitted on every keystroke."))
            .with_signal(SignalSpec::new("text_submitted", "Emitted when enter is pressed."))
            .with_property("text", IrValue::String("".into()))
            .with_property("placeholder_text", IrValue::String("".into())),

        NodeClassSpec::new("ProgressBar", Some("Control"), NodeCategory::ControlUi, "Visual progress indicator bar.")
            .with_property("min_value", IrValue::F64(0.0))
            .with_property("max_value", IrValue::F64(100.0))
            .with_property("value", IrValue::F64(0.0))
            .with_property("show_percentage", IrValue::Bool(true)),

        NodeClassSpec::new("TextureRect", Some("Control"), NodeCategory::ControlUi, "Displays an image texture inside a UI control.")
            .with_property("stretch_mode", IrValue::I64(0)),

        NodeClassSpec::new("ColorRect", Some("Control"), NodeCategory::ControlUi, "Draws a solid filled color rectangle.")
            .with_property("color", IrValue::Color([1.0, 1.0, 1.0, 1.0])),

        NodeClassSpec::new("PanelContainer", Some("Control"), NodeCategory::ControlUi, "Container wrapping contents in a panel background."),
        NodeClassSpec::new("HBoxContainer", Some("Control"), NodeCategory::ControlUi, "Arranges child controls horizontally.")
            .with_property("alignment", IrValue::I64(0)),

        NodeClassSpec::new("VBoxContainer", Some("Control"), NodeCategory::ControlUi, "Arranges child controls vertically.")
            .with_property("alignment", IrValue::I64(0)),

        NodeClassSpec::new("GridContainer", Some("Control"), NodeCategory::ControlUi, "Arranges children in a grid of rows and columns.")
            .with_property("columns", IrValue::I64(2)),

        NodeClassSpec::new("MarginContainer", Some("Control"), NodeCategory::ControlUi, "Adds padding around its child control."),
        NodeClassSpec::new("CenterContainer", Some("Control"), NodeCategory::ControlUi, "Keeps children centered inside its bounds."),
        NodeClassSpec::new("ScrollContainer", Some("Control"), NodeCategory::ControlUi, "Allows scrolling over overflowing children."),
        NodeClassSpec::new("TabContainer", Some("Control"), NodeCategory::ControlUi, "Arranges children into selectable tabs.")
            .with_signal(SignalSpec::new("tab_changed", "Emitted when active tab changes."))
            .with_property("current_tab", IrValue::I64(0)),

        // -------------------------------------------------------------
        // 5. ANIMATION & AUDIO NODES (5 classes)
        // -------------------------------------------------------------
        NodeClassSpec::new("AnimationPlayer", Some("Node"), NodeCategory::Animation, "Plays back keyframe animation clips.")
            .with_signal(SignalSpec::new("animation_finished", "Emitted when an animation completes."))
            .with_signal(SignalSpec::new("animation_started", "Emitted when an animation begins."))
            .with_property("speed_scale", IrValue::F64(1.0)),

        NodeClassSpec::new("AnimationTree", Some("Node"), NodeCategory::Animation, "Controls complex animation blending trees and state machines.")
            .with_property("active", IrValue::Bool(true)),

        NodeClassSpec::new("AudioStreamPlayer", Some("Node"), NodeCategory::Audio, "Non-positional audio playback.")
            .with_signal(SignalSpec::new("finished", "Emitted when sound finishes playing."))
            .with_property("volume_db", IrValue::F64(0.0))
            .with_property("autoplay", IrValue::Bool(false)),

        NodeClassSpec::new("AudioStreamPlayer2D", Some("Node2D"), NodeCategory::Audio, "2D positional audio with distance attenuation.")
            .with_signal(SignalSpec::new("finished", "Emitted when sound finishes playing."))
            .with_property("volume_db", IrValue::F64(0.0))
            .with_property("max_distance", IrValue::F64(2000.0)),

        NodeClassSpec::new("AudioStreamPlayer3D", Some("Node3D"), NodeCategory::Audio, "3D positional spatial audio with Doppler and attenuation.")
            .with_signal(SignalSpec::new("finished", "Emitted when sound finishes playing."))
            .with_property("volume_db", IrValue::F64(0.0))
            .with_property("max_distance", IrValue::F64(100.0))
            .with_property("unit_size", IrValue::F64(10.0)),
    ]
}
