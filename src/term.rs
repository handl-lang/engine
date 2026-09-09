//! The arrow-calculus `Term` kernel (ADR-0104b).
//!
//! `Term` is the arrow-calculus evaluation kernel. A `Term` is an
//! engine-agnostic description of *what* to evaluate, not *how*. Engines
//! (tree-walk, bytecode, cranelift) interpret schemes; the scheme itself is
//! pure data. Surface constructs are lowered to a small set of core
//! combinators. [`Term::desugar`] recursively expands any remaining derived
//! forms into this core set before interpretation.
//!
//! `Value` here is [`serde_json::Value`] — no new value type is invented.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A side for a `Whisker` cell attachment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Side {
    /// The left side of a whisker.
    Left,
    /// The right side of a whisker.
    Right,
}

/// The arrow-calculus kernel: a small data-only combinator set.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Term {
    /// Pure constant arrow (Sanada's `arr`).
    Arr {
        /// The constant value.
        value: Value,
    },
    /// Pure constant value (ADR-0104b).
    Const {
        /// The constant value.
        value: Value,
    },
    /// Named graded operation (Sanada's `op`).
    Op {
        /// The operator name (e.g. `"add"`, `"proj_env"`).
        name: String,
        /// The argument sub-schemes feeding the op.
        args: Vec<Term>,
        /// Whether this is a native pure op or a foreign (host) call.
        source: ArrSource,
        /// An inline literal operand for ops that take a literal value
        /// rather than a sub-scheme (`proj_env`, `proj_child`).
        /// `None` for ordinary ops. Literal operands ride here so `args`
        /// stays a pure list of sub-schemes.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        lit: Option<Value>,
    },

    /// `>>>` — sequential composition.
    Seq {
        /// The first arrow; its output feeds `snd`.
        fst: Box<Term>,
        /// The second arrow; fed by `fst`'s output.
        snd: Box<Term>,
    },

    /// `first` — fan-out: run `inner` on the first component of a pair,
    /// carry the second component through unchanged.
    First {
        /// The arrow to run on the first component.
        inner: Box<Term>,
    },

    /// `ArrowChoice` — route on `Either`.
    Choice {
        /// The arrow handling the `Left` case.
        l: Box<Term>,
        /// The arrow handling the `Right` case.
        r: Box<Term>,
    },

    /// `ArrowLoop` — feedback.
    Loop {
        /// The feedback body.
        body: Box<Term>,
    },

    /// `app` / `ArrowApply` — dynamic/higher-order arrow application.
    App {
        /// The callee arrow.
        callee: Box<Term>,
        /// The argument arrows.
        args: Vec<Term>,
    },

    /// Bounded iteration.
    Iter {
        /// The remaining iteration budget.
        fuel: Box<Term>,
        /// The predicate arrow `state -> bool`.
        predicate: Box<Term>,
        /// The step arrow `state -> state`.
        step: Box<Term>,
        /// The initial state.
        initial: Box<Term>,
    },

    /// Child projection by named role (ADR-0081).
    Child {
        /// The child role to project from the current AST node.
        role: String,
    },

    /// `handle body with target = impl_` (ADR-0076).
    Handle {
        /// The handler body.
        body: Box<Term>,
        /// The capability name being managed.
        target: String,
        /// The handler implementation.
        impl_: Box<Term>,
    },

    /// `rewrite source ~> target` — a scheme rewrite step.
    Rewrite {
        /// The source arrow before rewriting.
        source: Box<Term>,
        /// The target arrow after rewriting.
        target: Box<Term>,
        /// Why the rewrite is sound.
        witness: RewriteWitness,
    },

    /// `path (a, b) [witness]` — a path between two terms (ADR-0100).
    Path {
        /// The endpoints of the path.
        endpoints: (Box<Term>, Box<Term>),
        /// An optional witness for the path.
        witness: Option<Box<Term>>,
    },

    /// `equiv a b [path]` — equality between two terms (ADR-0100).
    Equiv {
        /// The left-hand side.
        a: Box<Term>,
        /// The right-hand side.
        b: Box<Term>,
        /// An optional path witnessing the equivalence.
        path: Option<Box<Term>>,
    },

    /// Identity of a cell in a frontier context.
    CellId {
        /// The arrow identifying the cell.
        arrow: Box<Term>,
    },

    /// Source face of a 2-cell.
    Src {
        /// The 2-cell value.
        cell: Box<Term>,
    },

    /// Target face of a 2-cell.
    Tgt {
        /// The 2-cell value.
        cell: Box<Term>,
    },

    /// Vertical cell composition.
    CellCompV {
        /// The first arrow.
        first: Box<Term>,
        /// The second arrow.
        second: Box<Term>,
    },

    /// A whisker attaching an arrow to a cell on a side.
    Whisker {
        /// The side of attachment.
        side: Side,
        /// The arrow to attach.
        arrow: Box<Term>,
        /// The cell to attach to.
        cell: Box<Term>,
    },

    /// Record field projection.
    RecordProj {
        /// The record to project from.
        record: Box<Term>,
        /// The field name.
        field: String,
    },

    /// Record field update.
    RecordWith {
        /// The record to update.
        record: Box<Term>,
        /// The field name.
        field: String,
        /// The new value for the field.
        value: Box<Term>,
    },

    /// Tagged value wrapper.
    Tag {
        /// The tag name.
        tag: String,
        /// The inner value.
        value: Box<Term>,
    },

    /// Univalence from an equivalence.
    Univalence {
        /// The equivalence arrow.
        equiv: Box<Term>,
    },

    /// Glue a family along a path.
    Glue {
        /// The family.
        family: Box<Term>,
        /// The path.
        path: Box<Term>,
    },

    /// Fibrant type former.
    Fibrant {
        /// The base.
        base: Box<Term>,
        /// The family.
        family: Box<Term>,
    },

    /// J-elimination.
    JElim {
        /// The path.
        path: Box<Term>,
        /// The motive.
        motive: Box<Term>,
        /// The base.
        base: Box<Term>,
    },

    /// Refine a sort by a predicate.
    Refine {
        /// The sort.
        sort: Box<Term>,
        /// The predicate.
        predicate: Box<Term>,
    },

    /// Cobind over a grade and body.
    Cobind {
        /// The grade.
        grade: Box<Term>,
        /// The body.
        body: Box<Term>,
    },
}

