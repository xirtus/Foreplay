//! Godot 4 GDScript Language Server Protocol (LSP) analyzer.
//!
//! Provides document symbols, syntax/type diagnostics, context-aware completions,
//! hover tooltips, and go-to-definition.

use crate::builtins_catalog::builtin_catalog;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Line/column position (1-indexed).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct GdPosition {
    pub line: usize,
    pub col: usize,
}

/// Span range within a document.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct GdRange {
    pub start: GdPosition,
    pub end: GdPosition,
}

/// Symbol kind in the GDScript document outline.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum GdSymbolKind {
    Class,
    Function,
    Variable,
    Constant,
    Property,
    Signal,
    Enum,
    Parameter,
}

/// Document symbol representation.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct GdSymbol {
    pub name: String,
    pub kind: GdSymbolKind,
    pub detail: Option<String>,
    pub range: GdRange,
    pub children: Vec<GdSymbol>,
}

/// Diagnostic severity level.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum GdDiagnosticSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

/// Diagnostic message produced during static analysis.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct GdDiagnostic {
    pub range: GdRange,
    pub severity: GdDiagnosticSeverity,
    pub message: String,
    pub code: Option<String>,
}

/// Complete result of analyzing a GDScript source document.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct GdDocumentAnalysis {
    pub uri: String,
    pub symbols: Vec<GdSymbol>,
    pub diagnostics: Vec<GdDiagnostic>,
    pub executable_lines: Vec<usize>,
}

/// Kind of auto-completion suggestion.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum GdCompletionKind {
    Function,
    Method,
    Variable,
    Property,
    Keyword,
    Annotation,
    Class,
    Constant,
    Snippet,
}

/// Auto-completion item.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct GdCompletionItem {
    pub label: String,
    pub kind: GdCompletionKind,
    pub detail: Option<String>,
    pub documentation: Option<String>,
    pub insert_text: Option<String>,
}

/// Tooltip documentation returned for hover requests.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct GdHoverInfo {
    pub contents: String,
    pub range: Option<GdRange>,
}

/// Location reference for go-to-definition.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct GdLocation {
    pub uri: String,
    pub range: GdRange,
}

/// Static analyzer implementing GDScript LSP services.
#[derive(Clone, Default, Debug)]
pub struct GdLspAnalyzer;

impl GdLspAnalyzer {
    pub fn new() -> Self {
        Self
    }

