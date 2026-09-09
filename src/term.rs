//! The arrow-calculus `Term` kernel (ADR-0052).
//!
//! `Term` is the arrow-calculus evaluation kernel: the six arrow
//! primitives — `arr`, `>>>` (`Seq`), `first`, `Choice`, `Loop`, and `app`
//! — plus an `ArrSource` discipline on `arr` that keeps the capability
//! boundary structural. A `Term` is an engine-agnostic description of
//! *what* to evaluate for a given AST node kind, not *how*. Engines
//! (tree-walk, bytecode, cranelift) interpret schemes; the scheme itself is
//! pure data. Surface constructs are expressed as [`DesugarRule`]s, not new
//! variants.
//!
//! `Value` here is [`serde_json::Value`] — no new value type is invented.
//!
//! # The `lit` payload
//!
//! `Arr::args` is `Vec<Term>` — a literal value is not a sub-scheme. Literal
//! operands (the value of a `const`, the name of a `proj_env`, the index of a
//! `proj_child`) ride on `Arr`'s optional `lit: Option<Value>` payload, while
//! `args` stays a pure list of sub-schemes. The field serializes only when
//! present (`skip_serializing_if = "Option::is_none"`, `default`), so
//! foreign/prim arrows without a literal payload round-trip cleanly.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The arrow-calculus kernel: six primitives as data (ADR-0052).
///
/// Mirrors the ADR-0052 §"Kernel representation" enum. Arrows are
/// first-class computational descriptions; handlers provide
/// interpretations of those descriptions. Every variant *constructs* a
/// new arrow description; none *execute*.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Term {
    /// Pure constant arrow (Sanada's `arr`).
    Arr {
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
    ///
    /// Constructs a new arrow whose output of `fst` feeds `snd`. This
    /// *composes* arrows; it does **not execute** them. The composed
    /// arrow is a description; a handler interprets it.
    Seq {
        /// The first arrow; its output feeds `snd`.
        fst: Box<Term>,
        /// The second arrow; fed by `fst`'s output.
        snd: Box<Term>,
    },

    /// `first` — fan-out: run `inner` on the first component of a pair,
    /// carry the second component through unchanged.
    ///
    /// Constructs a new arrow; does not execute.
    First {
        /// The arrow to run on the first component.
        inner: Box<Term>,
    },

    /// `ArrowChoice` — route on `Either`: `l` handles `Left`, `r` handles
    /// `Right`.
    ///
    /// Constructs a routing arrow; does not execute.
    Choice {
        /// The arrow handling the `Left` case.
        l: Box<Term>,
        /// The arrow handling the `Right` case.
        r: Box<Term>,
    },

    /// `ArrowLoop` — feedback (NOT iteration).
    ///
    /// `body` has shape `(In, State) ~> (Out, State)`; `Loop` hides
    /// `State` in a feedback channel, exposing `In ~> Out`. Recursion,
    /// `while`, `for`, state machines are derived patterns over `Loop`
    /// + `Choice`. The feedback semantics (causal/strict vs
    /// lazy/fixpoint) are specified separately — `Loop` in the kernel is
    /// the *description* of a feedback channel; the interpretation
    /// strategy is the handler's concern.
    Loop {
        /// The feedback body: `(In, State) ~> (Out, State)`.
        body: Box<Term>,
    },

    /// `app` / `ArrowApply` — dynamic/higher-order arrow application.
    ///
    /// Given `f : A ~> B` and `x : A`, `app(f, x)` invokes `f` on `x`.
    /// Required for higher-order arrows (an arrow received as runtime
    /// data). Distinct from external interpretation: `app` is a calculus
    /// operation *within* an arrow; `handle H program` interprets an
    /// entire arrow from outside.
    App {
        /// The callee arrow.
        callee: Box<Term>,
        /// The argument arrows.
        args: Vec<Term>,
    },

    /// Bounded iteration (`iter` / `for` fuel loop).
    ///
    /// `fuel` evaluates to a non-negative integer; while it is positive,
    /// `predicate` is run on the state, and if it succeeds, `step` is
    /// applied to the state and `fuel` is decremented. `initial` is the
    /// starting state.
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

    /// `handle body with target = impl_` (ADR-0076): declares that `target`
    /// is a managed capability, with `body` as the handler body and `impl_`
    /// as the handler implementation. Eliminated at compile time by
    /// [`Term::eliminate_handlers`].
    Handle {
        /// The handler body — the program being handled. Bare `arr(target)`
        /// references within it are replaced by `impl_` during elimination.
        body: Box<Term>,
        /// The capability name being managed (the handle target).
        target: String,
        /// The handler implementation substituted for bare `arr(target)`
        /// references during elimination.
        impl_: Box<Term>,
    },

    /// `rewrite source ~> target` — a scheme rewrite step.
    ///
    /// Carries the source and target arrows plus a witness that records why
    /// the rewrite is sound (a named rule, a computed step, a handler hint,
    /// or a cast).
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

    /// `transport path value` — transport a value along a path (ADR-0100).
    Transport {
        /// The path along which to transport.
        path: Box<Term>,
        /// The value to transport.
        value: Box<Term>,
    },
}

