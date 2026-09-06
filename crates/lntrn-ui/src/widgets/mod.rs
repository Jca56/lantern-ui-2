//! The widget set. Every widget is a method on [`crate::Ui`] that allocates
//! a rect, hit-tests it, mutates the caller's value and draws itself.

mod audio;
mod basic;
mod choice;
mod color;
mod combo;
mod curve;
mod dropdown;
mod keymap_editor;
mod knob;
mod pad;
mod path_bar;
mod range;
mod scroll;
mod slider;
mod table;
mod text_area;
mod text_field;
mod tree;

pub use curve::CurveResponse;
pub use path_bar::PathBarResponse;
pub use scroll::ScrollView;
pub use table::{Align, Cell, Column, RowStep, Table, TableResponse};
pub use text_field::{TextOpts, TextResponse, Validate};
pub use tree::TreeResponse;
