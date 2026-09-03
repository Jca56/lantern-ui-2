//! The widget set. Every widget is a method on [`crate::Ui`] that allocates
//! a rect, hit-tests it, mutates the caller's value and draws itself.

mod basic;
mod color;
mod dropdown;
mod knob;
mod scroll;
mod slider;
mod text_field;
mod tree;

pub use text_field::TextResponse;
pub use tree::TreeResponse;
