use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EditorState {
    pub cursor_position: (usize, usize),
    pub scroll_position: (usize, usize),
    pub selections: Vec<Selection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Selection {
    pub start: (usize, usize),
    pub end: (usize, usize),
}