    /// Analyzes a GDScript document, generating symbols, diagnostics, and executable lines.
    pub fn analyze_document(&self, uri: &str, source: &str) -> GdDocumentAnalysis {
        let mut symbols = Vec::new();
        let mut diagnostics = Vec::new();
        let mut executable_lines = Vec::new();

        let lines: Vec<&str> = source.lines().collect();
        let mut in_function = false;

        for (idx, &raw_line) in lines.iter().enumerate() {
            let line_num = idx + 1;
            let trimmed = raw_line.trim();

            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            let indent = raw_line.len() - raw_line.trim_start().len();
            if indent == 0 {
                in_function = false;
            }

            let col_start = indent + 1;
            let col_end = col_start + trimmed.len();
            let range = GdRange {
                start: GdPosition { line: line_num, col: col_start },
                end: GdPosition { line: line_num, col: col_end },
            };

            // 1. class_name
            if let Some(rest) = trimmed.strip_prefix("class_name ") {
                let name = rest.trim().to_string();
                symbols.push(GdSymbol {
                    name,
                    kind: GdSymbolKind::Class,
                    detail: Some("class declaration".into()),
                    range,
                    children: Vec::new(),
                });
                continue;
            }

            // 2. signal
            if let Some(rest) = trimmed.strip_prefix("signal ") {
                let sig_name = rest.split('(').next().unwrap_or(rest).trim().to_string();
                symbols.push(GdSymbol {
                    name: sig_name,
                    kind: GdSymbolKind::Signal,
                    detail: Some(rest.trim().to_string()),
                    range,
                    children: Vec::new(),
                });
                continue;
            }

            // 3. const
            if let Some(rest) = trimmed.strip_prefix("const ") {
                let const_name = rest.split(['=', ':']).next().unwrap_or(rest).trim().to_string();
                symbols.push(GdSymbol {
                    name: const_name,
                    kind: GdSymbolKind::Constant,
                    detail: Some(rest.trim().to_string()),
                    range,
                    children: Vec::new(),
                });
                continue;
            }

            // 4. enum
            if let Some(rest) = trimmed.strip_prefix("enum ") {
                let enum_name = rest.split('{').next().unwrap_or(rest).trim().to_string();
                symbols.push(GdSymbol {
                    name: enum_name,
                    kind: GdSymbolKind::Enum,
                    detail: Some("enum definition".into()),
                    range,
                    children: Vec::new(),
                });
                continue;
            }

            // 5. @export / @export_range
            if trimmed.starts_with("@export") {
                if let Some(v_idx) = trimmed.find("var ") {
                    let var_part = &trimmed[v_idx + 4..];
                    let var_name = var_part.split([':', '=']).next().unwrap_or(var_part).trim().to_string();
                    symbols.push(GdSymbol {
                        name: var_name,
                        kind: GdSymbolKind::Property,
                        detail: Some(trimmed.to_string()),
                        range,
                        children: Vec::new(),
                    });
                }
                continue;
            }

            // 6. Top-level var
            if indent == 0 && let Some(rest) = trimmed.strip_prefix("var ") {
                let var_name = rest.split([':', '=']).next().unwrap_or(rest).trim().to_string();
                symbols.push(GdSymbol {
                    name: var_name,
                    kind: GdSymbolKind::Variable,
                    detail: Some(rest.trim().to_string()),
                    range,
                    children: Vec::new(),
                });
                continue;
            }

            // 7. func
            if let Some(rest) = trimmed.strip_prefix("func ") {
                in_function = true;
                let clean_decl = rest.trim_end_matches(':').trim();
                let func_name = clean_decl.split('(').next().unwrap_or(clean_decl).trim().to_string();

                let mut params = Vec::new();
                if let Some(open) = clean_decl.find('(')
                    && let Some(close) = clean_decl.rfind(')')
                {
                    let p_str = &clean_decl[open + 1..close];
                    for p in p_str.split(',') {
                        let p_clean = p.trim();
                        if !p_clean.is_empty() {
                            let p_name = p_clean.split([':', '=']).next().unwrap_or(p_clean).trim();
                            params.push(GdSymbol {
                                name: p_name.to_string(),
                                kind: GdSymbolKind::Parameter,
                                detail: Some(p_clean.to_string()),
                                range,
                                children: Vec::new(),
                            });
                        }
                    }
                }

                symbols.push(GdSymbol {
                    name: func_name,
                    kind: GdSymbolKind::Function,
                    detail: Some(clean_decl.to_string()),
                    range,
                    children: params,
                });
                continue;
            }

            // Inside function body executable lines
            if in_function && indent > 0 && trimmed != "pass" {
                executable_lines.push(line_num);
            }
        }

        // Check for unresolved identifiers or syntax issues
        let known_symbols: HashSet<String> = symbols.iter().map(|s| s.name.clone()).collect();
        for (idx, &line) in lines.iter().enumerate() {
            let line_num = idx + 1;
            let trimmed = line.trim();
            if trimmed.starts_with('#') || trimmed.is_empty() {
                continue;
            }

            // Check mismatched brackets
            let open_parens = trimmed.chars().filter(|&c| c == '(').count();
            let close_parens = trimmed.chars().filter(|&c| c == ')').count();
            if open_parens != close_parens {
                diagnostics.push(GdDiagnostic {
                    range: GdRange {
                        start: GdPosition { line: line_num, col: 1 },
                        end: GdPosition { line: line_num, col: line.len() + 1 },
                    },
                    severity: GdDiagnosticSeverity::Error,
                    message: "Unbalanced parentheses".into(),
                    code: Some("SYNTAX_ERROR".into()),
                });
            }

            // Check undefined bare identifiers in simple calls
            if let Some(call_start) = trimmed.find('(')
                && !trimmed.starts_with("func ")
                && !trimmed.starts_with("if ")
                && !trimmed.starts_with("while ")
                && !trimmed.starts_with("for ")
                && !trimmed.starts_with("match ")
                && !trimmed.starts_with("return")
            {
                let ident = trimmed[..call_start].trim();
                let clean_ident = ident.split('.').next_back().unwrap_or(ident);
                if !clean_ident.is_empty()
                    && clean_ident.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    && !known_symbols.contains(clean_ident)
                    && !builtin_catalog().iter().any(|b| b.name == clean_ident)
                    && !is_keyword(clean_ident)
                {
                    diagnostics.push(GdDiagnostic {
                        range: GdRange {
                            start: GdPosition { line: line_num, col: line.find(clean_ident).unwrap_or(0) + 1 },
                            end: GdPosition { line: line_num, col: line.find(clean_ident).unwrap_or(0) + clean_ident.len() + 1 },
                        },
                        severity: GdDiagnosticSeverity::Warning,
                        message: format!("Unresolved function or identifier '{clean_ident}'"),
                        code: Some("UNRESOLVED_IDENTIFIER".into()),
                    });
                }
            }
        }

        GdDocumentAnalysis {
            uri: uri.to_string(),
            symbols,
            diagnostics,
            executable_lines,
        }
    }

