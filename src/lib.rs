//! # plugin-engine
//!
//! Foundation types for pluggable interpretation and rewrite models.

pub mod term;

pub use term::{
    ArrSource, RewriteWitness, Term, eliminate_all_handlers, eliminate_handler_in_scheme,
};

/// A reference to a node in an AST arena.
///
/// The real AST type lives in `handl-language` (`handl_language::AstRef`, a
/// `usize` newtype); `plugin-engine` cannot name that type without creating
/// a dependency cycle (`handl-language` already depends on `plugin-engine`
/// for [`Term`]), so the handle is exposed here as the bare arena index.
pub type AstRef = usize;

/// Placeholder marker for an AST node in [`ModelCtx`].
///
/// The concrete node type is `handl_language::AstNode`, which
/// `plugin-engine` cannot name without a dependency cycle. `Model` impls
/// cannot inspect it yet — it only keeps the reference alive. Node access
/// goes through [`AstView`] instead.
#[derive(Debug, Clone)]
pub struct AstNode;

/// A minimal, object-safe view over a language definition.
///
/// Implemented by `handl_language::Language` (in `handl-language`, which
/// already depends on `plugin-engine`, so no cycle is introduced).
/// [`Model`] impls use this to resolve AST child positions by named role
/// (ADR-0081) and to look up eval schemes / type rules by node kind —
/// never hardcoded `kind ==` checks.
pub trait LanguageView {
    /// Upcast for implementations that need the concrete language type.
    fn as_any(&self) -> &dyn std::any::Any;

    /// Resolve the child position of `role` within nodes of kind `kind`
    /// (ADR-0081). Returns `None` when the kind has no shape or no such
    /// role (positional fallback is then the caller's concern).
    fn child_role_index(&self, kind: &str, role: &str) -> Option<usize>;

    /// The eval scheme (`Term`) registered for `kind`, if any.
    fn eval_rule(&self, kind: &str) -> Option<&Term>;

    /// The type-rule synth scheme (`Term`) registered for `kind`, if any.
    fn type_rule(&self, kind: &str) -> Option<&Term>;

    /// The evaluated `op` declaration value for `name`, if any.
    ///
    /// An `op` value is a tagged record
    /// `{"__tag__": "op", "name": ..., "inputs": ..., "output": ...,
    /// "grade": ...}`.
    fn op_decl(&self, name: &str) -> Option<serde_json::Value>;

    /// The first `handler` value whose `grades` list contains `grade`, if any.
    ///
    /// A handler value is a tagged record
    /// `{"__tag__": "Handler", "grades": [...], "impls": {...}}`.
    fn handler_for_grade(&self, grade: &str) -> Option<serde_json::Value>;
}

/// A minimal, object-safe view over an AST arena.
///
/// Implemented by `handl_language::Ast` (in `handl-language`). The real
/// node type cannot cross this boundary, so nodes are exposed through
/// accessors: [`node_kind`](AstView::node_kind) and
/// [`node_child`](AstView::node_child) (children ride in
/// `node.payload["children"]` as arena indices).
pub trait AstView {
    /// Upcast for implementations that need the concrete AST type.
    fn as_any(&self) -> &dyn std::any::Any;

    /// The kind string of the node at `r`, if `r` is in bounds.
    fn node_kind(&self, r: AstRef) -> Option<&str>;

    /// The arena ref of the child at position `index` of node `r`, if the
    /// node exists and has a child at that position.
    fn node_child(&self, r: AstRef, index: usize) -> Option<AstRef>;
}

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
    /// The full AST being processed, viewed through [`AstView`].
    pub ast: &'a dyn AstView,
    /// The language definition in scope, viewed through [`LanguageView`].
    pub lang: &'a dyn LanguageView,
    /// The current AST node (opaque marker — access it via `ast` instead).
    pub current_node: &'a AstNode,
    /// A reference to the current node.
    pub node_ref: AstRef,
    /// Runtime handler stack for `Term::Handle` / `Term::Op` dynamic
    /// dispatch. `Term::Handle` pushes its evaluated implementation here
    /// for the duration of its body; `Term::Op` dispatch searches the
    /// stack innermost-last for a `{"__tag__": "Handler", "impls": {...}}`
    /// value providing the op name.
    pub handler_stack: Vec<serde_json::Value>,
}

impl ModelCtx<'_> {
    /// Push a handler value onto the handler stack.
    pub fn push_handler(&mut self, value: serde_json::Value) {
        self.handler_stack.push(value);
    }

    /// Pop the innermost handler value from the handler stack, if any.
    pub fn pop_handler(&mut self) -> Option<serde_json::Value> {
        self.handler_stack.pop()
    }
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
        ctx: &mut ModelCtx,
    ) -> Result<Self::Value, LangError>;

    /// Attempt to rewrite `source` into `target` under the given witness,
    /// using the provided `incoming` value.
    fn rewrite(
        &self,
        source: &Term,
        target: &Term,
        witness: &RewriteWitness,
        incoming: &Self::Value,
        ctx: &mut ModelCtx,
    ) -> Result<RewriteResult, LangError>;
}
