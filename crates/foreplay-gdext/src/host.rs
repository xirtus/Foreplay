//! Godot 4.4 `GDExtensionHost` runtime bridge and plugin lifecycle manager.
//!
//! Orchestrates plugin loading, multi-level initialization (`Core -> Servers -> Scene -> Editor`),
//! automatic ClassDB synchronization, and runtime C-ABI method dispatching.

use crate::c_abi::{
    AbiStringName, AbiVariant, GDExtensionBool, GDExtensionCallError, GDExtensionCallErrorType,
    GDExtensionClassCreationInfo3, GDExtensionClassInstancePtr, GDExtensionClassLibraryPtr,
    GDExtensionClassMethodInfo, GDExtensionConstStringNamePtr, GDExtensionConstVariantPtr,
    GDExtensionInitialization, GDExtensionInitializationLevel,
    GDExtensionInterfaceGetProcAddress, GDExtensionInt, GDExtensionObjectPtr,
    GDExtensionPropertyInfo, GDExtensionVariantPtr, GDExtensionVariantType,
    access_registry, gdext_float_from_variant, gdext_get_proc_address, gdext_variant_from_float,
};
use foreplay_core::classdb::{ClassDb, ClassInfo};
use foreplay_ir::IrValue;
use std::collections::HashMap;
use std::ffi::{c_char, c_void};
use thiserror::Error;

/// Errors produced during GDExtension host operations or method dispatches.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum GdExtError {
    #[error("plugin initialization returned false: {0}")]
    PluginInitFailed(String),
    #[error("class not found: {0}")]
    ClassNotFound(String),
    #[error("method not found: {0}")]
    MethodNotFound(String),
    #[error("instance not found: {0}")]
    InstanceNotFound(u64),
    #[error("call error during extension execution: {0}")]
    CallError(String),
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
}

/// Dynamic plugin entry point function pointer matching Godot 4.4 C-ABI.
pub type GDExtensionInitializationFunction = unsafe extern "C" fn(
    p_get_proc_address: GDExtensionInterfaceGetProcAddress,
    p_library: GDExtensionClassLibraryPtr,
    r_initialization: *mut GDExtensionInitialization,
) -> GDExtensionBool;

/// Tracks a loaded GDExtension plugin.
#[derive(Debug)]
pub struct GdExtensionPlugin {
    pub name: String,
    pub initialization: GDExtensionInitialization,
    pub current_level: Option<GDExtensionInitializationLevel>,
}

/// Tracks an instantiated extension object instance.
#[derive(Debug)]
pub struct GdExtensionInstance {
    pub instance_id: u64,
    pub class_name: String,
    pub instance_ptr: GDExtensionClassInstancePtr,
    pub userdata: *mut c_void,
    pub properties: HashMap<String, IrValue>,
}

// SAFETY: Instance pointers are managed by host lifecycle and confined to host execution.
unsafe impl Send for GdExtensionInstance {}
// SAFETY: Instance pointers are managed by host lifecycle and protected by host borrowing.
unsafe impl Sync for GdExtensionInstance {}

/// Central runtime host for Godot 4.4 GDExtensions.
#[derive(Debug)]
pub struct GdExtensionHost {
    pub class_db: ClassDb,
    pub plugins: HashMap<String, GdExtensionPlugin>,
    pub instances: HashMap<u64, GdExtensionInstance>,
    next_instance_id: u64,
}

impl Default for GdExtensionHost {
    fn default() -> Self {
        Self {
            class_db: ClassDb::global_godot44().clone(),
            plugins: HashMap::new(),
            instances: HashMap::new(),
            next_instance_id: 1,
        }
    }
}

impl GdExtensionHost {
    /// Creates a new `GdExtensionHost` pre-populated with canonical Godot 4.4 classes.
    pub fn new() -> Self {
        Self::default()
    }

