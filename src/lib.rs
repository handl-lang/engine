//! # hnl-engine
//!
//! Foundation types for pluggable interpretation and rewrite models.
//!
//! The arrow-calculus IR ([`Term`], [`TermData`]) and the pluggable model
//! seam ([`LanguageView`], [`AstView`], [`Model`], [`ModelData`],
//! [`CompositeModel`], [`CompositeDataModel`], …) now live in the
//! `handl-calculus` crate in the kernel repo — `handl-language` needs them
//! and cannot depend on `hnl-engine` without a cycle. They are re-exported
//! here under their original paths (`hnl_engine::Term`,
//! `hnl_engine::term::Term`, `hnl_engine::Model`, …) so downstream crates
//! keep compiling unchanged. This crate retains only the interpreters:
//! [`term_interpreter`] (tree-walk) and [`nbe`] (normalization by
//! evaluation).

pub mod nbe;
pub mod term_interpreter;

// Re-exported module paths (were `pub mod term;` / `pub mod term_data;`).
pub use handl_calculus::term;
pub use handl_calculus::term_data;

// Re-exported IR + model surface (were defined in this crate's lib.rs).
pub use handl_calculus::{
    ArrSource, AstNode, AstRef, AstView, CompositeDataModel, CompositeModel, LangError,
    LanguageView, Model, ModelCtx, ModelData, ModelDataAdapter, RewriteResult, RewriteWitness,
    Side, Term, TermData, eliminate_all_handlers, eliminate_handler_in_scheme, is_hott_tag,
};
pub use term_interpreter::interpret;
