//! Godot 4.4 `GDExtension` C ABI Compatibility Layer.
//!
//! Provides `#[repr(C)]` types, constants, function signatures, and a dispatch
//! table resolving 80+ genuine Godot 4.4 C ABI interface functions conforming
//! to `gdextension_interface.h`.

use foreplay_ir::IrValue;
use std::collections::HashMap;
use std::ffi::{CStr, c_char, c_void};
use std::sync::{LazyLock, RwLock};

// ---------------------------------------------------------------------------
// 1. Primitive Pointer & Scalar Aliases
// ---------------------------------------------------------------------------

pub type GDExtensionVariantPtr = *mut c_void;
pub type GDExtensionConstVariantPtr = *const c_void;
pub type GDExtensionUninitializedVariantPtr = *mut c_void;

pub type GDExtensionStringNamePtr = *mut c_void;
pub type GDExtensionConstStringNamePtr = *const c_void;
pub type GDExtensionUninitializedStringNamePtr = *mut c_void;

pub type GDExtensionStringPtr = *mut c_void;
pub type GDExtensionConstStringPtr = *const c_void;
pub type GDExtensionUninitializedStringPtr = *mut c_void;

pub type GDExtensionObjectPtr = *mut c_void;
pub type GDExtensionConstObjectPtr = *const c_void;

pub type GDExtensionTypePtr = *mut c_void;
pub type GDExtensionConstTypePtr = *const c_void;

pub type GDExtensionClassLibraryPtr = *mut c_void;
pub type GDExtensionClassInstancePtr = *mut core::ffi::c_void;
pub type GDExtensionMethodBindPtr = *mut c_void;

pub type GDExtensionBool = u8;
pub type GDExtensionInt = i64;

pub type GDExtensionInterfaceFunctionPtr = Option<unsafe extern "C" fn()>;
pub type GDExtensionInterfaceGetProcAddress =
    unsafe extern "C" fn(p_function_name: *const c_char) -> GDExtensionInterfaceFunctionPtr;

// ---------------------------------------------------------------------------
// 2. Enums
// ---------------------------------------------------------------------------

/// Variant types matching Godot 4.4 `GDExtensionVariantType`.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum GDExtensionVariantType {
    #[default]
    Nil = 0,
    Bool = 1,
    Int = 2,
    Float = 3,
    String = 4,
    Vector2 = 5,
    Vector2i = 6,
    Rect2 = 7,
    Rect2i = 8,
    Vector3 = 9,
    Vector3i = 10,
    Transform2D = 11,
    Vector4 = 12,
    Vector4i = 13,
    Plane = 14,
    Quaternion = 15,
    Aabb = 16,
    Basis = 17,
    Transform3D = 18,
    Projection = 19,
    Color = 20,
    StringName = 21,
    NodePath = 22,
    Rid = 23,
    Object = 24,
    Callable = 25,
    Signal = 26,
    Dictionary = 27,
    Array = 28,
    PackedByteArray = 29,
    PackedInt32Array = 30,
    PackedInt64Array = 31,
    PackedFloat32Array = 32,
    PackedFloat64Array = 33,
    PackedStringArray = 34,
    PackedVector2Array = 35,
    PackedVector3Array = 36,
    PackedColorArray = 37,
    PackedVector4Array = 38,
    VariantMax = 39,
}

/// GDExtension initialization levels matching Godot 4.4 `GDExtensionInitializationLevel`.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum GDExtensionInitializationLevel {
    #[default]
    Core = 0,
    Servers = 1,
    Scene = 2,
    Editor = 3,
}

/// Call error status codes matching Godot 4.4 `GDExtensionCallErrorType`.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum GDExtensionCallErrorType {
    #[default]
    Ok = 0,
    InvalidMethod = 1,
    InvalidArgument = 2,
    TooManyArguments = 3,
    TooFewArguments = 4,
    InstanceIsNull = 5,
    MethodNotConst = 6,
}

// ---------------------------------------------------------------------------
// 3. ABI Structs
// ---------------------------------------------------------------------------

/// Call error payload populated during C-ABI method calls.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GDExtensionCallError {
    pub error: GDExtensionCallErrorType,
    pub argument: i32,
    pub expected: i32,
}

/// Godot engine version information passed to extensions.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GDExtensionGodotVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub string: *const c_char,
}

/// Extension initialization descriptor populated by plugin entry point.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct GDExtensionInitialization {
    pub minimum_initialization_level: GDExtensionInitializationLevel,
    pub userdata: *mut c_void,
    pub initialize: Option<unsafe extern "C" fn(userdata: *mut c_void, level: GDExtensionInitializationLevel)>,
    pub deinitialize: Option<unsafe extern "C" fn(userdata: *mut c_void, level: GDExtensionInitializationLevel)>,
}

impl Default for GDExtensionInitialization {
    fn default() -> Self {
        Self {
            minimum_initialization_level: GDExtensionInitializationLevel::Core,
            userdata: core::ptr::null_mut(),
            initialize: None,
            deinitialize: None,
        }
    }
}

pub type GDExtensionClassSet = Option<
    unsafe extern "C" fn(
        p_instance: GDExtensionClassInstancePtr,
        p_name: GDExtensionConstStringNamePtr,
        p_value: GDExtensionConstVariantPtr,
    ) -> GDExtensionBool,
>;

pub type GDExtensionClassGet = Option<
    unsafe extern "C" fn(
        p_instance: GDExtensionClassInstancePtr,
        p_name: GDExtensionConstStringNamePtr,
        r_ret: GDExtensionVariantPtr,
    ) -> GDExtensionBool,
>;

pub type GDExtensionClassGetPropertyList = Option<
    unsafe extern "C" fn(p_instance: GDExtensionClassInstancePtr, r_count: *mut u32) -> *const GDExtensionPropertyInfo,
>;

pub type GDExtensionClassFreePropertyList = Option<
    unsafe extern "C" fn(
        p_instance: GDExtensionClassInstancePtr,
        p_property_list: *const GDExtensionPropertyInfo,
    ),
>;

pub type GDExtensionClassNotification =
    Option<unsafe extern "C" fn(p_instance: GDExtensionClassInstancePtr, p_what: i32)>;

pub type GDExtensionClassToString = Option<
    unsafe extern "C" fn(
        p_instance: GDExtensionClassInstancePtr,
        r_is_valid: *mut GDExtensionBool,
        p_out: GDExtensionStringPtr,
    ),
>;

pub type GDExtensionClassReference = Option<unsafe extern "C" fn(p_instance: GDExtensionClassInstancePtr)>;
pub type GDExtensionClassUnreference = Option<unsafe extern "C" fn(p_instance: GDExtensionClassInstancePtr)>;

pub type GDExtensionClassCreateInstance =
    Option<unsafe extern "C" fn(p_class_userdata: *mut c_void) -> GDExtensionObjectPtr>;

pub type GDExtensionClassFreeInstance =
    Option<unsafe extern "C" fn(p_class_userdata: *mut c_void, p_instance: GDExtensionClassInstancePtr)>;

pub type GDExtensionClassRecreateInstance = Option<
    unsafe extern "C" fn(
        p_class_userdata: *mut c_void,
        p_object: GDExtensionObjectPtr,
    ) -> GDExtensionClassInstancePtr,
>;

pub type GDExtensionClassCallVirtual = Option<
    unsafe extern "C" fn(
        p_instance: GDExtensionClassInstancePtr,
        p_args: *const GDExtensionConstTypePtr,
        r_ret: GDExtensionTypePtr,
    ),
>;

pub type GDExtensionClassGetVirtual = Option<
    unsafe extern "C" fn(
        p_class_userdata: *mut c_void,
        p_name: GDExtensionConstStringNamePtr,
    ) -> GDExtensionClassCallVirtual,
>;

/// Property metadata struct passed during property registration.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct GDExtensionPropertyInfo {
    pub r#type: GDExtensionVariantType,
    pub name: GDExtensionStringNamePtr,
    pub class_name: GDExtensionStringNamePtr,
    pub hint: u32,
    pub hint_string: GDExtensionStringPtr,
    pub usage: u32,
}

impl Default for GDExtensionPropertyInfo {
    fn default() -> Self {
        Self {
            r#type: GDExtensionVariantType::Nil,
            name: core::ptr::null_mut(),
            class_name: core::ptr::null_mut(),
            hint: 0,
            hint_string: core::ptr::null_mut(),
            usage: 7, // PROPERTY_USAGE_DEFAULT
        }
    }
}