/// Whether an `Op` is a native pure function or a foreign host call.
///
/// `Foreign(name)` keeps the capability boundary structural: the
/// typechecker and load-time capability audit match on `source`, not on
/// a string prefix.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ArrSource {
    /// A native pure op (`add`, `proj_env`, `id`, …).
    Native,
    /// A foreign (host) function call — capability-tracked. The `name` is
    /// the registered host-function name (e.g. `"fs.read"`).
    Foreign(String),
    /// A typecheck judgment op (ADR-0054 §1, §2). The op name is a
    /// judgment op (`synth_child`, `check_child`, `unify`, `extend`,
    /// `fresh_meta`, `solve_meta`, `lookup`, `quote`, `normalize`,
    /// `construct`, `match_type`). Judgment ops are interpreted by the
    /// typecheck engine, not the eval engines.
    Judgment,
    /// A dynamic host dispatch op (ADR-0060). The actual host function name
    /// is not known at scheme-construction time — it arrives as a runtime
    /// value (e.g. a string callee in a `call` node). The op name is the
    /// dispatch primitive (e.g. `call_host`); the actual host function name
    /// is a sub-scheme argument evaluated at runtime. Capability tracking
    /// for dynamic calls is handled by the engine at the call site, not by
    /// static `source` inspection.
    DynamicForeign,
}

