//! Canonical Godot 4.4 ClassDB Type Registry (470+ genuine Godot classes).
//!
//! Provides exact single-inheritance hierarchy, default property instantiation,
//! subclass query inspection, and dynamic GDExtension class registration.

use foreplay_ir::IrValue;
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// Error during ClassDb lookups or registration.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClassDbError {
    #[error("class not found: {0}")]
    ClassNotFound(String),
    #[error("ancestor class not found: {0}")]
    AncestorNotFound(String),
    #[error("class already registered: {0}")]
    ClassAlreadyExists(String),
    #[error("cannot instantiate abstract class: {0}")]
    ClassNotInstantiable(String),
}

/// Metadata describing a Godot 4.4 class or dynamic GDExtension class.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassInfo {
    pub name: String,
    pub parent: Option<String>,
    pub is_instantiable: bool,
    pub default_properties: BTreeMap<String, IrValue>,
    pub signals: Vec<String>,
}

impl ClassInfo {
    pub fn new(name: impl Into<String>, parent: Option<&str>, is_instantiable: bool) -> Self {
        Self {
            name: name.into(),
            parent: parent.map(|s| s.to_string()),
            is_instantiable,
            default_properties: BTreeMap::new(),
            signals: Vec::new(),
        }
    }

    pub fn with_property(mut self, key: impl Into<String>, val: IrValue) -> Self {
        self.default_properties.insert(key.into(), val);
        self
    }

    pub fn with_signal(mut self, sig: impl Into<String>) -> Self {
        self.signals.push(sig.into());
        self
    }
}

/// Complete registry of Godot 4.4 built-in and GDExtension classes.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ClassDb {
    classes: BTreeMap<String, ClassInfo>,
}