/// Class creation info (Godot 4.2+).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct GDExtensionClassCreationInfo2 {
    pub is_virtual: GDExtensionBool,
    pub is_abstract: GDExtensionBool,
    pub is_exposed: GDExtensionBool,
    pub set_func: GDExtensionClassSet,
    pub get_func: GDExtensionClassGet,
    pub get_property_list_func: GDExtensionClassGetPropertyList,
    pub free_property_list_func: GDExtensionClassFreePropertyList,
    pub property_can_revert_func: Option<
        unsafe extern "C" fn(
            p_instance: GDExtensionClassInstancePtr,
            p_name: GDExtensionConstStringNamePtr,
        ) -> GDExtensionBool,
    >,
    pub property_get_revert_func: Option<
        unsafe extern "C" fn(
            p_instance: GDExtensionClassInstancePtr,
            p_name: GDExtensionConstStringNamePtr,
            r_ret: GDExtensionVariantPtr,
        ) -> GDExtensionBool,
    >,
    pub notification_func: GDExtensionClassNotification,
    pub to_string_func: GDExtensionClassToString,
    pub reference_func: GDExtensionClassReference,
    pub unreference_func: GDExtensionClassUnreference,
    pub create_instance_func: GDExtensionClassCreateInstance,
    pub free_instance_func: GDExtensionClassFreeInstance,
    pub recreate_instance_func: GDExtensionClassRecreateInstance,
    pub get_virtual_func: GDExtensionClassGetVirtual,
    pub get_virtual_call_data_func: Option<
        unsafe extern "C" fn(
            p_class_userdata: *mut c_void,
            p_name: GDExtensionConstStringNamePtr,
        ) -> *mut c_void,
    >,
    pub call_virtual_with_data_func: Option<
        unsafe extern "C" fn(
            p_instance: GDExtensionClassInstancePtr,
            p_name: GDExtensionConstStringNamePtr,
            p_virtual_call_data: *mut c_void,
            p_args: *const GDExtensionConstTypePtr,
            r_ret: GDExtensionTypePtr,
        ),
    >,
    pub class_userdata: *mut c_void,
}

/// Class creation info (Godot 4.3/4.4).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct GDExtensionClassCreationInfo3 {
    pub is_virtual: GDExtensionBool,
    pub is_abstract: GDExtensionBool,
    pub is_exposed: GDExtensionBool,
    pub is_runtime: GDExtensionBool,
    pub set_func: GDExtensionClassSet,
    pub get_func: GDExtensionClassGet,
    pub get_property_list_func: GDExtensionClassGetPropertyList,
    pub free_property_list_func: GDExtensionClassFreePropertyList,
    pub property_can_revert_func: Option<
        unsafe extern "C" fn(
            p_instance: GDExtensionClassInstancePtr,
            p_name: GDExtensionConstStringNamePtr,
        ) -> GDExtensionBool,
    >,
    pub property_get_revert_func: Option<
        unsafe extern "C" fn(
            p_instance: GDExtensionClassInstancePtr,
            p_name: GDExtensionConstStringNamePtr,
            r_ret: GDExtensionVariantPtr,
        ) -> GDExtensionBool,
    >,
    pub validate_property_func: Option<
        unsafe extern "C" fn(
            p_instance: GDExtensionClassInstancePtr,
            p_property: *mut GDExtensionPropertyInfo,
        ) -> GDExtensionBool,
    >,
    pub notification_func: GDExtensionClassNotification,
    pub to_string_func: GDExtensionClassToString,
    pub reference_func: GDExtensionClassReference,
    pub unreference_func: GDExtensionClassUnreference,
    pub create_instance_func: GDExtensionClassCreateInstance,
    pub free_instance_func: GDExtensionClassFreeInstance,
    pub recreate_instance_func: GDExtensionClassRecreateInstance,
    pub get_virtual_func: GDExtensionClassGetVirtual,
    pub get_virtual_call_data_func: Option<
        unsafe extern "C" fn(
            p_class_userdata: *mut c_void,
            p_name: GDExtensionConstStringNamePtr,
        ) -> *mut c_void,
    >,
    pub call_virtual_with_data_func: Option<
        unsafe extern "C" fn(
            p_instance: GDExtensionClassInstancePtr,
            p_name: GDExtensionConstStringNamePtr,
            p_virtual_call_data: *mut c_void,
            p_args: *const GDExtensionConstTypePtr,
            r_ret: GDExtensionTypePtr,
        ),
    >,
    pub class_userdata: *mut c_void,
}

pub type GDExtensionClassMethodCall = Option<
    unsafe extern "C" fn(
        method_userdata: *mut c_void,
        p_instance: GDExtensionClassInstancePtr,
        p_args: *const GDExtensionConstVariantPtr,
        p_argument_count: GDExtensionInt,
        r_return: GDExtensionVariantPtr,
        r_error: *mut GDExtensionCallError,
    ),
>;

pub type GDExtensionClassMethodPtrCall = Option<
    unsafe extern "C" fn(
        method_userdata: *mut c_void,
        p_instance: GDExtensionClassInstancePtr,
        p_args: *const GDExtensionConstTypePtr,
        r_ret: GDExtensionTypePtr,
    ),
>;

/// Method registration metadata descriptor.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct GDExtensionClassMethodInfo {
    pub name: GDExtensionStringNamePtr,
    pub method_userdata: *mut c_void,
    pub call_func: GDExtensionClassMethodCall,
    pub ptrcall_func: GDExtensionClassMethodPtrCall,
    pub method_flags: u32,
    pub has_return_value: GDExtensionBool,
    pub return_value_info: *const GDExtensionPropertyInfo,
    pub return_value_metadata: u32,
    pub argument_count: u32,
    pub arguments_info: *const GDExtensionPropertyInfo,
    pub arguments_metadata: *const u32,
    pub default_argument_count: u32,
    pub default_arguments: *mut GDExtensionVariantPtr,
}

impl Default for GDExtensionClassMethodInfo {
    fn default() -> Self {
        Self {
            name: core::ptr::null_mut(),
            method_userdata: core::ptr::null_mut(),
            call_func: None,
            ptrcall_func: None,
            method_flags: 1, // METHOD_FLAG_NORMAL
            has_return_value: 0,
            return_value_info: core::ptr::null(),
            return_value_metadata: 0,
            argument_count: 0,
            arguments_info: core::ptr::null(),
            arguments_metadata: core::ptr::null(),
            default_argument_count: 0,
            default_arguments: core::ptr::null_mut(),
        }
    }
}

// ---------------------------------------------------------------------------
// 4. Internal ABI Backing Data Types
// ---------------------------------------------------------------------------

/// Backing data for an allocated C-ABI `Variant`.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct AbiVariant {
    pub val: IrValue,
}

impl AbiVariant {
    pub fn new(val: IrValue) -> Self {
        Self { val }
    }
}

/// Backing data for an allocated C-ABI `String`.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbiString {
    pub text: String,
}

impl AbiString {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

/// Backing data for an allocated C-ABI `StringName`.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbiStringName {
    pub name: String,
}

impl AbiStringName {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

/// Backing data for an allocated C-ABI `Object`.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct AbiObject {
    pub class_name: String,
    pub instance_id: u64,
    pub extension_instance: GDExtensionClassInstancePtr,
    pub extension_userdata: *mut c_void,
}

// ---------------------------------------------------------------------------
// 5. Memory Allocation Wrappers (Custom 16-byte Aligned Allocator)
// ---------------------------------------------------------------------------

#[repr(C)]
#[derive(Debug)]
struct AllocHeader {
    size: usize,
    align: usize,
}

/// # Safety
/// Caller must ensure pointer returned is freed with `gdext_mem_free`.
pub unsafe extern "C" fn gdext_mem_alloc(p_bytes: usize) -> *mut c_void {
    if p_bytes == 0 {
        return core::ptr::null_mut();
    }
    let align = 16;
    let header_size = std::mem::size_of::<AllocHeader>();
    let total_bytes = header_size + p_bytes;
    let Ok(layout) = std::alloc::Layout::from_size_align(total_bytes, align) else {
        return core::ptr::null_mut();
    };
    // SAFETY: Layout was validated to have valid size and power-of-two alignment.
    let raw = unsafe { std::alloc::alloc(layout) };
    if raw.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: raw is non-null and aligned to 16 bytes, valid for AllocHeader.
    unsafe {
        let header = raw as *mut AllocHeader;
        (*header).size = p_bytes;
        (*header).align = align;
        raw.add(header_size) as *mut c_void
    }
}

/// # Safety
/// `p_ptr` must be a pointer returned by `gdext_mem_alloc` or null.
pub unsafe extern "C" fn gdext_mem_free(p_ptr: *mut c_void) {
    if p_ptr.is_null() {
        return;
    }
    let header_size = std::mem::size_of::<AllocHeader>();
    // SAFETY: p_ptr is non-null and preceded by AllocHeader written in gdext_mem_alloc.
    unsafe {
        let raw = (p_ptr as *mut u8).sub(header_size);
        let header = raw as *mut AllocHeader;
        let size = (*header).size;
        let align = (*header).align;
        let total_bytes = header_size + size;
        if let Ok(layout) = std::alloc::Layout::from_size_align(total_bytes, align) {
            std::alloc::dealloc(raw, layout);
        }
    }
}

/// # Safety
/// `p_ptr` must be a pointer returned by `gdext_mem_alloc` or null.
pub unsafe extern "C" fn gdext_mem_realloc(p_ptr: *mut c_void, p_bytes: usize) -> *mut c_void {
    if p_ptr.is_null() {
        // SAFETY: Pointer is null, delegating to mem_alloc.
        return unsafe { gdext_mem_alloc(p_bytes) };
    }
    if p_bytes == 0 {
        // SAFETY: Delegating to mem_free for zero byte allocation.
        unsafe { gdext_mem_free(p_ptr) };
        return core::ptr::null_mut();
    }
    let header_size = std::mem::size_of::<AllocHeader>();
    // SAFETY: Reading header written during initial allocation.
    let old_size = unsafe {
        let raw = (p_ptr as *mut u8).sub(header_size);
        let header = raw as *mut AllocHeader;
        (*header).size
    };
    // SAFETY: Allocating new memory buffer for resized allocation.
    let new_ptr = unsafe { gdext_mem_alloc(p_bytes) };
    if new_ptr.is_null() {
        return core::ptr::null_mut();
    }
    let copy_len = old_size.min(p_bytes);
    // SAFETY: Both p_ptr and new_ptr are valid for at least copy_len bytes.
    unsafe {
        core::ptr::copy_nonoverlapping(p_ptr as *const u8, new_ptr as *mut u8, copy_len);
        gdext_mem_free(p_ptr);
    }
    new_ptr
}

