//! Godot 4.4 Comprehensive Built-in Functions & Type Methods Catalog + Evaluator.
//!
//! Provides documentation and runtime evaluation for 400+ GDScript built-ins across
//! `@GlobalScope`, Math, String, Array, Dictionary, Vector2/3/4, Color, Rect2, Transform2D/3D,
//! Basis, Quaternion, Plane, AABB, and Node/SceneTree/Object methods.

use crate::callable::GdCallable;
use crate::types::{typeof_code, type_string};
use foreplay_ir::{IrNode, IrValue};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::LazyLock;

/// Complete documentation model for a GDScript built-in function or type method.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct GdBuiltinDoc {
    pub name: String,
    pub receiver: Option<String>,
    pub signature: String,
    pub return_type: String,
    pub doc: String,
}

macro_rules! doc {
    ($name:literal, $recv:expr, $sig:literal, $ret:literal, $doc:literal) => {
        GdBuiltinDoc {
            name: $name.to_string(),
            receiver: $recv.map(|s: &str| s.to_string()),
            signature: $sig.to_string(),
            return_type: $ret.to_string(),
            doc: $doc.to_string(),
        }
    };
}

/// Catalog of 400+ Godot 4.4 built-in functions and methods.
pub static CATALOG: LazyLock<Vec<GdBuiltinDoc>> = LazyLock::new(|| {
    let mut v = Vec::with_capacity(450);

    // =========================================================================
    // 1. Global & @GlobalScope Math / Utility Functions (~85 entries)
    // =========================================================================
    v.push(doc!("sin", None, "sin(angle_rad: float) -> float", "float", "Returns the sine of angle in radians."));
    v.push(doc!("cos", None, "cos(angle_rad: float) -> float", "float", "Returns the cosine of angle in radians."));
    v.push(doc!("tan", None, "tan(angle_rad: float) -> float", "float", "Returns the tangent of angle in radians."));
    v.push(doc!("asin", None, "asin(x: float) -> float", "float", "Returns the arc sine in radians."));
    v.push(doc!("acos", None, "acos(x: float) -> float", "float", "Returns the arc cosine in radians."));
    v.push(doc!("atan", None, "atan(x: float) -> float", "float", "Returns the arc tangent in radians."));
    v.push(doc!("atan2", None, "atan2(y: float, x: float) -> float", "float", "Returns the arc tangent of y/x in radians using signs to determine quadrant."));
    v.push(doc!("sinh", None, "sinh(x: float) -> float", "float", "Returns the hyperbolic sine."));
    v.push(doc!("cosh", None, "cosh(x: float) -> float", "float", "Returns the hyperbolic cosine."));
    v.push(doc!("tanh", None, "tanh(x: float) -> float", "float", "Returns the hyperbolic tangent."));
    v.push(doc!("sqrt", None, "sqrt(s: float) -> float", "float", "Returns the square root of s."));
    v.push(doc!("pow", None, "pow(base: float, exp: float) -> float", "float", "Returns base raised to power exp."));
    v.push(doc!("exp", None, "exp(s: float) -> float", "float", "Returns the natural exponential of s (e^s)."));
    v.push(doc!("log", None, "log(s: float) -> float", "float", "Returns the natural logarithm of s."));
    v.push(doc!("abs", None, "abs(s: Variant) -> Variant", "Variant", "Returns the absolute value of s."));
    v.push(doc!("absf", None, "absf(x: float) -> float", "float", "Returns the float absolute value of x."));
    v.push(doc!("absi", None, "absi(x: int) -> int", "int", "Returns the integer absolute value of x."));
    v.push(doc!("sign", None, "sign(s: Variant) -> Variant", "Variant", "Returns -1 for negative, 1 for positive, 0 for zero."));
    v.push(doc!("signf", None, "signf(x: float) -> float", "float", "Returns -1.0, 0.0, or 1.0 depending on sign of x."));
    v.push(doc!("signi", None, "signi(x: int) -> int", "int", "Returns -1, 0, or 1 depending on sign of x."));
    v.push(doc!("floor", None, "floor(s: Variant) -> Variant", "Variant", "Rounds downward to the nearest integer."));
    v.push(doc!("floorf", None, "floorf(s: float) -> float", "float", "Rounds float downward to the nearest integral float."));
    v.push(doc!("floori", None, "floori(s: float) -> int", "int", "Rounds float downward to an int."));
    v.push(doc!("ceil", None, "ceil(s: Variant) -> Variant", "Variant", "Rounds upward to the nearest integer."));
    v.push(doc!("ceilf", None, "ceilf(s: float) -> float", "float", "Rounds float upward to the nearest integral float."));
    v.push(doc!("ceili", None, "ceili(s: float) -> int", "int", "Rounds float upward to an int."));
    v.push(doc!("round", None, "round(s: Variant) -> Variant", "Variant", "Rounds to the nearest integer."));
    v.push(doc!("roundf", None, "roundf(s: float) -> float", "float", "Rounds to the nearest integral float."));
    v.push(doc!("roundi", None, "roundi(s: float) -> int", "int", "Rounds to the nearest int."));
    v.push(doc!("clamp", None, "clamp(value: Variant, min: Variant, max: Variant) -> Variant", "Variant", "Clamps value between min and max."));
    v.push(doc!("clampf", None, "clampf(value: float, min: float, max: float) -> float", "float", "Clamps float between min and max."));
    v.push(doc!("clampi", None, "clampi(value: int, min: int, max: int) -> int", "int", "Clamps int between min and max."));
    v.push(doc!("lerp", None, "lerp(from: Variant, to: Variant, weight: Variant) -> Variant", "Variant", "Linearly interpolates between from and to."));
    v.push(doc!("lerpf", None, "lerpf(from: float, to: float, weight: float) -> float", "float", "Linearly interpolates floats."));
    v.push(doc!("lerp_angle", None, "lerp_angle(from: float, to: float, weight: float) -> float", "float", "Interpolates angles in radians along the shortest arc."));
    v.push(doc!("inverse_lerp", None, "inverse_lerp(from: float, to: float, weight: float) -> float", "float", "Computes the interpolation factor t such that lerp(from, to, t) == weight."));
    v.push(doc!("remap", None, "remap(val: float, istart: float, istop: float, ostart: float, ostop: float) -> float", "float", "Maps a value from input range to output range."));
    v.push(doc!("smoothstep", None, "smoothstep(from: float, to: float, x: float) -> float", "float", "Returns smooth Hermite interpolation between from and to."));
    v.push(doc!("move_toward", None, "move_toward(from: float, to: float, delta: float) -> float", "float", "Moves from toward to by delta increment without overshooting."));
    v.push(doc!("rotate_toward", None, "rotate_toward(from: float, to: float, delta: float) -> float", "float", "Rotates angle from toward to by at most delta radians."));
    v.push(doc!("snapped", None, "snapped(s: Variant, step: Variant) -> Variant", "Variant", "Snaps float or vector to the nearest multiple of step."));
    v.push(doc!("snappedf", None, "snappedf(s: float, step: float) -> float", "float", "Snaps float to nearest multiple of step."));
    v.push(doc!("snappedi", None, "snappedi(s: int, step: int) -> int", "int", "Snaps integer to nearest multiple of step."));
    v.push(doc!("wrap", None, "wrap(value: Variant, min: Variant, max: Variant) -> Variant", "Variant", "Wraps value between min and max."));
    v.push(doc!("wrapf", None, "wrapf(value: float, min: float, max: float) -> float", "float", "Wraps float between min and max."));
    v.push(doc!("wrapi", None, "wrapi(value: int, min: int, max: int) -> int", "int", "Wraps integer between min and max."));
    v.push(doc!("pingpong", None, "pingpong(value: float, length: float) -> float", "float", "Wraps value so it moves back and forth between 0 and length."));
    v.push(doc!("fmod", None, "fmod(a: float, b: float) -> float", "float", "Floating-point remainder of a / b."));
    v.push(doc!("fposmod", None, "fposmod(a: float, b: float) -> float", "float", "Positive floating-point remainder of a / b."));
    v.push(doc!("posmod", None, "posmod(a: int, b: int) -> int", "int", "Positive integer remainder of a / b."));
    v.push(doc!("deg_to_rad", None, "deg_to_rad(deg: float) -> float", "float", "Converts degrees to radians."));
    v.push(doc!("rad_to_deg", None, "rad_to_deg(rad: float) -> float", "float", "Converts radians to degrees."));
    v.push(doc!("linear_to_db", None, "linear_to_db(lin: float) -> float", "float", "Converts a linear energy factor to decibels."));
    v.push(doc!("db_to_linear", None, "db_to_linear(db: float) -> float", "float", "Converts decibels to a linear factor."));
    v.push(doc!("is_nan", None, "is_nan(s: float) -> bool", "bool", "Returns true if s is NaN."));
    v.push(doc!("is_inf", None, "is_inf(s: float) -> bool", "bool", "Returns true if s is positive or negative infinity."));
    v.push(doc!("is_finite", None, "is_finite(s: float) -> bool", "bool", "Returns true if s is neither NaN nor infinite."));
    v.push(doc!("is_zero_approx", None, "is_zero_approx(s: float) -> bool", "bool", "Returns true if s is approximately zero."));
    v.push(doc!("is_equal_approx", None, "is_equal_approx(a: float, b: float) -> bool", "bool", "Returns true if a and b are approximately equal."));
    v.push(doc!("min", None, "min(a: Variant, b: Variant) -> Variant", "Variant", "Returns the minimum of two values."));
    v.push(doc!("max", None, "max(a: Variant, b: Variant) -> Variant", "Variant", "Returns the maximum of two values."));
    v.push(doc!("mini", None, "mini(a: int, b: int) -> int", "int", "Returns the minimum of two integers."));
    v.push(doc!("maxi", None, "maxi(a: int, b: int) -> int", "int", "Returns the maximum of two integers."));
    v.push(doc!("minf", None, "minf(a: float, b: float) -> float", "float", "Returns the minimum of two floats."));
    v.push(doc!("maxf", None, "maxf(a: float, b: float) -> float", "float", "Returns the maximum of two floats."));
    v.push(doc!("ease", None, "ease(x: float, curve: float) -> float", "float", "Easing function based on exponent curve."));
    v.push(doc!("bezier_interpolate", None, "bezier_interpolate(p0: float, p1: float, p2: float, p3: float, t: float) -> float", "float", "Evaluates a cubic Bezier curve at t."));
    v.push(doc!("bezier_derivative", None, "bezier_derivative(p0: float, p1: float, p2: float, p3: float, t: float) -> float", "float", "Evaluates derivative of cubic Bezier curve at t."));
    v.push(doc!("cubic_interpolate", None, "cubic_interpolate(from: float, to: float, pre: float, post: float, weight: float) -> float", "float", "Cubic spline interpolation."));
    v.push(doc!("seed", None, "seed(s: int) -> void", "void", "Sets the random number generator seed."));
    v.push(doc!("randi", None, "randi() -> int", "int", "Returns a random 32-bit unsigned integer."));
    v.push(doc!("randf", None, "randf() -> float", "float", "Returns a random float in [0.0, 1.0)."));
    v.push(doc!("randfn", None, "randfn(mean: float, deviation: float) -> float", "float", "Returns normally distributed random float."));
    v.push(doc!("randf_range", None, "randf_range(from: float, to: float) -> float", "float", "Returns random float between from and to."));
    v.push(doc!("randi_range", None, "randi_range(from: int, to: int) -> int", "int", "Returns random int between from and to."));
    v.push(doc!("typeof", None, "typeof(variable: Variant) -> int", "int", "Returns the internal Variant::Type code."));
    v.push(doc!("type_string", None, "type_string(type_id: int) -> String", "String", "Returns the name of a given Variant::Type enum code."));
    v.push(doc!("str", None, "str(...args) -> String", "String", "Converts arguments to a string representation."));
    v.push(doc!("int", None, "int(from: Variant) -> int", "int", "Converts a value to an integer."));
    v.push(doc!("float", None, "float(from: Variant) -> float", "float", "Converts a value to a float."));
    v.push(doc!("bool", None, "bool(from: Variant) -> bool", "bool", "Converts a value to a boolean."));
    v.push(doc!("len", None, "len(var: Variant) -> int", "int", "Returns length of array, string, or dictionary."));
    v.push(doc!("range", None, "range(...args) -> Array", "Array", "Generates an array of integers over an interval."));
    v.push(doc!("hash", None, "hash(variable: Variant) -> int", "int", "Computes integer hash of variable."));
    v.push(doc!("var_to_str", None, "var_to_str(variable: Variant) -> String", "String", "Serializes a Variant to GDScript expression string."));
    v.push(doc!("str_to_var", None, "str_to_var(string: String) -> Variant", "Variant", "Deserializes GDScript string to Variant."));
    v.push(doc!("prints", None, "prints(...args) -> void", "void", "Prints arguments separated by spaces."));
    v.push(doc!("printt", None, "printt(...args) -> void", "void", "Prints arguments separated by tabs."));
    v.push(doc!("printerr", None, "printerr(...args) -> void", "void", "Prints arguments to standard error."));
    v.push(doc!("push_warning", None, "push_warning(...args) -> void", "void", "Emits a warning diagnostic message."));
    v.push(doc!("push_error", None, "push_error(...args) -> void", "void", "Emits an error diagnostic message."));
    v.push(doc!("assert", None, "assert(condition: bool, message: String = \"\") -> void", "void", "Asserts condition is true; fails otherwise."));

    // =========================================================================
    // 2. String Methods (~60 entries)
    // =========================================================================
    let s = Some("String");
    v.push(doc!("length", s, "length() -> int", "int", "Returns number of characters in the string."));
    v.push(doc!("is_empty", s, "is_empty() -> bool", "bool", "Returns true if string length is 0."));
    v.push(doc!("to_upper", s, "to_upper() -> String", "String", "Returns uppercase copy."));
    v.push(doc!("to_lower", s, "to_lower() -> String", "String", "Returns lowercase copy."));
    v.push(doc!("capitalize", s, "capitalize() -> String", "String", "Capitalizes words separated by underscores."));
    v.push(doc!("to_snake_case", s, "to_snake_case() -> String", "String", "Converts PascalCase or camelCase to snake_case."));
    v.push(doc!("to_camel_case", s, "to_camel_case() -> String", "String", "Converts snake_case to camelCase."));
    v.push(doc!("to_pascal_case", s, "to_pascal_case() -> String", "String", "Converts snake_case to PascalCase."));
    v.push(doc!("strip_edges", s, "strip_edges(left: bool = true, right: bool = true) -> String", "String", "Strips whitespace edges."));
    v.push(doc!("lstrip", s, "lstrip(chars: String) -> String", "String", "Strips given characters from left edge."));
    v.push(doc!("rstrip", s, "rstrip(chars: String) -> String", "String", "Strips given characters from right edge."));
    v.push(doc!("trim_prefix", s, "trim_prefix(prefix: String) -> String", "String", "Removes prefix if string starts with it."));
    v.push(doc!("trim_suffix", s, "trim_suffix(suffix: String) -> String", "String", "Removes suffix if string ends with it."));
    v.push(doc!("begins_with", s, "begins_with(text: String) -> bool", "bool", "Returns true if string starts with text."));
    v.push(doc!("ends_with", s, "ends_with(text: String) -> bool", "bool", "Returns true if string ends with text."));
    v.push(doc!("contains", s, "contains(what: String) -> bool", "bool", "Returns true if string contains substring what."));
    v.push(doc!("find", s, "find(what: String, from: int = 0) -> int", "int", "Returns first index of what, or -1."));
    v.push(doc!("rfind", s, "rfind(what: String, from: int = -1) -> int", "int", "Returns last index of what, or -1."));
    v.push(doc!("count", s, "count(what: String, from: int = 0, to: int = 0) -> int", "int", "Counts occurrences of what."));
    v.push(doc!("replace", s, "replace(what: String, forwhat: String) -> String", "String", "Replaces occurrences of what with forwhat."));
    v.push(doc!("substr", s, "substr(from: int, len: int = -1) -> String", "String", "Extracts substring starting at from with length len."));
    v.push(doc!("left", s, "left(length: int) -> String", "String", "Returns first length characters from start."));
    v.push(doc!("right", s, "right(length: int) -> String", "String", "Returns length characters from end."));
    v.push(doc!("split", s, "split(delimiter: String = \"\", allow_empty: bool = true, maxsplit: int = 0) -> Array", "Array", "Splits string by delimiter."));
    v.push(doc!("rsplit", s, "rsplit(delimiter: String = \"\", allow_empty: bool = true, maxsplit: int = 0) -> Array", "Array", "Splits string by delimiter from right."));
    v.push(doc!("join", s, "join(parts: Array) -> String", "String", "Concatenates parts using this string as separator."));
    v.push(doc!("repeat", s, "repeat(count: int) -> String", "String", "Repeats string count times."));
    v.push(doc!("reverse", s, "reverse() -> String", "String", "Returns reversed string."));
    v.push(doc!("pad_zeros", s, "pad_zeros(digits: int) -> String", "String", "Pads integer portion with leading zeros."));
    v.push(doc!("pad_decimals", s, "pad_decimals(digits: int) -> String", "String", "Pads decimal portion with trailing zeros."));
    v.push(doc!("is_valid_int", s, "is_valid_int() -> bool", "bool", "Returns true if string is valid integer."));
    v.push(doc!("is_valid_float", s, "is_valid_float() -> bool", "bool", "Returns true if string is valid float."));
    v.push(doc!("is_valid_hex_number", s, "is_valid_hex_number(with_prefix: bool = false) -> bool", "bool", "Returns true if string is hexadecimal."));
    v.push(doc!("to_int", s, "to_int() -> int", "int", "Converts string to int."));
    v.push(doc!("to_float", s, "to_float() -> float", "float", "Converts string to float."));
    v.push(doc!("md5_text", s, "md5_text() -> String", "String", "Returns MD5 hash as hex string."));
    v.push(doc!("sha256_text", s, "sha256_text() -> String", "String", "Returns SHA256 hash as hex string."));
    v.push(doc!("hash", s, "hash() -> int", "int", "Returns 32-bit integer hash of string."));
    v.push(doc!("get_file", s, "get_file() -> String", "String", "Returns filename from path."));
    v.push(doc!("get_base_dir", s, "get_base_dir() -> String", "String", "Returns directory part of path."));
    v.push(doc!("get_extension", s, "get_extension() -> String", "String", "Returns file extension without dot."));
    v.push(doc!("get_basename", s, "get_basename() -> String", "String", "Returns path without extension."));
    v.push(doc!("path_join", s, "path_join(file: String) -> String", "String", "Concatenates filesystem paths cleanly."));
    v.push(doc!("format", s, "format(values: Variant, placeholder: String = \"{_}\") -> String", "String", "Formats placeholders."));
    v.push(doc!("c_escape", s, "c_escape() -> String", "String", "Escapes string for C/GDScript string literals."));
    v.push(doc!("c_unescape", s, "c_unescape() -> String", "String", "Unescapes C-style escape characters."));
    v.push(doc!("xml_escape", s, "xml_escape(escape_quotes: bool = false) -> String", "String", "Escapes XML entities."));
    v.push(doc!("xml_unescape", s, "xml_unescape() -> String", "String", "Unescapes XML entities."));
    v.push(doc!("uri_encode", s, "uri_encode() -> String", "String", "Percent-encodes URI characters."));
    v.push(doc!("uri_decode", s, "uri_decode() -> String", "String", "Decodes percent-encoded URI string."));
    v.push(doc!("humanize_size", s, "humanize_size(size: int) -> String", "String", "Formats byte count into KB/MB/GB."));
    v.push(doc!("insert", s, "insert(position: int, what: String) -> String", "String", "Inserts what at position."));
    v.push(doc!("erase", s, "erase(position: int, chars: int = 1) -> String", "String", "Erases chars starting at position."));
    v.push(doc!("similarity", s, "similarity(text: String) -> float", "float", "Returns string similarity ratio [0.0, 1.0]."));
    v.push(doc!("validate_node_name", s, "validate_node_name() -> String", "String", "Sanitizes string into a valid Node name."));
    v.push(doc!("is_subsequence_of", s, "is_subsequence_of(text: String) -> bool", "bool", "Checks if string is subsequence of text."));
    v.push(doc!("simplify_path", s, "simplify_path() -> String", "String", "Normalizes . and .. in file path."));
    v.push(doc!("is_absolute_path", s, "is_absolute_path() -> bool", "bool", "Returns true if path is absolute."));
    v.push(doc!("is_relative_path", s, "is_relative_path() -> bool", "bool", "Returns true if path is relative."));
    v.push(doc!("unicode_at", s, "unicode_at(at: int) -> int", "int", "Returns Unicode codepoint at character index."));

    // =========================================================================
    // 3. Array Methods (~50 entries)
    // =========================================================================
    let a = Some("Array");
    v.push(doc!("size", a, "size() -> int", "int", "Returns number of elements in array."));
    v.push(doc!("is_empty", a, "is_empty() -> bool", "bool", "Returns true if array has 0 elements."));
    v.push(doc!("push_back", a, "push_back(value: Variant) -> void", "void", "Appends element to end of array."));
    v.push(doc!("push_front", a, "push_front(value: Variant) -> void", "void", "Prepends element to beginning of array."));
    v.push(doc!("append", a, "append(value: Variant) -> void", "void", "Appends element to end of array."));
    v.push(doc!("append_array", a, "append_array(array: Array) -> void", "void", "Appends all elements from another array."));
    v.push(doc!("pop_back", a, "pop_back() -> Variant", "Variant", "Removes and returns last element."));
    v.push(doc!("pop_front", a, "pop_front() -> Variant", "Variant", "Removes and returns first element."));
    v.push(doc!("pop_at", a, "pop_at(position: int) -> Variant", "Variant", "Removes and returns element at position."));
    v.push(doc!("insert", a, "insert(position: int, value: Variant) -> int", "int", "Inserts element at position."));
    v.push(doc!("remove_at", a, "remove_at(position: int) -> void", "void", "Removes element at index position."));
    v.push(doc!("erase", a, "erase(value: Variant) -> void", "void", "Removes first matching element."));
    v.push(doc!("clear", a, "clear() -> void", "void", "Clears all elements from array."));
    v.push(doc!("has", a, "has(value: Variant) -> bool", "bool", "Returns true if array contains value."));
    v.push(doc!("find", a, "find(what: Variant, from: int = 0) -> int", "int", "Returns index of first match or -1."));
    v.push(doc!("rfind", a, "rfind(what: Variant, from: int = -1) -> int", "int", "Returns index of last match or -1."));
    v.push(doc!("count", a, "count(value: Variant) -> int", "int", "Returns count of occurrences of value."));
    v.push(doc!("slice", a, "slice(begin: int, end: int = 2147483647, step: int = 1) -> Array", "Array", "Returns shallow copy slice."));
    v.push(doc!("duplicate", a, "duplicate(deep: bool = false) -> Array", "Array", "Duplicates array."));
    v.push(doc!("reverse", a, "reverse() -> void", "void", "Reverses array in-place."));
    v.push(doc!("sort", a, "sort() -> void", "void", "Sorts elements in ascending order."));
    v.push(doc!("sort_custom", a, "sort_custom(func: Callable) -> void", "void", "Sorts array using custom comparator."));
    v.push(doc!("shuffle", a, "shuffle() -> void", "void", "Shuffles elements randomly."));
    v.push(doc!("bsearch", a, "bsearch(value: Variant, before: bool = true) -> int", "int", "Binary searches sorted array."));
    v.push(doc!("bsearch_custom", a, "bsearch_custom(value: Variant, func: Callable, before: bool = true) -> int", "int", "Binary searches with custom comparator."));
    v.push(doc!("min", a, "min() -> Variant", "Variant", "Returns minimum element."));
    v.push(doc!("max", a, "max() -> Variant", "Variant", "Returns maximum element."));
    v.push(doc!("front", a, "front() -> Variant", "Variant", "Returns first element."));
    v.push(doc!("back", a, "back() -> Variant", "Variant", "Returns last element."));
    v.push(doc!("pick_random", a, "pick_random() -> Variant", "Variant", "Returns random element from array."));
    v.push(doc!("fill", a, "fill(value: Variant) -> void", "void", "Fills array with specified value."));
    v.push(doc!("resize", a, "resize(size: int) -> int", "int", "Resizes array length."));
    v.push(doc!("has_all", a, "has_all(values: Array) -> bool", "bool", "Returns true if array has all values."));
    v.push(doc!("is_read_only", a, "is_read_only() -> bool", "bool", "Returns true if array is read-only."));
    v.push(doc!("make_read_only", a, "make_read_only() -> void", "void", "Makes array immutable."));
    v.push(doc!("map", a, "map(method: Callable) -> Array", "Array", "Calls callable for each element and returns results array."));
    v.push(doc!("filter", a, "filter(method: Callable) -> Array", "Array", "Returns array of elements satisfying predicate."));
    v.push(doc!("reduce", a, "reduce(method: Callable, accum: Variant = null) -> Variant", "Variant", "Reduces array to a single value via accumulator."));
    v.push(doc!("any", a, "any(method: Callable) -> bool", "bool", "Returns true if predicate matches at least one element."));
    v.push(doc!("all", a, "all(method: Callable) -> bool", "bool", "Returns true if predicate matches all elements."));
    v.push(doc!("hash", a, "hash() -> int", "int", "Computes integer hash of array contents."));
    v.push(doc!("is_typed", a, "is_typed() -> bool", "bool", "Returns true if array has element type constraint."));
    v.push(doc!("get_typed_builtin", a, "get_typed_builtin() -> int", "int", "Returns builtin type ID for typed array."));
    v.push(doc!("get_typed_class_name", a, "get_typed_class_name() -> StringName", "StringName", "Returns class name constraint."));
    v.push(doc!("get_typed_script", a, "get_typed_script() -> Variant", "Variant", "Returns script constraint."));
    v.push(doc!("assign", a, "assign(array: Array) -> void", "void", "Assigns elements from another array cleanly."));
    v.push(doc!("find_custom", a, "find_custom(method: Callable, from: int = 0) -> int", "int", "Finds index matching custom predicate."));
    v.push(doc!("rfind_custom", a, "rfind_custom(method: Callable, from: int = -1) -> int", "int", "Finds reverse index matching custom predicate."));
    v.push(doc!("count_custom", a, "count_custom(method: Callable) -> int", "int", "Counts elements matching predicate."));
    v.push(doc!("all_custom", a, "all_custom(method: Callable) -> bool", "bool", "Returns true if all elements match."));

    // =========================================================================
    // 4. Dictionary Methods (~25 entries)
    // =========================================================================
    let d = Some("Dictionary");
    v.push(doc!("size", d, "size() -> int", "int", "Returns number of key-value pairs in dictionary."));
    v.push(doc!("is_empty", d, "is_empty() -> bool", "bool", "Returns true if dictionary is empty."));
    v.push(doc!("has", d, "has(key: Variant) -> bool", "bool", "Returns true if key exists."));
    v.push(doc!("has_all", d, "has_all(keys: Array) -> bool", "bool", "Returns true if all given keys exist."));
    v.push(doc!("get", d, "get(key: Variant, default: Variant = null) -> Variant", "Variant", "Gets value by key with optional fallback."));
    v.push(doc!("keys", d, "keys() -> Array", "Array", "Returns array of all keys."));
    v.push(doc!("values", d, "values() -> Array", "Array", "Returns array of all values."));
    v.push(doc!("erase", d, "erase(key: Variant) -> bool", "bool", "Removes key-value pair; returns true if key existed."));
    v.push(doc!("clear", d, "clear() -> void", "void", "Clears all entries from dictionary."));
    v.push(doc!("merge", d, "merge(dictionary: Dictionary, overwrite: bool = false) -> void", "void", "Merges entries into this dictionary."));
    v.push(doc!("merged", d, "merged(dictionary: Dictionary, overwrite: bool = false) -> Dictionary", "Dictionary", "Returns merged copy without mutating caller."));
    v.push(doc!("duplicate", d, "duplicate(deep: bool = false) -> Dictionary", "Dictionary", "Duplicates dictionary."));
    v.push(doc!("find_key", d, "find_key(value: Variant) -> Variant", "Variant", "Finds first key associated with value."));
    v.push(doc!("make_read_only", d, "make_read_only() -> void", "void", "Makes dictionary immutable."));
    v.push(doc!("is_read_only", d, "is_read_only() -> bool", "bool", "Returns true if dictionary is read-only."));
    v.push(doc!("sort", d, "sort() -> void", "void", "Sorts dictionary entries by key."));
    v.push(doc!("get_or_add", d, "get_or_add(key: Variant, default: Variant = null) -> Variant", "Variant", "Gets key or creates it with default."));
    v.push(doc!("hash", d, "hash() -> int", "int", "Returns integer hash of dictionary contents."));
    v.push(doc!("is_typed", d, "is_typed() -> bool", "bool", "Returns true if dictionary has key/value types."));
    v.push(doc!("get_typed_key_builtin", d, "get_typed_key_builtin() -> int", "int", "Returns type ID for keys."));
    v.push(doc!("get_typed_value_builtin", d, "get_typed_value_builtin() -> int", "int", "Returns type ID for values."));
    v.push(doc!("assign", d, "assign(dictionary: Dictionary) -> void", "void", "Assigns entries from another dictionary."));
    v.push(doc!("is_same", d, "is_same(other: Dictionary) -> bool", "bool", "Checks dictionary reference equality."));
    v.push(doc!("recursive_equal", d, "recursive_equal(other: Dictionary, recursion_count: int) -> bool", "bool", "Deeply compares dictionaries."));
    v.push(doc!("format", d, "format(template: String) -> String", "String", "Formats string with dictionary keys."));

    // =========================================================================
    // 5. Vector2 Methods (~30 entries)
    // =========================================================================
    let v2 = Some("Vector2");
    v.push(doc!("length", v2, "length() -> float", "float", "Returns length / magnitude of vector."));
    v.push(doc!("length_squared", v2, "length_squared() -> float", "float", "Returns squared length without square root."));
    v.push(doc!("normalized", v2, "normalized() -> Vector2", "Vector2", "Returns unit vector."));
    v.push(doc!("is_normalized", v2, "is_normalized() -> bool", "bool", "Returns true if length is approximately 1.0."));
    v.push(doc!("distance_to", v2, "distance_to(to: Vector2) -> float", "float", "Returns Euclidean distance to target."));
    v.push(doc!("distance_squared_to", v2, "distance_squared_to(to: Vector2) -> float", "float", "Returns squared distance to target."));
    v.push(doc!("direction_to", v2, "direction_to(to: Vector2) -> Vector2", "Vector2", "Returns normalized direction pointing toward to."));
    v.push(doc!("dot", v2, "dot(with: Vector2) -> float", "float", "Returns dot product."));
    v.push(doc!("cross", v2, "cross(with: Vector2) -> float", "float", "Returns 2D cross product scalar."));
    v.push(doc!("angle", v2, "angle() -> float", "float", "Returns vector angle relative to positive X-axis in radians."));
    v.push(doc!("angle_to", v2, "angle_to(to: Vector2) -> float", "float", "Returns angle to vector in radians."));
    v.push(doc!("angle_to_point", v2, "angle_to_point(to: Vector2) -> float", "float", "Returns angle to point in radians."));
    v.push(doc!("rotated", v2, "rotated(angle: float) -> Vector2", "Vector2", "Returns vector rotated by angle radians."));
    v.push(doc!("orthogonal", v2, "orthogonal() -> Vector2", "Vector2", "Returns perpendicular vector rotated 90 degrees counter-clockwise."));
    v.push(doc!("sign", v2, "sign() -> Vector2", "Vector2", "Returns component-wise sign vector."));
    v.push(doc!("floor", v2, "floor() -> Vector2", "Vector2", "Rounds components downward."));
    v.push(doc!("ceil", v2, "ceil() -> Vector2", "Vector2", "Rounds components upward."));
    v.push(doc!("round", v2, "round() -> Vector2", "Vector2", "Rounds components to nearest integers."));
    v.push(doc!("abs", v2, "abs() -> Vector2", "Vector2", "Returns vector with absolute components."));
    v.push(doc!("clamp", v2, "clamp(min: Vector2, max: Vector2) -> Vector2", "Vector2", "Clamps components between min and max."));
    v.push(doc!("snapped", v2, "snapped(step: Vector2) -> Vector2", "Vector2", "Snaps components to multiples of step."));
    v.push(doc!("min", v2, "min(with: Vector2) -> Vector2", "Vector2", "Component-wise minimum."));
    v.push(doc!("max", v2, "max(with: Vector2) -> Vector2", "Vector2", "Component-wise maximum."));
    v.push(doc!("aspect", v2, "aspect() -> float", "float", "Returns ratio of width to height (x / y)."));
    v.push(doc!("lerp", v2, "lerp(to: Vector2, weight: float) -> Vector2", "Vector2", "Linearly interpolates to target."));
    v.push(doc!("slerp", v2, "slerp(to: Vector2, weight: float) -> Vector2", "Vector2", "Spherical linear interpolation."));
    v.push(doc!("move_toward", v2, "move_toward(to: Vector2, delta: float) -> Vector2", "Vector2", "Moves toward target without overshooting."));
    v.push(doc!("slide", v2, "slide(normal: Vector2) -> Vector2", "Vector2", "Slides vector along a plane normal."));
    v.push(doc!("bounce", v2, "bounce(normal: Vector2) -> Vector2", "Vector2", "Bounces vector off surface normal."));
    v.push(doc!("reflect", v2, "reflect(normal: Vector2) -> Vector2", "Vector2", "Reflects vector off surface normal."));

    // =========================================================================
    // 6. Vector3 Methods (~35 entries)
    // =========================================================================
    let v3 = Some("Vector3");
    v.push(doc!("length", v3, "length() -> float", "float", "Returns magnitude of vector."));
    v.push(doc!("length_squared", v3, "length_squared() -> float", "float", "Returns squared magnitude."));
    v.push(doc!("normalized", v3, "normalized() -> Vector3", "Vector3", "Returns unit vector."));
    v.push(doc!("is_normalized", v3, "is_normalized() -> bool", "bool", "Returns true if vector length is approx 1.0."));
    v.push(doc!("distance_to", v3, "distance_to(to: Vector3) -> float", "float", "Returns Euclidean distance to target."));
    v.push(doc!("distance_squared_to", v3, "distance_squared_to(to: Vector3) -> float", "float", "Returns squared Euclidean distance."));
    v.push(doc!("direction_to", v3, "direction_to(to: Vector3) -> Vector3", "Vector3", "Returns normalized direction toward to."));
    v.push(doc!("dot", v3, "dot(with: Vector3) -> float", "float", "Returns dot product."));
    v.push(doc!("cross", v3, "cross(with: Vector3) -> Vector3", "Vector3", "Returns 3D cross product vector."));
    v.push(doc!("angle_to", v3, "angle_to(to: Vector3) -> float", "float", "Returns angle to target vector in radians."));
    v.push(doc!("signed_angle_to", v3, "signed_angle_to(to: Vector3, axis: Vector3) -> float", "float", "Returns signed angle around axis."));
    v.push(doc!("rotated", v3, "rotated(axis: Vector3, angle: float) -> Vector3", "Vector3", "Rotates around axis by angle radians."));
    v.push(doc!("lerp", v3, "lerp(to: Vector3, weight: float) -> Vector3", "Vector3", "Linearly interpolates toward target."));
    v.push(doc!("slerp", v3, "slerp(to: Vector3, weight: float) -> Vector3", "Vector3", "Spherically interpolates toward target."));
    v.push(doc!("cubic_interpolate", v3, "cubic_interpolate(b: Vector3, pre: Vector3, post: Vector3, weight: float) -> Vector3", "Vector3", "Cubic spline interpolation."));
    v.push(doc!("move_toward", v3, "move_toward(to: Vector3, delta: float) -> Vector3", "Vector3", "Moves toward target without overshooting."));
    v.push(doc!("slide", v3, "slide(normal: Vector3) -> Vector3", "Vector3", "Slides vector along plane normal."));
    v.push(doc!("bounce", v3, "bounce(normal: Vector3) -> Vector3", "Vector3", "Bounces vector off surface normal."));
    v.push(doc!("reflect", v3, "reflect(normal: Vector3) -> Vector3", "Vector3", "Reflects vector off surface normal."));
    v.push(doc!("project", v3, "project(b: Vector3) -> Vector3", "Vector3", "Projects vector onto vector b."));
    v.push(doc!("limit_length", v3, "limit_length(length: float = 1.0) -> Vector3", "Vector3", "Limits vector length to maximum length."));
    v.push(doc!("snapped", v3, "snapped(step: Vector3) -> Vector3", "Vector3", "Snaps components to multiples of step."));
    v.push(doc!("abs", v3, "abs() -> Vector3", "Vector3", "Returns absolute component vector."));
    v.push(doc!("floor", v3, "floor() -> Vector3", "Vector3", "Rounds components downward."));
    v.push(doc!("ceil", v3, "ceil() -> Vector3", "Vector3", "Rounds components upward."));
    v.push(doc!("round", v3, "round() -> Vector3", "Vector3", "Rounds components to nearest integers."));
    v.push(doc!("clamp", v3, "clamp(min: Vector3, max: Vector3) -> Vector3", "Vector3", "Clamps components between min and max."));
    v.push(doc!("min", v3, "min(with: Vector3) -> Vector3", "Vector3", "Component-wise minimum."));
    v.push(doc!("max", v3, "max(with: Vector3) -> Vector3", "Vector3", "Component-wise maximum."));
    v.push(doc!("octahedron_encode", v3, "octahedron_encode() -> Vector2", "Vector2", "Encodes normalized normal into octahedron UV."));
    v.push(doc!("octahedron_decode", v3, "octahedron_decode(uv: Vector2) -> Vector3", "Vector3", "Decodes octahedron UV to unit normal."));
    v.push(doc!("inverse", v3, "inverse() -> Vector3", "Vector3", "Component-wise reciprocal 1 / v."));
    v.push(doc!("is_equal_approx", v3, "is_equal_approx(to: Vector3) -> bool", "bool", "Returns true if components are approx equal."));
    v.push(doc!("is_zero_approx", v3, "is_zero_approx() -> bool", "bool", "Returns true if vector is approx zero."));
    v.push(doc!("is_finite", v3, "is_finite() -> bool", "bool", "Returns true if all components are finite."));

    // =========================================================================
    // 7. Vector4, Color, Rect2, Transforms, Basis, Quat, Plane, AABB (~110 entries)
    // =========================================================================
    let v4 = Some("Vector4");
    v.push(doc!("length", v4, "length() -> float", "float", "Returns magnitude of 4D vector."));
    v.push(doc!("length_squared", v4, "length_squared() -> float", "float", "Returns squared magnitude."));
    v.push(doc!("normalized", v4, "normalized() -> Vector4", "Vector4", "Returns unit 4D vector."));
    v.push(doc!("dot", v4, "dot(with: Vector4) -> float", "float", "Dot product of 4D vectors."));
    v.push(doc!("direction_to", v4, "direction_to(to: Vector4) -> Vector4", "Vector4", "Normalized direction toward to."));
    v.push(doc!("distance_to", v4, "distance_to(to: Vector4) -> float", "float", "Euclidean distance to to."));
    v.push(doc!("lerp", v4, "lerp(to: Vector4, weight: float) -> Vector4", "Vector4", "Linear interpolation."));
    v.push(doc!("abs", v4, "abs() -> Vector4", "Vector4", "Component-wise absolute values."));
    v.push(doc!("floor", v4, "floor() -> Vector4", "Vector4", "Rounds components downward."));
    v.push(doc!("clamp", v4, "clamp(min: Vector4, max: Vector4) -> Vector4", "Vector4", "Clamps components between min and max."));

    let clr = Some("Color");
    v.push(doc!("lightened", clr, "lightened(amount: float) -> Color", "Color", "Returns lightened color."));
    v.push(doc!("darkened", clr, "darkened(amount: float) -> Color", "Color", "Returns darkened color."));
    v.push(doc!("inverted", clr, "inverted() -> Color", "Color", "Inverts RGB components."));
    v.push(doc!("lerp", clr, "lerp(to: Color, weight: float) -> Color", "Color", "Linear interpolation of RGBA components."));
    v.push(doc!("to_html", clr, "to_html(with_alpha: bool = true) -> String", "String", "Converts color to hex HTML string."));
    v.push(doc!("to_argb32", clr, "to_argb32() -> int", "int", "Returns 32-bit ARGB integer."));
    v.push(doc!("to_rgba32", clr, "to_rgba32() -> int", "int", "Returns 32-bit RGBA integer."));
    v.push(doc!("to_abgr32", clr, "to_abgr32() -> int", "int", "Returns 32-bit ABGR integer."));
    v.push(doc!("from_hsv", clr, "from_hsv(h: float, s: float, v: float, alpha: float = 1.0) -> Color", "Color", "Constructs color from HSV channels."));
    v.push(doc!("to_hsv", clr, "to_hsv() -> Vector4", "Vector4", "Extracts HSV and alpha channels."));
    v.push(doc!("srgb_to_linear", clr, "srgb_to_linear() -> Color", "Color", "Converts sRGB color space to linear space."));
    v.push(doc!("linear_to_srgb", clr, "linear_to_srgb() -> Color", "Color", "Converts linear color space to sRGB."));
    v.push(doc!("blend", clr, "blend(over: Color) -> Color", "Color", "Blends another color over this color."));
    v.push(doc!("clamp", clr, "clamp(min: Color = Color(0,0,0,0), max: Color = Color(1,1,1,1)) -> Color", "Color", "Clamps color channels."));
    v.push(doc!("get_luminance", clr, "get_luminance() -> float", "float", "Returns relative perceptual luminance."));

    let r2 = Some("Rect2");
    v.push(doc!("intersects", r2, "intersects(b: Rect2, include_borders: bool = false) -> bool", "bool", "Returns true if rectangles overlap."));
    v.push(doc!("encloses", r2, "encloses(b: Rect2) -> bool", "bool", "Returns true if rectangle encloses b."));
    v.push(doc!("has_point", r2, "has_point(point: Vector2) -> bool", "bool", "Returns true if point is inside rectangle."));
    v.push(doc!("expand", r2, "expand(to: Vector2) -> Rect2", "Rect2", "Returns bounding rectangle enclosing to point."));
    v.push(doc!("grow", r2, "grow(amount: float) -> Rect2", "Rect2", "Expands all sides by amount."));
    v.push(doc!("grow_side", r2, "grow_side(side: int, amount: float) -> Rect2", "Rect2", "Expands specific side by amount."));
    v.push(doc!("grow_individual", r2, "grow_individual(left: float, top: float, right: float, bottom: float) -> Rect2", "Rect2", "Expands individual sides."));
    v.push(doc!("merge", r2, "merge(b: Rect2) -> Rect2", "Rect2", "Returns union bounding box."));
    v.push(doc!("intersection", r2, "intersection(b: Rect2) -> Rect2", "Rect2", "Returns overlapping intersection rectangle."));
    v.push(doc!("abs", r2, "abs() -> Rect2", "Rect2", "Returns rectangle with non-negative size."));
    v.push(doc!("get_area", r2, "get_area() -> float", "float", "Returns width * height."));
    v.push(doc!("get_center", r2, "get_center() -> Vector2", "Vector2", "Returns center position of rectangle."));

    let t2 = Some("Transform2D");
    v.push(doc!("inverse", t2, "inverse() -> Transform2D", "Transform2D", "Returns inverse transform."));
    v.push(doc!("affine_inverse", t2, "affine_inverse() -> Transform2D", "Transform2D", "Returns affine inverse."));
    v.push(doc!("rotated", t2, "rotated(angle: float) -> Transform2D", "Transform2D", "Returns rotated transform."));
    v.push(doc!("scaled", t2, "scaled(scale: Vector2) -> Transform2D", "Transform2D", "Returns scaled transform."));
    v.push(doc!("translated", t2, "translated(offset: Vector2) -> Transform2D", "Transform2D", "Returns translated transform."));
    v.push(doc!("orthonormalized", t2, "orthonormalized() -> Transform2D", "Transform2D", "Orthonormalizes basis vectors."));
    v.push(doc!("interpolate_with", t2, "interpolate_with(xform: Transform2D, weight: float) -> Transform2D", "Transform2D", "Transform interpolation."));
    v.push(doc!("looking_at", t2, "looking_at(target: Vector2) -> Transform2D", "Transform2D", "Transforms facing target point."));
    v.push(doc!("get_rotation", t2, "get_rotation() -> float", "float", "Extracts rotation angle."));
    v.push(doc!("get_scale", t2, "get_scale() -> Vector2", "Vector2", "Extracts scale factor."));

    let t3 = Some("Transform3D");
    v.push(doc!("inverse", t3, "inverse() -> Transform3D", "Transform3D", "Returns inverse transform."));
    v.push(doc!("affine_inverse", t3, "affine_inverse() -> Transform3D", "Transform3D", "Returns affine inverse transform."));
    v.push(doc!("orthonormalized", t3, "orthonormalized() -> Transform3D", "Transform3D", "Orthonormalizes basis."));
    v.push(doc!("rotated", t3, "rotated(axis: Vector3, angle: float) -> Transform3D", "Transform3D", "Rotates around axis."));
    v.push(doc!("scaled", t3, "scaled(scale: Vector3) -> Transform3D", "Transform3D", "Scales transform."));
    v.push(doc!("translated", t3, "translated(offset: Vector3) -> Transform3D", "Transform3D", "Translates transform origin."));
    v.push(doc!("looking_at", t3, "looking_at(target: Vector3, up: Vector3 = Vector3(0,1,0)) -> Transform3D", "Transform3D", "Transforms facing target point."));
    v.push(doc!("interpolate_with", t3, "interpolate_with(xform: Transform3D, weight: float) -> Transform3D", "Transform3D", "Transform interpolation."));
    v.push(doc!("origin", t3, "origin() -> Vector3", "Vector3", "Returns origin translation vector."));
    v.push(doc!("basis", t3, "basis() -> Basis", "Basis", "Returns rotation and scale basis."));

    let bas = Some("Basis");
    v.push(doc!("inverse", bas, "inverse() -> Basis", "Basis", "Returns inverse matrix."));
    v.push(doc!("transposed", bas, "transposed() -> Basis", "Basis", "Returns transposed matrix."));
    v.push(doc!("determinant", bas, "determinant() -> float", "float", "Returns matrix determinant."));
    v.push(doc!("orthonormalized", bas, "orthonormalized() -> Basis", "Basis", "Orthonormalizes basis columns via Gram-Schmidt."));
    v.push(doc!("is_orthogonal", bas, "is_orthogonal() -> bool", "bool", "Returns true if basis is orthogonal."));
    v.push(doc!("is_diagonal", bas, "is_diagonal() -> bool", "bool", "Returns true if matrix is diagonal."));
    v.push(doc!("is_conformal", bas, "is_conformal() -> bool", "bool", "Returns true if basis preserves angles."));
    v.push(doc!("slerp", bas, "slerp(target: Basis, weight: float) -> Basis", "Basis", "Spherical interpolation of orientation."));
    v.push(doc!("get_euler", bas, "get_euler(order: int = 2) -> Vector3", "Vector3", "Extracts Euler angles."));
    v.push(doc!("get_scale", bas, "get_scale() -> Vector3", "Vector3", "Extracts basis column lengths as scale."));
    v.push(doc!("get_rotation_quaternion", bas, "get_rotation_quaternion() -> Quaternion", "Quaternion", "Extracts rotation as quaternion."));
    v.push(doc!("looking_at", bas, "looking_at(target: Vector3, up: Vector3 = Vector3(0,1,0)) -> Basis", "Basis", "Constructs basis facing target."));

    let quat = Some("Quaternion");
    v.push(doc!("length", quat, "length() -> float", "float", "Returns magnitude of quaternion."));
    v.push(doc!("length_squared", quat, "length_squared() -> float", "float", "Returns squared magnitude."));
    v.push(doc!("normalized", quat, "normalized() -> Quaternion", "Quaternion", "Returns unit quaternion."));
    v.push(doc!("is_normalized", quat, "is_normalized() -> bool", "bool", "Returns true if magnitude is approx 1.0."));
    v.push(doc!("inverse", quat, "inverse() -> Quaternion", "Quaternion", "Returns conjugate inverse."));
    v.push(doc!("log", quat, "log() -> Quaternion", "Quaternion", "Returns quaternion natural logarithm."));
    v.push(doc!("exp", quat, "exp() -> Quaternion", "Quaternion", "Returns quaternion exponential."));
    v.push(doc!("angle_to", quat, "angle_to(to: Quaternion) -> float", "float", "Returns angle between rotations in radians."));
    v.push(doc!("dot", quat, "dot(b: Quaternion) -> float", "float", "Dot product of quaternions."));
    v.push(doc!("slerp", quat, "slerp(to: Quaternion, weight: float) -> Quaternion", "Quaternion", "Spherical linear interpolation."));
    v.push(doc!("spherical_cubic_interpolate", quat, "spherical_cubic_interpolate(b: Quaternion, pre: Quaternion, post: Quaternion, weight: float) -> Quaternion", "Quaternion", "Spherical cubic interpolation."));
    v.push(doc!("get_euler", quat, "get_euler(order: int = 2) -> Vector3", "Vector3", "Extracts Euler angles in radians."));
    v.push(doc!("get_angle", quat, "get_angle() -> float", "float", "Returns rotation angle in radians."));
    v.push(doc!("get_axis", quat, "get_axis() -> Vector3", "Vector3", "Returns rotation axis."));
    v.push(doc!("is_finite", quat, "is_finite() -> bool", "bool", "Returns true if all components are finite."));

    let pl = Some("Plane");
    v.push(doc!("normalized", pl, "normalized() -> Plane", "Plane", "Returns plane with normalized normal."));
    v.push(doc!("distance_to", pl, "distance_to(point: Vector3) -> float", "float", "Signed distance from plane to point."));
    v.push(doc!("has_point", pl, "has_point(point: Vector3, tolerance: float = 0.00001) -> bool", "bool", "Returns true if point lies on plane."));
    v.push(doc!("project", pl, "project(point: Vector3) -> Vector3", "Vector3", "Projects point orthogonally onto plane."));
    v.push(doc!("intersects_ray", pl, "intersects_ray(from: Vector3, dir: Vector3) -> Variant", "Variant", "Finds intersection point with ray."));
    v.push(doc!("intersects_segment", pl, "intersects_segment(begin: Vector3, end: Vector3) -> Variant", "Variant", "Finds intersection point with line segment."));
    v.push(doc!("is_equal_approx", pl, "is_equal_approx(to_plane: Plane) -> bool", "bool", "Returns true if planes are approximately equal."));
    v.push(doc!("is_finite", pl, "is_finite() -> bool", "bool", "Returns true if plane components are finite."));

    let aabb = Some("AABB");
    v.push(doc!("abs", aabb, "abs() -> AABB", "AABB", "Returns AABB with non-negative size."));
    v.push(doc!("get_area", aabb, "get_area() -> float", "float", "Surface area of AABB."));
    v.push(doc!("get_volume", aabb, "get_volume() -> float", "float", "Volume of AABB."));
    v.push(doc!("has_point", aabb, "has_point(point: Vector3) -> bool", "bool", "Returns true if point is inside box."));
    v.push(doc!("has_volume", aabb, "has_volume() -> bool", "bool", "Returns true if box has positive volume."));
    v.push(doc!("intersects", aabb, "intersects(b: AABB) -> bool", "bool", "Returns true if bounding boxes overlap."));
    v.push(doc!("encloses", aabb, "encloses(b: AABB) -> bool", "bool", "Returns true if this box completely contains b."));
    v.push(doc!("expand", aabb, "expand(to_point: Vector3) -> AABB", "AABB", "Expands box to enclose given point."));
    v.push(doc!("grow", aabb, "grow(by: float) -> AABB", "AABB", "Expands all faces outward by distance by."));
    v.push(doc!("merge", aabb, "merge(with: AABB) -> AABB", "AABB", "Returns union bounding box."));

    // =========================================================================
    // 8. Node / SceneTree / Object Methods (~50 entries)
    // =========================================================================
    let n = Some("Node");
    v.push(doc!("get_name", n, "get_name() -> StringName", "StringName", "Returns name of the node."));
    v.push(doc!("set_name", n, "set_name(name: String) -> void", "void", "Sets name of the node."));
    v.push(doc!("get_path", n, "get_path() -> NodePath", "NodePath", "Returns absolute tree path of the node."));
    v.push(doc!("get_node", n, "get_node(path: NodePath) -> Node", "Node", "Fetches node by relative or absolute path."));
    v.push(doc!("get_node_or_null", n, "get_node_or_null(path: NodePath) -> Node", "Node", "Fetches node or returns null if not found."));
    v.push(doc!("has_node", n, "has_node(path: NodePath) -> bool", "bool", "Returns true if path exists in scene hierarchy."));
    v.push(doc!("get_parent", n, "get_parent() -> Node", "Node", "Returns parent node or null."));
    v.push(doc!("get_children", n, "get_children(include_internal: bool = false) -> Array", "Array", "Returns array of child nodes."));
    v.push(doc!("get_child_count", n, "get_child_count(include_internal: bool = false) -> int", "int", "Returns number of children."));
    v.push(doc!("get_child", n, "get_child(idx: int, include_internal: bool = false) -> Node", "Node", "Returns child at index idx."));
    v.push(doc!("add_child", n, "add_child(node: Node, force_readable_name: bool = false) -> void", "void", "Adds a child node."));
    v.push(doc!("remove_child", n, "remove_child(node: Node) -> void", "void", "Removes child node."));
    v.push(doc!("queue_free", n, "queue_free() -> void", "void", "Queues node for deletion at frame end."));
    v.push(doc!("is_queued_for_deletion", n, "is_queued_for_deletion() -> bool", "bool", "Returns true if node is queued to be freed."));
    v.push(doc!("add_to_group", n, "add_to_group(group: StringName, persistent: bool = false) -> void", "void", "Adds node to a group."));
    v.push(doc!("remove_from_group", n, "remove_from_group(group: StringName) -> void", "void", "Removes node from group."));
    v.push(doc!("is_in_group", n, "is_in_group(group: StringName) -> bool", "bool", "Returns true if node is member of group."));
    v.push(doc!("get_groups", n, "get_groups() -> Array", "Array", "Returns array of group names."));
    v.push(doc!("set_process", n, "set_process(enable: bool) -> void", "void", "Enables or disables _process tick."));
    v.push(doc!("is_processing", n, "is_processing() -> bool", "bool", "Returns true if _process is active."));
    v.push(doc!("set_physics_process", n, "set_physics_process(enable: bool) -> void", "void", "Enables or disables _physics_process."));
    v.push(doc!("is_physics_processing", n, "is_physics_processing() -> bool", "bool", "Returns true if _physics_process is active."));
    v.push(doc!("emit_signal", n, "emit_signal(signal: StringName, ...args) -> int", "int", "Emits a signal with variable payload."));
    v.push(doc!("connect", n, "connect(signal: StringName, callable: Callable, flags: int = 0) -> int", "int", "Connects signal to callable."));
    v.push(doc!("disconnect", n, "disconnect(signal: StringName, callable: Callable) -> void", "void", "Disconnects signal from callable."));
    v.push(doc!("is_connected", n, "is_connected(signal: StringName, callable: Callable) -> bool", "bool", "Returns true if connection exists."));
    v.push(doc!("set_meta", n, "set_meta(name: StringName, value: Variant) -> void", "void", "Stores metadata on the object."));
    v.push(doc!("get_meta", n, "get_meta(name: StringName, default: Variant = null) -> Variant", "Variant", "Retrieves metadata with default fallback."));
    v.push(doc!("has_meta", n, "has_meta(name: StringName) -> bool", "bool", "Returns true if metadata key exists."));
    v.push(doc!("remove_meta", n, "remove_meta(name: StringName) -> void", "void", "Deletes metadata key."));
    v.push(doc!("get_meta_list", n, "get_meta_list() -> Array", "Array", "Returns array of all metadata keys."));
    v.push(doc!("get_class", n, "get_class() -> String", "String", "Returns the class inheritance name."));
    v.push(doc!("is_class", n, "is_class(name: String) -> bool", "bool", "Returns true if object inherits from class name."));
    v.push(doc!("get_tree", n, "get_tree() -> SceneTree", "SceneTree", "Returns the current SceneTree instance."));
    v.push(doc!("create_timer", n, "create_timer(time_sec: float, process_always: bool = true, process_in_physics: bool = false, ignore_time_scale: bool = false) -> SceneTreeTimer", "SceneTreeTimer", "Creates and returns a one-shot countdown timer."));
    v.push(doc!("create_tween", n, "create_tween() -> Tween", "Tween", "Creates a tween for animating properties."));
    v.push(doc!("reparent", n, "reparent(new_parent: Node, keep_global_transform: bool = true) -> void", "void", "Changes parent node in scene hierarchy."));
    v.push(doc!("duplicate", n, "duplicate(flags: int = 15) -> Node", "Node", "Duplicates the node and optionally its children."));
    v.push(doc!("find_child", n, "find_child(pattern: String, recursive: bool = true, owned: bool = true) -> Node", "Node", "Finds child by name or pattern."));
    v.push(doc!("find_children", n, "find_children(pattern: String, type: String = \"\", recursive: bool = true, owned: bool = true) -> Array", "Array", "Finds all matching child nodes."));
    v.push(doc!("propagate_call", n, "propagate_call(method: StringName, args: Array = [], parent_first: bool = false) -> void", "void", "Calls method recursively on self and all descendants."));
    v.push(doc!("print_tree", n, "print_tree() -> void", "void", "Prints node hierarchy to console."));
    v.push(doc!("print_tree_pretty", n, "print_tree_pretty() -> void", "void", "Prints pretty ASCII tree."));
    v.push(doc!("get_index", n, "get_index(include_internal: bool = false) -> int", "int", "Returns index order among parent's children."));
    v.push(doc!("move_child", n, "move_child(child_node: Node, to_index: int) -> void", "void", "Reorders child to new index position."));
    v.push(doc!("raise", n, "raise() -> void", "void", "Moves node to top of parent's children."));
    v.push(doc!("set_owner", n, "set_owner(owner: Node) -> void", "void", "Sets scene tree owner node for serialization."));
    v.push(doc!("get_owner", n, "get_owner() -> Node", "Node", "Returns owner node."));
    v.push(doc!("get_scene_file_path", n, "get_scene_file_path() -> String", "String", "Returns instanced scene file path."));
    v.push(doc!("set_scene_file_path", n, "set_scene_file_path(scene_file_path: String) -> void", "void", "Sets instanced scene file path."));

    v
});

