//! The widget set. Every widget is a method on [`crate::Ui`] that allocates
//! a rect, hit-tests it, mutates the caller's value and draws itself.

mod basic;
mod color;
mod dropdown;
mod keymap_editor;
mod knob;
mod scroll;
mod slider;
mod table;
mod text_area;
mod text_field;
mod tree;

pub use scroll::ScrollView;
pub use table::{Align, Cell, Column, RowStep, Table, TableResponse};
pub use text_field::TextResponse;
pub use tree::TreeResponse;
