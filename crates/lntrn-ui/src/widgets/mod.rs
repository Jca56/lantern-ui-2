//! The widget set. Every widget is a method on [`crate::Ui`] that allocates
//! a rect, hit-tests it, mutates the caller's value and draws itself.

mod basic;
mod choice;
mod color;
mod combo;
mod dropdown;
mod keymap_editor;
mod knob;
mod pad;
mod range;
mod scroll;
mod slider;
mod table;
mod text_area;
mod text_field;
mod tree;

pub use scroll::ScrollView;
pub use table::{Align, Cell, Column, RowStep, Table, TableResponse};
pub use text_field::{TextOpts, TextResponse};
pub use tree::TreeResponse;