/// Whether an `Op` is a native pure function or a foreign host call.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ArrSource {
    /// A native pure op.
    Native,
    /// A foreign (host) function call — capability-tracked.
    Foreign(String),
    /// A typecheck judgment op (ADR-0054).
    Judgment,
    /// A dynamic host dispatch op (ADR-0060).
    DynamicForeign,
}

/// A witness recording why a `Term::Rewrite` is sound.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RewriteWitness {
    /// A declared law or rewrite rule by name from `Language` data.
    Named(String),
    /// A computed rewrite step.
    Compute,
    /// A hint produced during handler elimination.
    HandlerHint,
    /// A type or capability cast.
    Cast,
}

impl Term {
    /// Convenience constructor for a pure constant arrow.
    pub fn const_value(value: impl Into<Value>) -> Self {
        Term::Arr {
            value: value.into(),
        }
    }

    /// Convenience constructor for a pure constant value.
    pub fn constant(value: impl Into<Value>) -> Self {
        Term::Const {
            value: value.into(),
        }
    }

    /// Convenience constructor for a native `op` with no literal payload.
    pub fn op_native(name: impl Into<String>, args: Vec<Term>) -> Self {
        Term::Op {
            name: name.into(),
            args,
            source: ArrSource::Native,
            lit: None,
        }
    }

    /// Backward-compatible alias for [`Term::op_native`].
    pub fn arr_native(op: impl Into<String>, args: Vec<Term>) -> Self {
        let op = op.into();
        if op == "const" && args.is_empty() {
            Term::const_value(Value::Null)
        } else {
            Term::op_native(op, args)
        }
    }