    /// Loads a plugin directly from an entry point function.
    pub fn load_plugin_from_entry(
        &mut self,
        name: impl Into<String>,
        entry_func: GDExtensionInitializationFunction,
    ) -> Result<(), GdExtError> {
        let name_str = name.into();
        let mut init = GDExtensionInitialization::default();
        let dummy_library = 0x1000_usize as GDExtensionClassLibraryPtr;

        // SAFETY: Calling entry_func with real gdext_get_proc_address and valid pointer to init struct.
        let ok = unsafe { entry_func(gdext_get_proc_address, dummy_library, &mut init) };
        if ok == 0 {
            return Err(GdExtError::PluginInitFailed(name_str));
        }

        self.plugins.insert(
            name_str.clone(),
            GdExtensionPlugin {
                name: name_str,
                initialization: init,
                current_level: None,
            },
        );

        Ok(())
    }

    /// Steps a specific initialization level across all loaded plugins.
    pub fn initialize_level(&mut self, level: GDExtensionInitializationLevel) {
        for plugin in self.plugins.values_mut() {
            if level as u32 >= plugin.initialization.minimum_initialization_level as u32 {
                if let Some(init_fn) = plugin.initialization.initialize {
                    // SAFETY: Invoking plugin initialize callback with provided userdata.
                    unsafe {
                        init_fn(plugin.initialization.userdata, level);
                    }
                }
                plugin.current_level = Some(level);
            }
        }
        self.sync_classes_to_classdb();
    }

    /// Executes full initialization through all four levels in standard order:
    /// `Core -> Servers -> Scene -> Editor`.
    pub fn initialize_all(&mut self) {
        self.initialize_level(GDExtensionInitializationLevel::Core);
        self.initialize_level(GDExtensionInitializationLevel::Servers);
        self.initialize_level(GDExtensionInitializationLevel::Scene);
        self.initialize_level(GDExtensionInitializationLevel::Editor);
    }

    /// Steps a specific deinitialization level across all loaded plugins.
    pub fn deinitialize_level(&mut self, level: GDExtensionInitializationLevel) {
        for plugin in self.plugins.values_mut() {
            if let Some(deinit_fn) = plugin.initialization.deinitialize {
                // SAFETY: Invoking plugin deinitialize callback with provided userdata.
                unsafe {
                    deinit_fn(plugin.initialization.userdata, level);
                }
            }
        }
    }

    /// Executes full deinitialization in reverse order:
    /// `Editor -> Scene -> Servers -> Core`.
    pub fn deinitialize_all(&mut self) {
        self.deinitialize_level(GDExtensionInitializationLevel::Editor);
        self.deinitialize_level(GDExtensionInitializationLevel::Scene);
        self.deinitialize_level(GDExtensionInitializationLevel::Servers);
        self.deinitialize_level(GDExtensionInitializationLevel::Core);
    }

    /// Synchronizes all dynamically registered extension classes into the host's `ClassDb`.
    pub fn sync_classes_to_classdb(&mut self) {
        access_registry(|reg| {
            for class in reg.classes.values() {
                let mut info =
                    ClassInfo::new(&class.name, Some(&class.parent_name), !class.is_abstract);
                for prop_name in class.properties.keys() {
                    info = info.with_property(prop_name, IrValue::Nil);
                }
                for sig_name in class.signals.keys() {
                    info = info.with_signal(sig_name);
                }
                let _ = self.class_db.register_extension_class(info);
            }
        });
    }

    /// Instantiates an extension class and registers it in the instance map.
    pub fn instantiate_class(&mut self, class_name: &str) -> Result<u64, GdExtError> {
        let (create_fn, class_userdata) = access_registry(|reg| {
            reg.classes.get(class_name).map(|c| {
                (
                    c.creation_info.create_instance_func,
                    c.creation_info.class_userdata,
                )
            })
        })
        .ok_or_else(|| GdExtError::ClassNotFound(class_name.to_string()))?;

        let instance_ptr = if let Some(create) = create_fn {
            // SAFETY: Calling extension create_instance_func with class_userdata.
            unsafe { create(class_userdata) }
        } else {
            core::ptr::null_mut()
        };

        let id = self.next_instance_id;
        self.next_instance_id += 1;

        self.instances.insert(
            id,
            GdExtensionInstance {
                instance_id: id,
                class_name: class_name.to_string(),
                instance_ptr,
                userdata: class_userdata,
                properties: HashMap::new(),
            },
        );

        Ok(id)
    }

