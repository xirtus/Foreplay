//! Lossless, zero-panic parser and deterministic serializer for `.fscn` scene documents.

use crate::{IrNode, IrScene, IrValue, NodeId, SignalConnection};
use std::collections::BTreeMap;

/// Errors that can occur when parsing a `.fscn` document.
#[derive(Clone, PartialEq, Eq, Debug, thiserror::Error)]
pub enum FscnParseError {
    #[error("unexpected end of file: {0}")]
    UnexpectedEof(String),
    #[error("invalid header: {0}")]
    InvalidHeader(String),
    #[error("invalid section: {0}")]
    InvalidSection(String),
    #[error("invalid attribute '{0}'")]
    InvalidAttribute(String),
    #[error("invalid value: {0}")]
    InvalidValue(String),
    #[error("invalid node id '{0}'")]
    InvalidNodeId(String),
    #[error("duplicate node id {0:?}")]
    DuplicateNode(NodeId),
    #[error("syntax error at line {line}: {message}")]
    SyntaxError { line: usize, message: String },
}

/// Serializes an `IrValue` to a deterministic string.
pub fn serialize_ir_value(val: &IrValue) -> String {
    match val {
        IrValue::Nil => "nil".to_string(),
        IrValue::Bool(b) => if *b { "true".to_string() } else { "false".to_string() },
        IrValue::I64(n) => format!("{n}"),
        IrValue::F64(f) => format_f64(*f),
        IrValue::String(s) => format!("\"{}\"", escape_str(s)),
        IrValue::Vec2([x, y]) => format!("Vec2({}, {})", format_f32(*x), format_f32(*y)),
        IrValue::Vec3([x, y, z]) => format!("Vec3({}, {}, {})", format_f32(*x), format_f32(*y), format_f32(*z)),
        IrValue::Quat([x, y, z, w]) => {
            format!("Quat({}, {}, {}, {})", format_f32(*x), format_f32(*y), format_f32(*z), format_f32(*w))
        }
        IrValue::Color([r, g, b, a]) => {
            format!("Color({}, {}, {}, {})", format_f32(*r), format_f32(*g), format_f32(*b), format_f32(*a))
        }
        IrValue::NodePath(s) => format!("NodePath(\"{}\")", escape_str(s)),
        IrValue::ResourcePath(s) => format!("ResourcePath(\"{}\")", escape_str(s)),
        IrValue::Array(arr) => {
            let mut out = String::from("[");
            for (i, elem) in arr.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&serialize_ir_value(elem));
            }
            out.push(']');
            out
        }
        IrValue::Map(map) => {
            let mut out = String::from("{");
            for (i, (k, v)) in map.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&format!("\"{}\": {}", escape_str(k), serialize_ir_value(v)));
            }
            out.push('}');
            out
        }
    }
}

/// Formats an `f64` deterministically and losslessly.
pub fn format_f64(v: f64) -> String {
    if v.is_nan() {
        "nan".to_string()
    } else if v.is_infinite() {
        if v > 0.0 { "inf".to_string() } else { "-inf".to_string() }
    } else {
        let s = format!("{v}");
        if !s.contains('.') && !s.contains('e') && !s.contains('E') {
            format!("{s}.0")
        } else {
            s
        }
    }
}

/// Formats an `f32` deterministically and losslessly.
pub fn format_f32(v: f32) -> String {
    if v.is_nan() {
        "nan".to_string()
    } else if v.is_infinite() {
        if v > 0.0 { "inf".to_string() } else { "-inf".to_string() }
    } else {
        let s = format!("{v}");
        if !s.contains('.') && !s.contains('e') && !s.contains('E') {
            format!("{s}.0")
        } else {
            s
        }
    }
}

/// Escapes special characters in string literals.
pub fn escape_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out
}

/// Unescapes special characters in string literals without panicking.
pub fn unescape_str(s: &str) -> Result<String, FscnParseError> {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some(other) => out.push(other),
                None => return Err(FscnParseError::InvalidValue("unclosed escape sequence".into())),
            }
        } else {
            out.push(c);
        }
    }
    Ok(out)
}