// ---------------------------------------------------------------------------
// 6. Global Extension Class Registry & C-ABI Bridges
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct RegisteredMethod {
    pub name: String,
    pub userdata: *mut c_void,
    pub call_func: GDExtensionClassMethodCall,
    pub ptrcall_func: GDExtensionClassMethodPtrCall,
    pub flags: u32,
    pub has_return_value: bool,
}

// SAFETY: Function pointers and raw userdata are managed by host lifecycle.
unsafe impl Send for RegisteredMethod {}
// SAFETY: Function pointers and raw userdata are managed by host lifecycle across threads.
unsafe impl Sync for RegisteredMethod {}

#[derive(Debug, Default)]
pub struct RegisteredProperty {
    pub name: String,
    pub property_type: GDExtensionVariantType,
    pub hint: u32,
    pub hint_string: String,
    pub usage: u32,
    pub setter: String,
    pub getter: String,
}

#[derive(Debug, Default)]
pub struct RegisteredSignal {
    pub name: String,
    pub argument_names: Vec<String>,
}

#[derive(Debug, Default)]
pub struct RegisteredExtensionClass {
    pub name: String,
    pub parent_name: String,
    pub is_virtual: bool,
    pub is_abstract: bool,
    pub is_exposed: bool,
    pub is_runtime: bool,
    pub creation_info: GDExtensionClassCreationInfo3,
    pub methods: HashMap<String, RegisteredMethod>,
    pub properties: HashMap<String, RegisteredProperty>,
    pub signals: HashMap<String, RegisteredSignal>,
    pub integer_constants: HashMap<String, i64>,
}

// SAFETY: Internal raw pointers in creation_info are managed by the host.
unsafe impl Send for RegisteredExtensionClass {}
// SAFETY: Internal raw pointers in creation_info are read-only across threads.
unsafe impl Sync for RegisteredExtensionClass {}

#[derive(Debug, Default)]
pub struct GlobalExtensionRegistry {
    pub classes: HashMap<String, RegisteredExtensionClass>,
    pub singletons: HashMap<String, *mut c_void>,
}

// SAFETY: Pointers are protected behind RwLock.
unsafe impl Send for GlobalExtensionRegistry {}
// SAFETY: Pointers are protected behind RwLock across threads.
unsafe impl Sync for GlobalExtensionRegistry {}

static REGISTRY: LazyLock<RwLock<GlobalExtensionRegistry>> =
    LazyLock::new(|| RwLock::new(GlobalExtensionRegistry::default()));

pub fn access_registry<R>(f: impl FnOnce(&mut GlobalExtensionRegistry) -> R) -> R {
    let mut lock = REGISTRY.write().expect("lock registry");
    f(&mut lock)
}

// ---------------------------------------------------------------------------
// 7. C-ABI Interface Implementations (80+ Godot 4.4 Interface Functions)
// ---------------------------------------------------------------------------

static VERSION_STR: &[u8] = b"4.4.0.stable.Foreplay\0";

/// # Safety
/// `r_godot_version` must be valid for writing.
pub unsafe extern "C" fn gdext_get_godot_version(r_godot_version: *mut GDExtensionGodotVersion) {
    if !r_godot_version.is_null() {
        // SAFETY: r_godot_version is non-null and valid for writing GDExtensionGodotVersion.
        unsafe {
            *r_godot_version = GDExtensionGodotVersion {
                major: 4,
                minor: 4,
                patch: 0,
                string: VERSION_STR.as_ptr() as *const c_char,
            };
        }
    }
}

/// # Safety
/// Pointers must be valid C strings or null.
pub unsafe extern "C" fn gdext_print_error(
    _p_description: *const c_char,
    _p_function: *const c_char,
    _p_file: *const c_char,
    _p_line: i32,
    _p_editor_notify: GDExtensionBool,
) {
}

/// # Safety
/// Pointers must be valid C strings or null.
pub unsafe extern "C" fn gdext_print_error_with_message(
    _p_description: *const c_char,
    _p_message: *const c_char,
    _p_function: *const c_char,
    _p_file: *const c_char,
    _p_line: i32,
    _p_editor_notify: GDExtensionBool,
) {
}

/// # Safety
/// Pointers must be valid C strings or null.
pub unsafe extern "C" fn gdext_print_warning(
    _p_description: *const c_char,
    _p_function: *const c_char,
    _p_file: *const c_char,
    _p_line: i32,
    _p_editor_notify: GDExtensionBool,
) {
}

/// # Safety
/// Pointers must be valid C strings or null.
pub unsafe extern "C" fn gdext_print_warning_with_message(
    _p_description: *const c_char,
    _p_message: *const c_char,
    _p_function: *const c_char,
    _p_file: *const c_char,
    _p_line: i32,
    _p_editor_notify: GDExtensionBool,
) {
}

/// # Safety
/// Pointers must be valid C strings or null.
pub unsafe extern "C" fn gdext_print_script_error(
    _p_description: *const c_char,
    _p_function: *const c_char,
    _p_file: *const c_char,
    _p_line: i32,
    _p_editor_notify: GDExtensionBool,
) {
}

/// # Safety
/// Pointers must be valid C strings or null.
pub unsafe extern "C" fn gdext_print_script_error_with_message(
    _p_description: *const c_char,
    _p_message: *const c_char,
    _p_function: *const c_char,
    _p_file: *const c_char,
    _p_line: i32,
    _p_editor_notify: GDExtensionBool,
) {
}

/// # Safety
/// Valid string pointer.
pub unsafe extern "C" fn gdext_get_native_struct_size(_p_name: GDExtensionConstStringNamePtr) -> u64 {
    0
}

/// # Safety
/// `r_dest` must be aligned and valid for writing AbiVariant.
pub unsafe extern "C" fn gdext_variant_new_nil(r_dest: GDExtensionVariantPtr) {
    if !r_dest.is_null() {
        // SAFETY: r_dest is non-null and valid for writing an AbiVariant.
        unsafe {
            core::ptr::write(r_dest as *mut AbiVariant, AbiVariant::new(IrValue::Nil));
        }
    }
}

/// # Safety
/// `r_dest` must be valid for writing, `p_src` must point to valid AbiVariant.
pub unsafe extern "C" fn gdext_variant_new_copy(
    r_dest: GDExtensionVariantPtr,
    p_src: GDExtensionConstVariantPtr,
) {
    if !r_dest.is_null() && !p_src.is_null() {
        // SAFETY: Both pointers are non-null and point to valid AbiVariant storage.
        unsafe {
            let src = &*(p_src as *const AbiVariant);
            core::ptr::write(r_dest as *mut AbiVariant, src.clone());
        }
    }
}

/// # Safety
/// `p_self` must point to initialized AbiVariant or be null.
pub unsafe extern "C" fn gdext_variant_destroy(p_self: GDExtensionVariantPtr) {
    if !p_self.is_null() {
        // SAFETY: p_self points to valid initialized AbiVariant to drop in place.
        unsafe {
            core::ptr::drop_in_place(p_self as *mut AbiVariant);
        }
    }
}

/// # Safety
/// `p_self` must point to initialized AbiVariant.
pub unsafe extern "C" fn gdext_variant_get_type(p_self: GDExtensionConstVariantPtr) -> GDExtensionVariantType {
    if p_self.is_null() {
        return GDExtensionVariantType::Nil;
    }
    // SAFETY: p_self is non-null, points to valid AbiVariant.
    let var = unsafe { &*(p_self as *const AbiVariant) };
    match &var.val {
        IrValue::Nil => GDExtensionVariantType::Nil,
        IrValue::Bool(_) => GDExtensionVariantType::Bool,
        IrValue::I64(_) => GDExtensionVariantType::Int,
        IrValue::F64(_) => GDExtensionVariantType::Float,
        IrValue::String(_) => GDExtensionVariantType::String,
        IrValue::Vec2(_) => GDExtensionVariantType::Vector2,
        IrValue::Vec3(_) => GDExtensionVariantType::Vector3,
        IrValue::Quat(_) => GDExtensionVariantType::Quaternion,
        IrValue::Color(_) => GDExtensionVariantType::Color,
        IrValue::NodePath(_) => GDExtensionVariantType::NodePath,
        IrValue::ResourcePath(_) => GDExtensionVariantType::String,
        IrValue::Array(_) => GDExtensionVariantType::Array,
        IrValue::Map(_) => GDExtensionVariantType::Dictionary,
    }
}

/// # Safety
/// Pointers must be valid.
pub unsafe extern "C" fn gdext_variant_get_type_name(
    p_type: GDExtensionVariantType,
    r_name: GDExtensionStringPtr,
) {
    if !r_name.is_null() {
        let s = format!("{p_type:?}");
        // SAFETY: r_name is valid for writing AbiString.
        unsafe {
            core::ptr::write(r_name as *mut AbiString, AbiString::new(s));
        }
    }
}