/// A witness recording why a `Term::Rewrite` is sound.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RewriteWitness {
    /// A declared law or rewrite rule by name from `Language` data.
    Named(String),
    /// A computed rewrite step (e.g. normalization).
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
    ///
    /// The host function `name` is used both as the op and as the
    /// `Foreign(name)` source, matching ADR-0052's lowering of
    /// `Foreign { name, args }`.
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

    /// Convenience constructor for a native `op` carrying a literal
    /// payload (e.g. `proj_env`, `proj_child`).
    pub fn op_native_lit(name: impl Into<String>, args: Vec<Term>, lit: impl Into<Value>) -> Self {
        Term::Op {
            name: name.into(),
            args,
            source: ArrSource::Native,
            lit: Some(lit.into()),
        }
    }

    /// Backward-compatible alias for [`Term::op_native_lit`].
    ///
    /// For the `const` op it now produces [`Term::Arr`] via
    /// [`Term::const_value`], matching the ADR-0098/0100 calculus.
    pub fn arr_native_lit(op: impl Into<String>, args: Vec<Term>, lit: impl Into<Value>) -> Self {
        let op = op.into();
        if op == "const" && args.is_empty() {
            Term::const_value(lit)
        } else {
            Term::op_native_lit(op, args, lit)
        }
    }

    /// Convenience constructor for a typecheck judgment `op` (ADR-0054
    /// §1, §2) with no literal payload.
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

    /// Convenience constructor for a typecheck judgment `op` (ADR-0054
    /// §1, §2) carrying a literal payload.
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

    /// Convenience constructor for `first` (fan-out on the first
    /// component of a pair).
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
    ///
    /// Named `loop_feedback` rather than `loop` (a reserved word) to keep
    /// the constructor name clear and non-raw.
    pub fn loop_feedback(body: Term) -> Self {
        Term::Loop {
            body: Box::new(body),
        }
    }

    /// Convenience constructor for `app` / `ArrowApply` (dynamic
    /// higher-order arrow application).
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

    /// Eliminate every [`Term::Handle`] node in this scheme (ADR-0076).
    ///
    /// For each `handle body with target = impl_`:
    /// 1. Recursively eliminate handlers in `body` and `impl_`.
    /// 2. Substitute bare `arr(target)` references in the eliminated body
    ///    with the eliminated impl.
    /// 3. Return the substituted body.
    pub fn eliminate_handlers(&self) -> Term {
        match self {
            Term::Arr { value } => Term::Arr {
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
            Term::Child { role } => Term::Child { role: role.clone() },
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
            Term::Transport { path, value } => Term::Transport {
                path: Box::new(path.eliminate_handlers()),
                value: Box::new(value.eliminate_handlers()),
            },
        }
    }
}

