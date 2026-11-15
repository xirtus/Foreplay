//! Godot 4.4 `Control` Anchor/Offset and Container Layout Engine.

use foreplay_ir::NodeId;
use std::collections::BTreeMap;

/// Standard Godot 4 Anchor Presets (16 total).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AnchorPreset {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    CenterLeft,
    CenterTop,
    CenterRight,
    CenterBottom,
    Center,
    LeftWide,
    TopWide,
    RightWide,
    BottomWide,
    VCenterWide,
    HCenterWide,
    FullRect,
}

impl AnchorPreset {
    /// Returns the [left, top, right, bottom] anchor fractions in 0.0..1.0 range.
    pub const fn anchors(self) -> [f32; 4] {
        match self {
            Self::TopLeft => [0.0, 0.0, 0.0, 0.0],
            Self::TopRight => [1.0, 0.0, 1.0, 0.0],
            Self::BottomLeft => [0.0, 1.0, 0.0, 1.0],
            Self::BottomRight => [1.0, 1.0, 1.0, 1.0],
            Self::CenterLeft => [0.0, 0.5, 0.0, 0.5],
            Self::CenterTop => [0.5, 0.0, 0.5, 0.0],
            Self::CenterRight => [1.0, 0.5, 1.0, 0.5],
            Self::CenterBottom => [0.5, 1.0, 0.5, 1.0],
            Self::Center => [0.5, 0.5, 0.5, 0.5],
            Self::LeftWide => [0.0, 0.0, 0.0, 1.0],
            Self::TopWide => [0.0, 0.0, 1.0, 0.0],
            Self::RightWide => [1.0, 0.0, 1.0, 1.0],
            Self::BottomWide => [0.0, 1.0, 1.0, 1.0],
            Self::VCenterWide => [0.5, 0.0, 0.5, 1.0],
            Self::HCenterWide => [0.0, 0.5, 1.0, 0.5],
            Self::FullRect => [0.0, 0.0, 1.0, 1.0],
        }
    }
}

/// Sizing policy flags used by Container children (`Control::SizeFlags`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SizeFlags {
    pub fill: bool,
    pub expand: bool,
    pub shrink_center: bool,
    pub shrink_end: bool,
    pub stretch_ratio: f32,
}

impl Default for SizeFlags {
    fn default() -> Self {
        Self {
            fill: true,
            expand: false,
            shrink_center: false,
            shrink_end: false,
            stretch_ratio: 1.0,
        }
    }
}

impl SizeFlags {
    pub const fn fill() -> Self {
        Self {
            fill: true,
            expand: false,
            shrink_center: false,
            shrink_end: false,
            stretch_ratio: 1.0,
        }
    }

    pub const fn expand_fill() -> Self {
        Self {
            fill: true,
            expand: true,
            shrink_center: false,
            shrink_end: false,
            stretch_ratio: 1.0,
        }
    }

    pub const fn shrink_center() -> Self {
        Self {
            fill: false,
            expand: false,
            shrink_center: true,
            shrink_end: false,
            stretch_ratio: 1.0,
        }
    }

    pub fn with_stretch(mut self, ratio: f32) -> Self {
        self.stretch_ratio = ratio.max(0.01);
        self
    }
}

/// Computed screen/viewport axis-aligned rectangle.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct ControlRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl ControlRect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width: width.max(0.0),
            height: height.max(0.0),
        }
    }

    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }
}

/// Supported Godot container layout algorithms.
#[derive(Clone, PartialEq, Debug)]
pub enum ContainerKind {
    Control,
    MarginContainer {
        margins: [f32; 4], // [left, top, right, bottom]
    },
    CenterContainer,
    HBoxContainer {
        separation: f32,
    },
    VBoxContainer {
        separation: f32,
    },
    GridContainer {
        columns: usize,
        h_separation: f32,
        v_separation: f32,
    },
}