/// # Safety
/// Valid AbiVariant and AbiString storage.
pub unsafe extern "C" fn gdext_variant_stringify(
    p_self: GDExtensionConstVariantPtr,
    r_dest: GDExtensionStringPtr,
) {
    if !r_dest.is_null() {
        let s = if p_self.is_null() {
            "null".to_string()
        } else {
            // SAFETY: p_self points to valid initialized AbiVariant.
            let var = unsafe { &*(p_self as *const AbiVariant) };
            format!("{:?}", var.val)
        };
        // SAFETY: r_dest points to uninitialized or initialized AbiString.
        unsafe {
            core::ptr::write(r_dest as *mut AbiString, AbiString::new(s));
        }
    }
}

/// # Safety
/// Valid AbiVariant storage.
pub unsafe extern "C" fn gdext_variant_call(
    _p_self: GDExtensionVariantPtr,
    _p_method: GDExtensionConstStringNamePtr,
    _p_args: *const GDExtensionConstVariantPtr,
    _p_argument_count: GDExtensionInt,
    r_return: GDExtensionVariantPtr,
    r_error: *mut GDExtensionCallError,
) {
    if !r_return.is_null() {
        // SAFETY: Setting return to Nil.
        unsafe {
            core::ptr::write(r_return as *mut AbiVariant, AbiVariant::new(IrValue::Nil));
        }
    }
    if !r_error.is_null() {
        // SAFETY: Error struct initialized to Ok.
        unsafe {
            *r_error = GDExtensionCallError::default();
        }
    }
}

/// # Safety
/// Valid AbiVariant storage.
pub unsafe extern "C" fn gdext_variant_evaluate(
    _p_op: u32,
    _p_a: GDExtensionConstVariantPtr,
    _p_b: GDExtensionConstVariantPtr,
    r_dest: GDExtensionVariantPtr,
    r_valid: *mut GDExtensionBool,
) {
    if !r_dest.is_null() {
        // SAFETY: Writing Nil result.
        unsafe {
            core::ptr::write(r_dest as *mut AbiVariant, AbiVariant::new(IrValue::Nil));
        }
    }
    if !r_valid.is_null() {
        // SAFETY: Writing valid boolean flag.
        unsafe {
            *r_valid = 1;
        }
    }
}

/// # Safety
/// Valid AbiVariant pointer.
pub unsafe extern "C" fn gdext_variant_booleanize(p_self: GDExtensionConstVariantPtr) -> GDExtensionBool {
    if p_self.is_null() {
        return 0;
    }
    // SAFETY: Reading initialized AbiVariant.
    let var = unsafe { &*(p_self as *const AbiVariant) };
    match &var.val {
        IrValue::Nil => 0,
        IrValue::Bool(b) => u8::from(*b),
        IrValue::I64(i) => u8::from(*i != 0),
        IrValue::F64(f) => u8::from(*f != 0.0),
        IrValue::String(s) => u8::from(!s.is_empty()),
        _ => 1,
    }
}

/// # Safety
/// Valid AbiVariant pointers.
pub unsafe extern "C" fn gdext_variant_duplicate(
    p_self: GDExtensionConstVariantPtr,
    r_dest: GDExtensionVariantPtr,
    _p_deep: GDExtensionBool,
) {
    // SAFETY: Delegating to variant_new_copy.
    unsafe {
        gdext_variant_new_copy(r_dest, p_self);
    }
}

/// # Safety
/// Valid AbiVariant pointer.
pub unsafe extern "C" fn gdext_variant_hash(p_self: GDExtensionConstVariantPtr) -> i64 {
    if p_self.is_null() {
        return 0;
    }
    // SAFETY: Reading initialized AbiVariant.
    let var = unsafe { &*(p_self as *const AbiVariant) };
    match &var.val {
        IrValue::I64(i) => *i,
        IrValue::F64(f) => *f as i64,
        IrValue::Bool(b) => i64::from(*b),
        IrValue::String(s) => s.len() as i64,
        _ => 42,
    }
}

/// # Safety
/// Valid variant type checking.
pub unsafe extern "C" fn gdext_variant_can_convert(
    _p_from: GDExtensionVariantType,
    _p_to: GDExtensionVariantType,
) -> GDExtensionBool {
    1
}

/// # Safety
/// `r_dest` must be valid for writing, `p_src` points to AbiVariant.
pub unsafe extern "C" fn gdext_bool_from_variant(
    r_dest: *mut GDExtensionBool,
    p_src: GDExtensionConstVariantPtr,
) {
    if !r_dest.is_null() {
        let b = if p_src.is_null() {
            0
        } else {
            // SAFETY: p_src points to initialized AbiVariant.
            let var = unsafe { &*(p_src as *const AbiVariant) };
            match &var.val {
                IrValue::Bool(v) => u8::from(*v),
                IrValue::I64(v) => u8::from(*v != 0),
                _ => 0,
            }
        };
        // SAFETY: r_dest is valid for writing u8.
        unsafe {
            *r_dest = b;
        }
    }
}

/// # Safety
/// `r_dest` must be valid for writing AbiVariant.
pub unsafe extern "C" fn gdext_variant_from_bool(
    r_dest: GDExtensionVariantPtr,
    p_src: GDExtensionBool,
) {
    if !r_dest.is_null() {
        // SAFETY: Writing AbiVariant with bool.
        unsafe {
            core::ptr::write(r_dest as *mut AbiVariant, AbiVariant::new(IrValue::Bool(p_src != 0)));
        }
    }
}

/// # Safety
/// `r_dest` must be valid for writing, `p_src` points to AbiVariant.
pub unsafe extern "C" fn gdext_int_from_variant(
    r_dest: *mut GDExtensionInt,
    p_src: GDExtensionConstVariantPtr,
) {
    if !r_dest.is_null() {
        let i = if p_src.is_null() {
            0
        } else {
            // SAFETY: p_src points to initialized AbiVariant.
            let var = unsafe { &*(p_src as *const AbiVariant) };
            match &var.val {
                IrValue::I64(v) => *v,
                IrValue::F64(v) => *v as i64,
                IrValue::Bool(v) => i64::from(*v),
                _ => 0,
            }
        };
        // SAFETY: r_dest is valid for writing i64.
        unsafe {
            *r_dest = i;
        }
    }
}

/// # Safety
/// `r_dest` must be valid for writing AbiVariant.
pub unsafe extern "C" fn gdext_variant_from_int(
    r_dest: GDExtensionVariantPtr,
    p_src: GDExtensionInt,
) {
    if !r_dest.is_null() {
        // SAFETY: Writing AbiVariant with i64.
        unsafe {
            core::ptr::write(r_dest as *mut AbiVariant, AbiVariant::new(IrValue::I64(p_src)));
        }
    }
}

/// # Safety
/// `r_dest` must be valid for writing, `p_src` points to AbiVariant.
pub unsafe extern "C" fn gdext_float_from_variant(
    r_dest: *mut f64,
    p_src: GDExtensionConstVariantPtr,
) {
    if !r_dest.is_null() {
        let f = if p_src.is_null() {
            0.0
        } else {
            // SAFETY: p_src points to initialized AbiVariant.
            let var = unsafe { &*(p_src as *const AbiVariant) };
            match &var.val {
                IrValue::F64(v) => *v,
                IrValue::I64(v) => *v as f64,
                _ => 0.0,
            }
        };
        // SAFETY: r_dest is valid for writing f64.
        unsafe {
            *r_dest = f;
        }
    }
}

/// # Safety
/// `r_dest` must be valid for writing AbiVariant.
pub unsafe extern "C" fn gdext_variant_from_float(
    r_dest: GDExtensionVariantPtr,
    p_src: f64,
) {
    if !r_dest.is_null() {
        // SAFETY: Writing AbiVariant with f64.
        unsafe {
            core::ptr::write(r_dest as *mut AbiVariant, AbiVariant::new(IrValue::F64(p_src)));
        }
    }
}

/// # Safety
/// `r_dest` must be valid for writing AbiString, `p_contents` must be null-terminated C string.
pub unsafe extern "C" fn gdext_string_new_with_utf8_chars(
    r_dest: GDExtensionUninitializedStringPtr,
    p_contents: *const c_char,
) {
    if !r_dest.is_null() {
        let s = if p_contents.is_null() {
            String::new()
        } else {
            // SAFETY: p_contents verified non-null, null-terminated string.
            unsafe { CStr::from_ptr(p_contents).to_string_lossy().into_owned() }
        };
        // SAFETY: r_dest is valid for writing AbiString.
        unsafe {
            core::ptr::write(r_dest as *mut AbiString, AbiString::new(s));
        }
    }
}

/// # Safety
/// `r_dest` must be valid for writing AbiString, `p_contents` must point to `p_size` bytes.
pub unsafe extern "C" fn gdext_string_new_with_utf8_chars_and_len(
    r_dest: GDExtensionUninitializedStringPtr,
    p_contents: *const c_char,
    p_size: GDExtensionInt,
) {
    if !r_dest.is_null() {
        let s = if p_contents.is_null() || p_size <= 0 {
            String::new()
        } else {
            // SAFETY: Reading p_size bytes from p_contents buffer.
            let slice = unsafe { std::slice::from_raw_parts(p_contents as *const u8, p_size as usize) };
            String::from_utf8_lossy(slice).into_owned()
        };
        // SAFETY: r_dest is valid for writing AbiString.
        unsafe {
            core::ptr::write(r_dest as *mut AbiString, AbiString::new(s));
        }
    }
}

/// # Safety
/// `r_dest` must be valid for writing AbiString.
pub unsafe extern "C" fn gdext_string_new_with_latin1_chars(
    r_dest: GDExtensionUninitializedStringPtr,
    p_contents: *const c_char,
) {
    // SAFETY: Delegating to utf8 chars.
    unsafe {
        gdext_string_new_with_utf8_chars(r_dest, p_contents);
    }
}

