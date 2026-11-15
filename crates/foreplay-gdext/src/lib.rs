//! Godot 4.4 `GDExtension` C ABI compatibility layer for Foreplay Game Engine.

pub mod c_abi;
pub mod host;
pub mod manifest;

pub use c_abi::*;
pub use host::*;
pub use manifest::*;

/// Version triple matching Godot 4.4 GDExtension ABI compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GdExtVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Default for GdExtVersion {
    fn default() -> Self {
        Self {
            major: 4,
            minor: 4,
            patch: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use foreplay_ir::IrValue;
    use std::ffi::{CStr, c_char};

    #[test]
    fn test_supported_proc_names_exceeds_60_entries() {
        let procs = supported_proc_names();
        assert!(
            procs.len() >= 60,
            "Expected at least 60 interface functions, found {}",
            procs.len()
        );
        assert!(procs.contains(&"get_godot_version"));
        assert!(procs.contains(&"mem_alloc"));
        assert!(procs.contains(&"variant_new_nil"));
        assert!(procs.contains(&"classdb_register_extension_class3"));
        assert!(procs.contains(&"classdb_register_extension_class_method"));
    }

    #[test]
    fn test_gdext_get_proc_address_resolves_all_supported_names() {
        for &name in supported_proc_names() {
            let c_name = std::ffi::CString::new(name).unwrap();
            // SAFETY: c_name is a valid null-terminated C string.
            let ptr = unsafe { gdext_get_proc_address(c_name.as_ptr()) };
            assert!(
                ptr.is_some(),
                "Failed to resolve supported proc name: '{name}'"
            );
        }

        // Unknown function resolves to None
        let dummy = std::ffi::CString::new("non_existent_function_name").unwrap();
        // SAFETY: dummy is a valid null-terminated C string.
        let unknown = unsafe { gdext_get_proc_address(dummy.as_ptr()) };
        assert!(unknown.is_none());
    }

    #[test]
    fn test_gdext_get_godot_version() {
        let mut ver = GDExtensionGodotVersion {
            major: 0,
            minor: 0,
            patch: 0,
            string: core::ptr::null(),
        };
        // SAFETY: ver is a valid mutable reference to GDExtensionGodotVersion.
        unsafe {
            gdext_get_godot_version(&mut ver);
        }
        assert_eq!(ver.major, 4);
        assert_eq!(ver.minor, 4);
        assert_eq!(ver.patch, 0);
        assert!(!ver.string.is_null());
        // SAFETY: ver.string is guaranteed to point to static null-terminated version string.
        let s = unsafe { CStr::from_ptr(ver.string).to_str().unwrap() };
        assert!(s.contains("4.4.0"));
    }

    #[test]
    fn test_mem_alloc_realloc_free() {
        // SAFETY: Allocating 64 bytes via C ABI allocator.
        let ptr = unsafe { gdext_mem_alloc(64) };
        assert!(!ptr.is_null());
        assert_eq!((ptr as usize) % 16, 0, "Pointer must be 16-byte aligned");

        // SAFETY: Writing into allocated memory.
        unsafe {
            let slice = std::slice::from_raw_parts_mut(ptr as *mut u8, 64);
            slice.fill(0xAA);
        }

        // SAFETY: Reallocating buffer to 128 bytes.
        let realloc_ptr = unsafe { gdext_mem_realloc(ptr, 128) };
        assert!(!realloc_ptr.is_null());
        assert_eq!((realloc_ptr as usize) % 16, 0);

        // Verify preserved prefix
        // SAFETY: Verifying copied data.
        unsafe {
            let slice = std::slice::from_raw_parts(realloc_ptr as *const u8, 64);
            assert!(slice.iter().all(|&b| b == 0xAA));
        }

        // SAFETY: Freeing reallocated memory.
        unsafe {
            gdext_mem_free(realloc_ptr);
        }
    }

    #[test]
    fn test_variant_nil_and_copy() {
        let mut v1 = std::mem::MaybeUninit::<AbiVariant>::uninit();
        let mut v2 = std::mem::MaybeUninit::<AbiVariant>::uninit();

        // SAFETY: Initializing v1 as Nil.
        unsafe {
            gdext_variant_new_nil(v1.as_mut_ptr() as GDExtensionVariantPtr);
        }
        // SAFETY: v1 is initialized with Nil AbiVariant.
        let v1_ref = unsafe { &*v1.as_ptr() };
        assert_eq!(v1_ref.val, IrValue::Nil);

        // SAFETY: Copying v1 into v2.
        unsafe {
            gdext_variant_new_copy(
                v2.as_mut_ptr() as GDExtensionVariantPtr,
                v1.as_ptr() as GDExtensionConstVariantPtr,
            );
        }
        // SAFETY: v2 is initialized via variant_new_copy.
        let v2_ref = unsafe { &*v2.as_ptr() };
        assert_eq!(v2_ref.val, IrValue::Nil);

        // SAFETY: Destroying variants.
        unsafe {
            gdext_variant_destroy(v1.as_mut_ptr() as GDExtensionVariantPtr);
            gdext_variant_destroy(v2.as_mut_ptr() as GDExtensionVariantPtr);
        }
    }

    #[test]
    fn test_variant_bool_conversions() {
        let mut v = std::mem::MaybeUninit::<AbiVariant>::uninit();
        // SAFETY: Initializing variant from bool.
        unsafe {
            gdext_variant_from_bool(v.as_mut_ptr() as GDExtensionVariantPtr, 1);
        }
        // SAFETY: v was initialized with Bool AbiVariant.
        let v_ref = unsafe { &*v.as_ptr() };
        assert_eq!(v_ref.val, IrValue::Bool(true));

        let mut out_b: GDExtensionBool = 0;
        // SAFETY: Extracting bool from variant.
        unsafe {
            gdext_bool_from_variant(&mut out_b, v.as_ptr() as GDExtensionConstVariantPtr);
        }
        assert_eq!(out_b, 1);

        // SAFETY: Destroying variant.
        unsafe {
            gdext_variant_destroy(v.as_mut_ptr() as GDExtensionVariantPtr);
        }
    }

    #[test]
    fn test_variant_int_conversions() {
        let mut v = std::mem::MaybeUninit::<AbiVariant>::uninit();
        // SAFETY: Initializing variant from int.
        unsafe {
            gdext_variant_from_int(v.as_mut_ptr() as GDExtensionVariantPtr, 42069);
        }
        // SAFETY: v was initialized with I64 AbiVariant.
        let v_ref = unsafe { &*v.as_ptr() };
        assert_eq!(v_ref.val, IrValue::I64(42069));

        let mut out_i: GDExtensionInt = 0;
        // SAFETY: Extracting int from variant.
        unsafe {
            gdext_int_from_variant(&mut out_i, v.as_ptr() as GDExtensionConstVariantPtr);
        }
        assert_eq!(out_i, 42069);

        // SAFETY: Destroying variant.
        unsafe {
            gdext_variant_destroy(v.as_mut_ptr() as GDExtensionVariantPtr);
        }
    }

    #[test]
    fn test_variant_float_conversions() {
        let mut v = std::mem::MaybeUninit::<AbiVariant>::uninit();
        // SAFETY: Initializing variant from float.
        unsafe {
            gdext_variant_from_float(v.as_mut_ptr() as GDExtensionVariantPtr, 42.125);
        }
        // SAFETY: v was initialized with F64 AbiVariant.
        let v_ref = unsafe { &*v.as_ptr() };
        assert_eq!(v_ref.val, IrValue::F64(42.125));

        let mut out_f: f64 = 0.0;
        // SAFETY: Extracting float from variant.
        unsafe {
            gdext_float_from_variant(&mut out_f, v.as_ptr() as GDExtensionConstVariantPtr);
        }
        assert!((out_f - 42.125).abs() < 1e-6);

        // SAFETY: Destroying variant.
        unsafe {
            gdext_variant_destroy(v.as_mut_ptr() as GDExtensionVariantPtr);
        }
    }

    #[test]
    fn test_variant_type_and_stringify() {
        let mut v = std::mem::MaybeUninit::<AbiVariant>::uninit();
        // SAFETY: Initializing variant with int.
        unsafe {
            gdext_variant_from_int(v.as_mut_ptr() as GDExtensionVariantPtr, 777);
        }

        // SAFETY: Querying variant type.
        let t = unsafe {
            gdext_variant_get_type(v.as_ptr() as GDExtensionConstVariantPtr)
        };
        assert_eq!(t, GDExtensionVariantType::Int);

        let mut s = std::mem::MaybeUninit::<AbiString>::uninit();
        // SAFETY: Stringifying variant.
        unsafe {
            gdext_variant_stringify(
                v.as_ptr() as GDExtensionConstVariantPtr,
                s.as_mut_ptr() as GDExtensionStringPtr,
            );
        }
        // SAFETY: s was initialized with AbiString.
        let s_ref = unsafe { &*s.as_ptr() };
        assert!(s_ref.text.contains("777"));

        // SAFETY: Destroying variant and string.
        unsafe {
            gdext_variant_destroy(v.as_mut_ptr() as GDExtensionVariantPtr);
            gdext_string_destroy(s.as_mut_ptr() as GDExtensionStringPtr);
        }
    }

    #[test]
    fn test_string_conversions_and_lifecycle() {
        let mut s = std::mem::MaybeUninit::<AbiString>::uninit();
        let text = c"Foreplay Game Engine";
        // SAFETY: Initializing string with utf8 chars.
        unsafe {
            gdext_string_new_with_utf8_chars(s.as_mut_ptr() as GDExtensionUninitializedStringPtr, text.as_ptr());
        }
        // SAFETY: s was initialized with AbiString.
        let s_ref = unsafe { &*s.as_ptr() };
        assert_eq!(s_ref.text, "Foreplay Game Engine");

        let mut buffer = [0u8; 64];
        // SAFETY: Querying utf8 bytes.
        let written = unsafe {
            gdext_string_to_utf8_chars(
                s.as_ptr() as GDExtensionConstStringPtr,
                buffer.as_mut_ptr() as *mut c_char,
                64,
            )
        };
        assert_eq!(written, 20);
        let extracted = std::str::from_utf8(&buffer[..written as usize]).unwrap();
        assert_eq!(extracted, "Foreplay Game Engine");

        // SAFETY: Destroying string.
        unsafe {
            gdext_string_destroy(s.as_mut_ptr() as GDExtensionStringPtr);
        }
    }

    #[test]
    fn test_string_name_lifecycle() {
        let mut sn = std::mem::MaybeUninit::<AbiStringName>::uninit();
        let text = c"SkatePhysicsExtensionNode";
        // SAFETY: Initializing StringName.
        unsafe {
            gdext_string_name_new_with_utf8_chars(
                sn.as_mut_ptr() as GDExtensionUninitializedStringNamePtr,
                text.as_ptr(),
            );
        }
        // SAFETY: sn was initialized with AbiStringName.
        let sn_ref = unsafe { &*sn.as_ptr() };
        assert_eq!(sn_ref.name, "SkatePhysicsExtensionNode");

        // SAFETY: Destroying StringName.
        unsafe {
            gdext_string_name_destroy(sn.as_mut_ptr() as GDExtensionStringNamePtr);
        }
    }

    #[test]
    fn test_array_and_dictionary_c_abi() {
        let mut arr = std::mem::MaybeUninit::<AbiVariant>::uninit();
        // SAFETY: Initializing array variant.
        unsafe {
            gdext_array_new(arr.as_mut_ptr() as GDExtensionTypePtr);
        }
        // SAFETY: Querying array size.
        let size = unsafe {
            gdext_array_size(arr.as_ptr() as GDExtensionConstTypePtr)
        };
        assert_eq!(size, 0);

        let mut dict = std::mem::MaybeUninit::<AbiVariant>::uninit();
        // SAFETY: Initializing dictionary variant.
        unsafe {
            gdext_dictionary_new(dict.as_mut_ptr() as GDExtensionTypePtr);
        }
        // SAFETY: Querying dictionary size.
        let dsize = unsafe {
            gdext_dictionary_size(dict.as_ptr() as GDExtensionConstTypePtr)
        };
        assert_eq!(dsize, 0);

        // SAFETY: Destroying variants.
        unsafe {
            gdext_array_destroy(arr.as_mut_ptr() as GDExtensionTypePtr);
            gdext_dictionary_destroy(dict.as_mut_ptr() as GDExtensionTypePtr);
        }
    }

    #[test]
    fn test_manifest_parse_valid() {
        let raw = r#"
# Sample GDExtension Manifest
[configuration]
entry_symbol = "skate_extension_init"
compatibility_minimum = "4.2"
reloadable = true

[libraries]
macos.debug = "res://bin/libskate.macos.debug.dylib"
macos.release = "res://bin/libskate.macos.release.dylib"
windows.debug.x86_64 = "res://bin/libskate.windows.debug.x86_64.dll"
linux.release.x86_64 = "res://bin/libskate.linux.release.x86_64.so"

[dependencies]
macos.debug = ["res://bin/libphysics.dylib"]
"#;

        let manifest = parse_gdextension_manifest(raw).expect("manifest should parse");
        assert_eq!(manifest.configuration.entry_symbol, "skate_extension_init");
        assert_eq!(manifest.configuration.compatibility_minimum, "4.2");
        assert!(manifest.configuration.reloadable);
        assert_eq!(manifest.libraries.len(), 4);
        assert_eq!(
            manifest.libraries.get("macos.debug").unwrap(),
            "res://bin/libskate.macos.debug.dylib"
        );
        assert_eq!(manifest.dependencies.len(), 1);
    }

    #[test]
    fn test_manifest_missing_entry_symbol() {
        let raw = r#"
[configuration]
compatibility_minimum = "4.2"
"#;
        let res = parse_gdextension_manifest(raw);
        assert!(matches!(res, Err(ManifestError::MissingField(_))));
    }

    #[test]
    fn test_manifest_serialization_roundtrip() {
        let manifest = GdExtensionManifest::new("custom_plugin_entry")
            .with_compatibility_minimum("4.4")
            .with_library("macos.debug", "res://bin/libcustom.dylib")
            .with_dependency("macos.debug", "res://bin/libdep.dylib");

        let serialized = emit_gdextension_manifest(&manifest);
        assert!(serialized.contains("entry_symbol = \"custom_plugin_entry\""));
        assert!(serialized.contains("compatibility_minimum = \"4.4\""));
        assert!(serialized.contains("macos.debug = \"res://bin/libcustom.dylib\""));

        let reparsed = parse_gdextension_manifest(&serialized).expect("reparse must succeed");
        assert_eq!(manifest, reparsed);
    }

    #[test]
    fn test_resolve_library_for_host_platform() {
        let manifest = GdExtensionManifest::new("entry")
            .with_library("macos.debug.arm64", "res://bin/lib_macos_debug_arm64.dylib")
            .with_library("macos.release", "res://bin/lib_macos_release.dylib")
            .with_library("linux.release.x86_64", "res://bin/lib_linux_release.so")
            .with_library("windows.debug.x86_64", "res://bin/lib_win_debug.dll");

        // Exact match on macOS arm64 debug
        let p1 = resolve_library_for_host(&manifest, "macos", "arm64", true);
        assert_eq!(p1.as_deref(), Some("res://bin/lib_macos_debug_arm64.dylib"));

        // Fallback to release when debug arm64 is missing
        let p2 = resolve_library_for_host(&manifest, "darwin", "x86_64", false);
        assert_eq!(p2.as_deref(), Some("res://bin/lib_macos_release.dylib"));

        // Linux exact match
        let p3 = resolve_library_for_host(&manifest, "linux", "amd64", false);
        assert_eq!(p3.as_deref(), Some("res://bin/lib_linux_release.so"));

        // Unknown platform returns None
        let p4 = resolve_library_for_host(&manifest, "android", "arm64", true);
        assert!(p4.is_none());
    }

    #[test]
    fn test_host_initialization_with_builtin_classdb() {
        let host = GdExtensionHost::new();
        // ClassDb must contain genuine Godot classes
        assert!(host.class_db.class_count() >= 265);
        assert!(host.class_db.inherits("CharacterBody3D", "Node3D"));
        assert!(host.class_db.inherits("Node3D", "Node"));
    }

    #[test]
    fn test_end_to_end_c_abi_sample_plugin_roundtrip() {
        let mut host = GdExtensionHost::new();

        // Load sample extension via C-ABI entry function
        host.load_plugin_from_entry("skate_plugin", sample_rust_gdextension_entry)
            .expect("plugin load must succeed");

        // Execute all 4 initialization levels: Core -> Servers -> Scene -> Editor
        host.initialize_all();

        // 1. Verify ClassDb synchronization: SkatePhysicsExtensionNode inherits Node3D
        assert!(
            host.class_db
                .inherits("SkatePhysicsExtensionNode", "Node3D"),
            "SkatePhysicsExtensionNode must inherit Node3D in ClassDb"
        );

        let class_info = host
            .class_db
            .get("SkatePhysicsExtensionNode")
            .expect("class info must be registered in ClassDb");
        assert!(class_info.signals.contains(&"trick_landed".to_string()));

        // 2. Instantiate and invoke C-ABI method: apply_kickflip(impulse: 20.0) -> 20.0 * 1.5 + 100.0 = 130.0
        let instance_id = host
            .instantiate_class("SkatePhysicsExtensionNode")
            .expect("must instantiate extension class");

        let result = host
            .invoke_extension_method(
                "SkatePhysicsExtensionNode",
                instance_id,
                "apply_kickflip",
                &[IrValue::F64(20.0)],
            )
            .expect("method invocation must succeed");

        assert_eq!(result, IrValue::F64(130.0));

        // 3. Deinitialize all levels
        host.deinitialize_all();
    }
}