/// Layout specification for a single node inside the UI layout tree.
#[derive(Clone, PartialEq, Debug)]
pub struct ControlLayoutNode {
    pub id: NodeId,
    pub kind: ContainerKind,
    pub anchors: [f32; 4], // [left, top, right, bottom]
    pub offsets: [f32; 4], // [left, top, right, bottom]
    pub min_size: [f32; 2],
    pub size_flags_h: SizeFlags,
    pub size_flags_v: SizeFlags,
    pub children: Vec<ControlLayoutNode>,
}

impl ControlLayoutNode {
    pub fn new(id: NodeId, kind: ContainerKind) -> Self {
        Self {
            id,
            kind,
            anchors: [0.0, 0.0, 0.0, 0.0],
            offsets: [0.0, 0.0, 0.0, 0.0],
            min_size: [0.0, 0.0],
            size_flags_h: SizeFlags::default(),
            size_flags_v: SizeFlags::default(),
            children: Vec::new(),
        }
    }

    pub fn with_preset(mut self, preset: AnchorPreset) -> Self {
        self.anchors = preset.anchors();
        self
    }

    pub fn with_min_size(mut self, width: f32, height: f32) -> Self {
        self.min_size = [width, height];
        self
    }

    pub fn with_offsets(mut self, offsets: [f32; 4]) -> Self {
        self.offsets = offsets;
        self
    }

    pub fn with_size_flags(mut self, h: SizeFlags, v: SizeFlags) -> Self {
        self.size_flags_h = h;
        self.size_flags_v = v;
        self
    }

    pub fn add_child(&mut self, child: ControlLayoutNode) {
        self.children.push(child);
    }
}

/// Solves recursive Godot 4 Control and Container hierarchy layout into exact screen rects.
pub fn solve_control_hierarchy(
    viewport_rect: ControlRect,
    root: &ControlLayoutNode,
) -> BTreeMap<NodeId, ControlRect> {
    let mut results = BTreeMap::new();
    let root_computed = compute_control_rect(viewport_rect, root);
    results.insert(root.id, root_computed);
    solve_children_recursive(root_computed, root, &mut results);
    results
}

fn compute_control_rect(parent: ControlRect, node: &ControlLayoutNode) -> ControlRect {
    let left_anchor_px = parent.x + parent.width * node.anchors[0];
    let top_anchor_px = parent.y + parent.height * node.anchors[1];
    let right_anchor_px = parent.x + parent.width * node.anchors[2];
    let bottom_anchor_px = parent.y + parent.height * node.anchors[3];

    let x = left_anchor_px + node.offsets[0];
    let y = top_anchor_px + node.offsets[1];
    let right = right_anchor_px + node.offsets[2];
    let bottom = bottom_anchor_px + node.offsets[3];

    let w = (right - x).max(node.min_size[0]);
    let h = (bottom - y).max(node.min_size[1]);

    ControlRect::new(x, y, w, h)
}