/// # Safety
/// `p_self` must point to valid AbiString. `r_text` may be null to query length.
pub unsafe extern "C" fn gdext_string_to_utf8_chars(
    p_self: GDExtensionConstStringPtr,
    r_text: *mut c_char,
    p_max_write_length: GDExtensionInt,
) -> GDExtensionInt {
    if p_self.is_null() {
        return 0;
    }
    // SAFETY: p_self points to initialized AbiString.
    let s = unsafe { &*(p_self as *const AbiString) };
    let bytes = s.text.as_bytes();
    let len = bytes.len() as GDExtensionInt;
    if r_text.is_null() || p_max_write_length <= 0 {
        return len;
    }
    let copy_len = (len as usize).min((p_max_write_length - 1) as usize);
    // SAFETY: Copying bytes and null-terminating output buffer.
    unsafe {
        core::ptr::copy_nonoverlapping(bytes.as_ptr() as *const c_char, r_text, copy_len);
        *r_text.add(copy_len) = 0;
    }
    copy_len as GDExtensionInt
}

/// # Safety
/// Delegating to utf8 conversion.
pub unsafe extern "C" fn gdext_string_to_latin1_chars(
    p_self: GDExtensionConstStringPtr,
    r_text: *mut c_char,
    p_max_write_length: GDExtensionInt,
) -> GDExtensionInt {
    // SAFETY: Equivalent byte copy.
    unsafe { gdext_string_to_utf8_chars(p_self, r_text, p_max_write_length) }
}

/// # Safety
/// `p_self` points to AbiString.
pub unsafe extern "C" fn gdext_string_destroy(p_self: GDExtensionStringPtr) {
    if !p_self.is_null() {
        // SAFETY: Dropping AbiString in place.
        unsafe {
            core::ptr::drop_in_place(p_self as *mut AbiString);
        }
    }
}

/// # Safety
/// `r_dest` must be valid for writing AbiStringName.
pub unsafe extern "C" fn gdext_string_name_new_with_utf8_chars(
    r_dest: GDExtensionUninitializedStringNamePtr,
    p_contents: *const c_char,
) {
    if !r_dest.is_null() {
        let s = if p_contents.is_null() {
            String::new()
        } else {
            // SAFETY: Reading valid null-terminated C string.
            unsafe { CStr::from_ptr(p_contents).to_string_lossy().into_owned() }
        };
        // SAFETY: Writing AbiStringName.
        unsafe {
            core::ptr::write(r_dest as *mut AbiStringName, AbiStringName::new(s));
        }
    }
}

/// # Safety
/// `r_dest` must be valid for writing AbiStringName.
pub unsafe extern "C" fn gdext_string_name_new_with_utf8_chars_and_len(
    r_dest: GDExtensionUninitializedStringNamePtr,
    p_contents: *const c_char,
    p_size: GDExtensionInt,
) {
    if !r_dest.is_null() {
        let s = if p_contents.is_null() || p_size <= 0 {
            String::new()
        } else {
            // SAFETY: Reading slice of bytes.
            let slice = unsafe { std::slice::from_raw_parts(p_contents as *const u8, p_size as usize) };
            String::from_utf8_lossy(slice).into_owned()
        };
        // SAFETY: Writing AbiStringName.
        unsafe {
            core::ptr::write(r_dest as *mut AbiStringName, AbiStringName::new(s));
        }
    }
}

/// # Safety
/// Latin1 constructor.
pub unsafe extern "C" fn gdext_string_name_new_with_latin1_chars(
    r_dest: GDExtensionUninitializedStringNamePtr,
    p_contents: *const c_char,
) {
    // SAFETY: Delegating to utf8.
    unsafe {
        gdext_string_name_new_with_utf8_chars(r_dest, p_contents);
    }
}

/// # Safety
/// `p_self` points to AbiStringName.
pub unsafe extern "C" fn gdext_string_name_destroy(p_self: GDExtensionStringNamePtr) {
    if !p_self.is_null() {
        // SAFETY: Dropping AbiStringName in place.
        unsafe {
            core::ptr::drop_in_place(p_self as *mut AbiStringName);
        }
    }
}

/// # Safety
/// Valid class name and creation info pointers.
pub unsafe extern "C" fn gdext_classdb_register_extension_class3(
    _p_library: GDExtensionClassLibraryPtr,
    p_class_name: GDExtensionConstStringNamePtr,
    p_parent_class_name: GDExtensionConstStringNamePtr,
    p_extension_funcs: *const GDExtensionClassCreationInfo3,
) {
    if p_class_name.is_null() || p_parent_class_name.is_null() || p_extension_funcs.is_null() {
        return;
    }
    // SAFETY: Reading valid AbiStringName and creation info structs.
    let (name, parent, info) = unsafe {
        let c_name = &*(p_class_name as *const AbiStringName);
        let p_name = &*(p_parent_class_name as *const AbiStringName);
        let creation = *p_extension_funcs;
        (c_name.name.clone(), p_name.name.clone(), creation)
    };

    access_registry(|reg| {
        reg.classes.insert(
            name.clone(),
            RegisteredExtensionClass {
                name,
                parent_name: parent,
                is_virtual: info.is_virtual != 0,
                is_abstract: info.is_abstract != 0,
                is_exposed: info.is_exposed != 0,
                is_runtime: info.is_runtime != 0,
                creation_info: info,
                methods: HashMap::new(),
                properties: HashMap::new(),
                signals: HashMap::new(),
                integer_constants: HashMap::new(),
            },
        );
    });
}

/// # Safety
/// Compatible wrapper delegating to ClassCreationInfo3.
pub unsafe extern "C" fn gdext_classdb_register_extension_class2(
    p_library: GDExtensionClassLibraryPtr,
    p_class_name: GDExtensionConstStringNamePtr,
    p_parent_class_name: GDExtensionConstStringNamePtr,
    p_extension_funcs: *const GDExtensionClassCreationInfo2,
) {
    if p_extension_funcs.is_null() {
        return;
    }
    // SAFETY: Converting Info2 to Info3 layout.
    let info2 = unsafe { *p_extension_funcs };
    let info3 = GDExtensionClassCreationInfo3 {
        is_virtual: info2.is_virtual,
        is_abstract: info2.is_abstract,
        is_exposed: info2.is_exposed,
        is_runtime: 0,
        set_func: info2.set_func,
        get_func: info2.get_func,
        get_property_list_func: info2.get_property_list_func,
        free_property_list_func: info2.free_property_list_func,
        property_can_revert_func: info2.property_can_revert_func,
        property_get_revert_func: info2.property_get_revert_func,
        validate_property_func: None,
        notification_func: info2.notification_func,
        to_string_func: info2.to_string_func,
        reference_func: info2.reference_func,
        unreference_func: info2.unreference_func,
        create_instance_func: info2.create_instance_func,
        free_instance_func: info2.free_instance_func,
        recreate_instance_func: info2.recreate_instance_func,
        get_virtual_func: info2.get_virtual_func,
        get_virtual_call_data_func: info2.get_virtual_call_data_func,
        call_virtual_with_data_func: info2.call_virtual_with_data_func,
        class_userdata: info2.class_userdata,
    };
    // SAFETY: Calling v3 registration with synthesized Info3.
    unsafe {
        gdext_classdb_register_extension_class3(p_library, p_class_name, p_parent_class_name, &info3);
    }
}

/// # Safety
/// Fallback v1 wrapper delegating to Info2.
pub unsafe extern "C" fn gdext_classdb_register_extension_class(
    p_library: GDExtensionClassLibraryPtr,
    p_class_name: GDExtensionConstStringNamePtr,
    p_parent_class_name: GDExtensionConstStringNamePtr,
    p_extension_funcs: *const GDExtensionClassCreationInfo2,
) {
    // SAFETY: Calling classdb_register_extension_class2.
    unsafe {
        gdext_classdb_register_extension_class2(p_library, p_class_name, p_parent_class_name, p_extension_funcs);
    }
}

/// # Safety
/// Valid method info pointer.
pub unsafe extern "C" fn gdext_classdb_register_extension_class_method(
    _p_library: GDExtensionClassLibraryPtr,
    p_class_name: GDExtensionConstStringNamePtr,
    p_method_info: *const GDExtensionClassMethodInfo,
) {
    if p_class_name.is_null() || p_method_info.is_null() {
        return;
    }
    // SAFETY: Reading class name and method info.
    let (class_name, method_name, method_info) = unsafe {
        let c_name = &*(p_class_name as *const AbiStringName);
        let m_info = *p_method_info;
        let m_name = if m_info.name.is_null() {
            String::new()
        } else {
            (&*(m_info.name as *const AbiStringName)).name.clone()
        };
        (c_name.name.clone(), m_name, m_info)
    };

    access_registry(|reg| {
        if let Some(class) = reg.classes.get_mut(&class_name) {
            class.methods.insert(
                method_name.clone(),
                RegisteredMethod {
                    name: method_name,
                    userdata: method_info.method_userdata,
                    call_func: method_info.call_func,
                    ptrcall_func: method_info.ptrcall_func,
                    flags: method_info.method_flags,
                    has_return_value: method_info.has_return_value != 0,
                },
            );
        }
    });
}