impl ClassDb {
    /// Returns the global canonical Godot 4.4 ClassDB instance.
    pub fn global_godot44() -> &'static Self {
        static GLOBAL: OnceLock<ClassDb> = OnceLock::new();
        GLOBAL.get_or_init(Self::build_godot44)
    }

    /// Creates a new empty ClassDB registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of registered classes.
    pub fn class_count(&self) -> usize {
        self.classes.len()
    }

    /// Looks up a class by name.
    pub fn get(&self, name: &str) -> Option<&ClassInfo> {
        self.classes.get(name)
    }

    /// Checks if `class` inherits `ancestor` (including self).
    pub fn inherits(&self, class: &str, ancestor: &str) -> bool {
        if class == ancestor {
            return true;
        }
        let mut curr = class;
        while let Some(info) = self.get(curr) {
            if let Some(ref parent) = info.parent {
                if parent == ancestor {
                    return true;
                }
                curr = parent.as_str();
            } else {
                break;
            }
        }
        false
    }

    /// Returns all direct and indirect subclasses of `ancestor` (excluding `ancestor` itself).
    pub fn subclasses_of(&self, ancestor: &str) -> Vec<String> {
        self.classes
            .iter()
            .filter(|(name, _)| *name != ancestor && self.inherits(name, ancestor))
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// Instantiates default property dictionary for `class`, resolving inheritance chain from root.
    pub fn instantiate_default_properties(&self, class: &str) -> Result<BTreeMap<String, IrValue>, ClassDbError> {
        let Some(target) = self.get(class) else {
            return Err(ClassDbError::ClassNotFound(class.to_string()));
        };
        if !target.is_instantiable {
            return Err(ClassDbError::ClassNotInstantiable(class.to_string()));
        }
        let mut chain = Vec::new();
        let mut curr = class;
        while let Some(info) = self.get(curr) {
            chain.push(info);
            if let Some(ref parent) = info.parent {
                curr = parent.as_str();
            } else {
                break;
            }
        }
        let mut props = BTreeMap::new();
        for info in chain.into_iter().rev() {
            for (k, v) in &info.default_properties {
                props.insert(k.clone(), v.clone());
            }
        }
        Ok(props)
    }

    /// Registers a dynamic GDExtension class at runtime.
    pub fn register_extension_class(&mut self, class_info: ClassInfo) -> Result<(), ClassDbError> {
        if self.classes.contains_key(&class_info.name) {
            return Err(ClassDbError::ClassAlreadyExists(class_info.name));
        }
        if let Some(ref parent) = class_info.parent
            && !self.classes.contains_key(parent)
        {
            return Err(ClassDbError::AncestorNotFound(parent.clone()));
        }
        self.classes.insert(class_info.name.clone(), class_info);
        Ok(())
    }

    fn register_internal(&mut self, name: &'static str, parent: Option<&'static str>, instantiable: bool, props: &[(&'static str, IrValue)], sigs: &[&'static str]) {
        let mut info = ClassInfo::new(name, parent, instantiable);
        for (k, v) in props {
            info = info.with_property(*k, v.clone());
        }
        for s in sigs {
            info = info.with_signal(*s);
        }
        self.classes.insert(name.to_string(), info);
    }

    fn build_godot44() -> Self {
        let mut db = Self::new();
        db.register_internal("Object", None, false, &[], &[]);
        db.register_internal("RefCounted", Some("Object"), false, &[], &[]);
        db.register_internal("Time", Some("Object"), false, &[], &[]);
        db.register_internal("Engine", Some("Object"), false, &[], &[]);
        db.register_internal("OS", Some("Object"), false, &[], &[]);
        db.register_internal("DisplayServer", Some("Object"), false, &[], &[]);
        db.register_internal("RenderingServer", Some("Object"), false, &[], &[]);
        db.register_internal("AudioServer", Some("Object"), false, &[], &[]);
        db.register_internal("PhysicsServer2D", Some("Object"), false, &[], &[]);
        db.register_internal("PhysicsServer3D", Some("Object"), false, &[], &[]);
        db.register_internal("NavigationServer2D", Some("Object"), false, &[], &[]);
        db.register_internal("NavigationServer3D", Some("Object"), false, &[], &[]);
        db.register_internal("Input", Some("Object"), false, &[], &[]);
        db.register_internal("InputMap", Some("Object"), false, &[], &[]);
        db.register_internal("IP", Some("Object"), false, &[], &[]);
        db.register_internal("Geometry2D", Some("Object"), false, &[], &[]);
        db.register_internal("Geometry3D", Some("Object"), false, &[], &[]);
        db.register_internal("Marshalls", Some("Object"), false, &[], &[]);
        db.register_internal("TranslationServer", Some("Object"), false, &[], &[]);
        db.register_internal("Performance", Some("Object"), false, &[], &[]);
        db.register_internal("ConfigFile", Some("RefCounted"), true, &[], &[]);
        db.register_internal("Crypto", Some("RefCounted"), true, &[], &[]);
        db.register_internal("DirAccess", Some("RefCounted"), true, &[], &[]);
        db.register_internal("FileAccess", Some("RefCounted"), true, &[], &[]);
        db.register_internal("HashingContext", Some("RefCounted"), true, &[], &[]);
        db.register_internal("HTTPClient", Some("RefCounted"), true, &[], &[]);
        db.register_internal("KinematicCollision2D", Some("RefCounted"), true, &[], &[]);
        db.register_internal("KinematicCollision3D", Some("RefCounted"), true, &[], &[]);
        db.register_internal("Mutex", Some("RefCounted"), true, &[], &[]);
        db.register_internal("PackedDataContainerRef", Some("RefCounted"), true, &[], &[]);
        db.register_internal("PacketPeer", Some("RefCounted"), false, &[], &[]);
        db.register_internal("PacketPeerStream", Some("PacketPeer"), true, &[], &[]);
        db.register_internal("PacketPeerUDP", Some("PacketPeer"), true, &[], &[]);
        db.register_internal("PacketPeerDTLS", Some("PacketPeer"), true, &[], &[]);
        db.register_internal("WebsocketPeer", Some("PacketPeer"), true, &[], &[]);
        db.register_internal("StreamPeer", Some("RefCounted"), false, &[], &[]);
        db.register_internal("StreamPeerBuffer", Some("StreamPeer"), true, &[], &[]);
        db.register_internal("StreamPeerTCP", Some("StreamPeer"), true, &[], &[]);
        db.register_internal("StreamPeerTLS", Some("StreamPeer"), true, &[], &[]);
        db.register_internal("RandomNumberGenerator", Some("RefCounted"), true, &[], &[]);
        db.register_internal("Semaphore", Some("RefCounted"), true, &[], &[]);
        db.register_internal("SurfaceTool", Some("RefCounted"), true, &[], &[]);
        db.register_internal("MeshConvexDecompositionSettings", Some("RefCounted"), true, &[], &[]);
        db.register_internal("MeshDataTool", Some("RefCounted"), true, &[], &[]);
        db.register_internal("Thread", Some("RefCounted"), true, &[], &[]);
        db.register_internal("TriangleMesh", Some("RefCounted"), true, &[], &[]);
        db.register_internal("WeakRef", Some("RefCounted"), true, &[], &[]);
        db.register_internal("XMLParser", Some("RefCounted"), true, &[], &[]);
        db.register_internal("ZIPPacker", Some("RefCounted"), true, &[], &[]);
        db.register_internal("ZIPReader", Some("RefCounted"), true, &[], &[]);
        db.register_internal("Expression", Some("RefCounted"), true, &[], &[]);
        db.register_internal("RegEx", Some("RefCounted"), true, &[], &[]);
        db.register_internal("RegExMatch", Some("RefCounted"), true, &[], &[]);
        db.register_internal("AStar2D", Some("RefCounted"), true, &[], &[]);
        db.register_internal("AStar3D", Some("RefCounted"), true, &[], &[]);
        db.register_internal("AStarGrid2D", Some("RefCounted"), true, &[], &[]);
        db.register_internal("Tween", Some("RefCounted"), true, &[], &["finished", "loop_finished", "step_finished"]);
        db.register_internal("Tweener", Some("RefCounted"), false, &[], &["finished"]);
        db.register_internal("PropertyTweener", Some("Tweener"), true, &[], &[]);
        db.register_internal("IntervalTweener", Some("Tweener"), true, &[], &[]);
        db.register_internal("CallbackTweener", Some("Tweener"), true, &[], &[]);
        db.register_internal("MethodTweener", Some("Tweener"), true, &[], &[]);
        db.register_internal("AudioStreamPlayback", Some("RefCounted"), false, &[], &[]);
        db.register_internal("Image", Some("RefCounted"), true, &[], &[]);
        db.register_internal("Resource", Some("RefCounted"), true, &[("resource_local_to_scene", IrValue::Bool(false))], &["changed"]);
        db.register_internal("PackedScene", Some("Resource"), true, &[], &[]);
        db.register_internal("World2D", Some("Resource"), true, &[], &[]);
        db.register_internal("World3D", Some("Resource"), true, &[], &[]);
        db.register_internal("VoxelGIData", Some("Resource"), true, &[], &[]);
        db.register_internal("LightmapGIData", Some("Resource"), true, &[], &[]);
        db.register_internal("BitMap", Some("Resource"), true, &[], &[]);
        db.register_internal("ShaderInclude", Some("Resource"), true, &[], &[]);
        db.register_internal("RichTextEffect", Some("Resource"), true, &[], &[]);
        db.register_internal("ButtonGroup", Some("Resource"), true, &[], &["pressed"]);
        db.register_internal("Shortcut", Some("Resource"), true, &[], &[]);
        db.register_internal("SyntaxHighlighter", Some("Resource"), true, &[], &[]);
        db.register_internal("CodeHighlighter", Some("SyntaxHighlighter"), true, &[], &[]);
        db.register_internal("OggPacketSequence", Some("Resource"), true, &[], &[]);
        db.register_internal("JSON", Some("Resource"), true, &[], &[]);
        db.register_internal("PackedDataContainer", Some("Resource"), true, &[], &[]);
        db.register_internal("Translation", Some("Resource"), true, &[], &[]);
        db.register_internal("OptimizedTranslation", Some("Translation"), true, &[], &[]);
        db.register_internal("CryptoKey", Some("Resource"), true, &[], &[]);
        db.register_internal("X509Certificate", Some("Resource"), true, &[], &[]);
        db.register_internal("PhysicsMaterial", Some("Resource"), true, &[("friction", IrValue::F64(1.0)), ("bounce", IrValue::F64(0.0))], &[]);
        db.register_internal("SkeletonProfile", Some("Resource"), true, &[("root_bone", IrValue::String("Root".into()))], &[]);
        db.register_internal("SkeletonProfileHumanoid", Some("SkeletonProfile"), true, &[], &[]);
        db.register_internal("BoneMap", Some("Resource"), true, &[], &[]);
        db.register_internal("Material", Some("Resource"), false, &[("render_priority", IrValue::I64(0))], &[]);
        db.register_internal("BaseMaterial3D", Some("Material"), false, &[("roughness", IrValue::F64(1.0)), ("metallic", IrValue::F64(0.0))], &[]);
        db.register_internal("StandardMaterial3D", Some("BaseMaterial3D"), true, &[("albedo_color", IrValue::Color([1.0, 1.0, 1.0, 1.0]))], &[]);
        db.register_internal("ORMMaterial3D", Some("BaseMaterial3D"), true, &[], &[]);
        db.register_internal("ShaderMaterial", Some("Material"), true, &[], &[]);
        db.register_internal("CanvasItemMaterial", Some("Material"), true, &[("blend_mode", IrValue::I64(0))], &[]);
        db.register_internal("ParticleProcessMaterial", Some("Material"), true, &[("gravity", IrValue::Vec3([0.0, -9.8, 0.0]))], &[]);
        db.register_internal("PanoramaSkyMaterial", Some("Material"), true, &[], &[]);
        db.register_internal("ProceduralSkyMaterial", Some("Material"), true, &[("sky_energy_multiplier", IrValue::F64(1.0))], &[]);
        db.register_internal("PhysicalSkyMaterial", Some("Material"), true, &[], &[]);
        db.register_internal("FogMaterial", Some("Material"), true, &[("density", IrValue::F64(0.001))], &[]);
        db.register_internal("Sky", Some("Resource"), true, &[], &[]);
        db.register_internal("Environment", Some("Resource"), true, &[("background_energy_multiplier", IrValue::F64(1.0))], &[]);
        db.register_internal("CameraAttributes", Some("Resource"), false, &[], &[]);
        db.register_internal("CameraAttributesPractical", Some("CameraAttributes"), true, &[], &[]);
        db.register_internal("CameraAttributesPhysical", Some("CameraAttributes"), true, &[], &[]);
        db.register_internal("Shader", Some("Resource"), true, &[("code", IrValue::String("".into()))], &[]);
        db.register_internal("VisualShader", Some("Shader"), true, &[], &[]);
        db.register_internal("Mesh", Some("Resource"), false, &[], &[]);
        db.register_internal("ArrayMesh", Some("Mesh"), true, &[], &[]);
        db.register_internal("PrimitiveMesh", Some("Mesh"), false, &[], &[]);
        db.register_internal("BoxMesh", Some("PrimitiveMesh"), true, &[("size", IrValue::Vec3([1.0, 1.0, 1.0]))], &[]);
        db.register_internal("SphereMesh", Some("PrimitiveMesh"), true, &[("radius", IrValue::F64(0.5)), ("height", IrValue::F64(1.0))], &[]);
        db.register_internal("CapsuleMesh", Some("PrimitiveMesh"), true, &[("radius", IrValue::F64(0.5)), ("height", IrValue::F64(2.0))], &[]);
        db.register_internal("CylinderMesh", Some("PrimitiveMesh"), true, &[("top_radius", IrValue::F64(0.5)), ("bottom_radius", IrValue::F64(0.5)), ("height", IrValue::F64(2.0))], &[]);
        db.register_internal("PlaneMesh", Some("PrimitiveMesh"), true, &[("size", IrValue::Vec2([2.0, 2.0]))], &[]);
        db.register_internal("QuadMesh", Some("PrimitiveMesh"), true, &[("size", IrValue::Vec2([1.0, 1.0]))], &[]);
        db.register_internal("PrismMesh", Some("PrimitiveMesh"), true, &[("size", IrValue::Vec3([1.0, 1.0, 1.0]))], &[]);
        db.register_internal("TorusMesh", Some("PrimitiveMesh"), true, &[("inner_radius", IrValue::F64(0.5)), ("outer_radius", IrValue::F64(1.0))], &[]);
        db.register_internal("RibbonTrailMesh", Some("PrimitiveMesh"), true, &[], &[]);
        db.register_internal("TubeTrailMesh", Some("PrimitiveMesh"), true, &[], &[]);
        db.register_internal("TextMesh", Some("PrimitiveMesh"), true, &[("text", IrValue::String("".into()))], &[]);
        db.register_internal("PointMesh", Some("PrimitiveMesh"), true, &[], &[]);
        db.register_internal("MultiMesh", Some("Resource"), true, &[("instance_count", IrValue::I64(0))], &[]);
        db.register_internal("Skin", Some("Resource"), true, &[], &[]);
        db.register_internal("Texture", Some("Resource"), false, &[], &[]);
        db.register_internal("Texture2D", Some("Texture"), false, &[], &[]);
        db.register_internal("ImageTexture", Some("Texture2D"), true, &[], &[]);
        db.register_internal("AtlasTexture", Some("Texture2D"), true, &[], &[]);
        db.register_internal("CompressedTexture2D", Some("Texture2D"), true, &[], &[]);
        db.register_internal("GradientTexture1D", Some("Texture2D"), true, &[("width", IrValue::I64(256))], &[]);
        db.register_internal("GradientTexture2D", Some("Texture2D"), true, &[("width", IrValue::I64(64)), ("height", IrValue::I64(64))], &[]);
        db.register_internal("NoiseTexture2D", Some("Texture2D"), true, &[("width", IrValue::I64(512)), ("height", IrValue::I64(512))], &[]);
        db.register_internal("ViewportTexture", Some("Texture2D"), true, &[], &[]);
        db.register_internal("CurveTexture", Some("Texture2D"), true, &[("width", IrValue::I64(256))], &[]);
        db.register_internal("CurveXYZTexture", Some("Texture2D"), true, &[("width", IrValue::I64(256))], &[]);
        db.register_internal("AnimatedTexture", Some("Texture2D"), true, &[("frames", IrValue::I64(1))], &[]);
        db.register_internal("CanvasTexture", Some("Texture2D"), true, &[], &[]);
        db.register_internal("CameraTexture", Some("Texture2D"), true, &[], &[]);
        db.register_internal("Texture3D", Some("Texture"), false, &[], &[]);
        db.register_internal("ImageTexture3D", Some("Texture3D"), true, &[], &[]);
        db.register_internal("CompressedTexture3D", Some("Texture3D"), true, &[], &[]);
        db.register_internal("NoiseTexture3D", Some("Texture3D"), true, &[("width", IrValue::I64(64)), ("height", IrValue::I64(64)), ("depth", IrValue::I64(64))], &[]);
        db.register_internal("TextureLayered", Some("Texture"), false, &[], &[]);
        db.register_internal("Texture2DArray", Some("TextureLayered"), true, &[], &[]);
        db.register_internal("Cubemap", Some("TextureLayered"), true, &[], &[]);
        db.register_internal("CubemapArray", Some("TextureLayered"), true, &[], &[]);
        db.register_internal("Shape3D", Some("Resource"), false, &[("margin", IrValue::F64(0.04))], &[]);
        db.register_internal("BoxShape3D", Some("Shape3D"), true, &[("size", IrValue::Vec3([1.0, 1.0, 1.0]))], &[]);
        db.register_internal("SphereShape3D", Some("Shape3D"), true, &[("radius", IrValue::F64(0.5))], &[]);
        db.register_internal("CapsuleShape3D", Some("Shape3D"), true, &[("radius", IrValue::F64(0.5)), ("height", IrValue::F64(2.0))], &[]);
        db.register_internal("CylinderShape3D", Some("Shape3D"), true, &[("radius", IrValue::F64(0.5)), ("height", IrValue::F64(2.0))], &[]);
        db.register_internal("ConcavePolygonShape3D", Some("Shape3D"), true, &[], &[]);
        db.register_internal("ConvexPolygonShape3D", Some("Shape3D"), true, &[], &[]);
        db.register_internal("HeightMapShape3D", Some("Shape3D"), true, &[], &[]);
        db.register_internal("WorldBoundaryShape3D", Some("Shape3D"), true, &[("distance", IrValue::F64(0.0))], &[]);
        db.register_internal("SeparationRayShape3D", Some("Shape3D"), true, &[("length", IrValue::F64(1.0))], &[]);
        db.register_internal("Shape2D", Some("Resource"), false, &[], &[]);
        db.register_internal("RectangleShape2D", Some("Shape2D"), true, &[("size", IrValue::Vec2([20.0, 20.0]))], &[]);
        db.register_internal("CircleShape2D", Some("Shape2D"), true, &[("radius", IrValue::F64(10.0))], &[]);
        db.register_internal("CapsuleShape2D", Some("Shape2D"), true, &[("radius", IrValue::F64(10.0)), ("height", IrValue::F64(30.0))], &[]);
        db.register_internal("SegmentShape2D", Some("Shape2D"), true, &[], &[]);
        db.register_internal("ConvexPolygonShape2D", Some("Shape2D"), true, &[], &[]);
        db.register_internal("ConcavePolygonShape2D", Some("Shape2D"), true, &[], &[]);
        db.register_internal("WorldBoundaryShape2D", Some("Shape2D"), true, &[("distance", IrValue::F64(0.0))], &[]);
        db.register_internal("SeparationRayShape2D", Some("Shape2D"), true, &[("length", IrValue::F64(20.0))], &[]);
        db.register_internal("AudioStream", Some("Resource"), false, &[], &[]);
        db.register_internal("AudioStreamWAV", Some("AudioStream"), true, &[("loop_mode", IrValue::I64(0))], &[]);
        db.register_internal("AudioStreamOggVorbis", Some("AudioStream"), true, &[("loop", IrValue::Bool(false))], &[]);
        db.register_internal("AudioStreamMP3", Some("AudioStream"), true, &[("loop", IrValue::Bool(false))], &[]);
        db.register_internal("AudioStreamPolyphonic", Some("AudioStream"), true, &[("polyphony", IrValue::I64(64))], &[]);
        db.register_internal("AudioStreamMicrophone", Some("AudioStream"), true, &[], &[]);
        db.register_internal("AudioStreamGenerator", Some("AudioStream"), true, &[("mix_rate", IrValue::F64(44100.0))], &[]);
        db.register_internal("AudioStreamSynchronized", Some("AudioStream"), true, &[], &[]);
        db.register_internal("AudioStreamInteractive", Some("AudioStream"), true, &[], &[]);
        db.register_internal("AudioStreamPlaylist", Some("AudioStream"), true, &[("loop", IrValue::Bool(true))], &[]);
        db.register_internal("AudioBusLayout", Some("Resource"), true, &[], &[]);
        db.register_internal("AudioEffect", Some("Resource"), false, &[], &[]);
        db.register_internal("AudioEffectReverb", Some("AudioEffect"), true, &[("room_size", IrValue::F64(0.8)), ("damping", IrValue::F64(0.5))], &[]);
        db.register_internal("AudioEffectEQ", Some("AudioEffect"), true, &[], &[]);
        db.register_internal("AudioEffectCompressor", Some("AudioEffect"), true, &[("threshold", IrValue::F64(0.0)), ("ratio", IrValue::F64(4.0))], &[]);
        db.register_internal("AudioEffectDelay", Some("AudioEffect"), true, &[("dry", IrValue::F64(1.0)), ("feedback_delay_ms", IrValue::F64(340.0))], &[]);
        db.register_internal("AudioEffectLimiter", Some("AudioEffect"), true, &[("ceiling_db", IrValue::F64(-0.1))], &[]);
        db.register_internal("AudioEffectDistortion", Some("AudioEffect"), true, &[("drive", IrValue::F64(0.0))], &[]);
        db.register_internal("AudioEffectFilter", Some("AudioEffect"), true, &[("cutoff_hz", IrValue::F64(2000.0))], &[]);
        db.register_internal("AudioEffectPitchShift", Some("AudioEffect"), true, &[("pitch_scale", IrValue::F64(1.0))], &[]);
        db.register_internal("AudioEffectPhaser", Some("AudioEffect"), true, &[("range_min_hz", IrValue::F64(440.0))], &[]);
        db.register_internal("AudioEffectChorus", Some("AudioEffect"), true, &[("voice_count", IrValue::I64(2))], &[]);
        db.register_internal("AudioEffectAmplify", Some("AudioEffect"), true, &[("volume_db", IrValue::F64(0.0))], &[]);
        db.register_internal("AudioEffectCapture", Some("AudioEffect"), true, &[], &[]);
        db.register_internal("AudioEffectStereoEnhance", Some("AudioEffect"), true, &[("pan_pullout", IrValue::F64(1.0))], &[]);
        db.register_internal("Animation", Some("Resource"), true, &[("length", IrValue::F64(1.0)), ("step", IrValue::F64(0.0166667))], &[]);
        db.register_internal("AnimationLibrary", Some("Resource"), true, &[], &[]);
        db.register_internal("AnimationNode", Some("Resource"), false, &[("filter_enabled", IrValue::Bool(false))], &["tree_changed"]);
        db.register_internal("AnimationRootNode", Some("AnimationNode"), false, &[], &[]);
        db.register_internal("AnimationNodeAnimation", Some("AnimationRootNode"), true, &[("animation", IrValue::String("".into()))], &[]);
        db.register_internal("AnimationNodeStateMachine", Some("AnimationRootNode"), true, &[], &[]);
        db.register_internal("AnimationNodeBlendSpace1D", Some("AnimationRootNode"), true, &[("min_space", IrValue::F64(-1.0)), ("max_space", IrValue::F64(1.0))], &[]);
        db.register_internal("AnimationNodeBlendSpace2D", Some("AnimationRootNode"), true, &[], &[]);
        db.register_internal("AnimationNodeBlendTree", Some("AnimationRootNode"), true, &[], &[]);
        db.register_internal("AnimationNodeBlend2", Some("AnimationRootNode"), true, &[("filter_enabled", IrValue::Bool(false))], &[]);
        db.register_internal("AnimationNodeBlend3", Some("AnimationRootNode"), true, &[], &[]);
        db.register_internal("AnimationNodeOneShot", Some("AnimationRootNode"), true, &[("fadein_time", IrValue::F64(0.1)), ("fadeout_time", IrValue::F64(0.1))], &[]);
        db.register_internal("AnimationNodeTimeSeek", Some("AnimationRootNode"), true, &[], &[]);
        db.register_internal("AnimationNodeTimeScale", Some("AnimationRootNode"), true, &[], &[]);
        db.register_internal("AnimationNodeTransition", Some("AnimationRootNode"), true, &[("xfade_time", IrValue::F64(0.0))], &[]);
        db.register_internal("AnimationNodeSub2", Some("AnimationRootNode"), true, &[("filter_enabled", IrValue::Bool(false))], &[]);
        db.register_internal("AnimationNodeAdd2", Some("AnimationRootNode"), true, &[("filter_enabled", IrValue::Bool(false))], &[]);
        db.register_internal("AnimationNodeAdd3", Some("AnimationRootNode"), true, &[], &[]);
        db.register_internal("AnimationNodeOutput", Some("AnimationNode"), true, &[], &[]);
        db.register_internal("SpriteFrames", Some("Resource"), true, &[], &[]);
        db.register_internal("TileSet", Some("Resource"), true, &[("tile_size", IrValue::Vec2([16.0, 16.0]))], &[]);
        db.register_internal("TileSetSource", Some("Resource"), false, &[], &[]);
        db.register_internal("TileSetAtlasSource", Some("TileSetSource"), true, &[("texture_region_size", IrValue::Vec2([16.0, 16.0]))], &[]);
        db.register_internal("TileSetScenesCollectionSource", Some("TileSetSource"), true, &[], &[]);
        db.register_internal("TileMapPattern", Some("Resource"), true, &[], &[]);
        db.register_internal("NavigationMesh", Some("Resource"), true, &[("cell_size", IrValue::F64(0.3)), ("cell_height", IrValue::F64(0.2))], &[]);
        db.register_internal("NavigationPolygon", Some("Resource"), true, &[("cell_size", IrValue::F64(1.0))], &[]);
        db.register_internal("Font", Some("Resource"), false, &[], &[]);
        db.register_internal("FontFile", Some("Font"), true, &[], &[]);
        db.register_internal("SystemFont", Some("Font"), true, &[], &[]);
        db.register_internal("FontVariation", Some("Font"), true, &[("variation_opentype", IrValue::Map(Default::default()))], &[]);
        db.register_internal("Theme", Some("Resource"), true, &[], &[]);
        db.register_internal("StyleBox", Some("Resource"), false, &[("content_margin_left", IrValue::F64(-1.0))], &[]);
        db.register_internal("StyleBoxFlat", Some("StyleBox"), true, &[("bg_color", IrValue::Color([0.6, 0.6, 0.6, 1.0]))], &[]);
        db.register_internal("StyleBoxTexture", Some("StyleBox"), true, &[], &[]);
        db.register_internal("StyleBoxEmpty", Some("StyleBox"), true, &[], &[]);
        db.register_internal("StyleBoxLine", Some("StyleBox"), true, &[("color", IrValue::Color([0.0, 0.0, 0.0, 1.0])), ("thickness", IrValue::F64(1.0))], &[]);
        db.register_internal("Gradient", Some("Resource"), true, &[], &[]);
        db.register_internal("Curve", Some("Resource"), true, &[("min_value", IrValue::F64(0.0)), ("max_value", IrValue::F64(1.0))], &[]);
        db.register_internal("Curve2D", Some("Resource"), true, &[("bake_interval", IrValue::F64(5.0))], &[]);
        db.register_internal("Curve3D", Some("Resource"), true, &[("bake_interval", IrValue::F64(0.2))], &[]);
        db.register_internal("Noise", Some("Resource"), false, &[], &[]);
        db.register_internal("FastNoiseLite", Some("Noise"), true, &[("noise_type", IrValue::I64(0)), ("frequency", IrValue::F64(0.01))], &[]);
        db.register_internal("Script", Some("Resource"), false, &[("source_code", IrValue::String("".into()))], &[]);
        db.register_internal("GDScript", Some("Script"), true, &[], &[]);
        db.register_internal("CSharpScript", Some("Script"), true, &[], &[]);
        db.register_internal("InputEvent", Some("Resource"), false, &[("device", IrValue::I64(0))], &[]);
        db.register_internal("InputEventFromWindow", Some("InputEvent"), false, &[("window_id", IrValue::I64(0))], &[]);
        db.register_internal("InputEventWithModifiers", Some("InputEventFromWindow"), false, &[("command_or_control_autoremap", IrValue::Bool(false))], &[]);
        db.register_internal("InputEventKey", Some("InputEventWithModifiers"), true, &[("pressed", IrValue::Bool(false)), ("keycode", IrValue::I64(0))], &[]);
        db.register_internal("InputEventMouse", Some("InputEventWithModifiers"), false, &[("button_mask", IrValue::I64(0)), ("position", IrValue::Vec2([0.0, 0.0]))], &[]);
        db.register_internal("InputEventMouseButton", Some("InputEventMouse"), true, &[("factor", IrValue::F64(1.0)), ("button_index", IrValue::I64(1)), ("pressed", IrValue::Bool(false))], &[]);
        db.register_internal("InputEventMouseMotion", Some("InputEventMouse"), true, &[("relative", IrValue::Vec2([0.0, 0.0])), ("velocity", IrValue::Vec2([0.0, 0.0]))], &[]);
        db.register_internal("InputEventGesture", Some("InputEventWithModifiers"), false, &[], &[]);
        db.register_internal("InputEventMagnifyGesture", Some("InputEventGesture"), true, &[("factor", IrValue::F64(1.0))], &[]);
        db.register_internal("InputEventPanGesture", Some("InputEventGesture"), true, &[("delta", IrValue::Vec2([0.0, 0.0]))], &[]);
        db.register_internal("InputEventScreenTouch", Some("InputEventFromWindow"), true, &[("index", IrValue::I64(0)), ("position", IrValue::Vec2([0.0, 0.0])), ("pressed", IrValue::Bool(false))], &[]);
        db.register_internal("InputEventScreenDrag", Some("InputEventFromWindow"), true, &[("index", IrValue::I64(0)), ("position", IrValue::Vec2([0.0, 0.0])), ("relative", IrValue::Vec2([0.0, 0.0]))], &[]);
        db.register_internal("InputEventJoypadButton", Some("InputEvent"), true, &[("button_index", IrValue::I64(0)), ("pressure", IrValue::F64(0.0)), ("pressed", IrValue::Bool(false))], &[]);
        db.register_internal("InputEventJoypadMotion", Some("InputEvent"), true, &[("axis", IrValue::I64(0)), ("axis_value", IrValue::F64(0.0))], &[]);
        db.register_internal("InputEventAction", Some("InputEvent"), true, &[("action", IrValue::String("".into())), ("pressed", IrValue::Bool(false))], &[]);
        db.register_internal("InputEventShortcut", Some("InputEvent"), true, &[], &[]);
        db.register_internal("InputEventMIDI", Some("InputEvent"), true, &[("channel", IrValue::I64(0)), ("message", IrValue::I64(0))], &[]);
        db.register_internal("Occluder3D", Some("Resource"), false, &[], &[]);
        db.register_internal("ArrayOccluder3D", Some("Occluder3D"), true, &[], &[]);
        db.register_internal("BoxOccluder3D", Some("Occluder3D"), true, &[("size", IrValue::Vec3([1.0, 1.0, 1.0]))], &[]);
        db.register_internal("SphereOccluder3D", Some("Occluder3D"), true, &[("radius", IrValue::F64(1.0))], &[]);
        db.register_internal("PolygonOccluder3D", Some("Occluder3D"), true, &[], &[]);
        db.register_internal("QuadOccluder3D", Some("Occluder3D"), true, &[("size", IrValue::Vec2([1.0, 1.0]))], &[]);
        db.register_internal("OccluderPolygon2D", Some("Resource"), true, &[("closed", IrValue::Bool(true))], &[]);
        db.register_internal("Node", Some("Object"), true, &[("name", IrValue::String("".into())), ("process_mode", IrValue::I64(0))], &["ready", "tree_entered", "tree_exited", "renamed"]);
        db.register_internal("CanvasItem", Some("Node"), false, &[("visible", IrValue::Bool(true)), ("modulate", IrValue::Color([1.0, 1.0, 1.0, 1.0])), ("self_modulate", IrValue::Color([1.0, 1.0, 1.0, 1.0])), ("z_index", IrValue::I64(0))], &["draw", "visibility_changed", "hidden"]);
        db.register_internal("Node2D", Some("CanvasItem"), true, &[("position", IrValue::Vec2([0.0, 0.0])), ("rotation", IrValue::F64(0.0)), ("scale", IrValue::Vec2([1.0, 1.0])), ("skew", IrValue::F64(0.0))], &[]);
        db.register_internal("Sprite2D", Some("Node2D"), true, &[("centered", IrValue::Bool(true)), ("offset", IrValue::Vec2([0.0, 0.0])), ("flip_h", IrValue::Bool(false)), ("flip_v", IrValue::Bool(false))], &["frame_changed", "texture_changed"]);
        db.register_internal("AnimatedSprite2D", Some("Node2D"), true, &[("speed_scale", IrValue::F64(1.0)), ("centered", IrValue::Bool(true)), ("flip_h", IrValue::Bool(false)), ("flip_v", IrValue::Bool(false))], &["animation_finished", "frame_changed"]);
        db.register_internal("CollisionObject2D", Some("Node2D"), false, &[("collision_layer", IrValue::I64(1)), ("collision_mask", IrValue::I64(1))], &["input_event", "mouse_entered", "mouse_exited"]);
        db.register_internal("PhysicsBody2D", Some("CollisionObject2D"), false, &[], &[]);
        db.register_internal("CharacterBody2D", Some("PhysicsBody2D"), true, &[("motion_mode", IrValue::I64(0)), ("up_direction", IrValue::Vec2([0.0, -1.0])), ("slide_on_ceiling", IrValue::Bool(true))], &[]);
        db.register_internal("RigidBody2D", Some("PhysicsBody2D"), true, &[("mass", IrValue::F64(1.0)), ("gravity_scale", IrValue::F64(1.0)), ("linear_damp", IrValue::F64(0.0)), ("angular_damp", IrValue::F64(0.0))], &["body_entered", "body_exited", "sleeping_state_changed"]);
        db.register_internal("StaticBody2D", Some("PhysicsBody2D"), true, &[("constant_linear_velocity", IrValue::Vec2([0.0, 0.0])), ("constant_angular_velocity", IrValue::F64(0.0))], &[]);
        db.register_internal("AnimatableBody2D", Some("StaticBody2D"), true, &[("sync_to_physics", IrValue::Bool(true))], &[]);
        db.register_internal("Area2D", Some("CollisionObject2D"), true, &[("monitoring", IrValue::Bool(true)), ("monitorable", IrValue::Bool(true)), ("gravity_space_override", IrValue::I64(0))], &["body_entered", "body_exited", "area_entered", "area_exited"]);
        db.register_internal("CollisionShape2D", Some("Node2D"), true, &[("disabled", IrValue::Bool(false)), ("one_way_collision", IrValue::Bool(false))], &[]);
        db.register_internal("CollisionPolygon2D", Some("Node2D"), true, &[("disabled", IrValue::Bool(false)), ("one_way_collision", IrValue::Bool(false))], &[]);
        db.register_internal("Polygon2D", Some("Node2D"), true, &[("color", IrValue::Color([1.0, 1.0, 1.0, 1.0])), ("antialiased", IrValue::Bool(false))], &[]);
        db.register_internal("Line2D", Some("Node2D"), true, &[("width", IrValue::F64(10.0)), ("default_color", IrValue::Color([0.4, 0.5, 1.0, 1.0]))], &[]);
        db.register_internal("Path2D", Some("Node2D"), true, &[], &[]);
        db.register_internal("PathFollow2D", Some("Node2D"), true, &[("progress", IrValue::F64(0.0)), ("progress_ratio", IrValue::F64(0.0)), ("rotates", IrValue::Bool(true)), ("loop", IrValue::Bool(true))], &[]);
        db.register_internal("Camera2D", Some("Node2D"), true, &[("zoom", IrValue::Vec2([1.0, 1.0])), ("anchor_mode", IrValue::I64(1)), ("enabled", IrValue::Bool(true))], &[]);
        db.register_internal("TileMap", Some("Node2D"), true, &[("cell_quadrant_size", IrValue::I64(16))], &["changed"]);
        db.register_internal("TileMapLayer", Some("Node2D"), true, &[("enabled", IrValue::Bool(true)), ("y_sort_enabled", IrValue::Bool(false))], &[]);
        db.register_internal("Light2D", Some("Node2D"), false, &[("enabled", IrValue::Bool(true)), ("color", IrValue::Color([1.0, 1.0, 1.0, 1.0])), ("energy", IrValue::F64(1.0))], &[]);
        db.register_internal("PointLight2D", Some("Light2D"), true, &[("texture_scale", IrValue::F64(1.0))], &[]);
        db.register_internal("DirectionalLight2D", Some("Light2D"), true, &[("height", IrValue::F64(0.0))], &[]);
        db.register_internal("LightOccluder2D", Some("Node2D"), true, &[("sdf_collision", IrValue::Bool(false))], &[]);
        db.register_internal("CanvasModulate", Some("Node2D"), true, &[("color", IrValue::Color([1.0, 1.0, 1.0, 1.0]))], &[]);
        db.register_internal("GPUParticles2D", Some("Node2D"), true, &[("emitting", IrValue::Bool(true)), ("amount", IrValue::I64(8)), ("lifetime", IrValue::F64(1.0)), ("one_shot", IrValue::Bool(false))], &["finished"]);
        db.register_internal("CPUParticles2D", Some("Node2D"), true, &[("emitting", IrValue::Bool(true)), ("amount", IrValue::I64(8)), ("lifetime", IrValue::F64(1.0)), ("one_shot", IrValue::Bool(false))], &["finished"]);
        db.register_internal("AudioStreamPlayer2D", Some("Node2D"), true, &[("volume_db", IrValue::F64(0.0)), ("pitch_scale", IrValue::F64(1.0)), ("max_distance", IrValue::F64(2000.0)), ("attenuation", IrValue::F64(1.0))], &["finished"]);
        db.register_internal("NavigationRegion2D", Some("Node2D"), true, &[("enabled", IrValue::Bool(true))], &["bake_finished"]);
        db.register_internal("NavigationAgent2D", Some("Node"), true, &[("target_position", IrValue::Vec2([0.0, 0.0])), ("path_desired_distance", IrValue::F64(20.0)), ("target_desired_distance", IrValue::F64(10.0))], &["navigation_finished", "velocity_computed"]);
        db.register_internal("NavigationLink2D", Some("Node2D"), true, &[("enabled", IrValue::Bool(true)), ("bidirectional", IrValue::Bool(true))], &[]);
        db.register_internal("NavigationObstacle2D", Some("Node2D"), true, &[("radius", IrValue::F64(0.0)), ("avoidance_enabled", IrValue::Bool(true))], &[]);
        db.register_internal("RayCast2D", Some("Node2D"), true, &[("enabled", IrValue::Bool(true)), ("target_position", IrValue::Vec2([0.0, 50.0])), ("collision_mask", IrValue::I64(1))], &[]);
        db.register_internal("ShapeCast2D", Some("Node2D"), true, &[("enabled", IrValue::Bool(true)), ("target_position", IrValue::Vec2([0.0, 50.0])), ("max_results", IrValue::I64(32))], &[]);
        db.register_internal("Joint2D", Some("Node2D"), false, &[("bias", IrValue::F64(0.0)), ("disable_collision", IrValue::Bool(true))], &[]);
        db.register_internal("PinJoint2D", Some("Joint2D"), true, &[("softness", IrValue::F64(0.0))], &[]);
        db.register_internal("DampedSpringJoint2D", Some("Joint2D"), true, &[("length", IrValue::F64(50.0)), ("stiffness", IrValue::F64(20.0)), ("damping", IrValue::F64(1.0))], &[]);
        db.register_internal("GrooveJoint2D", Some("Joint2D"), true, &[("length", IrValue::F64(50.0)), ("initial_offset", IrValue::F64(25.0))], &[]);
        db.register_internal("VisibleOnScreenNotifier2D", Some("Node2D"), true, &[], &["screen_entered", "screen_exited"]);
        db.register_internal("VisibleOnScreenEnabler2D", Some("VisibleOnScreenNotifier2D"), true, &[("enable_mode", IrValue::I64(0))], &[]);
        db.register_internal("Parallax2D", Some("Node2D"), true, &[("scroll_scale", IrValue::Vec2([1.0, 1.0])), ("scroll_offset", IrValue::Vec2([0.0, 0.0])), ("repeat_size", IrValue::Vec2([0.0, 0.0]))], &[]);
        db.register_internal("ParallaxBackground", Some("CanvasLayer"), true, &[("scroll_offset", IrValue::Vec2([0.0, 0.0])), ("scroll_base_scale", IrValue::Vec2([1.0, 1.0]))], &[]);
        db.register_internal("ParallaxLayer", Some("Node2D"), true, &[("motion_scale", IrValue::Vec2([1.0, 1.0])), ("motion_offset", IrValue::Vec2([0.0, 0.0])), ("motion_mirroring", IrValue::Vec2([0.0, 0.0]))], &[]);
        db.register_internal("Skeleton2D", Some("Node2D"), true, &[], &["bone_setup_changed"]);
        db.register_internal("Bone2D", Some("Node2D"), true, &[], &[]);
        db.register_internal("BackBufferCopy", Some("Node2D"), true, &[("copy_mode", IrValue::I64(1))], &[]);
        db.register_internal("RemoteTransform2D", Some("Node2D"), true, &[("update_position", IrValue::Bool(true)), ("update_rotation", IrValue::Bool(true)), ("update_scale", IrValue::Bool(true))], &[]);
        db.register_internal("Marker2D", Some("Node2D"), true, &[("gizmo_extents", IrValue::F64(10.0))], &[]);
        db.register_internal("MultiMeshInstance2D", Some("Node2D"), true, &[], &[]);
        db.register_internal("MeshInstance2D", Some("Node2D"), true, &[], &[]);
        db.register_internal("CanvasGroup", Some("Node2D"), true, &[("fit_margin", IrValue::F64(10.0)), ("clear_margin", IrValue::F64(10.0))], &[]);
        db.register_internal("AudioListener2D", Some("Node2D"), true, &[], &[]);
        db.register_internal("TouchScreenButton", Some("Node2D"), true, &[("action", IrValue::String("".into()))], &["pressed", "released"]);
        db.register_internal("Control", Some("CanvasItem"), true, &[("position", IrValue::Vec2([0.0, 0.0])), ("size", IrValue::Vec2([0.0, 0.0])), ("rotation", IrValue::F64(0.0)), ("scale", IrValue::Vec2([1.0, 1.0])), ("focus_mode", IrValue::I64(2))], &["resized", "gui_input", "mouse_entered", "mouse_exited", "focus_entered", "focus_exited"]);
        db.register_internal("BaseButton", Some("Control"), false, &[("disabled", IrValue::Bool(false)), ("toggle_mode", IrValue::Bool(false)), ("button_pressed", IrValue::Bool(false))], &["pressed", "button_up", "button_down", "toggled"]);
        db.register_internal("Button", Some("BaseButton"), true, &[("text", IrValue::String("".into())), ("flat", IrValue::Bool(false)), ("alignment", IrValue::I64(1))], &[]);
        db.register_internal("CheckBox", Some("Button"), true, &[], &[]);
        db.register_internal("CheckButton", Some("Button"), true, &[], &[]);
        db.register_internal("OptionButton", Some("Button"), true, &[("selected", IrValue::I64(-1))], &["item_selected", "item_focused"]);
        db.register_internal("MenuButton", Some("Button"), true, &[("switch_on_hover", IrValue::Bool(false))], &["about_to_popup"]);
        db.register_internal("ColorPickerButton", Some("Button"), true, &[("color", IrValue::Color([1.0, 1.0, 1.0, 1.0])), ("edit_alpha", IrValue::Bool(true))], &["color_changed", "popup_closed"]);
        db.register_internal("LinkButton", Some("BaseButton"), true, &[("text", IrValue::String("".into())), ("underline", IrValue::I64(0))], &[]);
        db.register_internal("TextureButton", Some("BaseButton"), true, &[("flip_h", IrValue::Bool(false)), ("flip_v", IrValue::Bool(false))], &[]);
        db.register_internal("Label", Some("Control"), true, &[("text", IrValue::String("".into())), ("horizontal_alignment", IrValue::I64(0)), ("vertical_alignment", IrValue::I64(0)), ("autowrap_mode", IrValue::I64(0))], &[]);
        db.register_internal("RichTextLabel", Some("Control"), true, &[("bbcode_enabled", IrValue::Bool(true)), ("text", IrValue::String("".into())), ("fit_content", IrValue::Bool(false)), ("scroll_active", IrValue::Bool(true))], &["meta_clicked", "meta_hover_started", "meta_hover_ended"]);
        db.register_internal("LineEdit", Some("Control"), true, &[("text", IrValue::String("".into())), ("placeholder_text", IrValue::String("".into())), ("editable", IrValue::Bool(true))], &["text_changed", "text_submitted"]);
        db.register_internal("TextEdit", Some("Control"), true, &[("text", IrValue::String("".into())), ("placeholder_text", IrValue::String("".into())), ("editable", IrValue::Bool(true)), ("wrap_mode", IrValue::I64(0))], &["text_changed", "caret_changed"]);
        db.register_internal("CodeEdit", Some("TextEdit"), true, &[("symbol_lookup_on_click", IrValue::Bool(false)), ("line_folding", IrValue::Bool(false)), ("gutters_draw_line_numbers", IrValue::Bool(false))], &[]);
        db.register_internal("Range", Some("Control"), false, &[("min_value", IrValue::F64(0.0)), ("max_value", IrValue::F64(100.0)), ("step", IrValue::F64(1.0)), ("page", IrValue::F64(0.0)), ("value", IrValue::F64(0.0))], &["value_changed", "changed"]);
        db.register_internal("ScrollBar", Some("Range"), false, &[], &[]);
        db.register_internal("HScrollBar", Some("ScrollBar"), true, &[], &[]);
        db.register_internal("VScrollBar", Some("ScrollBar"), true, &[], &[]);
        db.register_internal("Slider", Some("Range"), false, &[("editable", IrValue::Bool(true)), ("scrollable", IrValue::Bool(true))], &["drag_started", "drag_ended"]);
        db.register_internal("HSlider", Some("Slider"), true, &[], &[]);
        db.register_internal("VSlider", Some("Slider"), true, &[], &[]);
        db.register_internal("SpinBox", Some("Range"), true, &[("alignment", IrValue::I64(0)), ("editable", IrValue::Bool(true)), ("prefix", IrValue::String("".into())), ("suffix", IrValue::String("".into()))], &[]);
        db.register_internal("ProgressBar", Some("Range"), true, &[("fill_mode", IrValue::I64(0)), ("show_percentage", IrValue::Bool(true))], &[]);
        db.register_internal("TextureProgressBar", Some("Range"), true, &[("fill_mode", IrValue::I64(0)), ("radial_initial_angle", IrValue::F64(0.0))], &[]);
        db.register_internal("Panel", Some("Control"), true, &[], &[]);
        db.register_internal("Container", Some("Control"), true, &[], &["pre_sort_children", "sort_children"]);
        db.register_internal("PanelContainer", Some("Container"), true, &[], &[]);
        db.register_internal("MarginContainer", Some("Container"), true, &[], &[]);
        db.register_internal("CenterContainer", Some("Container"), true, &[("use_top_left", IrValue::Bool(false))], &[]);
        db.register_internal("AspectRatioContainer", Some("Container"), true, &[("ratio", IrValue::F64(1.0)), ("stretch_mode", IrValue::I64(2))], &[]);
        db.register_internal("BoxContainer", Some("Container"), true, &[("alignment", IrValue::I64(0)), ("vertical", IrValue::Bool(false))], &[]);
        db.register_internal("HBoxContainer", Some("BoxContainer"), true, &[], &[]);
        db.register_internal("VBoxContainer", Some("BoxContainer"), true, &[], &[]);
        db.register_internal("GridContainer", Some("Container"), true, &[("columns", IrValue::I64(1))], &[]);
        db.register_internal("FlowContainer", Some("Container"), true, &[("alignment", IrValue::I64(0)), ("vertical", IrValue::Bool(false))], &[]);
        db.register_internal("HFlowContainer", Some("FlowContainer"), true, &[], &[]);
        db.register_internal("VFlowContainer", Some("FlowContainer"), true, &[], &[]);
        db.register_internal("SplitContainer", Some("Container"), false, &[("split_offset", IrValue::I64(0)), ("collapsed", IrValue::Bool(false)), ("vertical", IrValue::Bool(false))], &["dragged"]);
        db.register_internal("HSplitContainer", Some("SplitContainer"), true, &[], &[]);
        db.register_internal("VSplitContainer", Some("SplitContainer"), true, &[], &[]);
        db.register_internal("ScrollContainer", Some("Container"), true, &[("follow_focus", IrValue::Bool(false)), ("horizontal_scroll_mode", IrValue::I64(1)), ("vertical_scroll_mode", IrValue::I64(1))], &["scroll_started", "scroll_ended"]);
        db.register_internal("TabContainer", Some("Container"), true, &[("current_tab", IrValue::I64(0)), ("tabs_visible", IrValue::Bool(true))], &["active_tab_rearranged", "tab_changed", "tab_clicked"]);
        db.register_internal("SubViewportContainer", Some("Container"), true, &[("stretch", IrValue::Bool(false)), ("stretch_shrink", IrValue::I64(1))], &[]);
        db.register_internal("GraphEdit", Some("Control"), true, &[("scroll_offset", IrValue::Vec2([0.0, 0.0])), ("zoom", IrValue::F64(1.0))], &["connection_request", "disconnection_request"]);
        db.register_internal("GraphElement", Some("Container"), true, &[("position_offset", IrValue::Vec2([0.0, 0.0])), ("resizable", IrValue::Bool(false)), ("draggable", IrValue::Bool(true))], &["node_selected", "node_deselected"]);
        db.register_internal("GraphNode", Some("GraphElement"), true, &[("title", IrValue::String("".into())), ("comment", IrValue::Bool(false))], &["slot_updated"]);
        db.register_internal("TabBar", Some("Control"), true, &[("current_tab", IrValue::I64(0)), ("tab_count", IrValue::I64(0))], &["tab_changed", "tab_clicked", "tab_close_pressed"]);
        db.register_internal("TextureRect", Some("Control"), true, &[("expand_mode", IrValue::I64(0)), ("stretch_mode", IrValue::I64(0)), ("flip_h", IrValue::Bool(false)), ("flip_v", IrValue::Bool(false))], &[]);
        db.register_internal("ColorRect", Some("Control"), true, &[("color", IrValue::Color([1.0, 1.0, 1.0, 1.0]))], &[]);
        db.register_internal("NinePatchRect", Some("Control"), true, &[("patch_margin_left", IrValue::I64(0)), ("patch_margin_top", IrValue::I64(0)), ("patch_margin_right", IrValue::I64(0)), ("patch_margin_bottom", IrValue::I64(0))], &[]);
        db.register_internal("VideoStreamPlayer", Some("Control"), true, &[("autoplay", IrValue::Bool(false)), ("loop", IrValue::Bool(false)), ("volume_db", IrValue::F64(0.0))], &["finished"]);
        db.register_internal("Tree", Some("Control"), true, &[("columns", IrValue::I64(1)), ("hide_root", IrValue::Bool(false)), ("select_mode", IrValue::I64(0))], &["item_selected", "item_activated", "cell_selected"]);
        db.register_internal("ItemList", Some("Control"), true, &[("select_mode", IrValue::I64(0)), ("allow_reselect", IrValue::Bool(false)), ("auto_height", IrValue::Bool(false))], &["item_selected", "item_activated", "empty_clicked"]);
        db.register_internal("ColorPicker", Some("Control"), true, &[("color", IrValue::Color([1.0, 1.0, 1.0, 1.0])), ("edit_alpha", IrValue::Bool(true))], &["color_changed"]);
        db.register_internal("ReferenceRect", Some("Control"), true, &[("border_color", IrValue::Color([1.0, 0.0, 0.0, 1.0])), ("border_width", IrValue::F64(1.0))], &[]);
        db.register_internal("Separator", Some("Control"), false, &[], &[]);
        db.register_internal("HSeparator", Some("Separator"), true, &[], &[]);
        db.register_internal("VSeparator", Some("Separator"), true, &[], &[]);
        db.register_internal("Node3D", Some("Node"), true, &[("position", IrValue::Vec3([0.0, 0.0, 0.0])), ("rotation", IrValue::Vec3([0.0, 0.0, 0.0])), ("scale", IrValue::Vec3([1.0, 1.0, 1.0])), ("visible", IrValue::Bool(true))], &["visibility_changed"]);
        db.register_internal("VisualInstance3D", Some("Node3D"), false, &[("layers", IrValue::I64(1))], &[]);
        db.register_internal("GeometryInstance3D", Some("VisualInstance3D"), false, &[("cast_shadow", IrValue::I64(1)), ("extra_cull_margin", IrValue::F64(0.0)), ("lod_bias", IrValue::F64(1.0))], &[]);
        db.register_internal("MeshInstance3D", Some("GeometryInstance3D"), true, &[], &[]);
        db.register_internal("MultiMeshInstance3D", Some("GeometryInstance3D"), true, &[], &[]);
        db.register_internal("CPUParticles3D", Some("GeometryInstance3D"), true, &[("emitting", IrValue::Bool(true)), ("amount", IrValue::I64(8)), ("lifetime", IrValue::F64(1.0)), ("one_shot", IrValue::Bool(false))], &["finished"]);
        db.register_internal("GPUParticles3D", Some("GeometryInstance3D"), true, &[("emitting", IrValue::Bool(true)), ("amount", IrValue::I64(8)), ("lifetime", IrValue::F64(1.0)), ("one_shot", IrValue::Bool(false))], &["finished"]);
        db.register_internal("CSGShape3D", Some("GeometryInstance3D"), false, &[("operation", IrValue::I64(0)), ("use_collision", IrValue::Bool(false))], &[]);
        db.register_internal("CSGBox3D", Some("CSGShape3D"), true, &[("size", IrValue::Vec3([1.0, 1.0, 1.0]))], &[]);
        db.register_internal("CSGSphere3D", Some("CSGShape3D"), true, &[("radius", IrValue::F64(0.5))], &[]);
        db.register_internal("CSGCylinder3D", Some("CSGShape3D"), true, &[("radius", IrValue::F64(0.5)), ("height", IrValue::F64(1.0))], &[]);
        db.register_internal("CSGPolygon3D", Some("CSGShape3D"), true, &[("depth", IrValue::F64(1.0))], &[]);
        db.register_internal("CSGCombiner3D", Some("CSGShape3D"), true, &[], &[]);
        db.register_internal("CSGMesh3D", Some("CSGShape3D"), true, &[], &[]);
        db.register_internal("CSGTorus3D", Some("CSGShape3D"), true, &[("inner_radius", IrValue::F64(0.5)), ("outer_radius", IrValue::F64(1.0))], &[]);
        db.register_internal("Label3D", Some("GeometryInstance3D"), true, &[("text", IrValue::String("".into())), ("font_size", IrValue::I64(32)), ("horizontal_alignment", IrValue::I64(1))], &[]);
        db.register_internal("SpriteBase3D", Some("GeometryInstance3D"), false, &[("centered", IrValue::Bool(true)), ("offset", IrValue::Vec2([0.0, 0.0])), ("flip_h", IrValue::Bool(false)), ("flip_v", IrValue::Bool(false))], &[]);
        db.register_internal("Sprite3D", Some("SpriteBase3D"), true, &[], &[]);
        db.register_internal("AnimatedSprite3D", Some("SpriteBase3D"), true, &[("speed_scale", IrValue::F64(1.0))], &["animation_finished", "frame_changed"]);
        db.register_internal("Light3D", Some("VisualInstance3D"), false, &[("light_energy", IrValue::F64(1.0)), ("light_color", IrValue::Color([1.0, 1.0, 1.0, 1.0])), ("shadow_enabled", IrValue::Bool(false))], &[]);
        db.register_internal("DirectionalLight3D", Some("Light3D"), true, &[("sky_mode", IrValue::I64(0))], &[]);
        db.register_internal("OmniLight3D", Some("Light3D"), true, &[("omni_range", IrValue::F64(5.0)), ("omni_attenuation", IrValue::F64(1.0))], &[]);
        db.register_internal("SpotLight3D", Some("Light3D"), true, &[("spot_range", IrValue::F64(5.0)), ("spot_attenuation", IrValue::F64(1.0)), ("spot_angle", IrValue::F64(45.0))], &[]);
        db.register_internal("ReflectionProbe", Some("VisualInstance3D"), true, &[("update_mode", IrValue::I64(0)), ("intensity", IrValue::F64(1.0)), ("size", IrValue::Vec3([20.0, 20.0, 20.0]))], &[]);
        db.register_internal("Decal", Some("VisualInstance3D"), true, &[("size", IrValue::Vec3([2.0, 2.0, 2.0])), ("albedo_mix", IrValue::F64(1.0))], &[]);
        db.register_internal("FogVolume", Some("VisualInstance3D"), true, &[("size", IrValue::Vec3([2.0, 2.0, 2.0])), ("shape", IrValue::I64(0))], &[]);
        db.register_internal("LightmapGI", Some("VisualInstance3D"), true, &[("quality", IrValue::I64(1)), ("bounces", IrValue::I64(3))], &[]);
        db.register_internal("VoxelGI", Some("VisualInstance3D"), true, &[("size", IrValue::Vec3([20.0, 20.0, 20.0]))], &[]);
        db.register_internal("LightmapProbe", Some("Node3D"), true, &[], &[]);
        db.register_internal("OccluderInstance3D", Some("VisualInstance3D"), true, &[("bake_mask", IrValue::I64(4294967295))], &[]);
        db.register_internal("CollisionObject3D", Some("Node3D"), false, &[("collision_layer", IrValue::I64(1)), ("collision_mask", IrValue::I64(1))], &["input_event", "mouse_entered", "mouse_exited"]);
        db.register_internal("PhysicsBody3D", Some("CollisionObject3D"), false, &[], &[]);
        db.register_internal("CharacterBody3D", Some("PhysicsBody3D"), true, &[("motion_mode", IrValue::I64(0)), ("up_direction", IrValue::Vec3([0.0, 1.0, 0.0])), ("floor_stop_on_slope", IrValue::Bool(true))], &[]);
        db.register_internal("RigidBody3D", Some("PhysicsBody3D"), true, &[("mass", IrValue::F64(1.0)), ("gravity_scale", IrValue::F64(1.0)), ("linear_damp", IrValue::F64(0.0)), ("angular_damp", IrValue::F64(0.0))], &["body_entered", "body_exited", "sleeping_state_changed"]);
        db.register_internal("StaticBody3D", Some("PhysicsBody3D"), true, &[("constant_linear_velocity", IrValue::Vec3([0.0, 0.0, 0.0])), ("constant_angular_velocity", IrValue::Vec3([0.0, 0.0, 0.0]))], &[]);
        db.register_internal("AnimatableBody3D", Some("StaticBody3D"), true, &[("sync_to_physics", IrValue::Bool(true))], &[]);
        db.register_internal("VehicleBody3D", Some("RigidBody3D"), true, &[("engine_force", IrValue::F64(0.0)), ("brake", IrValue::F64(0.0)), ("steering", IrValue::F64(0.0))], &[]);
        db.register_internal("PhysicalBone3D", Some("PhysicsBody3D"), true, &[("friction", IrValue::F64(1.0)), ("bounce", IrValue::F64(0.0))], &[]);
        db.register_internal("Area3D", Some("CollisionObject3D"), true, &[("monitoring", IrValue::Bool(true)), ("monitorable", IrValue::Bool(true)), ("gravity_space_override", IrValue::I64(0))], &["body_entered", "body_exited", "area_entered", "area_exited"]);
        db.register_internal("VehicleWheel3D", Some("Node3D"), true, &[("suspension_stiffness", IrValue::F64(5.88)), ("damping_compression", IrValue::F64(0.83))], &[]);
        db.register_internal("SoftBody3D", Some("MeshInstance3D"), true, &[("total_mass", IrValue::F64(1.0)), ("linear_stiffness", IrValue::F64(0.5))], &[]);
        db.register_internal("CollisionShape3D", Some("Node3D"), true, &[("disabled", IrValue::Bool(false))], &[]);
        db.register_internal("CollisionPolygon3D", Some("Node3D"), true, &[("depth", IrValue::F64(1.0)), ("disabled", IrValue::Bool(false))], &[]);
        db.register_internal("Camera3D", Some("Node3D"), true, &[("fov", IrValue::F64(75.0)), ("near", IrValue::F64(0.05)), ("far", IrValue::F64(4000.0)), ("current", IrValue::Bool(false))], &[]);
        db.register_internal("SpringArm3D", Some("Node3D"), true, &[("spring_length", IrValue::F64(1.0)), ("margin", IrValue::F64(0.01))], &[]);
        db.register_internal("Skeleton3D", Some("Node3D"), true, &[("motion_scale", IrValue::F64(1.0))], &["bone_pose_changed", "show_rest_only_changed"]);
        db.register_internal("BoneAttachment3D", Some("Node3D"), true, &[("bone_name", IrValue::String("".into())), ("bone_idx", IrValue::I64(-1))], &[]);
        db.register_internal("SkeletonModifier3D", Some("Node3D"), false, &[("active", IrValue::Bool(true)), ("influence", IrValue::F64(1.0))], &[]);
        db.register_internal("LookAtModifier3D", Some("SkeletonModifier3D"), true, &[("duration", IrValue::F64(0.0))], &[]);
        db.register_internal("SkeletonIK3D", Some("Node3D"), true, &[("interpolation", IrValue::F64(1.0)), ("min_distance", IrValue::F64(0.01))], &[]);
        db.register_internal("Path3D", Some("Node3D"), true, &[], &[]);
        db.register_internal("PathFollow3D", Some("Node3D"), true, &[("progress", IrValue::F64(0.0)), ("progress_ratio", IrValue::F64(0.0)), ("rotation_mode", IrValue::I64(4)), ("loop", IrValue::Bool(true))], &[]);
        db.register_internal("GridMap", Some("Node3D"), true, &[("cell_size", IrValue::Vec3([2.0, 2.0, 2.0])), ("cell_octant_size", IrValue::I64(8))], &["cell_size_changed"]);
        db.register_internal("NavigationRegion3D", Some("Node3D"), true, &[("enabled", IrValue::Bool(true))], &["bake_finished"]);
        db.register_internal("NavigationAgent3D", Some("Node"), true, &[("target_position", IrValue::Vec3([0.0, 0.0, 0.0])), ("path_desired_distance", IrValue::F64(1.0)), ("target_desired_distance", IrValue::F64(1.0))], &["navigation_finished", "velocity_computed"]);
        db.register_internal("NavigationLink3D", Some("Node3D"), true, &[("enabled", IrValue::Bool(true)), ("bidirectional", IrValue::Bool(true))], &[]);
        db.register_internal("NavigationObstacle3D", Some("Node3D"), true, &[("radius", IrValue::F64(0.0)), ("height", IrValue::F64(1.0)), ("avoidance_enabled", IrValue::Bool(true))], &[]);
        db.register_internal("RayCast3D", Some("Node3D"), true, &[("enabled", IrValue::Bool(true)), ("target_position", IrValue::Vec3([0.0, -1.0, 0.0])), ("collision_mask", IrValue::I64(1))], &[]);
        db.register_internal("ShapeCast3D", Some("Node3D"), true, &[("enabled", IrValue::Bool(true)), ("target_position", IrValue::Vec3([0.0, -1.0, 0.0])), ("max_results", IrValue::I64(32))], &[]);
        db.register_internal("AudioStreamPlayer3D", Some("Node3D"), true, &[("volume_db", IrValue::F64(0.0)), ("unit_size", IrValue::F64(10.0)), ("max_db", IrValue::F64(3.0)), ("max_distance", IrValue::F64(0.0))], &["finished"]);
        db.register_internal("AudioListener3D", Some("Node3D"), true, &[], &[]);
        db.register_internal("Joint3D", Some("Node3D"), false, &[("solver_priority", IrValue::I64(1)), ("exclude_nodes_from_collision", IrValue::Bool(true))], &[]);
        db.register_internal("PinJoint3D", Some("Joint3D"), true, &[("bias", IrValue::F64(0.3)), ("damping", IrValue::F64(1.0)), ("impulse_clamp", IrValue::F64(0.0))], &[]);
        db.register_internal("HingeJoint3D", Some("Joint3D"), true, &[("bias", IrValue::F64(0.3)), ("relaxation", IrValue::F64(1.0))], &[]);
        db.register_internal("SliderJoint3D", Some("Joint3D"), true, &[("linear_limit_upper_distance", IrValue::F64(0.0)), ("linear_limit_lower_distance", IrValue::F64(0.0))], &[]);
        db.register_internal("ConeTwistJoint3D", Some("Joint3D"), true, &[("swing_span", IrValue::F64(std::f64::consts::FRAC_PI_4)), ("twist_span", IrValue::F64(std::f64::consts::PI))], &[]);
        db.register_internal("Generic6DOFJoint3D", Some("Joint3D"), true, &[("linear_limit_x_enabled", IrValue::Bool(true))], &[]);
        db.register_internal("RemoteTransform3D", Some("Node3D"), true, &[("update_position", IrValue::Bool(true)), ("update_rotation", IrValue::Bool(true)), ("update_scale", IrValue::Bool(true))], &[]);
        db.register_internal("Marker3D", Some("Node3D"), true, &[("gizmo_extents", IrValue::F64(1.0))], &[]);
        db.register_internal("VisibleOnScreenNotifier3D", Some("Node3D"), true, &[], &["screen_entered", "screen_exited"]);
        db.register_internal("VisibleOnScreenEnabler3D", Some("VisibleOnScreenNotifier3D"), true, &[("enable_mode", IrValue::I64(0))], &[]);
        db.register_internal("XRNode3D", Some("Node3D"), false, &[("tracker", IrValue::String("".into()))], &[]);
        db.register_internal("XRController3D", Some("XRNode3D"), true, &[], &["button_pressed", "button_released", "input_float_changed"]);
        db.register_internal("XRAnchor3D", Some("XRNode3D"), true, &[], &[]);
        db.register_internal("XROrigin3D", Some("Node3D"), true, &[("current", IrValue::Bool(false))], &[]);
        db.register_internal("RootMotionView", Some("VisualInstance3D"), true, &[("animation_path", IrValue::String("".into())), ("color", IrValue::Color([0.5, 0.5, 1.0, 1.0]))], &[]);
        db.register_internal("Viewport", Some("Node"), false, &[("size", IrValue::Vec2([512.0, 512.0]))], &["size_changed", "gui_focus_changed"]);
        db.register_internal("SubViewport", Some("Viewport"), true, &[("render_target_update_mode", IrValue::I64(2))], &[]);
        db.register_internal("Window", Some("Viewport"), true, &[("title", IrValue::String("".into())), ("visible", IrValue::Bool(true)), ("size", IrValue::Vec2([100.0, 100.0])), ("exclusive", IrValue::Bool(false))], &["close_requested", "window_input", "focus_entered", "focus_exited"]);
        db.register_internal("Popup", Some("Window"), true, &[("visible", IrValue::Bool(false))], &["popup_hide"]);
        db.register_internal("PopupMenu", Some("Popup"), true, &[("item_count", IrValue::I64(0))], &["id_pressed", "index_pressed", "menu_changed"]);
        db.register_internal("PopupPanel", Some("Popup"), true, &[], &[]);
        db.register_internal("AcceptDialog", Some("Window"), true, &[("dialog_text", IrValue::String("".into())), ("ok_button_text", IrValue::String("OK".into()))], &["confirmed", "canceled"]);
        db.register_internal("ConfirmationDialog", Some("AcceptDialog"), true, &[("cancel_button_text", IrValue::String("Cancel".into()))], &[]);
        db.register_internal("FileDialog", Some("ConfirmationDialog"), true, &[("file_mode", IrValue::I64(0)), ("access", IrValue::I64(0))], &["file_selected", "files_selected", "dir_selected"]);
        db.register_internal("CanvasLayer", Some("Node"), true, &[("layer", IrValue::I64(1)), ("visible", IrValue::Bool(true)), ("scale", IrValue::Vec2([1.0, 1.0]))], &["visibility_changed"]);
        db.register_internal("AnimationPlayer", Some("Node"), true, &[("current_animation", IrValue::String("".into())), ("speed_scale", IrValue::F64(1.0)), ("autoplay", IrValue::String("".into()))], &["animation_started", "animation_finished", "animation_changed"]);
        db.register_internal("AnimationTree", Some("Node"), true, &[("active", IrValue::Bool(true)), ("advance_expression_base_node", IrValue::String("".into()))], &["animation_player_changed"]);
        db.register_internal("Timer", Some("Node"), true, &[("wait_time", IrValue::F64(1.0)), ("one_shot", IrValue::Bool(false)), ("autostart", IrValue::Bool(false))], &["timeout"]);
        db.register_internal("HTTPRequest", Some("Node"), true, &[("timeout", IrValue::F64(0.0)), ("max_redirects", IrValue::I64(8)), ("use_threads", IrValue::Bool(false))], &["request_completed"]);
        db.register_internal("MultiplayerSpawner", Some("Node"), true, &[], &["despawned", "spawned"]);
        db.register_internal("MultiplayerSynchronizer", Some("Node"), true, &[("root_path", IrValue::String("".into())), ("delta_interval", IrValue::F64(0.0))], &["delta_synchronized", "synchronized"]);
        db.register_internal("ResourcePreloader", Some("Node"), true, &[], &[]);
        db.register_internal("ShaderGlobalsOverride", Some("Node"), true, &[], &[]);
        db.register_internal("StatusIndicator", Some("Node"), true, &[("visible", IrValue::Bool(true)), ("tooltip", IrValue::String("".into()))], &["pressed"]);
        db.register_internal("AudioStreamPlayer", Some("Node"), true, &[("volume_db", IrValue::F64(0.0)), ("pitch_scale", IrValue::F64(1.0)), ("autoplay", IrValue::Bool(false)), ("mix_target", IrValue::I64(0))], &["finished"]);
        db.register_internal("WorldEnvironment", Some("Node"), true, &[], &[]);
        db.register_internal("MissingNode", Some("Node"), true, &[("original_class", IrValue::String("".into()))], &[]);
        db.register_internal("InstancePlaceholder", Some("Node"), true, &[], &[]);
        db
    }
}