/// Exposes the static slice of 400+ Godot 4.4 built-in functions & methods.
pub fn builtin_catalog() -> &'static [GdBuiltinDoc] {
    &CATALOG
}

/// Evaluates a global built-in function by name.
pub fn eval_global_builtin(
    name: &str,
    args: &[IrValue],
    logs: &mut Vec<String>,
) -> Option<Result<IrValue, String>> {
    match name {
        "sin" => {
            let v = to_f64_arg(args, 0);
            Some(Ok(IrValue::F64(v.sin())))
        }
        "cos" => {
            let v = to_f64_arg(args, 0);
            Some(Ok(IrValue::F64(v.cos())))
        }
        "tan" => {
            let v = to_f64_arg(args, 0);
            Some(Ok(IrValue::F64(v.tan())))
        }
        "asin" => {
            let v = to_f64_arg(args, 0);
            Some(Ok(IrValue::F64(v.asin())))
        }
        "acos" => {
            let v = to_f64_arg(args, 0);
            Some(Ok(IrValue::F64(v.acos())))
        }
        "atan" => {
            let v = to_f64_arg(args, 0);
            Some(Ok(IrValue::F64(v.atan())))
        }
        "atan2" => {
            let y = to_f64_arg(args, 0);
            let x = to_f64_arg(args, 1);
            Some(Ok(IrValue::F64(y.atan2(x))))
        }
        "sinh" => {
            let v = to_f64_arg(args, 0);
            Some(Ok(IrValue::F64(v.sinh())))
        }
        "cosh" => {
            let v = to_f64_arg(args, 0);
            Some(Ok(IrValue::F64(v.cosh())))
        }
        "tanh" => {
            let v = to_f64_arg(args, 0);
            Some(Ok(IrValue::F64(v.tanh())))
        }
        "sqrt" => {
            let v = to_f64_arg(args, 0);
            Some(Ok(IrValue::F64(v.sqrt())))
        }
        "pow" => {
            let b = to_f64_arg(args, 0);
            let e = to_f64_arg(args, 1);
            Some(Ok(IrValue::F64(b.powf(e))))
        }
        "exp" => {
            let v = to_f64_arg(args, 0);
            Some(Ok(IrValue::F64(v.exp())))
        }
        "log" => {
            let v = to_f64_arg(args, 0);
            Some(Ok(IrValue::F64(v.ln())))
        }
        "abs" | "absf" => {
            let v = to_f64_arg(args, 0);
            Some(Ok(IrValue::F64(v.abs())))
        }
        "absi" => {
            let i = to_i64_arg(args, 0);
            Some(Ok(IrValue::I64(i.abs())))
        }
        "sign" | "signf" => {
            let v = to_f64_arg(args, 0);
            let res = if v > 0.0 { 1.0 } else if v < 0.0 { -1.0 } else { 0.0 };
            Some(Ok(IrValue::F64(res)))
        }
        "signi" => {
            let i = to_i64_arg(args, 0);
            let res = if i > 0 { 1 } else if i < 0 { -1 } else { 0 };
            Some(Ok(IrValue::I64(res)))
        }
        "floor" | "floorf" => {
            let v = to_f64_arg(args, 0);
            Some(Ok(IrValue::F64(v.floor())))
        }
        "floori" => {
            let v = to_f64_arg(args, 0);
            Some(Ok(IrValue::I64(v.floor() as i64)))
        }
        "ceil" | "ceilf" => {
            let v = to_f64_arg(args, 0);
            Some(Ok(IrValue::F64(v.ceil())))
        }
        "ceili" => {
            let v = to_f64_arg(args, 0);
            Some(Ok(IrValue::I64(v.ceil() as i64)))
        }
        "round" | "roundf" => {
            let v = to_f64_arg(args, 0);
            Some(Ok(IrValue::F64(v.round())))
        }
        "roundi" => {
            let v = to_f64_arg(args, 0);
            Some(Ok(IrValue::I64(v.round() as i64)))
        }
        "clamp" | "clampf" => {
            let v = to_f64_arg(args, 0);
            let min = to_f64_arg(args, 1);
            let max = to_f64_arg(args, 2);
            Some(Ok(IrValue::F64(v.clamp(min, max))))
        }
        "clampi" => {
            let v = to_i64_arg(args, 0);
            let min = to_i64_arg(args, 1);
            let max = to_i64_arg(args, 2);
            Some(Ok(IrValue::I64(v.clamp(min, max))))
        }
        "lerp" | "lerpf" => {
            let a = to_f64_arg(args, 0);
            let b = to_f64_arg(args, 1);
            let t = to_f64_arg(args, 2);
            Some(Ok(IrValue::F64(a + (b - a) * t)))
        }
        "lerp_angle" => {
            let a = to_f64_arg(args, 0);
            let b = to_f64_arg(args, 1);
            let t = to_f64_arg(args, 2);
            let diff = (b - a).rem_euclid(std::f64::consts::TAU);
            let shortest = (2.0 * diff).rem_euclid(std::f64::consts::TAU) - diff;
            Some(Ok(IrValue::F64(a + shortest * t)))
        }
        "inverse_lerp" => {
            let from = to_f64_arg(args, 0);
            let to = to_f64_arg(args, 1);
            let weight = to_f64_arg(args, 2);
            if (to - from).abs() < 1e-15 {
                Some(Ok(IrValue::F64(0.0)))
            } else {
                Some(Ok(IrValue::F64((weight - from) / (to - from))))
            }
        }
        "remap" => {
            let val = to_f64_arg(args, 0);
            let istart = to_f64_arg(args, 1);
            let istop = to_f64_arg(args, 2);
            let ostart = to_f64_arg(args, 3);
            let ostop = to_f64_arg(args, 4);
            let t = if (istop - istart).abs() < 1e-15 { 0.0 } else { (val - istart) / (istop - istart) };
            Some(Ok(IrValue::F64(ostart + (ostop - ostart) * t)))
        }
        "smoothstep" => {
            let from = to_f64_arg(args, 0);
            let to = to_f64_arg(args, 1);
            let x = to_f64_arg(args, 2);
            let t = if (to - from).abs() < 1e-15 {
                0.0
            } else {
                ((x - from) / (to - from)).clamp(0.0, 1.0)
            };
            Some(Ok(IrValue::F64(t * t * (3.0 - 2.0 * t))))
        }
        "move_toward" => {
            let from = to_f64_arg(args, 0);
            let to = to_f64_arg(args, 1);
            let delta = to_f64_arg(args, 2);
            if (to - from).abs() <= delta {
                Some(Ok(IrValue::F64(to)))
            } else {
                Some(Ok(IrValue::F64(from + (to - from).signum() * delta)))
            }
        }
        "rotate_toward" => {
            let from = to_f64_arg(args, 0);
            let to = to_f64_arg(args, 1);
            let delta = to_f64_arg(args, 2);
            let diff = (b_angle_diff(from, to)).abs();
            if diff <= delta {
                Some(Ok(IrValue::F64(to)))
            } else {
                let step = (b_angle_diff(from, to)).signum() * delta;
                Some(Ok(IrValue::F64(from + step)))
            }
        }
        "snapped" | "snappedf" => {
            let val = to_f64_arg(args, 0);
            let step = to_f64_arg(args, 1);
            if step != 0.0 {
                Some(Ok(IrValue::F64((val / step).round() * step)))
            } else {
                Some(Ok(IrValue::F64(val)))
            }
        }
        "snappedi" => {
            let val = to_i64_arg(args, 0);
            let step = to_i64_arg(args, 1);
            if step != 0 {
                Some(Ok(IrValue::I64(((val as f64 / step as f64).round() as i64) * step)))
            } else {
                Some(Ok(IrValue::I64(val)))
            }
        }
        "wrap" | "wrapf" => {
            let val = to_f64_arg(args, 0);
            let min = to_f64_arg(args, 1);
            let max = to_f64_arg(args, 2);
            let range = max - min;
            let res = if range != 0.0 {
                min + (val - min).rem_euclid(range)
            } else {
                min
            };
            Some(Ok(IrValue::F64(res)))
        }
        "wrapi" => {
            let val = to_i64_arg(args, 0);
            let min = to_i64_arg(args, 1);
            let max = to_i64_arg(args, 2);
            let range = max - min;
            let res = if range != 0 {
                min + (val - min).rem_euclid(range)
            } else {
                min
            };
            Some(Ok(IrValue::I64(res)))
        }
        "pingpong" => {
            let val = to_f64_arg(args, 0);
            let len = to_f64_arg(args, 1);
            if len != 0.0 {
                let cycle = 2.0 * len;
                let m = val.rem_euclid(cycle);
                let res = if m < len { m } else { cycle - m };
                Some(Ok(IrValue::F64(res)))
            } else {
                Some(Ok(IrValue::F64(0.0)))
            }
        }
        "fmod" => {
            let a = to_f64_arg(args, 0);
            let b = to_f64_arg(args, 1);
            Some(Ok(IrValue::F64(a % b)))
        }
        "fposmod" => {
            let a = to_f64_arg(args, 0);
            let b = to_f64_arg(args, 1);
            Some(Ok(IrValue::F64(a.rem_euclid(b))))
        }
        "posmod" => {
            let a = to_i64_arg(args, 0);
            let b = to_i64_arg(args, 1);
            Some(Ok(IrValue::I64(a.rem_euclid(b))))
        }
        "deg_to_rad" => {
            let deg = to_f64_arg(args, 0);
            Some(Ok(IrValue::F64(deg.to_radians())))
        }
        "rad_to_deg" => {
            let rad = to_f64_arg(args, 0);
            Some(Ok(IrValue::F64(rad.to_degrees())))
        }
        "linear_to_db" => {
            let lin = to_f64_arg(args, 0);
            Some(Ok(IrValue::F64(lin.log10() * 20.0)))
        }
        "db_to_linear" => {
            let db = to_f64_arg(args, 0);
            Some(Ok(IrValue::F64(10.0_f64.powf(db / 20.0))))
        }
        "is_nan" => {
            let v = to_f64_arg(args, 0);
            Some(Ok(IrValue::Bool(v.is_nan())))
        }
        "is_inf" => {
            let v = to_f64_arg(args, 0);
            Some(Ok(IrValue::Bool(v.is_infinite())))
        }
        "is_finite" => {
            let v = to_f64_arg(args, 0);
            Some(Ok(IrValue::Bool(v.is_finite())))
        }
        "is_zero_approx" => {
            let v = to_f64_arg(args, 0);
            Some(Ok(IrValue::Bool(v.abs() < 1e-6)))
        }
        "is_equal_approx" => {
            let a = to_f64_arg(args, 0);
            let b = to_f64_arg(args, 1);
            Some(Ok(IrValue::Bool((a - b).abs() < 1e-6)))
        }
        "min" | "minf" => {
            let a = to_f64_arg(args, 0);
            let b = to_f64_arg(args, 1);
            Some(Ok(IrValue::F64(a.min(b))))
        }
        "max" | "maxf" => {
            let a = to_f64_arg(args, 0);
            let b = to_f64_arg(args, 1);
            Some(Ok(IrValue::F64(a.max(b))))
        }
        "mini" => {
            let a = to_i64_arg(args, 0);
            let b = to_i64_arg(args, 1);
            Some(Ok(IrValue::I64(a.min(b))))
        }
        "maxi" => {
            let a = to_i64_arg(args, 0);
            let b = to_i64_arg(args, 1);
            Some(Ok(IrValue::I64(a.max(b))))
        }
        "randi" => {
            static SEED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1234567);
            let s = SEED.fetch_update(std::sync::atomic::Ordering::Relaxed, std::sync::atomic::Ordering::Relaxed, |v| {
                Some(v.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407))
            }).unwrap_or(0);
            let val = (s >> 32) as i64;
            Some(Ok(IrValue::I64(val.abs())))
        }
        "randf" => {
            static SEED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(7654321);
            let s = SEED.fetch_update(std::sync::atomic::Ordering::Relaxed, std::sync::atomic::Ordering::Relaxed, |v| {
                Some(v.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407))
            }).unwrap_or(0);
            let val = ((s >> 32) as f64) / (u32::MAX as f64);
            Some(Ok(IrValue::F64(val)))
        }
        "randf_range" => {
            let from = to_f64_arg(args, 0);
            let to = to_f64_arg(args, 1);
            let mid = from + (to - from) * 0.5;
            Some(Ok(IrValue::F64(mid)))
        }
        "randi_range" => {
            let from = to_i64_arg(args, 0);
            let to = to_i64_arg(args, 1);
            Some(Ok(IrValue::I64((from + to) / 2)))
        }
        "typeof" => {
            let val = args.first().unwrap_or(&IrValue::Nil);
            Some(Ok(IrValue::I64(typeof_code(val))))
        }
        "type_string" => {
            let code = to_i64_arg(args, 0);
            Some(Ok(IrValue::String(type_string(code).to_string())))
        }
        "str" => {
            let s = args
                .iter()
                .map(ir_to_string)
                .collect::<Vec<_>>()
                .join("");
            Some(Ok(IrValue::String(s)))
        }
        "int" => {
            let i = to_i64_arg(args, 0);
            Some(Ok(IrValue::I64(i)))
        }
        "float" => {
            let f = to_f64_arg(args, 0);
            Some(Ok(IrValue::F64(f)))
        }
        "bool" => {
            let b = to_bool_arg(args, 0);
            Some(Ok(IrValue::Bool(b)))
        }
        "len" => match args.first() {
            Some(IrValue::String(s)) => Some(Ok(IrValue::I64(s.len() as i64))),
            Some(IrValue::Array(a)) => Some(Ok(IrValue::I64(a.len() as i64))),
            Some(IrValue::Map(m)) => Some(Ok(IrValue::I64(m.len() as i64))),
            _ => Some(Ok(IrValue::I64(0))),
        },
        "range" => {
            let (start, end, step) = if args.len() == 1 {
                (0, to_i64_arg(args, 0), 1)
            } else if args.len() == 2 {
                (to_i64_arg(args, 0), to_i64_arg(args, 1), 1)
            } else if args.len() >= 3 {
                (to_i64_arg(args, 0), to_i64_arg(args, 1), to_i64_arg(args, 2).max(1))
            } else {
                (0, 0, 1)
            };
            let mut arr = Vec::new();
            let mut cur = start;
            while cur < end {
                arr.push(IrValue::I64(cur));
                cur += step;
            }
            Some(Ok(IrValue::Array(arr)))
        }
        "hash" => {
            let s = ir_to_string(args.first().unwrap_or(&IrValue::Nil));
            let mut h: u64 = 5381;
            for b in s.bytes() {
                h = ((h << 5).wrapping_add(h)).wrapping_add(b as u64);
            }
            Some(Ok(IrValue::I64((h & 0x7FFFFFFF) as i64)))
        }
        "prints" => {
            let msg = args.iter().map(ir_to_string).collect::<Vec<_>>().join(" ");
            logs.push(msg);
            Some(Ok(IrValue::Nil))
        }
        "printt" => {
            let msg = args.iter().map(ir_to_string).collect::<Vec<_>>().join("\t");
            logs.push(msg);
            Some(Ok(IrValue::Nil))
        }
        "printerr" | "push_warning" | "push_error" => {
            let msg = args.iter().map(ir_to_string).collect::<Vec<_>>().join(" ");
            logs.push(format!("[{name}] {msg}"));
            Some(Ok(IrValue::Nil))
        }
        "assert" => {
            let cond = to_bool_arg(args, 0);
            if !cond {
                let msg = if args.len() > 1 {
                    ir_to_string(&args[1])
                } else {
                    "Assertion failed".to_string()
                };
                Some(Err(format!("Assertion failed: {msg}")))
            } else {
                Some(Ok(IrValue::Nil))
            }
        }
        _ => None,
    }
}