    /// Provides context-aware completions at the given line and column.
    pub fn complete_at(&self, source: &str, line: usize, col: usize) -> Vec<GdCompletionItem> {
        let lines: Vec<&str> = source.lines().collect();
        if line == 0 || line > lines.len() {
            return Vec::new();
        }

        let cur_line = lines[line - 1];
        let col_idx = col.min(cur_line.len());
        let prefix_line = &cur_line[..col_idx];
        let trimmed_prefix = prefix_line.trim_end();

        // 1. After dot -> Member / method completions
        if trimmed_prefix.ends_with('.') {
            let mut items = Vec::new();
            for b in builtin_catalog() {
                if b.receiver.is_some() {
                    items.push(GdCompletionItem {
                        label: b.name.clone(),
                        kind: GdCompletionKind::Method,
                        detail: Some(b.signature.clone()),
                        documentation: Some(b.doc.clone()),
                        insert_text: Some(format!("{}(", b.name)),
                    });
                }
            }
            return items;
        }

        // 2. After '@' -> Annotations
        if trimmed_prefix.ends_with('@') {
            let annotations = [
                ("export", "Exports property to inspector."),
                ("export_range", "Exports numeric range constraint."),
                ("export_enum", "Exports enumeration selector."),
                ("export_file", "Exports file path picker."),
                ("export_multiline", "Exports multi-line string text box."),
                ("export_flags", "Exports bit flag checkboxes."),
                ("onready", "Initializes variable when node is ready."),
                ("tool", "Runs script in the editor."),
                ("icon", "Specifies custom node icon."),
                ("rpc", "Configures remote procedure call mode."),
                ("warning_ignore", "Silences specific compiler warning."),
            ];
            return annotations
                .iter()
                .map(|(name, doc)| GdCompletionItem {
                    label: format!("@{name}"),
                    kind: GdCompletionKind::Annotation,
                    detail: Some(format!("@{name}")),
                    documentation: Some((*doc).to_string()),
                    insert_text: Some(name.to_string()),
                })
                .collect();
        }

        // 3. General completions: Keywords, Document Symbols, Global Builtins
        let mut items = Vec::new();

        let keywords = [
            "func", "var", "const", "signal", "extends", "class_name", "enum",
            "if", "elif", "else", "while", "for", "match", "when", "return",
            "pass", "await", "super", "self", "true", "false", "null", "and", "or", "not", "in",
        ];
        for kw in keywords {
            items.push(GdCompletionItem {
                label: kw.to_string(),
                kind: GdCompletionKind::Keyword,
                detail: Some(format!("GDScript keyword '{kw}'")),
                documentation: None,
                insert_text: Some(kw.to_string()),
            });
        }

        // Global builtins
        for b in builtin_catalog() {
            if b.receiver.is_none() {
                items.push(GdCompletionItem {
                    label: b.name.clone(),
                    kind: GdCompletionKind::Function,
                    detail: Some(b.signature.clone()),
                    documentation: Some(b.doc.clone()),
                    insert_text: Some(format!("{}(", b.name)),
                });
            }
        }

        // Document symbols
        let analysis = self.analyze_document("", source);
        for sym in analysis.symbols {
            let kind = match sym.kind {
                GdSymbolKind::Class => GdCompletionKind::Class,
                GdSymbolKind::Function => GdCompletionKind::Function,
                GdSymbolKind::Variable => GdCompletionKind::Variable,
                GdSymbolKind::Constant => GdCompletionKind::Constant,
                GdSymbolKind::Property => GdCompletionKind::Property,
                GdSymbolKind::Signal => GdCompletionKind::Function,
                GdSymbolKind::Enum => GdCompletionKind::Class,
                GdSymbolKind::Parameter => GdCompletionKind::Variable,
            };
            items.push(GdCompletionItem {
                label: sym.name.clone(),
                kind,
                detail: sym.detail,
                documentation: None,
                insert_text: Some(sym.name),
            });
        }

        items
    }