    /// Convenience constructor for a foreign `op`.
    pub fn op_foreign(name: impl Into<String>, args: Vec<Term>) -> Self {
        let name = name.into();
        Term::Op {
            name: name.clone(),
            args,
            source: ArrSource::Foreign(name),
            lit: None,
        }
    }

    /// Backward-compatible alias for [`Term::op_foreign`].
    pub fn arr_foreign(name: impl Into<String>, args: Vec<Term>) -> Self {
        Term::op_foreign(name, args)
    }

    /// Convenience constructor for a native `op` carrying a literal payload.
    pub fn op_native_lit(name: impl Into<String>, args: Vec<Term>, lit: impl Into<Value>) -> Self {
        Term::Op {
            name: name.into(),
            args,
            source: ArrSource::Native,
            lit: Some(lit.into()),
        }
    }

    /// Backward-compatible alias for [`Term::op_native_lit`].
    pub fn arr_native_lit(op: impl Into<String>, args: Vec<Term>, lit: impl Into<Value>) -> Self {
        let op = op.into();
        if op == "const" && args.is_empty() {
            Term::const_value(lit)
        } else {
            Term::op_native_lit(op, args, lit)
        }
    }

    /// Convenience constructor for a typecheck judgment `op` with no literal payload.
    pub fn op_judgment(name: impl Into<String>, args: Vec<Term>) -> Self {
        Term::Op {
            name: name.into(),
            args,
            source: ArrSource::Judgment,
            lit: None,
        }
    }

    /// Backward-compatible alias for [`Term::op_judgment`].
    pub fn arr_judgment(op: impl Into<String>, args: Vec<Term>) -> Self {
        Term::op_judgment(op, args)
    }

    /// Convenience constructor for a typecheck judgment `op` carrying a literal payload.
    pub fn op_judgment_lit(
        name: impl Into<String>,
        args: Vec<Term>,
        lit: impl Into<Value>,
    ) -> Self {
        Term::Op {
            name: name.into(),
            args,
            source: ArrSource::Judgment,
            lit: Some(lit.into()),
        }
    }

    /// Backward-compatible alias for [`Term::op_judgment_lit`].
    pub fn arr_judgment_lit(op: impl Into<String>, args: Vec<Term>, lit: impl Into<Value>) -> Self {
        Term::op_judgment_lit(op, args, lit)
    }

    /// Convenience constructor for `>>>` (sequential composition).
    pub fn seq(fst: Term, snd: Term) -> Self {
        Term::Seq {
            fst: Box::new(fst),
            snd: Box::new(snd),
        }
    }

    /// Convenience constructor for `first`.
    pub fn first(inner: Term) -> Self {
        Term::First {
            inner: Box::new(inner),
        }
    }

    /// Convenience constructor for `ArrowChoice` routing.
    pub fn choice(l: Term, r: Term) -> Self {
        Term::Choice {
            l: Box::new(l),
            r: Box::new(r),
        }
    }

    /// Convenience constructor for `ArrowLoop` feedback.
    pub fn loop_feedback(body: Term) -> Self {
        Term::Loop {
            body: Box::new(body),
        }
    }

    /// Convenience constructor for `app` / `ArrowApply`.
    pub fn app(callee: Term, args: Vec<Term>) -> Self {
        Term::App {
            callee: Box::new(callee),
            args,
        }
    }

    /// Construct a `handle body with target = impl_` node (ADR-0076).
    pub fn handle(body: Term, target: impl Into<String>, impl_: Term) -> Self {
        Term::Handle {
            body: Box::new(body),
            target: target.into(),
            impl_: Box::new(impl_),
        }
    }