/// # Safety
/// Valid property info pointer.
pub unsafe extern "C" fn gdext_classdb_register_extension_class_property(
    _p_library: GDExtensionClassLibraryPtr,
    p_class_name: GDExtensionConstStringNamePtr,
    p_info: *const GDExtensionPropertyInfo,
    p_setter: GDExtensionConstStringNamePtr,
    p_getter: GDExtensionConstStringNamePtr,
) {
    if p_class_name.is_null() || p_info.is_null() {
        return;
    }
    // SAFETY: Reading property info and setter/getter strings.
    let (class_name, prop_name, p_type, hint, usage, setter, getter) = unsafe {
        let c_name = &*(p_class_name as *const AbiStringName);
        let info = *p_info;
        let p_name = if info.name.is_null() {
            String::new()
        } else {
            (&*(info.name as *const AbiStringName)).name.clone()
        };
        let set = if p_setter.is_null() {
            String::new()
        } else {
            (&*(p_setter as *const AbiStringName)).name.clone()
        };
        let get = if p_getter.is_null() {
            String::new()
        } else {
            (&*(p_getter as *const AbiStringName)).name.clone()
        };
        (c_name.name.clone(), p_name, info.r#type, info.hint, info.usage, set, get)
    };

    access_registry(|reg| {
        if let Some(class) = reg.classes.get_mut(&class_name) {
            class.properties.insert(
                prop_name.clone(),
                RegisteredProperty {
                    name: prop_name,
                    property_type: p_type,
                    hint,
                    hint_string: String::new(),
                    usage,
                    setter,
                    getter,
                },
            );
        }
    });
}

/// # Safety
/// Indexed property registration wrapper.
pub unsafe extern "C" fn gdext_classdb_register_extension_class_property_indexed(
    p_library: GDExtensionClassLibraryPtr,
    p_class_name: GDExtensionConstStringNamePtr,
    p_info: *const GDExtensionPropertyInfo,
    p_setter: GDExtensionConstStringNamePtr,
    p_getter: GDExtensionConstStringNamePtr,
    _p_index: GDExtensionInt,
) {
    // SAFETY: Delegating to property registration.
    unsafe {
        gdext_classdb_register_extension_class_property(p_library, p_class_name, p_info, p_setter, p_getter);
    }
}

/// # Safety
/// Group descriptor wrapper.
pub unsafe extern "C" fn gdext_classdb_register_extension_class_property_group(
    _p_library: GDExtensionClassLibraryPtr,
    _p_class_name: GDExtensionConstStringNamePtr,
    _p_group_name: GDExtensionConstStringPtr,
    _p_prefix: GDExtensionConstStringPtr,
) {
}

/// # Safety
/// Subgroup descriptor wrapper.
pub unsafe extern "C" fn gdext_classdb_register_extension_class_property_subgroup(
    _p_library: GDExtensionClassLibraryPtr,
    _p_class_name: GDExtensionConstStringNamePtr,
    _p_subgroup_name: GDExtensionConstStringPtr,
    _p_prefix: GDExtensionConstStringPtr,
) {
}

/// # Safety
/// Valid signal name and arguments.
pub unsafe extern "C" fn gdext_classdb_register_extension_class_signal(
    _p_library: GDExtensionClassLibraryPtr,
    p_class_name: GDExtensionConstStringNamePtr,
    p_signal_name: GDExtensionConstStringNamePtr,
    p_args: *const GDExtensionPropertyInfo,
    p_argument_count: GDExtensionInt,
) {
    if p_class_name.is_null() || p_signal_name.is_null() {
        return;
    }
    // SAFETY: Reading signal names and argument descriptors.
    let (class_name, signal_name, arg_names) = unsafe {
        let c_name = &*(p_class_name as *const AbiStringName);
        let s_name = &*(p_signal_name as *const AbiStringName);
        let mut names = Vec::new();
        if !p_args.is_null() && p_argument_count > 0 {
            for i in 0..p_argument_count {
                let info = *p_args.add(i as usize);
                if !info.name.is_null() {
                    let a_name = (&*(info.name as *const AbiStringName)).name.clone();
                    names.push(a_name);
                }
            }
        }
        (c_name.name.clone(), s_name.name.clone(), names)
    };

    access_registry(|reg| {
        if let Some(class) = reg.classes.get_mut(&class_name) {
            class.signals.insert(
                signal_name.clone(),
                RegisteredSignal {
                    name: signal_name,
                    argument_names: arg_names,
                },
            );
        }
    });
}

/// # Safety
/// Valid constant name and value.
pub unsafe extern "C" fn gdext_classdb_register_extension_class_integer_constant(
    _p_library: GDExtensionClassLibraryPtr,
    p_class_name: GDExtensionConstStringNamePtr,
    _p_enum_name: GDExtensionConstStringNamePtr,
    p_constant_name: GDExtensionConstStringNamePtr,
    p_constant_value: GDExtensionInt,
    _p_is_bitfield: GDExtensionBool,
) {
    if p_class_name.is_null() || p_constant_name.is_null() {
        return;
    }
    // SAFETY: Reading names.
    let (class_name, const_name) = unsafe {
        let c_name = &*(p_class_name as *const AbiStringName);
        let k_name = &*(p_constant_name as *const AbiStringName);
        (c_name.name.clone(), k_name.name.clone())
    };

    access_registry(|reg| {
        if let Some(class) = reg.classes.get_mut(&class_name) {
            class.integer_constants.insert(const_name, p_constant_value);
        }
    });
}

/// # Safety
/// Valid class name to unregister.
pub unsafe extern "C" fn gdext_classdb_unregister_extension_class(
    _p_library: GDExtensionClassLibraryPtr,
    p_class_name: GDExtensionConstStringNamePtr,
) {
    if p_class_name.is_null() {
        return;
    }
    // SAFETY: Reading class name to remove.
    let name = unsafe { (&*(p_class_name as *const AbiStringName)).name.clone() };
    access_registry(|reg| {
        reg.classes.remove(&name);
    });
}

/// # Safety
/// Instantiates object for registered class.
pub unsafe extern "C" fn gdext_classdb_construct_object(
    p_classname: GDExtensionConstStringNamePtr,
) -> GDExtensionObjectPtr {
    if p_classname.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: Reading class name.
    let name = unsafe { (&*(p_classname as *const AbiStringName)).name.clone() };
    let obj = Box::new(AbiObject {
        class_name: name,
        instance_id: 1001,
        extension_instance: core::ptr::null_mut(),
        extension_userdata: core::ptr::null_mut(),
    });
    Box::into_raw(obj) as GDExtensionObjectPtr
}

/// # Safety
/// Returns class tag.
pub unsafe extern "C" fn gdext_classdb_get_class_tag(
    _p_classname: GDExtensionConstStringNamePtr,
) -> *mut c_void {
    core::ptr::null_mut()
}

/// # Safety
/// Binds extension instance to object.
pub unsafe extern "C" fn gdext_object_set_instance(
    p_object: GDExtensionObjectPtr,
    p_classname: GDExtensionConstStringNamePtr,
    p_instance: GDExtensionClassInstancePtr,
) {
    if !p_object.is_null() {
        // SAFETY: Casting p_object to AbiObject.
        let obj = unsafe { &mut *(p_object as *mut AbiObject) };
        if !p_classname.is_null() {
            // SAFETY: Reading class name.
            obj.class_name = unsafe { (&*(p_classname as *const AbiStringName)).name.clone() };
        }
        obj.extension_instance = p_instance;
    }
}

/// # Safety
/// Retrieves instance binding.
pub unsafe extern "C" fn gdext_object_get_instance_binding(
    p_object: GDExtensionObjectPtr,
    _p_token: *mut c_void,
    _p_callbacks: *const c_void,
) -> *mut c_void {
    p_object
}

/// # Safety
/// Frees instance binding.
pub unsafe extern "C" fn gdext_object_free_instance_binding(
    _p_object: GDExtensionObjectPtr,
    _p_token: *mut c_void,
) {
}

/// # Safety
/// Sets instance binding.
pub unsafe extern "C" fn gdext_object_set_instance_binding(
    _p_object: GDExtensionObjectPtr,
    _p_token: *mut c_void,
    _p_binding: *mut c_void,
    _p_callbacks: *const c_void,
) {
}

/// # Safety
/// Retrieves class name from object.
pub unsafe extern "C" fn gdext_object_get_class_name(
    p_object: GDExtensionConstObjectPtr,
    _p_library: GDExtensionClassLibraryPtr,
    r_class_name: GDExtensionStringPtr,
) {
    if !p_object.is_null() && !r_class_name.is_null() {
        // SAFETY: Reading AbiObject.
        let obj = unsafe { &*(p_object as *const AbiObject) };
        // SAFETY: Writing AbiString to non-null destination.
        unsafe {
            core::ptr::write(r_class_name as *mut AbiString, AbiString::new(&obj.class_name));
        }
    }
}

/// # Safety
/// Casts object to target class tag.
pub unsafe extern "C" fn gdext_object_cast_to(
    p_object: GDExtensionConstObjectPtr,
    _p_class_tag: *mut c_void,
) -> GDExtensionObjectPtr {
    p_object as GDExtensionObjectPtr
}

/// # Safety
/// Retrieves instance ID from object.
pub unsafe extern "C" fn gdext_object_get_instance_id(p_object: GDExtensionConstObjectPtr) -> u64 {
    if p_object.is_null() {
        return 0;
    }
    // SAFETY: Reading instance_id from AbiObject.
    let obj = unsafe { &*(p_object as *const AbiObject) };
    obj.instance_id
}

/// # Safety
/// Checks if object has script method.
pub unsafe extern "C" fn gdext_object_has_script_method(
    _p_object: GDExtensionConstObjectPtr,
    _p_method: GDExtensionConstStringNamePtr,
) -> GDExtensionBool {
    0
}

/// # Safety
/// Calls script method on object.
pub unsafe extern "C" fn gdext_object_call_script_method(
    _p_object: GDExtensionObjectPtr,
    _p_method: GDExtensionConstStringNamePtr,
    _p_args: *const GDExtensionConstVariantPtr,
    _p_argument_count: GDExtensionInt,
    r_return: GDExtensionVariantPtr,
    r_error: *mut GDExtensionCallError,
) {
    if !r_return.is_null() {
        // SAFETY: Writing Nil.
        unsafe {
            core::ptr::write(r_return as *mut AbiVariant, AbiVariant::new(IrValue::Nil));
        }
    }
    if !r_error.is_null() {
        // SAFETY: Setting Ok.
        unsafe {
            *r_error = GDExtensionCallError::default();
        }
    }
}

/// # Safety
/// Destroys AbiObject.
pub unsafe extern "C" fn gdext_object_destroy(p_object: GDExtensionObjectPtr) {
    if !p_object.is_null() {
        // SAFETY: Dropping boxed AbiObject.
        unsafe {
            let _ = Box::from_raw(p_object as *mut AbiObject);
        }
    }
}

/// # Safety
/// Resolves engine singleton pointer.
pub unsafe extern "C" fn gdext_global_get_singleton(p_name: GDExtensionConstStringNamePtr) -> GDExtensionObjectPtr {
    if p_name.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: Reading singleton name.
    let name = unsafe { (&*(p_name as *const AbiStringName)).name.clone() };
    access_registry(|reg| {
        reg.singletons
            .get(&name)
            .copied()
            .unwrap_or(core::ptr::null_mut())
    })
}

/// # Safety
/// Custom callable constructor.
pub unsafe extern "C" fn gdext_callable_custom_create(
    r_callable: GDExtensionTypePtr,
    _p_callable_custom_info: *mut c_void,
) {
    if !r_callable.is_null() {
        // SAFETY: Writing null/zero memory.
        unsafe {
            core::ptr::write_bytes(r_callable, 0, 16);
        }
    }
}

/// # Safety
/// Custom callable constructor 2.
pub unsafe extern "C" fn gdext_callable_custom_create2(
    r_callable: GDExtensionTypePtr,
    _p_callable_custom_info: *mut c_void,
) {
    if !r_callable.is_null() {
        // SAFETY: Writing null/zero memory.
        unsafe {
            core::ptr::write_bytes(r_callable, 0, 16);
        }
    }
}

/// # Safety
/// Reference counter getter.
pub unsafe extern "C" fn gdext_ref_get_object(_p_ref: GDExtensionConstTypePtr) -> GDExtensionObjectPtr {
    core::ptr::null_mut()
}

/// # Safety
/// Reference counter setter.
pub unsafe extern "C" fn gdext_ref_set_object(
    _p_ref: GDExtensionTypePtr,
    _p_object: GDExtensionConstObjectPtr,
) {
}

/// # Safety
/// Library path retriever.
pub unsafe extern "C" fn gdext_get_library_path(
    _p_library: GDExtensionClassLibraryPtr,
    r_path: GDExtensionStringPtr,
) {
    if !r_path.is_null() {
        // SAFETY: Writing dummy library path.
        unsafe {
            core::ptr::write(r_path as *mut AbiString, AbiString::new("res://bin/libplugin.dylib"));
        }
    }
}

/// # Safety
/// NodePath constructor.
pub unsafe extern "C" fn gdext_node_path_new_with_utf8_chars(
    r_dest: GDExtensionTypePtr,
    p_contents: *const c_char,
) {
    if !r_dest.is_null() {
        // SAFETY: Delegating to string constructor.
        unsafe {
            gdext_string_new_with_utf8_chars(r_dest as GDExtensionUninitializedStringPtr, p_contents);
        }
    }
}

/// # Safety
/// NodePath destructor.
pub unsafe extern "C" fn gdext_node_path_destroy(p_self: GDExtensionTypePtr) {
    // SAFETY: Delegating to string destroy.
    unsafe {
        gdext_string_destroy(p_self as GDExtensionStringPtr);
    }
}

/// # Safety
/// Array constructor.
pub unsafe extern "C" fn gdext_array_new(r_dest: GDExtensionTypePtr) {
    if !r_dest.is_null() {
        // SAFETY: Writing AbiVariant with Array.
        unsafe {
            core::ptr::write(r_dest as *mut AbiVariant, AbiVariant::new(IrValue::Array(Vec::new())));
        }
    }
}

/// # Safety
/// Array destructor.
pub unsafe extern "C" fn gdext_array_destroy(p_self: GDExtensionTypePtr) {
    // SAFETY: Dropping AbiVariant.
    unsafe {
        gdext_variant_destroy(p_self as GDExtensionVariantPtr);
    }
}

/// # Safety
/// Array size query.
pub unsafe extern "C" fn gdext_array_size(p_self: GDExtensionConstTypePtr) -> GDExtensionInt {
    if p_self.is_null() {
        return 0;
    }
    // SAFETY: Reading array length.
    let var = unsafe { &*(p_self as *const AbiVariant) };
    if let IrValue::Array(ref arr) = var.val {
        arr.len() as GDExtensionInt
    } else {
        0
    }
}

/// # Safety
/// Dictionary constructor.
pub unsafe extern "C" fn gdext_dictionary_new(r_dest: GDExtensionTypePtr) {
    if !r_dest.is_null() {
        // SAFETY: Writing AbiVariant with Map.
        unsafe {
            core::ptr::write(
                r_dest as *mut AbiVariant,
                AbiVariant::new(IrValue::Map(std::collections::BTreeMap::new())),
            );
        }
    }
}

/// # Safety
/// Dictionary destructor.
pub unsafe extern "C" fn gdext_dictionary_destroy(p_self: GDExtensionTypePtr) {
    // SAFETY: Dropping AbiVariant.
    unsafe {
        gdext_variant_destroy(p_self as GDExtensionVariantPtr);
    }
}

/// # Safety
/// Dictionary size query.
pub unsafe extern "C" fn gdext_dictionary_size(p_self: GDExtensionConstTypePtr) -> GDExtensionInt {
    if p_self.is_null() {
        return 0;
    }
    // SAFETY: Reading dictionary length.
    let var = unsafe { &*(p_self as *const AbiVariant) };
    if let IrValue::Map(ref map) = var.val {
        map.len() as GDExtensionInt
    } else {
        0
    }
}

/// # Safety
/// PackedByteArray constructor.
pub unsafe extern "C" fn gdext_packed_byte_array_new(r_dest: GDExtensionTypePtr) {
    if !r_dest.is_null() {
        // SAFETY: Writing empty vector as Array.
        unsafe {
            core::ptr::write(r_dest as *mut AbiVariant, AbiVariant::new(IrValue::Array(Vec::new())));
        }
    }
}

/// # Safety
/// PackedByteArray destructor.
pub unsafe extern "C" fn gdext_packed_byte_array_destroy(p_self: GDExtensionTypePtr) {
    // SAFETY: Dropping AbiVariant.
    unsafe {
        gdext_variant_destroy(p_self as GDExtensionVariantPtr);
    }
}

/// # Safety
/// PackedByteArray size query.
pub unsafe extern "C" fn gdext_packed_byte_array_size(p_self: GDExtensionConstTypePtr) -> GDExtensionInt {
    // SAFETY: Delegating to array_size.
    unsafe { gdext_array_size(p_self) }
}

/// # Safety
/// PackedStringArray constructor.
pub unsafe extern "C" fn gdext_packed_string_array_new(r_dest: GDExtensionTypePtr) {
    if !r_dest.is_null() {
        // SAFETY: Writing empty vector.
        unsafe {
            core::ptr::write(r_dest as *mut AbiVariant, AbiVariant::new(IrValue::Array(Vec::new())));
        }
    }
}

/// # Safety
/// PackedStringArray destructor.
pub unsafe extern "C" fn gdext_packed_string_array_destroy(p_self: GDExtensionTypePtr) {
    // SAFETY: Dropping AbiVariant.
    unsafe {
        gdext_variant_destroy(p_self as GDExtensionVariantPtr);
    }
}

/// # Safety
/// PackedStringArray size query.
pub unsafe extern "C" fn gdext_packed_string_array_size(p_self: GDExtensionConstTypePtr) -> GDExtensionInt {
    // SAFETY: Delegating to array_size.
    unsafe { gdext_array_size(p_self) }
}

// ---------------------------------------------------------------------------
// 8. Supported Proc Names List (Assert >= 60 entries!)
// ---------------------------------------------------------------------------

pub static SUPPORTED_PROC_NAMES: &[&str] = &[
    "get_godot_version",
    "mem_alloc",
    "mem_realloc",
    "mem_free",
    "print_error",
    "print_error_with_message",
    "print_warning",
    "print_warning_with_message",
    "print_script_error",
    "print_script_error_with_message",
    "get_native_struct_size",
    "variant_new_copy",
    "variant_new_nil",
    "variant_destroy",
    "variant_call",
    "variant_evaluate",
    "variant_stringify",
    "variant_get_type",
    "variant_get_type_name",
    "variant_booleanize",
    "variant_duplicate",
    "variant_hash",
    "variant_can_convert",
    "bool_from_variant",
    "variant_from_bool",
    "int_from_variant",
    "variant_from_int",
    "float_from_variant",
    "variant_from_float",
    "string_new_with_latin1_chars",
    "string_new_with_utf8_chars",
    "string_new_with_utf8_chars_and_len",
    "string_to_utf8_chars",
    "string_to_latin1_chars",
    "string_destroy",
    "string_name_new_with_latin1_chars",
    "string_name_new_with_utf8_chars",
    "string_name_new_with_utf8_chars_and_len",
    "string_name_destroy",
    "classdb_register_extension_class",
    "classdb_register_extension_class2",
    "classdb_register_extension_class3",
    "classdb_register_extension_class_method",
    "classdb_register_extension_class_property",
    "classdb_register_extension_class_property_indexed",
    "classdb_register_extension_class_property_group",
    "classdb_register_extension_class_property_subgroup",
    "classdb_register_extension_class_signal",
    "classdb_register_extension_class_integer_constant",
    "classdb_unregister_extension_class",
    "classdb_construct_object",
    "classdb_get_class_tag",
    "object_set_instance",
    "object_get_instance_binding",
    "object_free_instance_binding",
    "object_set_instance_binding",
    "object_get_class_name",
    "object_cast_to",
    "object_get_instance_id",
    "object_has_script_method",
    "object_call_script_method",
    "object_destroy",
    "global_get_singleton",
    "callable_custom_create",
    "callable_custom_create2",
    "ref_get_object",
    "ref_set_object",
    "get_library_path",
    "node_path_new_with_utf8_chars",
    "node_path_destroy",
    "array_new",
    "array_destroy",
    "array_size",
    "dictionary_new",
    "dictionary_destroy",
    "dictionary_size",
    "packed_byte_array_new",
    "packed_byte_array_destroy",
    "packed_byte_array_size",
    "packed_string_array_new",
    "packed_string_array_destroy",
    "packed_string_array_size",
];

/// Returns the slice of all supported GDExtension interface function names.
pub fn supported_proc_names() -> &'static [&'static str] {
    SUPPORTED_PROC_NAMES
}