/// Evaluates a method invocation on an `IrValue` receiver.
pub fn eval_method(
    receiver: &mut IrValue,
    method: &str,
    args: &[IrValue],
) -> Option<Result<IrValue, String>> {
    match receiver {
        IrValue::String(s) => match method {
            "length" => Some(Ok(IrValue::I64(s.len() as i64))),
            "is_empty" => Some(Ok(IrValue::Bool(s.is_empty()))),
            "to_upper" => Some(Ok(IrValue::String(s.to_uppercase()))),
            "to_lower" => Some(Ok(IrValue::String(s.to_lowercase()))),
            "capitalize" => {
                let words: Vec<String> = s
                    .replace('_', " ")
                    .split_whitespace()
                    .map(|w| {
                        let mut c = w.chars();
                        match c.next() {
                            None => String::new(),
                            Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                        }
                    })
                    .collect();
                Some(Ok(IrValue::String(words.join(" "))))
            }
            "to_snake_case" => {
                let mut out = String::new();
                for (i, ch) in s.chars().enumerate() {
                    if ch.is_uppercase() && i > 0 {
                        out.push('_');
                    }
                    out.push(ch.to_ascii_lowercase());
                }
                Some(Ok(IrValue::String(out)))
            }
            "to_camel_case" => {
                let mut out = String::new();
                let mut cap = false;
                for ch in s.chars() {
                    if ch == '_' {
                        cap = true;
                    } else if cap {
                        out.push(ch.to_ascii_uppercase());
                        cap = false;
                    } else {
                        out.push(ch.to_ascii_lowercase());
                    }
                }
                Some(Ok(IrValue::String(out)))
            }
            "to_pascal_case" => {
                let mut out = String::new();
                let mut cap = true;
                for ch in s.chars() {
                    if ch == '_' {
                        cap = true;
                    } else if cap {
                        out.push(ch.to_ascii_uppercase());
                        cap = false;
                    } else {
                        out.push(ch);
                    }
                }
                Some(Ok(IrValue::String(out)))
            }
            "strip_edges" => Some(Ok(IrValue::String(s.trim().to_string()))),
            "lstrip" => {
                let pat = args.first().map(ir_to_string).unwrap_or_default();
                Some(Ok(IrValue::String(s.trim_start_matches(pat.as_str()).to_string())))
            }
            "rstrip" => {
                let pat = args.first().map(ir_to_string).unwrap_or_default();
                Some(Ok(IrValue::String(s.trim_end_matches(pat.as_str()).to_string())))
            }
            "trim_prefix" => {
                let pat = args.first().map(ir_to_string).unwrap_or_default();
                Some(Ok(IrValue::String(s.strip_prefix(pat.as_str()).unwrap_or(s).to_string())))
            }
            "trim_suffix" => {
                let pat = args.first().map(ir_to_string).unwrap_or_default();
                Some(Ok(IrValue::String(s.strip_suffix(pat.as_str()).unwrap_or(s).to_string())))
            }
            "begins_with" => {
                let pat = args.first().map(ir_to_string).unwrap_or_default();
                Some(Ok(IrValue::Bool(s.starts_with(&pat))))
            }
            "ends_with" => {
                let pat = args.first().map(ir_to_string).unwrap_or_default();
                Some(Ok(IrValue::Bool(s.ends_with(&pat))))
            }
            "contains" => {
                let pat = args.first().map(ir_to_string).unwrap_or_default();
                Some(Ok(IrValue::Bool(s.contains(&pat))))
            }
            "find" => {
                let pat = args.first().map(ir_to_string).unwrap_or_default();
                let idx = s.find(&pat).map(|i| i as i64).unwrap_or(-1);
                Some(Ok(IrValue::I64(idx)))
            }
            "rfind" => {
                let pat = args.first().map(ir_to_string).unwrap_or_default();
                let idx = s.rfind(&pat).map(|i| i as i64).unwrap_or(-1);
                Some(Ok(IrValue::I64(idx)))
            }
            "count" => {
                let pat = args.first().map(ir_to_string).unwrap_or_default();
                let c = if pat.is_empty() { 0 } else { s.matches(&pat).count() as i64 };
                Some(Ok(IrValue::I64(c)))
            }
            "replace" => {
                let what = args.first().map(ir_to_string).unwrap_or_default();
                let forwhat = args.get(1).map(ir_to_string).unwrap_or_default();
                Some(Ok(IrValue::String(s.replace(&what, &forwhat))))
            }
            "substr" => {
                let from = to_i64_arg(args, 0).max(0) as usize;
                let len = if args.len() > 1 {
                    to_i64_arg(args, 1)
                } else {
                    -1
                };
                if from >= s.len() {
                    Some(Ok(IrValue::String(String::new())))
                } else if len < 0 {
                    Some(Ok(IrValue::String(s[from..].to_string())))
                } else {
                    let end = (from + len as usize).min(s.len());
                    Some(Ok(IrValue::String(s[from..end].to_string())))
                }
            }
            "left" => {
                let len = to_i64_arg(args, 0).max(0) as usize;
                let end = len.min(s.len());
                Some(Ok(IrValue::String(s[..end].to_string())))
            }
            "right" => {
                let len = to_i64_arg(args, 0).max(0) as usize;
                let start = s.len().saturating_sub(len);
                Some(Ok(IrValue::String(s[start..].to_string())))
            }
            "split" => {
                let delim = args.first().map(ir_to_string).unwrap_or_default();
                let parts: Vec<IrValue> = if delim.is_empty() {
                    s.chars().map(|c| IrValue::String(c.to_string())).collect()
                } else {
                    s.split(&delim).map(|p| IrValue::String(p.to_string())).collect()
                };
                Some(Ok(IrValue::Array(parts)))
            }
            "join" => {
                let parts = match args.first() {
                    Some(IrValue::Array(arr)) => arr
                        .iter()
                        .map(ir_to_string)
                        .collect::<Vec<_>>()
                        .join(s),
                    _ => String::new(),
                };
                Some(Ok(IrValue::String(parts)))
            }
            "repeat" => {
                let count = to_i64_arg(args, 0).max(0) as usize;
                Some(Ok(IrValue::String(s.repeat(count))))
            }
            "reverse" => Some(Ok(IrValue::String(s.chars().rev().collect()))),
            "is_valid_int" => Some(Ok(IrValue::Bool(s.trim().parse::<i64>().is_ok()))),
            "is_valid_float" => Some(Ok(IrValue::Bool(s.trim().parse::<f64>().is_ok()))),
            "to_int" => Some(Ok(IrValue::I64(s.trim().parse::<i64>().unwrap_or(0)))),
            "to_float" => Some(Ok(IrValue::F64(s.trim().parse::<f64>().unwrap_or(0.0)))),
            "get_file" => {
                let name = s.rsplit('/').next().unwrap_or(s);
                Some(Ok(IrValue::String(name.to_string())))
            }
            "get_base_dir" => {
                let dir = s.rfind('/').map(|i| &s[..i]).unwrap_or("");
                Some(Ok(IrValue::String(dir.to_string())))
            }
            "get_extension" => {
                let ext = s.rsplit('.').next().unwrap_or("");
                Some(Ok(IrValue::String(ext.to_string())))
            }
            "get_basename" => {
                let base = s.rfind('.').map(|i| &s[..i]).unwrap_or(s);
                Some(Ok(IrValue::String(base.to_string())))
            }
            "path_join" => {
                let file = args.first().map(ir_to_string).unwrap_or_default();
                let joined = format!("{}/{}", s.trim_end_matches('/'), file.trim_start_matches('/'));
                Some(Ok(IrValue::String(joined)))
            }
            _ => None,
        },
        IrValue::Array(arr) => match method {
            "size" => Some(Ok(IrValue::I64(arr.len() as i64))),
            "is_empty" => Some(Ok(IrValue::Bool(arr.is_empty()))),
            "push_back" | "append" => {
                if let Some(v) = args.first() {
                    arr.push(v.clone());
                }
                Some(Ok(IrValue::Nil))
            }
            "push_front" => {
                if let Some(v) = args.first() {
                    arr.insert(0, v.clone());
                }
                Some(Ok(IrValue::Nil))
            }
            "append_array" => {
                if let Some(IrValue::Array(other)) = args.first() {
                    arr.extend(other.clone());
                }
                Some(Ok(IrValue::Nil))
            }
            "pop_back" => Some(Ok(arr.pop().unwrap_or(IrValue::Nil))),
            "pop_front" => {
                if arr.is_empty() {
                    Some(Ok(IrValue::Nil))
                } else {
                    Some(Ok(arr.remove(0)))
                }
            }
            "pop_at" => {
                let idx = to_i64_arg(args, 0);
                if idx >= 0 && (idx as usize) < arr.len() {
                    Some(Ok(arr.remove(idx as usize)))
                } else {
                    Some(Ok(IrValue::Nil))
                }
            }
            "insert" => {
                let idx = to_i64_arg(args, 0).max(0) as usize;
                let val = args.get(1).cloned().unwrap_or(IrValue::Nil);
                if idx <= arr.len() {
                    arr.insert(idx, val);
                } else {
                    arr.push(val);
                }
                Some(Ok(IrValue::I64(0)))
            }
            "remove_at" => {
                let idx = to_i64_arg(args, 0);
                if idx >= 0 && (idx as usize) < arr.len() {
                    arr.remove(idx as usize);
                }
                Some(Ok(IrValue::Nil))
            }
            "erase" => {
                if let Some(target) = args.first()
                    && let Some(pos) = arr.iter().position(|x| x == target)
                {
                    arr.remove(pos);
                }
                Some(Ok(IrValue::Nil))
            }
            "clear" => {
                arr.clear();
                Some(Ok(IrValue::Nil))
            }
            "has" => {
                let target = args.first().unwrap_or(&IrValue::Nil);
                Some(Ok(IrValue::Bool(arr.contains(target))))
            }
            "find" => {
                let target = args.first().unwrap_or(&IrValue::Nil);
                let pos = arr.iter().position(|x| x == target).map(|i| i as i64).unwrap_or(-1);
                Some(Ok(IrValue::I64(pos)))
            }
            "rfind" => {
                let target = args.first().unwrap_or(&IrValue::Nil);
                let pos = arr.iter().rposition(|x| x == target).map(|i| i as i64).unwrap_or(-1);
                Some(Ok(IrValue::I64(pos)))
            }
            "count" => {
                let target = args.first().unwrap_or(&IrValue::Nil);
                let c = arr.iter().filter(|&x| x == target).count() as i64;
                Some(Ok(IrValue::I64(c)))
            }
            "slice" => {
                let begin = to_i64_arg(args, 0).max(0) as usize;
                let end = if args.len() > 1 {
                    (to_i64_arg(args, 1) as usize).min(arr.len())
                } else {
                    arr.len()
                };
                if begin >= arr.len() || begin >= end {
                    Some(Ok(IrValue::Array(Vec::new())))
                } else {
                    Some(Ok(IrValue::Array(arr[begin..end].to_vec())))
                }
            }
            "duplicate" => Some(Ok(IrValue::Array(arr.clone()))),
            "reverse" => {
                arr.reverse();
                Some(Ok(IrValue::Nil))
            }
            "sort" => {
                arr.sort_by(|a, b| {
                    let fa = to_f64_arg(std::slice::from_ref(a), 0);
                    let fb = to_f64_arg(std::slice::from_ref(b), 0);
                    fa.partial_cmp(&fb).unwrap_or(std::cmp::Ordering::Equal)
                });
                Some(Ok(IrValue::Nil))
            }
            "min" => {
                let mut min_val = arr.first().cloned().unwrap_or(IrValue::Nil);
                for it in arr.iter().skip(1) {
                    if to_f64_arg(std::slice::from_ref(it), 0) < to_f64_arg(std::slice::from_ref(&min_val), 0) {
                        min_val = it.clone();
                    }
                }
                Some(Ok(min_val))
            }
            "max" => {
                let mut max_val = arr.first().cloned().unwrap_or(IrValue::Nil);
                for it in arr.iter().skip(1) {
                    if to_f64_arg(std::slice::from_ref(it), 0) > to_f64_arg(std::slice::from_ref(&max_val), 0) {
                        max_val = it.clone();
                    }
                }
                Some(Ok(max_val))
            }
            "front" => Some(Ok(arr.first().cloned().unwrap_or(IrValue::Nil))),
            "back" => Some(Ok(arr.last().cloned().unwrap_or(IrValue::Nil))),
            "pick_random" => {
                if arr.is_empty() {
                    Some(Ok(IrValue::Nil))
                } else {
                    Some(Ok(arr[arr.len() / 2].clone()))
                }
            }
            "fill" => {
                let val = args.first().cloned().unwrap_or(IrValue::Nil);
                for it in arr.iter_mut() {
                    *it = val.clone();
                }
                Some(Ok(IrValue::Nil))
            }
            "resize" => {
                let new_size = to_i64_arg(args, 0).max(0) as usize;
                arr.resize(new_size, IrValue::Nil);
                Some(Ok(IrValue::I64(0)))
            }
            "has_all" => {
                if let Some(IrValue::Array(req)) = args.first() {
                    let all_present = req.iter().all(|r| arr.contains(r));
                    Some(Ok(IrValue::Bool(all_present)))
                } else {
                    Some(Ok(IrValue::Bool(false)))
                }
            }
            _ => None,
        },
        IrValue::Map(m) => {
            // Check if it's a Callable
            if m.get("__type__") == Some(&IrValue::String("Callable".into())) {
                match method {
                    "is_valid" => Some(Ok(IrValue::Bool(true))),
                    "bind" => {
                        if let Some(c) = GdCallable::from_ir_value(receiver) {
                            let bound = c.bind(args);
                            Some(Ok(bound.to_ir_value()))
                        } else {
                            Some(Ok(receiver.clone()))
                        }
                    }
                    _ => None,
                }
            } else {
                match method {
                    "size" => Some(Ok(IrValue::I64(m.len() as i64))),
                    "is_empty" => Some(Ok(IrValue::Bool(m.is_empty()))),
                    "has" => {
                        let k = ir_to_string(args.first().unwrap_or(&IrValue::Nil));
                        Some(Ok(IrValue::Bool(m.contains_key(&k))))
                    }
                    "has_all" => {
                        if let Some(IrValue::Array(keys)) = args.first() {
                            let all_p = keys.iter().all(|k| m.contains_key(&ir_to_string(k)));
                            Some(Ok(IrValue::Bool(all_p)))
                        } else {
                            Some(Ok(IrValue::Bool(false)))
                        }
                    }
                    "get" => {
                        let k = ir_to_string(args.first().unwrap_or(&IrValue::Nil));
                        let def = args.get(1).cloned().unwrap_or(IrValue::Nil);
                        Some(Ok(m.get(&k).cloned().unwrap_or(def)))
                    }
                    "keys" => {
                        let ks: Vec<IrValue> = m.keys().map(|k| IrValue::String(k.clone())).collect();
                        Some(Ok(IrValue::Array(ks)))
                    }
                    "values" => {
                        let vs: Vec<IrValue> = m.values().cloned().collect();
                        Some(Ok(IrValue::Array(vs)))
                    }
                    "erase" => {
                        let k = ir_to_string(args.first().unwrap_or(&IrValue::Nil));
                        let existed = m.remove(&k).is_some();
                        Some(Ok(IrValue::Bool(existed)))
                    }
                    "clear" => {
                        m.clear();
                        Some(Ok(IrValue::Nil))
                    }
                    "merge" => {
                        if let Some(IrValue::Map(other)) = args.first() {
                            let overwrite = to_bool_arg(args, 1);
                            for (k, v) in other {
                                if overwrite || !m.contains_key(k) {
                                    m.insert(k.clone(), v.clone());
                                }
                            }
                        }
                        Some(Ok(IrValue::Nil))
                    }
                    "merged" => {
                        let mut copy = m.clone();
                        if let Some(IrValue::Map(other)) = args.first() {
                            let overwrite = to_bool_arg(args, 1);
                            for (k, v) in other {
                                if overwrite || !copy.contains_key(k) {
                                    copy.insert(k.clone(), v.clone());
                                }
                            }
                        }
                        Some(Ok(IrValue::Map(copy)))
                    }
                    "duplicate" => Some(Ok(IrValue::Map(m.clone()))),
                    "find_key" => {
                        let target = args.first().unwrap_or(&IrValue::Nil);
                        let found = m.iter().find(|(_, v)| *v == target).map(|(k, _)| IrValue::String(k.clone()));
                        Some(Ok(found.unwrap_or(IrValue::Nil)))
                    }
                    _ => None,
                }
            }
        }
        IrValue::Vec2([x, y]) => match method {
            "length" => Some(Ok(IrValue::F64(((*x as f64).powi(2) + (*y as f64).powi(2)).sqrt()))),
            "length_squared" => Some(Ok(IrValue::F64((*x as f64).powi(2) + (*y as f64).powi(2)))),
            "normalized" => {
                let l = ((*x as f64).powi(2) + (*y as f64).powi(2)).sqrt() as f32;
                if l > 0.0 {
                    Some(Ok(IrValue::Vec2([*x / l, *y / l])))
                } else {
                    Some(Ok(IrValue::Vec2([0.0, 0.0])))
                }
            }
            "distance_to" => {
                let other = to_vec2_arg(args, 0);
                let dx = (*x - other[0]) as f64;
                let dy = (*y - other[1]) as f64;
                Some(Ok(IrValue::F64((dx * dx + dy * dy).sqrt())))
            }
            "direction_to" => {
                let other = to_vec2_arg(args, 0);
                let dx = other[0] - *x;
                let dy = other[1] - *y;
                let l = (dx * dx + dy * dy).sqrt();
                if l > 0.0 {
                    Some(Ok(IrValue::Vec2([dx / l, dy / l])))
                } else {
                    Some(Ok(IrValue::Vec2([0.0, 0.0])))
                }
            }
            "dot" => {
                let other = to_vec2_arg(args, 0);
                Some(Ok(IrValue::F64((*x * other[0] + *y * other[1]) as f64)))
            }
            "cross" => {
                let other = to_vec2_arg(args, 0);
                Some(Ok(IrValue::F64((*x * other[1] - *y * other[0]) as f64)))
            }
            "angle" => Some(Ok(IrValue::F64((*y as f64).atan2(*x as f64)))),
            "rotated" => {
                let phi = to_f64_arg(args, 0) as f32;
                let (sin, cos) = phi.sin_cos();
                Some(Ok(IrValue::Vec2([*x * cos - *y * sin, *x * sin + *y * cos])))
            }
            "orthogonal" => Some(Ok(IrValue::Vec2([-*y, *x]))),
            "lerp" => {
                let other = to_vec2_arg(args, 0);
                let w = to_f64_arg(args, 1) as f32;
                Some(Ok(IrValue::Vec2([*x + (other[0] - *x) * w, *y + (other[1] - *y) * w])))
            }
            "move_toward" => {
                let other = to_vec2_arg(args, 0);
                let delta = to_f64_arg(args, 1) as f32;
                let dx = other[0] - *x;
                let dy = other[1] - *y;
                let dist = (dx * dx + dy * dy).sqrt();
                if dist <= delta || dist == 0.0 {
                    Some(Ok(IrValue::Vec2(other)))
                } else {
                    Some(Ok(IrValue::Vec2([*x + dx / dist * delta, *y + dy / dist * delta])))
                }
            }
            "abs" => Some(Ok(IrValue::Vec2([x.abs(), y.abs()]))),
            "floor" => Some(Ok(IrValue::Vec2([x.floor(), y.floor()]))),
            "ceil" => Some(Ok(IrValue::Vec2([x.ceil(), y.ceil()]))),
            "round" => Some(Ok(IrValue::Vec2([x.round(), y.round()]))),
            "aspect" => {
                if *y != 0.0 {
                    Some(Ok(IrValue::F64((*x / *y) as f64)))
                } else {
                    Some(Ok(IrValue::F64(0.0)))
                }
            }
            _ => None,
        },
        IrValue::Vec3([x, y, z]) => match method {
            "length" => {
                let sq = (*x as f64).powi(2) + (*y as f64).powi(2) + (*z as f64).powi(2);
                Some(Ok(IrValue::F64(sq.sqrt())))
            }
            "length_squared" => {
                let sq = (*x as f64).powi(2) + (*y as f64).powi(2) + (*z as f64).powi(2);
                Some(Ok(IrValue::F64(sq)))
            }
            "normalized" => {
                let l = ((*x as f64).powi(2) + (*y as f64).powi(2) + (*z as f64).powi(2)).sqrt() as f32;
                if l > 0.0 {
                    Some(Ok(IrValue::Vec3([*x / l, *y / l, *z / l])))
                } else {
                    Some(Ok(IrValue::Vec3([0.0, 0.0, 0.0])))
                }
            }
            "distance_to" => {
                let o = to_vec3_arg(args, 0);
                let dx = (*x - o[0]) as f64;
                let dy = (*y - o[1]) as f64;
                let dz = (*z - o[2]) as f64;
                Some(Ok(IrValue::F64((dx * dx + dy * dy + dz * dz).sqrt())))
            }
            "direction_to" => {
                let o = to_vec3_arg(args, 0);
                let dx = o[0] - *x;
                let dy = o[1] - *y;
                let dz = o[2] - *z;
                let l = (dx * dx + dy * dy + dz * dz).sqrt();
                if l > 0.0 {
                    Some(Ok(IrValue::Vec3([dx / l, dy / l, dz / l])))
                } else {
                    Some(Ok(IrValue::Vec3([0.0, 0.0, 0.0])))
                }
            }
            "dot" => {
                let o = to_vec3_arg(args, 0);
                Some(Ok(IrValue::F64((*x * o[0] + *y * o[1] + *z * o[2]) as f64)))
            }
            "cross" => {
                let o = to_vec3_arg(args, 0);
                Some(Ok(IrValue::Vec3([
                    *y * o[2] - *z * o[1],
                    *z * o[0] - *x * o[2],
                    *x * o[1] - *y * o[0],
                ])))
            }
            "lerp" => {
                let o = to_vec3_arg(args, 0);
                let w = to_f64_arg(args, 1) as f32;
                Some(Ok(IrValue::Vec3([
                    *x + (o[0] - *x) * w,
                    *y + (o[1] - *y) * w,
                    *z + (o[2] - *z) * w,
                ])))
            }
            "move_toward" => {
                let o = to_vec3_arg(args, 0);
                let delta = to_f64_arg(args, 1) as f32;
                let dx = o[0] - *x;
                let dy = o[1] - *y;
                let dz = o[2] - *z;
                let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                if dist <= delta || dist == 0.0 {
                    Some(Ok(IrValue::Vec3(o)))
                } else {
                    Some(Ok(IrValue::Vec3([
                        *x + dx / dist * delta,
                        *y + dy / dist * delta,
                        *z + dz / dist * delta,
                    ])))
                }
            }
            "abs" => Some(Ok(IrValue::Vec3([x.abs(), y.abs(), z.abs()]))),
            "floor" => Some(Ok(IrValue::Vec3([x.floor(), y.floor(), z.floor()]))),
            "ceil" => Some(Ok(IrValue::Vec3([x.ceil(), y.ceil(), z.ceil()]))),
            "round" => Some(Ok(IrValue::Vec3([x.round(), y.round(), z.round()]))),
            _ => None,
        },
        IrValue::Color([r, g, b, a]) => match method {
            "lightened" => {
                let amt = to_f64_arg(args, 0) as f32;
                Some(Ok(IrValue::Color([
                    *r + (1.0 - *r) * amt,
                    *g + (1.0 - *g) * amt,
                    *b + (1.0 - *b) * amt,
                    *a,
                ])))
            }
            "darkened" => {
                let amt = to_f64_arg(args, 0) as f32;
                Some(Ok(IrValue::Color([
                    *r * (1.0 - amt),
                    *g * (1.0 - amt),
                    *b * (1.0 - amt),
                    *a,
                ])))
            }
            "inverted" => Some(Ok(IrValue::Color([1.0 - *r, 1.0 - *g, 1.0 - *b, *a]))),
            "to_html" => {
                let hex = format!(
                    "{:02x}{:02x}{:02x}{:02x}",
                    (*r * 255.0) as u8,
                    (*g * 255.0) as u8,
                    (*b * 255.0) as u8,
                    (*a * 255.0) as u8
                );
                Some(Ok(IrValue::String(hex)))
            }
            _ => None,
        },
        _ => None,
    }
}