/// Parses a `NodeId` from format `"index:generation"`.
pub fn parse_node_id(s: &str) -> Result<NodeId, FscnParseError> {
    let clean = s.trim().trim_matches('"');
    let parts: Vec<&str> = clean.split(':').collect();
    if parts.len() != 2 {
        return Err(FscnParseError::InvalidNodeId(s.to_string()));
    }
    let index = parts[0]
        .trim()
        .parse::<u32>()
        .map_err(|_| FscnParseError::InvalidNodeId(s.to_string()))?;
    let generation = parts[1]
        .trim()
        .parse::<u32>()
        .map_err(|_| FscnParseError::InvalidNodeId(s.to_string()))?;
    NodeId::new(index, generation).ok_or_else(|| FscnParseError::InvalidNodeId(s.to_string()))
}

/// Parses an `IrValue` from a trimmed string slice without panicking.
pub fn parse_ir_value(input: &str) -> Result<IrValue, FscnParseError> {
    let mut cursor = Cursor::new(input);
    let val = parse_ir_value_cursor(&mut cursor)?;
    cursor.skip_whitespace();
    if !cursor.is_eof() {
        return Err(FscnParseError::InvalidValue(format!(
            "trailing unexpected characters: '{}'",
            cursor.remaining()
        )));
    }
    Ok(val)
}

struct Cursor<'a> {
    chars: Vec<char>,
    pos: usize,
    _marker: std::marker::PhantomData<&'a str>,
}