    /// Invokes a registered GDExtension C-ABI method on an instance.
    pub fn invoke_extension_method(
        &mut self,
        class_name: &str,
        instance_id: u64,
        method_name: &str,
        args: &[IrValue],
    ) -> Result<IrValue, GdExtError> {
        // Ensure instance exists; instantiate if needed.
        if let std::collections::hash_map::Entry::Vacant(e) = self.instances.entry(instance_id) {
            let (create_fn, class_userdata) = access_registry(|reg| {
                reg.classes.get(class_name).map(|c| {
                    (
                        c.creation_info.create_instance_func,
                        c.creation_info.class_userdata,
                    )
                })
            })
            .ok_or_else(|| GdExtError::ClassNotFound(class_name.to_string()))?;

            let instance_ptr = if let Some(create) = create_fn {
                // SAFETY: Calling extension create_instance_func with class_userdata.
                unsafe { create(class_userdata) }
            } else {
                core::ptr::null_mut()
            };

            e.insert(GdExtensionInstance {
                instance_id,
                class_name: class_name.to_string(),
                instance_ptr,
                userdata: class_userdata,
                properties: HashMap::new(),
            });
        }

        let instance = self
            .instances
            .get(&instance_id)
            .ok_or(GdExtError::InstanceNotFound(instance_id))?;
        let instance_ptr = instance.instance_ptr;

        let (call_func, method_userdata) = access_registry(|reg| {
            let class = reg.classes.get(class_name)?;
            let method = class.methods.get(method_name)?;
            Some((method.call_func, method.userdata))
        })
        .ok_or_else(|| GdExtError::MethodNotFound(format!("{class_name}::{method_name}")))?;

        let Some(call) = call_func else {
            return Err(GdExtError::MethodNotFound(format!(
                "method '{method_name}' has no call_func pointer"
            )));
        };

        // Convert arguments to AbiVariant pointers.
        let mut abi_args: Vec<AbiVariant> = args.iter().cloned().map(AbiVariant::new).collect();
        let arg_ptrs: Vec<GDExtensionConstVariantPtr> = abi_args
            .iter_mut()
            .map(|v| v as *mut AbiVariant as GDExtensionConstVariantPtr)
            .collect();

        let mut ret_variant = AbiVariant::new(IrValue::Nil);
        let ret_ptr = &mut ret_variant as *mut AbiVariant as GDExtensionVariantPtr;
        let mut call_error = GDExtensionCallError::default();

        // SAFETY: Calling registered C-ABI method with properly formatted argument array.
        unsafe {
            call(
                method_userdata,
                instance_ptr,
                arg_ptrs.as_ptr(),
                args.len() as GDExtensionInt,
                ret_ptr,
                &mut call_error,
            );
        }

        if call_error.error != GDExtensionCallErrorType::Ok {
            return Err(GdExtError::CallError(format!("{:?}", call_error.error)));
        }

        Ok(ret_variant.val)
    }
}

// ---------------------------------------------------------------------------
// Sample GDExtension Plugin: SkatePhysicsExtensionNode
// ---------------------------------------------------------------------------

struct SamplePluginState {
    library: GDExtensionClassLibraryPtr,
    get_proc_address: GDExtensionInterfaceGetProcAddress,
}