/// Evaluates Node / SceneTree / Object built-in method calls on a bound `IrNode`.
pub fn eval_node_builtin(
    node: &mut IrNode,
    method: &str,
    args: &[IrValue],
    signals: &mut Vec<(String, Vec<IrValue>)>,
) -> Option<Result<IrValue, String>> {
    match method {
        "get_name" => Some(Ok(IrValue::String(node.name.clone()))),
        "set_name" => {
            if let Some(n) = args.first().map(ir_to_string) {
                node.name = n;
            }
            Some(Ok(IrValue::Nil))
        }
        "get_path" => Some(Ok(IrValue::NodePath(format!("/root/{}", node.name)))),
        "get_class" => Some(Ok(IrValue::String(node.class_name.clone()))),
        "is_class" => {
            let cls = args.first().map(ir_to_string).unwrap_or_default();
            Some(Ok(IrValue::Bool(node.class_name == cls || cls == "Node" || cls == "Object")))
        }
        "add_to_group" => {
            let grp = args.first().map(ir_to_string).unwrap_or_default();
            if !node.groups.contains(&grp) {
                node.groups.push(grp);
            }
            Some(Ok(IrValue::Nil))
        }
        "remove_from_group" => {
            let grp = args.first().map(ir_to_string).unwrap_or_default();
            node.groups.retain(|g| g != &grp);
            Some(Ok(IrValue::Nil))
        }
        "is_in_group" => {
            let grp = args.first().map(ir_to_string).unwrap_or_default();
            Some(Ok(IrValue::Bool(node.groups.contains(&grp))))
        }
        "get_groups" => {
            let gs: Vec<IrValue> = node.groups.iter().map(|g| IrValue::String(g.clone())).collect();
            Some(Ok(IrValue::Array(gs)))
        }
        "emit_signal" => {
            let sig_name = args.first().map(ir_to_string).unwrap_or_default();
            let sig_args = if args.len() > 1 { args[1..].to_vec() } else { Vec::new() };
            signals.push((sig_name, sig_args));
            Some(Ok(IrValue::Nil))
        }
        "get_tree" => {
            let mut map = BTreeMap::new();
            map.insert("__type__".to_string(), IrValue::String("SceneTree".into()));
            Some(Ok(IrValue::Map(map)))
        }
        "create_timer" => {
            let time_sec = to_f64_arg(args, 0);
            let mut map = BTreeMap::new();
            map.insert("__type__".to_string(), IrValue::String("SceneTreeTimer".into()));
            map.insert("remaining_sec".to_string(), IrValue::F64(time_sec));
            Some(Ok(IrValue::Map(map)))
        }
        _ => None,
    }
}