    /// Provides hover information for symbol under the cursor.
    pub fn hover_at(&self, source: &str, line: usize, col: usize) -> Option<GdHoverInfo> {
        let word = extract_word_at(source, line, col)?;

        // 1. Check built-ins
        if let Some(b) = builtin_catalog().iter().find(|b| b.name == word) {
            let recv_str = match &b.receiver {
                Some(r) => format!("{r}."),
                None => String::new(),
            };
            let contents = format!(
                "```gdscript\n{}{}\n```\n\n{}\n\n**Returns:** `{}`",
                recv_str, b.signature, b.doc, b.return_type
            );
            return Some(GdHoverInfo {
                contents,
                range: None,
            });
        }

        // 2. Check document symbols
        let analysis = self.analyze_document("", source);
        if let Some(sym) = analysis.symbols.iter().find(|s| s.name == word) {
            let contents = format!(
                "```gdscript\n{:?}: {}\n```\n{}",
                sym.kind,
                sym.name,
                sym.detail.as_deref().unwrap_or("")
            );
            return Some(GdHoverInfo {
                contents,
                range: Some(sym.range),
            });
        }

        // 3. Check keywords
        if is_keyword(&word) {
            return Some(GdHoverInfo {
                contents: format!("**GDScript keyword**: `{word}`"),
                range: None,
            });
        }

        None
    }

    /// Finds the declaration location of the symbol under the cursor.
    pub fn goto_definition(&self, source: &str, line: usize, col: usize) -> Option<GdLocation> {
        let word = extract_word_at(source, line, col)?;
        let analysis = self.analyze_document("", source);

        for sym in &analysis.symbols {
            if sym.name == word {
                return Some(GdLocation {
                    uri: analysis.uri.clone(),
                    range: sym.range,
                });
            }
            for child in &sym.children {
                if child.name == word {
                    return Some(GdLocation {
                        uri: analysis.uri.clone(),
                        range: child.range,
                    });
                }
            }
        }

        None
    }
}

fn extract_word_at(source: &str, line: usize, col: usize) -> Option<String> {
    let lines: Vec<&str> = source.lines().collect();
    if line == 0 || line > lines.len() {
        return None;
    }
    let cur_line = lines[line - 1];
    if cur_line.is_empty() {
        return None;
    }

    let char_indices: Vec<(usize, char)> = cur_line.char_indices().collect();
    let col_idx = col.saturating_sub(1).min(char_indices.len().saturating_sub(1));

    let mut start = col_idx;
    while start > 0 && is_ident_char(char_indices[start - 1].1) {
        start -= 1;
    }

    let mut end = col_idx;
    while end < char_indices.len() && is_ident_char(char_indices[end].1) {
        end += 1;
    }

    if start < end {
        let byte_start = char_indices[start].0;
        let byte_end = if end < char_indices.len() {
            char_indices[end].0
        } else {
            cur_line.len()
        };
        Some(cur_line[byte_start..byte_end].to_string())
    } else {
        None
    }
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '@'
}

fn is_keyword(s: &str) -> bool {
    matches!(
        s,
        "func"
            | "var"
            | "const"
            | "signal"
            | "extends"
            | "class_name"
            | "enum"
            | "if"
            | "elif"
            | "else"
            | "while"
            | "for"
            | "match"
            | "when"
            | "return"
            | "pass"
            | "await"
            | "super"
            | "self"
            | "true"
            | "false"
            | "null"
            | "and"
            | "or"
            | "not"
            | "in"
    )
}
