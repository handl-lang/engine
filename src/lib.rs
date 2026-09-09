//! # plugin-engine
//!
//! Foundation types for pluggable interpretation and rewrite models.

pub mod term;

pub use term::{
    eliminate_all_handlers, eliminate_handler_in_scheme, ArrSource, RewriteWitness, Term,
};

/// Placeholder `Ast` type for the `Model` skeleton.
#[derive(Debug, Clone)]
pub struct Ast;

/// Placeholder `AstNode` type for the `Model` skeleton.
#[derive(Debug, Clone)]
pub struct AstNode;

/// Placeholder `AstRef` type for the `Model` skeleton.
#[derive(Debug, Clone, Copy)]
pub struct AstRef;

/// Placeholder `Language` type for the `Model` skeleton.
#[derive(Debug, Clone)]
pub struct Language;

/// Placeholder language error type for the `Model` skeleton.
#[derive(Debug, Clone, PartialEq)]
pub enum LangError {
    /// Feature not yet implemented.
    NotImplemented,
}

/// The result of a model-level rewrite attempt.
pub enum RewriteResult {
    /// The source and target are equal.
    Equal,
    /// Replace the source with the target.
    Replace,
    /// The rewrite is logically proved.
    Proved,
    /// The rewrite is compiled to a runtime form.
    Compiled,
    /// The rewrite is not supported by this model.
    Unsupported,
}

/// Context passed to `Model` methods.
pub struct ModelCtx<'a> {
    /// The full AST being processed.
    pub ast: &'a Ast,
    /// The language definition in scope.
    pub lang: &'a Language,
    /// The current AST node.
    pub current_node: &'a AstNode,
    /// A reference to the current node.
    pub node_ref: AstRef,
}

/// A pluggable model for interpreting and rewriting `Term` schemes.
pub trait Model {
    /// The value type the model produces and consumes.
    type Value;

    /// Run the model on a `Term` with the given incoming value.
    fn run(
        &self,
        term: &Term,
        incoming: Self::Value,
        ctx: &ModelCtx,
    ) -> Result<Self::Value, LangError>;

    /// Attempt to rewrite `source` into `target` under the given witness.
    fn rewrite(
        &self,
        source: &Term,
        target: &Term,
        witness: &RewriteWitness,
        ctx: &ModelCtx,
    ) -> Result<RewriteResult, LangError>;
}