// Helpers
fn to_f64_arg(args: &[IrValue], idx: usize) -> f64 {
    match args.get(idx) {
        Some(IrValue::F64(f)) => *f,
        Some(IrValue::I64(i)) => *i as f64,
        Some(IrValue::String(s)) => s.trim().parse::<f64>().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn to_i64_arg(args: &[IrValue], idx: usize) -> i64 {
    match args.get(idx) {
        Some(IrValue::I64(i)) => *i,
        Some(IrValue::F64(f)) => *f as i64,
        Some(IrValue::String(s)) => s.trim().parse::<i64>().unwrap_or(0),
        _ => 0,
    }
}

fn to_bool_arg(args: &[IrValue], idx: usize) -> bool {
    match args.get(idx) {
        Some(IrValue::Bool(b)) => *b,
        Some(IrValue::I64(i)) => *i != 0,
        Some(IrValue::F64(f)) => *f != 0.0,
        Some(IrValue::String(s)) => !s.is_empty(),
        _ => false,
    }
}

fn to_vec2_arg(args: &[IrValue], idx: usize) -> [f32; 2] {
    match args.get(idx) {
        Some(IrValue::Vec2(v)) => *v,
        _ => [0.0, 0.0],
    }
}

fn to_vec3_arg(args: &[IrValue], idx: usize) -> [f32; 3] {
    match args.get(idx) {
        Some(IrValue::Vec3(v)) => *v,
        _ => [0.0, 0.0, 0.0],
    }
}

pub fn ir_to_string(val: &IrValue) -> String {
    match val {
        IrValue::String(s) => s.clone(),
        IrValue::I64(i) => i.to_string(),
        IrValue::F64(f) => f.to_string(),
        IrValue::Bool(b) => b.to_string(),
        IrValue::Nil => "null".into(),
        IrValue::Vec2([x, y]) => format!("Vector2({x}, {y})"),
        IrValue::Vec3([x, y, z]) => format!("Vector3({x}, {y}, {z})"),
        IrValue::Color([r, g, b, a]) => format!("Color({r}, {g}, {b}, {a})"),
        IrValue::Array(arr) => {
            let items: Vec<String> = arr.iter().map(ir_to_string).collect();
            format!("[{}]", items.join(", "))
        }
        IrValue::Map(m) => {
            if m.get("__type__") == Some(&IrValue::String("Callable".into())) {
                format!("Callable({})", m.get("method").map(ir_to_string).unwrap_or_default())
            } else {
                let items: Vec<String> = m.iter().map(|(k, v)| format!("\"{k}\": {}", ir_to_string(v))).collect();
                format!("{{{}}}", items.join(", "))
            }
        }
        other => format!("{other:?}"),
    }
}

fn b_angle_diff(from: f64, to: f64) -> f64 {
    let diff = (to - from).rem_euclid(std::f64::consts::TAU);
    (2.0 * diff).rem_euclid(std::f64::consts::TAU) - diff
}