    /// Recursively desugar derived surface forms into the core `Term` combinators.
    pub fn desugar(&self) -> Term {
        match self {
            // Core forms: preserve shape and desugar children.
            Term::Arr { value } => Term::Arr {
                value: value.clone(),
            },
            Term::Const { value } => Term::Const {
                value: value.clone(),
            },
            Term::Op {
                name,
                args,
                source,
                lit,
            } => Term::Op {
                name: name.clone(),
                args: args.iter().map(Term::desugar).collect(),
                source: source.clone(),
                lit: lit.clone(),
            },
            Term::Seq { fst, snd } => Term::Seq {
                fst: Box::new(fst.desugar()),
                snd: Box::new(snd.desugar()),
            },
            Term::First { inner } => Term::First {
                inner: Box::new(inner.desugar()),
            },
            Term::Choice { l, r } => Term::Choice {
                l: Box::new(l.desugar()),
                r: Box::new(r.desugar()),
            },
            Term::Loop { body } => Term::Loop {
                body: Box::new(body.desugar()),
            },
            Term::App { callee, args } => Term::App {
                callee: Box::new(callee.desugar()),
                args: args.iter().map(Term::desugar).collect(),
            },
            Term::Iter {
                fuel,
                predicate,
                step,
                initial,
            } => Term::Iter {
                fuel: Box::new(fuel.desugar()),
                predicate: Box::new(predicate.desugar()),
                step: Box::new(step.desugar()),
                initial: Box::new(initial.desugar()),
            },
            Term::Child { role } => Term::Child {
                role: role.clone(),
            },
            Term::Handle {
                body,
                target,
                impl_,
            } => Term::Handle {
                body: Box::new(body.desugar()),
                target: target.clone(),
                impl_: Box::new(impl_.desugar()),
            },
            Term::Rewrite {
                source,
                target,
                witness,
            } => Term::Rewrite {
                source: Box::new(source.desugar()),
                target: Box::new(target.desugar()),
                witness: witness.clone(),
            },
            Term::Path {
                endpoints: (a, b),
                witness,
            } => Term::Path {
                endpoints: (Box::new(a.desugar()), Box::new(b.desugar())),
                witness: witness.as_ref().map(|w| Box::new(w.desugar())),
            },
            Term::Equiv { a, b, path } => Term::Equiv {
                a: Box::new(a.desugar()),
                b: Box::new(b.desugar()),
                path: path.as_ref().map(|p| Box::new(p.desugar())),
            },
            Term::CellId { arrow } => Term::CellId {
                arrow: Box::new(arrow.desugar()),
            },
            Term::Src { cell } => Term::Src {
                cell: Box::new(cell.desugar()),
            },
            Term::Tgt { cell } => Term::Tgt {
                cell: Box::new(cell.desugar()),
            },
            Term::CellCompV { first, second } => Term::CellCompV {
                first: Box::new(first.desugar()),
                second: Box::new(second.desugar()),
            },
            Term::Whisker { side, arrow, cell } => Term::Whisker {
                side: side.clone(),
                arrow: Box::new(arrow.desugar()),
                cell: Box::new(cell.desugar()),
            },
            Term::RecordProj { record, field } => Term::RecordProj {
                record: Box::new(record.desugar()),
                field: field.clone(),
            },
            Term::RecordWith { record, field, value } => Term::RecordWith {
                record: Box::new(record.desugar()),
                field: field.clone(),
                value: Box::new(value.desugar()),
            },
            Term::Tag { tag, value } => Term::Tag {
                tag: tag.clone(),
                value: Box::new(value.desugar()),
            },
            Term::Univalence { equiv } => Term::Univalence {
                equiv: Box::new(equiv.desugar()),
            },
            Term::Glue { family, path } => Term::Glue {
                family: Box::new(family.desugar()),
                path: Box::new(path.desugar()),
            },
            Term::Fibrant { base, family } => Term::Fibrant {
                base: Box::new(base.desugar()),
                family: Box::new(family.desugar()),
            },
            Term::JElim { path, motive, base } => Term::JElim {
                path: Box::new(path.desugar()),
                motive: Box::new(motive.desugar()),
                base: Box::new(base.desugar()),
            },
            Term::Refine { sort, predicate } => Term::Refine {
                sort: Box::new(sort.desugar()),
                predicate: Box::new(predicate.desugar()),
            },
            Term::Cobind { grade, body } => Term::Cobind {
                grade: Box::new(grade.desugar()),
                body: Box::new(body.desugar()),
            },
        }
    }