/// Genuine Godot 4.4 GDExtension C-ABI entry point for the sample `SkatePhysicsExtensionNode` plugin.
///
/// # Safety
/// Pointers must be valid per GDExtension initialization ABI.
pub unsafe extern "C" fn sample_rust_gdextension_entry(
    p_get_proc_address: GDExtensionInterfaceGetProcAddress,
    p_library: GDExtensionClassLibraryPtr,
    r_initialization: *mut GDExtensionInitialization,
) -> GDExtensionBool {
    if r_initialization.is_null() {
        return 0;
    }

    let state = Box::new(SamplePluginState {
        library: p_library,
        get_proc_address: p_get_proc_address,
    });

    // SAFETY: r_initialization is non-null and valid for writing GDExtensionInitialization.
    unsafe {
        (*r_initialization).minimum_initialization_level = GDExtensionInitializationLevel::Scene;
        (*r_initialization).userdata = Box::into_raw(state) as *mut c_void;
        (*r_initialization).initialize = Some(sample_initialize);
        (*r_initialization).deinitialize = Some(sample_deinitialize);
    }

    1
}

type FnClassdbRegisterExtensionClass3 = unsafe extern "C" fn(
    p_library: GDExtensionClassLibraryPtr,
    p_class_name: GDExtensionConstStringNamePtr,
    p_parent_class_name: GDExtensionConstStringNamePtr,
    p_extension_funcs: *const GDExtensionClassCreationInfo3,
);

type FnClassdbRegisterExtensionClassMethod = unsafe extern "C" fn(
    p_library: GDExtensionClassLibraryPtr,
    p_class_name: GDExtensionConstStringNamePtr,
    p_method_info: *const GDExtensionClassMethodInfo,
);

type FnClassdbRegisterExtensionClassProperty = unsafe extern "C" fn(
    p_library: GDExtensionClassLibraryPtr,
    p_class_name: GDExtensionConstStringNamePtr,
    p_info: *const GDExtensionPropertyInfo,
    p_setter: GDExtensionConstStringNamePtr,
    p_getter: GDExtensionConstStringNamePtr,
);

type FnClassdbRegisterExtensionClassSignal = unsafe extern "C" fn(
    p_library: GDExtensionClassLibraryPtr,
    p_class_name: GDExtensionConstStringNamePtr,
    p_signal_name: GDExtensionConstStringNamePtr,
    p_args: *const GDExtensionPropertyInfo,
    p_argument_count: GDExtensionInt,
);

type FnStringNameNewWithUtf8Chars =
    unsafe extern "C" fn(r_dest: *mut c_void, p_contents: *const c_char);