/// Substitute bare `arr(target)` references in `scheme` with `impl_`.
///
/// A bare `arr(target)` reference is an `Op` node whose `name` equals `target`,
/// with no sub-scheme arguments and no literal payload. Such a reference may
/// carry either a [`ArrSource::Native`] source (the common case, as produced
/// by `op_native`) or a [`ArrSource::Foreign`] source naming `target`. Both
/// are recognized and replaced by `impl_`. All other nodes are recursed into
/// structurally.
fn substitute_capability(scheme: &Term, target: &str, impl_: &Term) -> Term {
    match scheme {
        Term::Arr { value } => Term::Arr {
            value: value.clone(),
        },
        Term::Op {
            name,
            args,
            source,
            lit,
        } => {
            // Check if this is a bare `arr(target)` reference:
            // name == target, no args, no lit. The source may be Native (the
            // common case from `op_native`) or Foreign(target).
            if name == target && args.is_empty() && lit.is_none() {
                match source {
                    ArrSource::Native => return impl_.clone(),
                    ArrSource::Foreign(n) if n == target => return impl_.clone(),
                    _ => {}
                }
            }
            // Otherwise, recurse into args.
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
        Term::Child { role } => Term::Child { role: role.clone() },
        Term::Handle {
            body,
            target: t,
            impl_: i,
        } => {
            // Nested handle — should not occur after elimination, but handle
            // it by recursing into both children.
            Term::Handle {
                body: Box::new(substitute_capability(body, target, impl_)),
                target: t.clone(),
                impl_: Box::new(substitute_capability(i, target, impl_)),
            }
        }
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
        Term::Transport { path, value } => Term::Transport {
            path: Box::new(substitute_capability(path, target, impl_)),
            value: Box::new(substitute_capability(value, target, impl_)),
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

    fn roundtrip(s: &Term) -> Term {
        let j = serde_json::to_string(s).unwrap();
        serde_json::from_str(&j).unwrap()
    }

    #[test]
    fn op_native_constructs_and_roundtrips() {
        let s = Term::op_native("add", vec![Term::op_native_lit("proj_child", vec![], 0)]);
        assert_eq!(s, roundtrip(&s));
        let j = serde_json::to_string(&s).unwrap();
        assert!(j.contains("\"Op\""));
        assert!(j.contains("\"Native\""));
        // The outer op has no lit; the inner proj_child does.
        match s {
            Term::Op { lit, .. } => assert_eq!(lit, None),
            other => panic!("expected Op, got {other:?}"),
        }
    }

    #[test]
    fn op_foreign_constructs_and_roundtrips() {
        let s = Term::op_foreign(
            "fs.read",
            vec![Term::op_native_lit("proj_child", vec![], 0)],
        );
        assert_eq!(s, roundtrip(&s));
        let j = serde_json::to_string(&s).unwrap();
        assert!(j.contains("\"Foreign\""));
        assert!(j.contains("\"fs.read\""));
    }

    #[test]
    fn const_value_roundtrips() {
        let s = Term::const_value(Value::from(42));
        assert_eq!(s, roundtrip(&s));
        let j = serde_json::to_string(&s).unwrap();
        assert!(j.contains("\"Arr\""));
        assert!(j.contains("42"));
    }

    #[test]
    fn op_lit_absent_omitted() {
        let s = Term::op_native("id", vec![]);
        let j = serde_json::to_string(&s).unwrap();
        assert!(j.contains("\"Op\""));
        assert!(!j.contains("\"lit\""));
        assert_eq!(s, roundtrip(&s));
    }

    #[test]
    fn seq_constructs_and_roundtrips() {
        let s = Term::seq(
            Term::arr_native("id", vec![]),
            Term::arr_native("id", vec![]),
        );
        assert_eq!(s, roundtrip(&s));
        let j = serde_json::to_string(&s).unwrap();
        assert!(j.contains("\"Seq\""));
    }

    #[test]
    fn first_constructs_and_roundtrips() {
        let s = Term::first(Term::arr_native("id", vec![]));
        assert_eq!(s, roundtrip(&s));
        let j = serde_json::to_string(&s).unwrap();
        assert!(j.contains("\"First\""));
    }

    #[test]
    fn choice_constructs_and_roundtrips() {
        let s = Term::choice(
            Term::arr_native("id", vec![]),
            Term::arr_native("id", vec![]),
        );
        assert_eq!(s, roundtrip(&s));
        let j = serde_json::to_string(&s).unwrap();
        assert!(j.contains("\"Choice\""));
        assert!(j.contains("\"l\""));
        assert!(j.contains("\"r\""));
    }

    #[test]
    fn loop_feedback_constructs_and_roundtrips() {
        let s = Term::loop_feedback(Term::arr_native("id", vec![]));
        assert_eq!(s, roundtrip(&s));
        let j = serde_json::to_string(&s).unwrap();
        assert!(j.contains("\"Loop\""));
    }

    #[test]
    fn app_constructs_and_roundtrips() {
        let s = Term::app(
            Term::arr_native_lit("proj_env", vec![], "f"),
            vec![Term::arr_native_lit("const", vec![], 1)],
        );
        assert_eq!(s, roundtrip(&s));
        let j = serde_json::to_string(&s).unwrap();
        assert!(j.contains("\"App\""));
    }

    #[test]
    fn arr_source_native_roundtrips() {
        let src = ArrSource::Native;
        let j = serde_json::to_string(&src).unwrap();
        assert_eq!(j, "\"Native\"");
        let back: ArrSource = serde_json::from_str(&j).unwrap();
        assert_eq!(src, back);
    }

    #[test]
    fn arr_source_foreign_roundtrips() {
        let src = ArrSource::Foreign("fs.read".into());
        let j = serde_json::to_string(&src).unwrap();
        assert!(j.contains("\"Foreign\""));
        let back: ArrSource = serde_json::from_str(&j).unwrap();
        assert_eq!(src, back);
    }

    #[test]
    fn nested_scheme_roundtrips() {
        let s = Term::seq(
            Term::arr_native("id", vec![]),
            Term::choice(
                Term::loop_feedback(Term::arr_foreign("fs.read", vec![])),
                Term::app(
                    Term::arr_native_lit("proj_env", vec![], "f"),
                    vec![Term::arr_native_lit("const", vec![], Value::Bool(true))],
                ),
            ),
        );
        assert_eq!(s, roundtrip(&s));
    }
}