    /// Eliminate every `Handle` node in this scheme.
    pub fn eliminate_handlers(&self) -> Term {
        match self {
            Term::Arr { value } => Term::Arr {
                value: value.clone(),
            },
            Term::Const { value } => Term::Const {
                value: value.clone(),
            },
            Term::Op {
                name,
                args,
                source,
                lit,
            } => {
                let new_args: Vec<Term> = args.iter().map(|a| a.eliminate_handlers()).collect();
                Term::Op {
                    name: name.clone(),
                    args: new_args,
                    source: source.clone(),
                    lit: lit.clone(),
                }
            }
            Term::Iter {
                fuel,
                predicate,
                step,
                initial,
            } => Term::Iter {
                fuel: Box::new(fuel.eliminate_handlers()),
                predicate: Box::new(predicate.eliminate_handlers()),
                step: Box::new(step.eliminate_handlers()),
                initial: Box::new(initial.eliminate_handlers()),
            },
            Term::Child { role } => Term::Child {
                role: role.clone(),
            },
            Term::Seq { fst, snd } => Term::Seq {
                fst: Box::new(fst.eliminate_handlers()),
                snd: Box::new(snd.eliminate_handlers()),
            },
            Term::First { inner } => Term::First {
                inner: Box::new(inner.eliminate_handlers()),
            },
            Term::Choice { l, r } => Term::Choice {
                l: Box::new(l.eliminate_handlers()),
                r: Box::new(r.eliminate_handlers()),
            },
            Term::Loop { body } => Term::Loop {
                body: Box::new(body.eliminate_handlers()),
            },
            Term::App { callee, args } => {
                let new_args: Vec<Term> = args.iter().map(|a| a.eliminate_handlers()).collect();
                Term::App {
                    callee: Box::new(callee.eliminate_handlers()),
                    args: new_args,
                }
            }
            Term::Handle {
                body,
                target,
                impl_,
            } => {
                let elim_body = body.eliminate_handlers();
                let elim_impl = impl_.eliminate_handlers();
                substitute_capability(&elim_body, target, &elim_impl)
            }
            Term::Rewrite {
                source,
                target,
                witness,
            } => Term::Rewrite {
                source: Box::new(source.eliminate_handlers()),
                target: Box::new(target.eliminate_handlers()),
                witness: witness.clone(),
            },
            Term::Path {
                endpoints: (a, b),
                witness,
            } => Term::Path {
                endpoints: (
                    Box::new(a.eliminate_handlers()),
                    Box::new(b.eliminate_handlers()),
                ),
                witness: witness.as_ref().map(|w| Box::new(w.eliminate_handlers())),
            },
            Term::Equiv { a, b, path } => Term::Equiv {
                a: Box::new(a.eliminate_handlers()),
                b: Box::new(b.eliminate_handlers()),
                path: path.as_ref().map(|p| Box::new(p.eliminate_handlers())),
            },
            Term::CellId { arrow } => Term::CellId {
                arrow: Box::new(arrow.eliminate_handlers()),
            },
            Term::Src { cell } => Term::Src {
                cell: Box::new(cell.eliminate_handlers()),
            },
            Term::Tgt { cell } => Term::Tgt {
                cell: Box::new(cell.eliminate_handlers()),
            },
            Term::CellCompV { first, second } => Term::CellCompV {
                first: Box::new(first.eliminate_handlers()),
                second: Box::new(second.eliminate_handlers()),
            },
            Term::Whisker { side, arrow, cell } => Term::Whisker {
                side: side.clone(),
                arrow: Box::new(arrow.eliminate_handlers()),
                cell: Box::new(cell.eliminate_handlers()),
            },
            Term::RecordProj { record, field } => Term::RecordProj {
                record: Box::new(record.eliminate_handlers()),
                field: field.clone(),
            },
            Term::RecordWith { record, field, value } => Term::RecordWith {
                record: Box::new(record.eliminate_handlers()),
                field: field.clone(),
                value: Box::new(value.eliminate_handlers()),
            },
            Term::Tag { tag, value } => Term::Tag {
                tag: tag.clone(),
                value: Box::new(value.eliminate_handlers()),
            },
            Term::Univalence { equiv } => Term::Univalence {
                equiv: Box::new(equiv.eliminate_handlers()),
            },
            Term::Glue { family, path } => Term::Glue {
                family: Box::new(family.eliminate_handlers()),
                path: Box::new(path.eliminate_handlers()),
            },
            Term::Fibrant { base, family } => Term::Fibrant {
                base: Box::new(base.eliminate_handlers()),
                family: Box::new(family.eliminate_handlers()),
            },
            Term::JElim { path, motive, base } => Term::JElim {
                path: Box::new(path.eliminate_handlers()),
                motive: Box::new(motive.eliminate_handlers()),
                base: Box::new(base.eliminate_handlers()),
            },
            Term::Refine { sort, predicate } => Term::Refine {
                sort: Box::new(sort.eliminate_handlers()),
                predicate: Box::new(predicate.eliminate_handlers()),
            },
            Term::Cobind { grade, body } => Term::Cobind {
                grade: Box::new(grade.eliminate_handlers()),
                body: Box::new(body.eliminate_handlers()),
            },
        }
    }
}