unsafe extern "C" fn sample_initialize(
    userdata: *mut c_void,
    level: GDExtensionInitializationLevel,
) {
    if level != GDExtensionInitializationLevel::Scene || userdata.is_null() {
        return;
    }

    // SAFETY: userdata is valid pointer to SamplePluginState.
    let state = unsafe { &*(userdata as *const SamplePluginState) };
    let get_proc = state.get_proc_address;

    // Resolve function pointers via p_get_proc_address
    // SAFETY: Transmuting resolved interface function pointer to FnClassdbRegisterExtensionClass3.
    let fn_reg_class = unsafe {
        let p = get_proc(c"classdb_register_extension_class3".as_ptr());
        std::mem::transmute::<Option<unsafe extern "C" fn()>, Option<FnClassdbRegisterExtensionClass3>>(p)
    };
    // SAFETY: Transmuting resolved interface function pointer to FnClassdbRegisterExtensionClassMethod.
    let fn_reg_method = unsafe {
        let p = get_proc(c"classdb_register_extension_class_method".as_ptr());
        std::mem::transmute::<Option<unsafe extern "C" fn()>, Option<FnClassdbRegisterExtensionClassMethod>>(p)
    };
    // SAFETY: Transmuting resolved interface function pointer to FnClassdbRegisterExtensionClassProperty.
    let fn_reg_prop = unsafe {
        let p = get_proc(c"classdb_register_extension_class_property".as_ptr());
        std::mem::transmute::<Option<unsafe extern "C" fn()>, Option<FnClassdbRegisterExtensionClassProperty>>(p)
    };
    // SAFETY: Transmuting resolved interface function pointer to FnClassdbRegisterExtensionClassSignal.
    let fn_reg_sig = unsafe {
        let p = get_proc(c"classdb_register_extension_class_signal".as_ptr());
        std::mem::transmute::<Option<unsafe extern "C" fn()>, Option<FnClassdbRegisterExtensionClassSignal>>(p)
    };
    // SAFETY: Transmuting resolved interface function pointer to FnStringNameNewWithUtf8Chars.
    let fn_str_name = unsafe {
        let p = get_proc(c"string_name_new_with_utf8_chars".as_ptr());
        std::mem::transmute::<Option<unsafe extern "C" fn()>, Option<FnStringNameNewWithUtf8Chars>>(p)
    };

    let Some(reg_class) = fn_reg_class else { return };
    let Some(reg_method) = fn_reg_method else { return };
    let Some(reg_prop) = fn_reg_prop else { return };
    let Some(reg_sig) = fn_reg_sig else { return };
    let Some(str_name_ctor) = fn_str_name else { return };

    // Register class SkatePhysicsExtensionNode inheriting Node3D
    let mut class_name = std::mem::MaybeUninit::<AbiStringName>::uninit();
    let mut parent_name = std::mem::MaybeUninit::<AbiStringName>::uninit();
    // SAFETY: Constructing StringNames for class and parent.
    unsafe {
        str_name_ctor(class_name.as_mut_ptr() as *mut c_void, c"SkatePhysicsExtensionNode".as_ptr());
        str_name_ctor(parent_name.as_mut_ptr() as *mut c_void, c"Node3D".as_ptr());
    }

    let creation_info = GDExtensionClassCreationInfo3 {
        is_virtual: 0,
        is_abstract: 0,
        is_exposed: 1,
        is_runtime: 1,
        create_instance_func: Some(sample_create_instance),
        free_instance_func: Some(sample_free_instance),
        ..Default::default()
    };

    // SAFETY: Calling classdb_register_extension_class3 with valid pointers.
    unsafe {
        reg_class(
            state.library,
            class_name.as_ptr() as GDExtensionConstStringNamePtr,
            parent_name.as_ptr() as GDExtensionConstStringNamePtr,
            &creation_info,
        );
    }

    // Register property 'friction'
    let mut prop_name = std::mem::MaybeUninit::<AbiStringName>::uninit();
    let mut setter_name = std::mem::MaybeUninit::<AbiStringName>::uninit();
    let mut getter_name = std::mem::MaybeUninit::<AbiStringName>::uninit();
    // SAFETY: Constructing StringNames for property registration.
    unsafe {
        str_name_ctor(prop_name.as_mut_ptr() as *mut c_void, c"friction".as_ptr());
        str_name_ctor(setter_name.as_mut_ptr() as *mut c_void, c"set_friction".as_ptr());
        str_name_ctor(getter_name.as_mut_ptr() as *mut c_void, c"get_friction".as_ptr());
    }
    let prop_info = GDExtensionPropertyInfo {
        r#type: GDExtensionVariantType::Float,
        name: prop_name.as_mut_ptr() as *mut c_void,
        class_name: class_name.as_mut_ptr() as *mut c_void,
        hint: 0,
        hint_string: core::ptr::null_mut(),
        usage: 7,
    };
    // SAFETY: Calling classdb_register_extension_class_property.
    unsafe {
        reg_prop(
            state.library,
            class_name.as_ptr() as GDExtensionConstStringNamePtr,
            &prop_info,
            setter_name.as_ptr() as GDExtensionConstStringNamePtr,
            getter_name.as_ptr() as GDExtensionConstStringNamePtr,
        );
    }

    // Register signal 'trick_landed'
    let mut sig_name = std::mem::MaybeUninit::<AbiStringName>::uninit();
    let mut arg_name = std::mem::MaybeUninit::<AbiStringName>::uninit();
    // SAFETY: Constructing StringNames for signal registration.
    unsafe {
        str_name_ctor(sig_name.as_mut_ptr() as *mut c_void, c"trick_landed".as_ptr());
        str_name_ctor(arg_name.as_mut_ptr() as *mut c_void, c"points".as_ptr());
    }
    let sig_arg = GDExtensionPropertyInfo {
        r#type: GDExtensionVariantType::Float,
        name: arg_name.as_mut_ptr() as *mut c_void,
        ..Default::default()
    };
    // SAFETY: Calling classdb_register_extension_class_signal.
    unsafe {
        reg_sig(
            state.library,
            class_name.as_ptr() as GDExtensionConstStringNamePtr,
            sig_name.as_ptr() as GDExtensionConstStringNamePtr,
            &sig_arg,
            1,
        );
    }

    // Register callable C-ABI method 'apply_kickflip(impulse: f64) -> f64'
    let mut method_name = std::mem::MaybeUninit::<AbiStringName>::uninit();
    // SAFETY: Constructing StringName for method.
    unsafe {
        str_name_ctor(method_name.as_mut_ptr() as *mut c_void, c"apply_kickflip".as_ptr());
    }
    let method_info = GDExtensionClassMethodInfo {
        name: method_name.as_mut_ptr() as *mut c_void,
        method_userdata: 0x42_usize as *mut c_void,
        call_func: Some(sample_apply_kickflip),
        ptrcall_func: None,
        method_flags: 1,
        has_return_value: 1,
        argument_count: 1,
        ..Default::default()
    };
    // SAFETY: Calling classdb_register_extension_class_method.
    unsafe {
        reg_method(
            state.library,
            class_name.as_ptr() as GDExtensionConstStringNamePtr,
            &method_info,
        );
    }
}

