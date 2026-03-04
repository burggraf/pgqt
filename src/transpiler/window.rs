//! Window function support for the transpiler
//!
//! This module handles the reconstruction of PostgreSQL window definitions
//! and frame specifications into SQLite-compatible SQL.

use pg_query::protobuf::WindowDef;
use super::context::TranspileContext;
use crate::transpiler::reconstruct_node;

/// Frame option flags for window specifications
pub mod frame_options {
    #![allow(dead_code)]
    
    pub const NONDEFAULT: i32 = 0x00001;
    pub const RANGE: i32 = 0x00002;
    pub const ROWS: i32 = 0x00004;
    pub const GROUPS: i32 = 0x00008;
    pub const BETWEEN: i32 = 0x00010;
    pub const START_UNBOUNDED_PRECEDING: i32 = 0x00020;
    pub const END_UNBOUNDED_PRECEDING: i32 = 0x00040; 
    pub const START_UNBOUNDED_FOLLOWING: i32 = 0x00080; 
    pub const END_UNBOUNDED_FOLLOWING: i32 = 0x00100;
    pub const START_CURRENT_ROW: i32 = 0x00200;
    pub const END_CURRENT_ROW: i32 = 0x00400;
    pub const START_OFFSET_PRECEDING: i32 = 0x00800;
    pub const END_OFFSET_PRECEDING: i32 = 0x01000;
    pub const START_OFFSET_FOLLOWING: i32 = 0x02000;
    pub const END_OFFSET_FOLLOWING: i32 = 0x04000;
    pub const EXCLUDE_CURRENT_ROW: i32 = 0x08000;
    pub const EXCLUDE_GROUP: i32 = 0x10000;
    pub const EXCLUDE_TIES: i32 = 0x20000;
    pub const EXCLUSION: i32 = EXCLUDE_CURRENT_ROW | EXCLUDE_GROUP | EXCLUDE_TIES;
}

pub(crate) fn reconstruct_window_def(win_def: &WindowDef, ctx: &mut TranspileContext) -> String {
    let mut parts = Vec::new();

    // Handle named window reference (e.g., OVER w)
    if !win_def.refname.is_empty() {
        return win_def.refname.to_lowercase();
    }

    // PARTITION BY clause
    if !win_def.partition_clause.is_empty() {
        let partition_cols: Vec<String> = win_def
            .partition_clause
            .iter()
            .map(|n| reconstruct_node(n, ctx))
            .collect();
        parts.push(format!("partition by {}", partition_cols.join(", ")));
    }

    // ORDER BY clause
    if !win_def.order_clause.is_empty() {
        let order_cols: Vec<String> = win_def
            .order_clause
            .iter()
            .map(|n| reconstruct_node(n, ctx))
            .collect();
        parts.push(format!("order by {}", order_cols.join(", ")));
    }

    // Frame specification
    let frame_opts = win_def.frame_options;

    // Only add frame if NONDEFAULT is set (explicit frame specified)
    if frame_opts & frame_options::NONDEFAULT != 0 {
        let frame_str = reconstruct_frame_specification(win_def, ctx);
        if !frame_str.is_empty() {
            parts.push(frame_str);
        }
    }

    parts.join(" ")
}

/// Reconstruct frame specification (ROWS/RANGE/GROUPS BETWEEN ... AND ...)
pub(crate) fn reconstruct_frame_specification(win_def: &WindowDef, ctx: &mut TranspileContext) -> String {
    let frame_opts = win_def.frame_options;
    let mut parts = Vec::new();

    // Determine frame mode: ROWS, RANGE, or GROUPS
    let mode = if frame_opts & frame_options::ROWS != 0 {
        "rows"
    } else if frame_opts & frame_options::GROUPS != 0 {
        "groups"
    } else {
        "range" // default
    };

    // Check for BETWEEN
    let has_between = frame_opts & frame_options::BETWEEN != 0;

    // Build start bound
    let start_bound = if frame_opts & frame_options::START_UNBOUNDED_PRECEDING != 0 {
        "unbounded preceding".to_string()
    } else if frame_opts & frame_options::START_CURRENT_ROW != 0 {
        "current row".to_string()
    } else if frame_opts & frame_options::START_OFFSET_PRECEDING != 0 {
        if let Some(ref offset) = win_def.start_offset {
            format!("{} preceding", reconstruct_node(offset, ctx))
        } else {
            "unbounded preceding".to_string()
        }
    } else if frame_opts & frame_options::START_OFFSET_FOLLOWING != 0 {
        if let Some(ref offset) = win_def.start_offset {
            format!("{} following", reconstruct_node(offset, ctx))
        } else {
            "current row".to_string()
        }
    } else {
        // Default start
        "unbounded preceding".to_string()
    };

    // Build end bound
    let end_bound = if frame_opts & frame_options::END_UNBOUNDED_FOLLOWING != 0 {
        "unbounded following".to_string()
    } else if frame_opts & frame_options::END_CURRENT_ROW != 0 {
        "current row".to_string()
    } else if frame_opts & frame_options::END_OFFSET_PRECEDING != 0 {
        if let Some(ref offset) = win_def.end_offset {
            format!("{} preceding", reconstruct_node(offset, ctx))
        } else {
            "current row".to_string()
        }
    } else if frame_opts & frame_options::END_OFFSET_FOLLOWING != 0 {
        if let Some(ref offset) = win_def.end_offset {
            format!("{} following", reconstruct_node(offset, ctx))
        } else {
            "current row".to_string()
        }
    } else {
        // Default end
        "current row".to_string()
    };

    // Build frame string
    if has_between {
        parts.push(format!("{} between {} and {}", mode, start_bound, end_bound));
    } else {
        // Short form (e.g., ROWS UNBOUNDED PRECEDING)
        parts.push(format!("{} {}", mode, start_bound));
    }

    // Handle EXCLUDE clause
    if frame_opts & frame_options::EXCLUDE_CURRENT_ROW != 0 {
        parts.push("exclude current row".to_string());
    } else if frame_opts & frame_options::EXCLUDE_GROUP != 0 {
        parts.push("exclude group".to_string());
    } else if frame_opts & frame_options::EXCLUDE_TIES != 0 {
        parts.push("exclude ties".to_string());
    }
    // EXCLUDE NO OTHERS is the default, so we don't emit it

    parts.join(" ")
}