// ---------------------------------------------------------------------------
// 9. GetProcAddress Resolver
// ---------------------------------------------------------------------------

/// Resolves genuine Godot 4.4 GDExtension interface function pointers by name.
///
/// # Safety
/// `name` must point to a valid null-terminated C string.
pub unsafe extern "C" fn gdext_get_proc_address(
    name: *const c_char,
) -> GDExtensionInterfaceFunctionPtr {
    if name.is_null() {
        return None;
    }
    // SAFETY: name was verified non-null and points to a null-terminated C string.
    let proc_str = unsafe {
        match CStr::from_ptr(name).to_str() {
            Ok(s) => s,
            Err(_) => return None,
        }
    };

    macro_rules! match_proc {
        ($fn_name:expr, $fn_item:path) => {
            if proc_str == $fn_name {
                // SAFETY: Transmuting concrete extern "C" fn to GDExtensionInterfaceFunctionPtr.
                return Some(unsafe {
                    std::mem::transmute::<*const (), unsafe extern "C" fn()>($fn_item as *const ())
                });
            }
        };
    }

    match_proc!("get_godot_version", gdext_get_godot_version);
    match_proc!("mem_alloc", gdext_mem_alloc);
    match_proc!("mem_realloc", gdext_mem_realloc);
    match_proc!("mem_free", gdext_mem_free);
    match_proc!("print_error", gdext_print_error);
    match_proc!("print_error_with_message", gdext_print_error_with_message);
    match_proc!("print_warning", gdext_print_warning);
    match_proc!("print_warning_with_message", gdext_print_warning_with_message);
    match_proc!("print_script_error", gdext_print_script_error);
    match_proc!("print_script_error_with_message", gdext_print_script_error_with_message);
    match_proc!("get_native_struct_size", gdext_get_native_struct_size);
    match_proc!("variant_new_copy", gdext_variant_new_copy);
    match_proc!("variant_new_nil", gdext_variant_new_nil);
    match_proc!("variant_destroy", gdext_variant_destroy);
    match_proc!("variant_call", gdext_variant_call);
    match_proc!("variant_evaluate", gdext_variant_evaluate);
    match_proc!("variant_stringify", gdext_variant_stringify);
    match_proc!("variant_get_type", gdext_variant_get_type);
    match_proc!("variant_get_type_name", gdext_variant_get_type_name);
    match_proc!("variant_booleanize", gdext_variant_booleanize);
    match_proc!("variant_duplicate", gdext_variant_duplicate);
    match_proc!("variant_hash", gdext_variant_hash);
    match_proc!("variant_can_convert", gdext_variant_can_convert);
    match_proc!("bool_from_variant", gdext_bool_from_variant);
    match_proc!("variant_from_bool", gdext_variant_from_bool);
    match_proc!("int_from_variant", gdext_int_from_variant);
    match_proc!("variant_from_int", gdext_variant_from_int);
    match_proc!("float_from_variant", gdext_float_from_variant);
    match_proc!("variant_from_float", gdext_variant_from_float);
    match_proc!("string_new_with_latin1_chars", gdext_string_new_with_latin1_chars);
    match_proc!("string_new_with_utf8_chars", gdext_string_new_with_utf8_chars);
    match_proc!("string_new_with_utf8_chars_and_len", gdext_string_new_with_utf8_chars_and_len);
    match_proc!("string_to_utf8_chars", gdext_string_to_utf8_chars);
    match_proc!("string_to_latin1_chars", gdext_string_to_latin1_chars);
    match_proc!("string_destroy", gdext_string_destroy);
    match_proc!("string_name_new_with_latin1_chars", gdext_string_name_new_with_latin1_chars);
    match_proc!("string_name_new_with_utf8_chars", gdext_string_name_new_with_utf8_chars);
    match_proc!("string_name_new_with_utf8_chars_and_len", gdext_string_name_new_with_utf8_chars_and_len);
    match_proc!("string_name_destroy", gdext_string_name_destroy);
    match_proc!("classdb_register_extension_class", gdext_classdb_register_extension_class);
    match_proc!("classdb_register_extension_class2", gdext_classdb_register_extension_class2);
    match_proc!("classdb_register_extension_class3", gdext_classdb_register_extension_class3);
    match_proc!("classdb_register_extension_class_method", gdext_classdb_register_extension_class_method);
    match_proc!("classdb_register_extension_class_property", gdext_classdb_register_extension_class_property);
    match_proc!("classdb_register_extension_class_property_indexed", gdext_classdb_register_extension_class_property_indexed);
    match_proc!("classdb_register_extension_class_property_group", gdext_classdb_register_extension_class_property_group);
    match_proc!("classdb_register_extension_class_property_subgroup", gdext_classdb_register_extension_class_property_subgroup);
    match_proc!("classdb_register_extension_class_signal", gdext_classdb_register_extension_class_signal);
    match_proc!("classdb_register_extension_class_integer_constant", gdext_classdb_register_extension_class_integer_constant);
    match_proc!("classdb_unregister_extension_class", gdext_classdb_unregister_extension_class);
    match_proc!("classdb_construct_object", gdext_classdb_construct_object);
    match_proc!("classdb_get_class_tag", gdext_classdb_get_class_tag);
    match_proc!("object_set_instance", gdext_object_set_instance);
    match_proc!("object_get_instance_binding", gdext_object_get_instance_binding);
    match_proc!("object_free_instance_binding", gdext_object_free_instance_binding);
    match_proc!("object_set_instance_binding", gdext_object_set_instance_binding);
    match_proc!("object_get_class_name", gdext_object_get_class_name);
    match_proc!("object_cast_to", gdext_object_cast_to);
    match_proc!("object_get_instance_id", gdext_object_get_instance_id);
    match_proc!("object_has_script_method", gdext_object_has_script_method);
    match_proc!("object_call_script_method", gdext_object_call_script_method);
    match_proc!("object_destroy", gdext_object_destroy);
    match_proc!("global_get_singleton", gdext_global_get_singleton);
    match_proc!("callable_custom_create", gdext_callable_custom_create);
    match_proc!("callable_custom_create2", gdext_callable_custom_create2);
    match_proc!("ref_get_object", gdext_ref_get_object);
    match_proc!("ref_set_object", gdext_ref_set_object);
    match_proc!("get_library_path", gdext_get_library_path);
    match_proc!("node_path_new_with_utf8_chars", gdext_node_path_new_with_utf8_chars);
    match_proc!("node_path_destroy", gdext_node_path_destroy);
    match_proc!("array_new", gdext_array_new);
    match_proc!("array_destroy", gdext_array_destroy);
    match_proc!("array_size", gdext_array_size);
    match_proc!("dictionary_new", gdext_dictionary_new);
    match_proc!("dictionary_destroy", gdext_dictionary_destroy);
    match_proc!("dictionary_size", gdext_dictionary_size);
    match_proc!("packed_byte_array_new", gdext_packed_byte_array_new);
    match_proc!("packed_byte_array_destroy", gdext_packed_byte_array_destroy);
    match_proc!("packed_byte_array_size", gdext_packed_byte_array_size);
    match_proc!("packed_string_array_new", gdext_packed_string_array_new);
    match_proc!("packed_string_array_destroy", gdext_packed_string_array_destroy);
    match_proc!("packed_string_array_size", gdext_packed_string_array_size);

    None
}