fn solve_children_recursive(
    current_rect: ControlRect,
    node: &ControlLayoutNode,
    results: &mut BTreeMap<NodeId, ControlRect>,
) {
    if node.children.is_empty() {
        return;
    }

    match &node.kind {
        ContainerKind::Control => {
            for child in &node.children {
                let child_rect = compute_control_rect(current_rect, child);
                results.insert(child.id, child_rect);
                solve_children_recursive(child_rect, child, results);
            }
        }

        ContainerKind::MarginContainer { margins } => {
            let inner_x = current_rect.x + margins[0];
            let inner_y = current_rect.y + margins[1];
            let inner_w = (current_rect.width - margins[0] - margins[2]).max(0.0);
            let inner_h = (current_rect.height - margins[1] - margins[3]).max(0.0);
            let inner_rect = ControlRect::new(inner_x, inner_y, inner_w, inner_h);

            for child in &node.children {
                results.insert(child.id, inner_rect);
                solve_children_recursive(inner_rect, child, results);
            }
        }

        ContainerKind::CenterContainer => {
            for child in &node.children {
                let w = child.min_size[0];
                let h = child.min_size[1];
                let x = current_rect.x + (current_rect.width - w) * 0.5;
                let y = current_rect.y + (current_rect.height - h) * 0.5;
                let child_rect = ControlRect::new(x, y, w, h);
                results.insert(child.id, child_rect);
                solve_children_recursive(child_rect, child, results);
            }
        }

        ContainerKind::HBoxContainer { separation } => {
            let child_count = node.children.len();
            let total_sep = separation * (child_count.saturating_sub(1) as f32);
            let available_width = (current_rect.width - total_sep).max(0.0);

            let mut min_widths_total = 0.0;
            let mut total_stretch = 0.0;

            for child in &node.children {
                min_widths_total += child.min_size[0];
                if child.size_flags_h.expand {
                    total_stretch += child.size_flags_h.stretch_ratio;
                }
            }

            let leftover_width = (available_width - min_widths_total).max(0.0);
            let mut curr_x = current_rect.x;

            for child in &node.children {
                let mut w = child.min_size[0];
                if child.size_flags_h.expand && total_stretch > 0.0 {
                    w += leftover_width * (child.size_flags_h.stretch_ratio / total_stretch);
                }

                let (y, h) = if child.size_flags_v.fill {
                    (current_rect.y, current_rect.height.max(child.min_size[1]))
                } else if child.size_flags_v.shrink_center {
                    (
                        current_rect.y + (current_rect.height - child.min_size[1]) * 0.5,
                        child.min_size[1],
                    )
                } else {
                    (current_rect.y, child.min_size[1])
                };

                let child_rect = ControlRect::new(curr_x, y, w, h);
                results.insert(child.id, child_rect);
                solve_children_recursive(child_rect, child, results);

                curr_x += w + separation;
            }
        }

        ContainerKind::VBoxContainer { separation } => {
            let child_count = node.children.len();
            let total_sep = separation * (child_count.saturating_sub(1) as f32);
            let available_height = (current_rect.height - total_sep).max(0.0);

            let mut min_heights_total = 0.0;
            let mut total_stretch = 0.0;

            for child in &node.children {
                min_heights_total += child.min_size[1];
                if child.size_flags_v.expand {
                    total_stretch += child.size_flags_v.stretch_ratio;
                }
            }

            let leftover_height = (available_height - min_heights_total).max(0.0);
            let mut curr_y = current_rect.y;

            for child in &node.children {
                let mut h = child.min_size[1];
                if child.size_flags_v.expand && total_stretch > 0.0 {
                    h += leftover_height * (child.size_flags_v.stretch_ratio / total_stretch);
                }

                let (x, w) = if child.size_flags_h.fill {
                    (current_rect.x, current_rect.width.max(child.min_size[0]))
                } else if child.size_flags_h.shrink_center {
                    (
                        current_rect.x + (current_rect.width - child.min_size[0]) * 0.5,
                        child.min_size[0],
                    )
                } else {
                    (current_rect.x, child.min_size[0])
                };

                let child_rect = ControlRect::new(x, curr_y, w, h);
                results.insert(child.id, child_rect);
                solve_children_recursive(child_rect, child, results);

                curr_y += h + separation;
            }
        }

        ContainerKind::GridContainer {
            columns,
            h_separation,
            v_separation,
        } => {
            let cols = (*columns).max(1);
            let child_count = node.children.len();
            let rows = child_count.div_ceil(cols);

            let mut col_widths = vec![0.0f32; cols];
            let mut row_heights = vec![0.0f32; rows];

            for (i, child) in node.children.iter().enumerate() {
                let c = i % cols;
                let r = i / cols;
                col_widths[c] = col_widths[c].max(child.min_size[0]);
                row_heights[r] = row_heights[r].max(child.min_size[1]);
            }

            let mut row_y = current_rect.y;
            for (r, &row_h) in row_heights.iter().enumerate() {
                let mut col_x = current_rect.x;
                for (c, &col_w) in col_widths.iter().enumerate() {
                    let idx = r * cols + c;
                    if idx < child_count {
                        let child = &node.children[idx];
                        let child_rect = ControlRect::new(col_x, row_y, col_w, row_h);
                        results.insert(child.id, child_rect);
                        solve_children_recursive(child_rect, child, results);
                    }
                    col_x += col_w + h_separation;
                }
                row_y += row_h + v_separation;
            }
        }
    }
}