/// Substitute bare `arr(target)` references in `scheme` with `impl_`.
fn substitute_capability(scheme: &Term, target: &str, impl_: &Term) -> Term {
    match scheme {
        Term::Arr { value } => Term::Arr {
            value: value.clone(),
        },
        Term::Const { value } => Term::Const {
            value: value.clone(),
        },
        Term::Op {
            name,
            args,
            source,
            lit,
        } => {
            if name == target && args.is_empty() && lit.is_none() {
                match source {
                    ArrSource::Native => return impl_.clone(),
                    ArrSource::Foreign(n) if n == target => return impl_.clone(),
                    _ => {}
                }
            }
            let new_args: Vec<Term> = args
                .iter()
                .map(|a| substitute_capability(a, target, impl_))
                .collect();
            Term::Op {
                name: name.clone(),
                args: new_args,
                source: source.clone(),
                lit: lit.clone(),
            }
        }
        Term::Seq { fst, snd } => Term::Seq {
            fst: Box::new(substitute_capability(fst, target, impl_)),
            snd: Box::new(substitute_capability(snd, target, impl_)),
        },
        Term::First { inner } => Term::First {
            inner: Box::new(substitute_capability(inner, target, impl_)),
        },
        Term::Choice { l, r } => Term::Choice {
            l: Box::new(substitute_capability(l, target, impl_)),
            r: Box::new(substitute_capability(r, target, impl_)),
        },
        Term::Loop { body } => Term::Loop {
            body: Box::new(substitute_capability(body, target, impl_)),
        },
        Term::App { callee, args } => {
            let new_args: Vec<Term> = args
                .iter()
                .map(|a| substitute_capability(a, target, impl_))
                .collect();
            Term::App {
                callee: Box::new(substitute_capability(callee, target, impl_)),
                args: new_args,
            }
        }
        Term::Iter {
            fuel,
            predicate,
            step,
            initial,
        } => Term::Iter {
            fuel: Box::new(substitute_capability(fuel, target, impl_)),
            predicate: Box::new(substitute_capability(predicate, target, impl_)),
            step: Box::new(substitute_capability(step, target, impl_)),
            initial: Box::new(substitute_capability(initial, target, impl_)),
        },
        Term::Child { role } => Term::Child {
            role: role.clone(),
        },
        Term::Handle {
            body,
            target: t,
            impl_: i,
        } => Term::Handle {
            body: Box::new(substitute_capability(body, target, impl_)),
            target: t.clone(),
            impl_: Box::new(substitute_capability(i, target, impl_)),
        },
        Term::Rewrite {
            source,
            target: t,
            witness,
        } => Term::Rewrite {
            source: Box::new(substitute_capability(source, target, impl_)),
            target: Box::new(substitute_capability(t, target, impl_)),
            witness: witness.clone(),
        },
        Term::Path {
            endpoints: (a, b),
            witness,
        } => Term::Path {
            endpoints: (
                Box::new(substitute_capability(a, target, impl_)),
                Box::new(substitute_capability(b, target, impl_)),
            ),
            witness: witness
                .as_ref()
                .map(|w| Box::new(substitute_capability(w, target, impl_))),
        },
        Term::Equiv { a, b, path } => Term::Equiv {
            a: Box::new(substitute_capability(a, target, impl_)),
            b: Box::new(substitute_capability(b, target, impl_)),
            path: path
                .as_ref()
                .map(|p| Box::new(substitute_capability(p, target, impl_))),
        },
        Term::CellId { arrow } => Term::CellId {
            arrow: Box::new(substitute_capability(arrow, target, impl_)),
        },
        Term::Src { cell } => Term::Src {
            cell: Box::new(substitute_capability(cell, target, impl_)),
        },
        Term::Tgt { cell } => Term::Tgt {
            cell: Box::new(substitute_capability(cell, target, impl_)),
        },
        Term::CellCompV { first, second } => Term::CellCompV {
            first: Box::new(substitute_capability(first, target, impl_)),
            second: Box::new(substitute_capability(second, target, impl_)),
        },
        Term::Whisker { side, arrow, cell } => Term::Whisker {
            side: side.clone(),
            arrow: Box::new(substitute_capability(arrow, target, impl_)),
            cell: Box::new(substitute_capability(cell, target, impl_)),
        },
        Term::RecordProj { record, field } => Term::RecordProj {
            record: Box::new(substitute_capability(record, target, impl_)),
            field: field.clone(),
        },
        Term::RecordWith { record, field, value } => Term::RecordWith {
            record: Box::new(substitute_capability(record, target, impl_)),
            field: field.clone(),
            value: Box::new(substitute_capability(value, target, impl_)),
        },
        Term::Tag { tag, value } => Term::Tag {
            tag: tag.clone(),
            value: Box::new(substitute_capability(value, target, impl_)),
        },
        Term::Univalence { equiv } => Term::Univalence {
            equiv: Box::new(substitute_capability(equiv, target, impl_)),
        },
        Term::Glue { family, path } => Term::Glue {
            family: Box::new(substitute_capability(family, target, impl_)),
            path: Box::new(substitute_capability(path, target, impl_)),
        },
        Term::Fibrant { base, family } => Term::Fibrant {
            base: Box::new(substitute_capability(base, target, impl_)),
            family: Box::new(substitute_capability(family, target, impl_)),
        },
        Term::JElim { path, motive, base } => Term::JElim {
            path: Box::new(substitute_capability(path, target, impl_)),
            motive: Box::new(substitute_capability(motive, target, impl_)),
            base: Box::new(substitute_capability(base, target, impl_)),
        },
        Term::Refine { sort, predicate } => Term::Refine {
            sort: Box::new(substitute_capability(sort, target, impl_)),
            predicate: Box::new(substitute_capability(predicate, target, impl_)),
        },
        Term::Cobind { grade, body } => Term::Cobind {
            grade: Box::new(substitute_capability(grade, target, impl_)),
            body: Box::new(substitute_capability(body, target, impl_)),
        },
    }
}

/// Eliminate all handlers in a scheme (free function version).
pub fn eliminate_all_handlers(scheme: &Term) -> Term {
    scheme.eliminate_handlers()
}

/// Eliminate handlers in a scheme (alias for compatibility).
pub fn eliminate_handler_in_scheme(scheme: &Term) -> Term {
    scheme.eliminate_handlers()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn const_value_roundtrips() {
        let s = Term::const_value(Value::from(42));
        let j = serde_json::to_string(&s).unwrap();
        let back: Term = serde_json::from_str(&j).unwrap();
        assert_eq!(s, back);
    }

}