/// # Safety
/// Method invocation handler following C-ABI method call conventions.
pub unsafe extern "C" fn sample_apply_kickflip(
    _method_userdata: *mut c_void,
    _p_instance: GDExtensionClassInstancePtr,
    p_args: *const GDExtensionConstVariantPtr,
    p_argument_count: GDExtensionInt,
    r_return: GDExtensionVariantPtr,
    r_error: *mut GDExtensionCallError,
) {
    if !r_error.is_null() {
        // SAFETY: Initializing error status to Ok.
        unsafe {
            *r_error = GDExtensionCallError::default();
        }
    }
    if p_argument_count < 1 || p_args.is_null() {
        if !r_error.is_null() {
            // SAFETY: Reporting missing argument.
            unsafe {
                (*r_error).error = GDExtensionCallErrorType::TooFewArguments;
            }
        }
        return;
    }
    // SAFETY: Dereferencing first argument pointer.
    let arg0_ptr = unsafe { *p_args };
    let mut impulse = 0.0f64;
    // SAFETY: Extracting float from variant.
    unsafe {
        gdext_float_from_variant(&mut impulse, arg0_ptr);
    }

    let points = impulse * 1.5 + 100.0;

    if !r_return.is_null() {
        // SAFETY: Writing float result to return variant.
        unsafe {
            gdext_variant_from_float(r_return, points);
        }
    }
}

/// # Safety
/// Creates dummy instance pointer.
pub unsafe extern "C" fn sample_create_instance(_class_userdata: *mut c_void) -> GDExtensionObjectPtr {
    let dummy = Box::new(42u64);
    Box::into_raw(dummy) as GDExtensionObjectPtr
}

/// # Safety
/// Frees dummy instance pointer.
pub unsafe extern "C" fn sample_free_instance(
    _class_userdata: *mut c_void,
    p_instance: GDExtensionClassInstancePtr,
) {
    if !p_instance.is_null() {
        // SAFETY: Dropping boxed instance.
        unsafe {
            let _ = Box::from_raw(p_instance as *mut u64);
        }
    }
}

/// # Safety
/// Cleans up plugin state.
pub unsafe extern "C" fn sample_deinitialize(
    userdata: *mut c_void,
    level: GDExtensionInitializationLevel,
) {
    if level == GDExtensionInitializationLevel::Scene && !userdata.is_null() {
        // SAFETY: Freeing SamplePluginState.
        unsafe {
            let _ = Box::from_raw(userdata as *mut SamplePluginState);
        }
    }
}