impl<'a> Cursor<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            chars: input.chars().collect(),
            pos: 0,
            _marker: std::marker::PhantomData,
        }
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.chars.len()
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn next(&mut self) -> Option<char> {
        if self.pos < self.chars.len() {
            let c = self.chars[self.pos];
            self.pos += 1;
            Some(c)
        } else {
            None
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn remaining(&self) -> String {
        self.chars[self.pos..].iter().collect()
    }

    fn starts_with(&self, s: &str) -> bool {
        let expected: Vec<char> = s.chars().collect();
        if self.pos + expected.len() > self.chars.len() {
            return false;
        }
        for (i, &c) in expected.iter().enumerate() {
            if self.chars[self.pos + i] != c {
                return false;
            }
        }
        true
    }

    fn consume_tag(&mut self, s: &str) -> bool {
        if self.starts_with(s) {
            self.pos += s.chars().count();
            true
        } else {
            false
        }
    }
}

fn parse_ir_value_cursor(cur: &mut Cursor<'_>) -> Result<IrValue, FscnParseError> {
    cur.skip_whitespace();
    if cur.is_eof() {
        return Err(FscnParseError::UnexpectedEof("expected IrValue".into()));
    }

    if cur.consume_tag("nil") || cur.consume_tag("null") {
        return Ok(IrValue::Nil);
    }
    if cur.consume_tag("true") {
        return Ok(IrValue::Bool(true));
    }
    if cur.consume_tag("false") {
        return Ok(IrValue::Bool(false));
    }

    if cur.peek() == Some('"') {
        let s = parse_quoted_string_cursor(cur)?;
        return Ok(IrValue::String(s));
    }

    if cur.consume_tag("Vec2") {
        let nums = parse_parenthesized_floats(cur, 2)?;
        return Ok(IrValue::Vec2([nums[0] as f32, nums[1] as f32]));
    }
    if cur.consume_tag("Vec3") {
        let nums = parse_parenthesized_floats(cur, 3)?;
        return Ok(IrValue::Vec3([nums[0] as f32, nums[1] as f32, nums[2] as f32]));
    }
    if cur.consume_tag("Quat") {
        let nums = parse_parenthesized_floats(cur, 4)?;
        return Ok(IrValue::Quat([
            nums[0] as f32,
            nums[1] as f32,
            nums[2] as f32,
            nums[3] as f32,
        ]));
    }
    if cur.consume_tag("Color") {
        let nums = parse_parenthesized_floats(cur, 4)?;
        return Ok(IrValue::Color([
            nums[0] as f32,
            nums[1] as f32,
            nums[2] as f32,
            nums[3] as f32,
        ]));
    }
    if cur.consume_tag("NodePath") {
        cur.skip_whitespace();
        if cur.next() != Some('(') {
            return Err(FscnParseError::InvalidValue("expected '(' after NodePath".into()));
        }
        cur.skip_whitespace();
        let s = parse_quoted_string_cursor(cur)?;
        cur.skip_whitespace();
        if cur.next() != Some(')') {
            return Err(FscnParseError::InvalidValue("expected ')' after NodePath".into()));
        }
        return Ok(IrValue::NodePath(s));
    }
    if cur.consume_tag("ResourcePath") {
        cur.skip_whitespace();
        if cur.next() != Some('(') {
            return Err(FscnParseError::InvalidValue("expected '(' after ResourcePath".into()));
        }
        cur.skip_whitespace();
        let s = parse_quoted_string_cursor(cur)?;
        cur.skip_whitespace();
        if cur.next() != Some(')') {
            return Err(FscnParseError::InvalidValue("expected ')' after ResourcePath".into()));
        }
        return Ok(IrValue::ResourcePath(s));
    }

    if cur.peek() == Some('[') {
        cur.next(); // consume '['
        let mut arr = Vec::new();
        cur.skip_whitespace();
        while !cur.is_eof() && cur.peek() != Some(']') {
            let elem = parse_ir_value_cursor(cur)?;
            arr.push(elem);
            cur.skip_whitespace();
            if cur.peek() == Some(',') {
                cur.next();
                cur.skip_whitespace();
            } else if cur.peek() != Some(']') {
                return Err(FscnParseError::InvalidValue("expected ',' or ']' in array".into()));
            }
        }
        if cur.next() != Some(']') {
            return Err(FscnParseError::InvalidValue("unclosed array '['".into()));
        }
        return Ok(IrValue::Array(arr));
    }

    if cur.peek() == Some('{') {
        cur.next(); // consume '{'
        let mut map = BTreeMap::new();
        cur.skip_whitespace();
        while !cur.is_eof() && cur.peek() != Some('}') {
            let key = if cur.peek() == Some('"') {
                parse_quoted_string_cursor(cur)?
            } else {
                let mut ident = String::new();
                while let Some(c) = cur.peek() {
                    if c.is_alphanumeric() || c == '_' {
                        ident.push(c);
                        cur.next();
                    } else {
                        break;
                    }
                }
                if ident.is_empty() {
                    return Err(FscnParseError::InvalidValue("expected map key".into()));
                }
                ident
            };
            cur.skip_whitespace();
            if cur.next() != Some(':') {
                return Err(FscnParseError::InvalidValue("expected ':' after map key".into()));
            }
            let val = parse_ir_value_cursor(cur)?;
            map.insert(key, val);
            cur.skip_whitespace();
            if cur.peek() == Some(',') {
                cur.next();
                cur.skip_whitespace();
            } else if cur.peek() != Some('}') {
                return Err(FscnParseError::InvalidValue("expected ',' or '}' in map".into()));
            }
        }
        if cur.next() != Some('}') {
            return Err(FscnParseError::InvalidValue("unclosed map '{'".into()));
        }
        return Ok(IrValue::Map(map));
    }

    // Number (I64 or F64)
    parse_number_cursor(cur)
}

fn parse_quoted_string_cursor(cur: &mut Cursor<'_>) -> Result<String, FscnParseError> {
    cur.skip_whitespace();
    if cur.next() != Some('"') {
        return Err(FscnParseError::InvalidValue("expected leading '\"'".into()));
    }
    let mut raw = String::new();
    let mut closed = false;
    while let Some(c) = cur.next() {
        if c == '\\' {
            raw.push('\\');
            if let Some(escaped) = cur.next() {
                raw.push(escaped);
            } else {
                return Err(FscnParseError::UnexpectedEof("unclosed escape in string".into()));
            }
        } else if c == '"' {
            closed = true;
            break;
        } else {
            raw.push(c);
        }
    }
    if !closed {
        return Err(FscnParseError::UnexpectedEof("unclosed string literal".into()));
    }
    unescape_str(&raw)
}

fn parse_parenthesized_floats(cur: &mut Cursor<'_>, count: usize) -> Result<Vec<f64>, FscnParseError> {
    cur.skip_whitespace();
    if cur.next() != Some('(') {
        return Err(FscnParseError::InvalidValue("expected '('".into()));
    }
    let mut nums = Vec::with_capacity(count);
    for i in 0..count {
        cur.skip_whitespace();
        let val = parse_float_raw(cur)?;
        nums.push(val);
        cur.skip_whitespace();
        if i + 1 < count && cur.next() != Some(',') {
            return Err(FscnParseError::InvalidValue("expected ',' separating floats".into()));
        }
    }
    cur.skip_whitespace();
    if cur.next() != Some(')') {
        return Err(FscnParseError::InvalidValue("expected ')' closing vector".into()));
    }
    Ok(nums)
}

fn parse_float_raw(cur: &mut Cursor<'_>) -> Result<f64, FscnParseError> {
    cur.skip_whitespace();
    if cur.consume_tag("nan") {
        return Ok(f64::NAN);
    }
    if cur.consume_tag("inf") {
        return Ok(f64::INFINITY);
    }
    if cur.consume_tag("-inf") {
        return Ok(f64::NEG_INFINITY);
    }

    let mut num_str = String::new();
    if cur.peek() == Some('+') || cur.peek() == Some('-') {
        num_str.push(cur.next().unwrap());
    }
    while let Some(c) = cur.peek() {
        if c.is_ascii_digit() || c == '.' || c == 'e' || c == 'E' || c == '+' || c == '-' {
            // Only allow + or - if preceded by e or E
            if (c == '+' || c == '-') && !num_str.ends_with('e') && !num_str.ends_with('E') {
                break;
            }
            num_str.push(c);
            cur.next();
        } else {
            break;
        }
    }
    if num_str.is_empty() {
        return Err(FscnParseError::InvalidValue("expected number".into()));
    }
    num_str
        .parse::<f64>()
        .map_err(|e| FscnParseError::InvalidValue(format!("invalid float '{num_str}': {e}")))
}

fn parse_number_cursor(cur: &mut Cursor<'_>) -> Result<IrValue, FscnParseError> {
    cur.skip_whitespace();
    if cur.consume_tag("nan") {
        return Ok(IrValue::F64(f64::NAN));
    }
    if cur.consume_tag("inf") {
        return Ok(IrValue::F64(f64::INFINITY));
    }
    if cur.consume_tag("-inf") {
        return Ok(IrValue::F64(f64::NEG_INFINITY));
    }

    let mut num_str = String::new();
    let mut is_float = false;

    if cur.peek() == Some('+') || cur.peek() == Some('-') {
        num_str.push(cur.next().unwrap());
    }
    while let Some(c) = cur.peek() {
        if c.is_ascii_digit() {
            num_str.push(c);
            cur.next();
        } else if c == '.' || c == 'e' || c == 'E' {
            is_float = true;
            num_str.push(c);
            cur.next();
        } else if (c == '+' || c == '-') && (num_str.ends_with('e') || num_str.ends_with('E')) {
            num_str.push(c);
            cur.next();
        } else {
            break;
        }
    }

    if num_str.is_empty() || num_str == "-" || num_str == "+" {
        return Err(FscnParseError::InvalidValue(format!(
            "unexpected token when parsing number: '{}'",
            cur.remaining()
        )));
    }

    if is_float {
        let val = num_str
            .parse::<f64>()
            .map_err(|e| FscnParseError::InvalidValue(format!("failed to parse float '{num_str}': {e}")))?;
        Ok(IrValue::F64(val))
    } else {
        match num_str.parse::<i64>() {
            Ok(val) => Ok(IrValue::I64(val)),
            Err(_) => {
                // If exceeds i64, fall back to f64
                let val = num_str.parse::<f64>().map_err(|e| {
                    FscnParseError::InvalidValue(format!("failed to parse integer '{num_str}': {e}"))
                })?;
                Ok(IrValue::F64(val))
            }
        }
    }
}

/// Parses key-value attributes inside section headers like `[node id="0:1" name="Root"]`.
fn parse_attributes(attr_str: &str) -> Result<BTreeMap<String, String>, FscnParseError> {
    let mut attrs = BTreeMap::new();
    let mut cur = Cursor::new(attr_str);
    cur.skip_whitespace();

    while !cur.is_eof() {
        let mut key = String::new();
        while let Some(c) = cur.peek() {
            if c.is_alphanumeric() || c == '_' {
                key.push(c);
                cur.next();
            } else {
                break;
            }
        }
        if key.is_empty() {
            return Err(FscnParseError::InvalidAttribute(format!(
                "expected attribute key in: '{attr_str}'"
            )));
        }
        cur.skip_whitespace();
        if cur.next() != Some('=') {
            return Err(FscnParseError::InvalidAttribute(format!(
                "expected '=' after attribute key '{key}'"
            )));
        }
        cur.skip_whitespace();
        let val = if cur.peek() == Some('"') {
            parse_quoted_string_cursor(&mut cur)?
        } else if cur.peek() == Some('[') {
            // Parse raw array token e.g. ["group1", "group2"]
            let mut depth = 0;
            let mut raw_arr = String::new();
            while let Some(c) = cur.peek() {
                if c == '[' {
                    depth += 1;
                    raw_arr.push(c);
                    cur.next();
                } else if c == ']' {
                    depth -= 1;
                    raw_arr.push(c);
                    cur.next();
                    if depth == 0 {
                        break;
                    }
                } else {
                    raw_arr.push(c);
                    cur.next();
                }
            }
            raw_arr
        } else {
            let mut bare_val = String::new();
            while let Some(c) = cur.peek() {
                if c.is_whitespace() || c == ']' {
                    break;
                }
                bare_val.push(c);
                cur.next();
            }
            bare_val
        };

        attrs.insert(key, val);
        cur.skip_whitespace();
    }

    Ok(attrs)
}

/// Parses a groups array string like `["group1", "group2"]`.
fn parse_groups(s: &str) -> Result<Vec<String>, FscnParseError> {
    let val = parse_ir_value(s)?;
    match val {
        IrValue::Array(items) => {
            let mut groups = Vec::with_capacity(items.len());
            for item in items {
                match item {
                    IrValue::String(g) => groups.push(g),
                    other => {
                        return Err(FscnParseError::InvalidAttribute(format!(
                            "expected string in groups array, found {other:?}"
                        )))
                    }
                }
            }
            Ok(groups)
        }
        other => Err(FscnParseError::InvalidAttribute(format!(
            "expected array for groups, found {other:?}"
        ))),
    }
}

/// Parses a complete `.fscn` scene document.
pub fn parse_fscn(input: &str) -> Result<IrScene, FscnParseError> {
    let mut scene = IrScene::default();
    let mut current_node: Option<IrNode> = None;
    let mut header_seen = false;

    let lines: Vec<&str> = input.lines().collect();

    for (line_idx, raw_line) in lines.iter().enumerate() {
        let line_num = line_idx + 1;
        let line = raw_line.trim();

        // Skip blank lines and comments
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            // If we had an active node, save it before opening a new section
            if let Some(node) = current_node.take() {
                if scene.nodes.contains_key(&node.id) {
                    return Err(FscnParseError::DuplicateNode(node.id));
                }
                scene.nodes.insert(node.id, node);
            }

            let inside = &line[1..line.len() - 1].trim();
            let mut split_ws = inside.split_whitespace();
            let tag = split_ws.next().unwrap_or("");
            let rest = inside[tag.len()..].trim();

            match tag {
                "foreplay_scene" => {
                    header_seen = true;
                    let attrs = parse_attributes(rest).map_err(|e| FscnParseError::SyntaxError {
                        line: line_num,
                        message: format!("invalid foreplay_scene header: {e}"),
                    })?;
                    if let Some(name) = attrs.get("name") {
                        scene.name = name.clone();
                    }
                }
                "feel_spec" => {
                    let attrs = parse_attributes(rest).map_err(|e| FscnParseError::SyntaxError {
                        line: line_num,
                        message: format!("invalid feel_spec section: {e}"),
                    })?;
                    let key = attrs.get("key").ok_or_else(|| FscnParseError::SyntaxError {
                        line: line_num,
                        message: "missing 'key' in feel_spec".into(),
                    })?;
                    let val_str = attrs.get("value").ok_or_else(|| FscnParseError::SyntaxError {
                        line: line_num,
                        message: "missing 'value' in feel_spec".into(),
                    })?;
                    let val = val_str.parse::<f64>().map_err(|e| FscnParseError::SyntaxError {
                        line: line_num,
                        message: format!("invalid float in feel_spec value '{val_str}': {e}"),
                    })?;
                    scene.feel_overrides.insert(key.clone(), val);
                }
                "node" => {
                    let attrs = parse_attributes(rest).map_err(|e| FscnParseError::SyntaxError {
                        line: line_num,
                        message: format!("invalid node section: {e}"),
                    })?;
                    let id_str = attrs.get("id").ok_or_else(|| FscnParseError::SyntaxError {
                        line: line_num,
                        message: "missing 'id' attribute in node".into(),
                    })?;
                    let id = parse_node_id(id_str)?;
                    let name = attrs.get("name").cloned().unwrap_or_default();
                    let class_name = attrs.get("type").cloned().unwrap_or_else(|| "Node".to_string());
                    let parent = if let Some(p_str) = attrs.get("parent") {
                        Some(parse_node_id(p_str)?)
                    } else {
                        None
                    };
                    let instance_scene = attrs.get("instance").cloned();
                    let groups = if let Some(g_str) = attrs.get("groups") {
                        parse_groups(g_str).map_err(|e| FscnParseError::SyntaxError {
                            line: line_num,
                            message: format!("invalid groups attribute: {e}"),
                        })?
                    } else {
                        Vec::new()
                    };

                    current_node = Some(IrNode {
                        id,
                        name,
                        class_name,
                        parent,
                        instance_scene,
                        properties: BTreeMap::new(),
                        groups,
                    });
                }
                "connection" => {
                    let attrs = parse_attributes(rest).map_err(|e| FscnParseError::SyntaxError {
                        line: line_num,
                        message: format!("invalid connection section: {e}"),
                    })?;
                    let signal_name = attrs.get("signal").ok_or_else(|| FscnParseError::SyntaxError {
                        line: line_num,
                        message: "missing 'signal' in connection".into(),
                    })?;
                    let from_str = attrs.get("from").ok_or_else(|| FscnParseError::SyntaxError {
                        line: line_num,
                        message: "missing 'from' in connection".into(),
                    })?;
                    let to_str = attrs.get("to").ok_or_else(|| FscnParseError::SyntaxError {
                        line: line_num,
                        message: "missing 'to' in connection".into(),
                    })?;
                    let method_name = attrs.get("method").ok_or_else(|| FscnParseError::SyntaxError {
                        line: line_num,
                        message: "missing 'method' in connection".into(),
                    })?;

                    scene.connections.push(SignalConnection {
                        from_node: parse_node_id(from_str)?,
                        signal_name: signal_name.clone(),
                        to_node: parse_node_id(to_str)?,
                        method_name: method_name.clone(),
                    });
                }
                other => {
                    return Err(FscnParseError::SyntaxError {
                        line: line_num,
                        message: format!("unknown section header: [{other}]"),
                    });
                }
            }
        } else if let Some(ref mut node) = current_node {
            // Property line: key = value
            if let Some((k_part, v_part)) = line.split_once('=') {
                let key = k_part.trim().to_string();
                let val = parse_ir_value(v_part.trim()).map_err(|e| FscnParseError::SyntaxError {
                    line: line_num,
                    message: format!("error parsing property '{key}': {e}"),
                })?;
                node.properties.insert(key, val);
            } else {
                return Err(FscnParseError::SyntaxError {
                    line: line_num,
                    message: format!("expected 'key = value' line in node, found '{line}'"),
                });
            }
        } else {
            return Err(FscnParseError::SyntaxError {
                line: line_num,
                message: format!("property line outside of [node] section: '{line}'"),
            });
        }
    }

    // Flush last node if any
    if let Some(node) = current_node {
        if scene.nodes.contains_key(&node.id) {
            return Err(FscnParseError::DuplicateNode(node.id));
        }
        scene.nodes.insert(node.id, node);
    }

    if !header_seen && input.trim().is_empty() {
        return Ok(scene);
    }

    Ok(scene)
}
