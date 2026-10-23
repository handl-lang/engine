//! NbE evaluation of `Term` → `handl_value::nbe::Val` (phase-1 spec
//! WS-A). The arrow calculus evaluates into the shared semantic
//! domain: constants become `Val::Const`, `Seq`/`Choice`/`First`/
//! `App` thread the *incoming* value through the combinators (the
//! same arrow semantics as [`crate::term_interpreter`]), and `Op`
//! nodes dispatch to a caller-supplied [`OpHandler`] — judgment ops
//! belong to the checker, not the kernel (engine-genericity: the
//! kernel knows only the calculus vocabulary).
//!
//! Scope: the fragment the typechecker evaluates (`Arr`/`Const`/`Op`/
//! `Seq`/`First`/`Choice`/`App`/`RecordProj`/`RecordWith`). `Handle`/
//! `Rewrite`/`Tag`/`Cobind`/`Loop`/`Iter`/`Child`/`Opaque` and the
//! 2-cell faces return `NotImplemented` — they are runtime/effect or
//! derived-data positions that never appear inside type-level
//! computation (ADR-0054 pure-eval boundary); globular `Seq` axes
//! `k > 0` delegate to the handler as a `cell_comp_axis` op.

use std::rc::Rc;

use handl_value::nbe::{ArrowVal, Val};
use handl_value::TaggedValue;
use serde_json::Value;

use crate::term::Term;

/// Errors from NbE evaluation.
#[derive(Debug, Clone, PartialEq)]
pub enum EvalError {
    /// The term's vocabulary is outside the NbE fragment.
    NotImplemented(&'static str),
    /// The `OpHandler` rejected the op.
    Op(String),
    /// Applied a non-arrow value.
    NotAnArrow,
    /// `First`/`Choice`/record ops saw a malformed operand.
    BadOperand(&'static str),
    /// A `Seq` axis expression evaluated to a non-natural value.
    BadAxis,
}

impl std::fmt::Display for EvalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotImplemented(k) => write!(f, "nbe: `{k}` outside the eval fragment"),
            Self::Op(m) => write!(f, "nbe op error: {m}"),
            Self::NotAnArrow => write!(f, "nbe: application of non-arrow"),
            Self::BadOperand(k) => write!(f, "nbe: malformed operand for `{k}`"),
            Self::BadAxis => write!(f, "nbe: `Seq` axis is not a natural number"),
        }
    }
}

impl std::error::Error for EvalError {}

/// The op-dispatch seam: the checker installs the judgment ops
/// (`synth_child`, `unify`, `extend`, `construct`, …); the kernel
/// only forwards `(name, args, lit, dim, input)` — never knowing what
/// an op means. `input` is the `Seq`-threaded incoming value (the
/// arrow's operand), mirroring `term_interpreter::eval_op`.
pub trait OpHandler {
    /// Evaluate `name` with evaluated `args` and threaded `input`.
    /// `dim` is the declared dimension expression — handlers that
    /// ignore dimensions read `dim.nat_literal()`/`dim_label()`
    /// themselves or ignore it.
    fn op(
        &self,
        name: &str,
        args: &[Val],
        lit: Option<&Value>,
        dim: &Term,
        input: Val,
    ) -> Result<Val, EvalError>;

    /// Interpret an arrow *value*'s body on application — the engine
    /// hook. `body_def` is opaque to the kernel (a tagged `__ast__`
    /// value); only an engine can run it. Returning `Ok(None)` (the
    /// default) yields the residual closure: `binds` extended with
    /// the argument and the bound param popped — lazy application,
    /// which is what conversion over open arrows wants.
    fn apply_arrow(&self, arrow: &ArrowVal, arg: Val) -> Option<Result<Val, EvalError>> {
        let _ = (arrow, arg);
        None
    }
}

/// Evaluate `term` as an arrow from `input` to a `Val`.
///
/// `Term` has no variable binders — arrow-calculus binding happens
/// through application and `binds` maps, so there is no environment
/// here; rigid/flex heads are minted by the *checker* around calls
/// into `eval`, not by it.
pub fn eval(term: &Term, input: Val, ops: &dyn OpHandler) -> Result<Val, EvalError> {
    match term {
        // `arr f` / `const` — the constant arrow: ignores `input`.
        Term::Arr { value, .. } | Term::Const { value } => {
            Ok(Val::Const(TaggedValue::from(value.clone())))
        }

        Term::Op {
            dim,
            name,
            args,
            lit,
            ..
        } => {
            let mut vals = Vec::with_capacity(args.len());
            for a in args {
                vals.push(eval(a, input.clone(), ops)?);
            }
            ops.op(name, &vals, lit.as_ref(), dim, input)
        }

        Term::Seq { axis, fst, snd } => {
            // `>>>[k]`: literal axis 0 on `dim ≤ 1` operands is
            // sequential composition (`snd` sees `fst`'s output);
            // anything else is globular pasting, delegated to the
            // handler as `cell_comp_axis` (WS-D installs it).
            let k = match axis.nat_literal() {
                Some(k) => k,
                None => match eval(axis, input.clone(), ops)? {
                    Val::Const(TaggedValue::Int(n)) if n >= 0 => n as u64,
                    _ => return Err(EvalError::BadAxis),
                },
            };
            let d = fst.globular_dim();
            if k == 0 && d <= 1 {
                let mid = eval(fst, input, ops)?;
                let out = eval(snd, mid.clone(), ops)?;
                // `f >>> g` on *cell operands* is composition, not
                // threading (0106 amendment): a `snd` that is itself an
                // operand term — `Arr`/`Const` hold a value and never
                // read the incoming — means both sides evaluated to
                // cells, so the `>>>` forms the ∘₀ composite through
                // the same `cell_comp_axis` seam the globular route
                // delegates to (`Hom(1,a,b) ∘ Hom(1,b,c)` per the
                // checker's rule; `Const` payloads take the kernel's
                // `Arrow{first, second}` mint, matching the exec
                // engines' `flow` arm). An input-consuming `snd` (an
                // op/scheme) keeps the pipeline reading — `mid` is
                // threaded data, not a composable cell.
                if matches!(snd.as_ref(), Term::Arr { .. } | Term::Const { .. })
                    && is_cell_val(&mid)
                    && is_cell_val(&out)
                {
                    return compose_cell_vals(0, d, axis, &mid, &out, ops);
                }
                Ok(out)
            } else {
                let a = eval(fst, input.clone(), ops)?;
                let b = eval(snd, input, ops)?;
                let lit = serde_json::json!({"axis": k, "dim": d});
                ops.op(
                    "cell_comp_axis",
                    &[a, b],
                    Some(&lit),
                    axis,
                    Val::Const(TaggedValue::Null),
                )
            }
        }

        Term::First { inner } => {
            // `first inner`: run `inner` on the pair's first component,
            // carry the second through (arrow `***`/`first` law).
            match input {
                Val::Const(TaggedValue::Pair(p)) => {
                    let v = eval(inner, Val::Const(p.0.clone()), ops)?;
                    Ok(Val::Const(TaggedValue::Pair(Rc::new((v_as_tagged(v), p.1.clone())))))
                }
                _ => Err(EvalError::BadOperand("first")),
            }
        }

        Term::Choice { l, r } => match input {
            Val::Const(TaggedValue::EitherLeft(v)) => eval(l, Val::Const((*v).clone()), ops),
            Val::Const(TaggedValue::EitherRight(v)) => eval(r, Val::Const((*v).clone()), ops),
            _ => Err(EvalError::BadOperand("choice")),
        },

        Term::App { callee, args } => {
            let mut vals = Vec::with_capacity(args.len());
            for a in args {
                vals.push(eval(a, input.clone(), ops)?);
            }
            let arg = if vals.len() == 1 {
                vals.into_iter().next().unwrap()
            } else {
                Val::Const(TaggedValue::Arr(
                    vals.into_iter().map(v_as_tagged).collect::<Vec<_>>().into(),
                ))
            };
            let f = eval(callee, input, ops)?;
            apply(&f, arg, ops)
        }

        Term::RecordProj { record, field } => {
            let r = eval(record, input, ops)?;
            match r {
                Val::Const(TaggedValue::Obj(m)) => m
                    .get(field.as_str())
                    .cloned()
                    .map(Val::Const)
                    .ok_or(EvalError::BadOperand("record_proj")),
                _ => Err(EvalError::BadOperand("record_proj")),
            }
        }

        Term::RecordWith {
            record,
            field,
            value,
        } => {
            let r = eval(record, input.clone(), ops)?;
            let v = eval(value, input, ops)?;
            match r {
                Val::Const(TaggedValue::Obj(m)) => {
                    let mut m = (*m).clone();
                    m.insert(Rc::from(field.as_str()), v_as_tagged(v));
                    Ok(Val::Const(TaggedValue::Obj(Rc::new(m))))
                }
                _ => Err(EvalError::BadOperand("record_with")),
            }
        }

        Term::Loop { .. } => Err(EvalError::NotImplemented("loop")),
        Term::Iter { .. } => Err(EvalError::NotImplemented("iter")),
        Term::Child { .. } => Err(EvalError::NotImplemented("child")),
        Term::Handle { .. } => Err(EvalError::NotImplemented("handle")),
        Term::Rewrite { .. } => Err(EvalError::NotImplemented("rewrite")),
        Term::Src { .. } => Err(EvalError::NotImplemented("src")),
        Term::Tgt { .. } => Err(EvalError::NotImplemented("tgt")),
        Term::Tag { .. } => Err(EvalError::NotImplemented("tag")),
        Term::Cobind { .. } => Err(EvalError::NotImplemented("cobind")),
        Term::Opaque { .. } => Err(EvalError::NotImplemented("opaque")),
    }
}

/// Whether `v` denotes a *cell* — a morphism value rather than plain
/// data (ADR-0106: `>>>` on cells composes, it does not apply). A
/// domain arrow (`Val::Arrow`), a cell former application
/// (`cell.comp`/`cell.id` — `>>>`/`id_k` composite data), or a
/// boundary `TaggedValue` carrying an arrow/cell record:
/// `__arrow__` closures, `{"__tag__": "Arrow", first|inputs}`
/// composites and `dim_arrow`s, `Cell`/`Cell2`/`__dim__`/`__src__`
/// n-cells. Neutrals (`Rigid`/`Flex`/`Unfold`) stay non-cells — their
/// cell-ness lives in their checker-side type, so `>>>` on an open
/// operand keeps the declared-dimension route (the `d > 1` paste).
pub fn is_cell_val(v: &Val) -> bool {
    match v {
        Val::Arrow(_) => true,
        Val::Former { head, .. } => {
            let h = head.handle();
            h == handl_value::Sym::seeded("cell.comp").handle()
                || h == handl_value::Sym::seeded("cell.id").handle()
        }
        Val::Const(tv) => is_cell_tagged(tv),
        _ => false,
    }
}

/// The boundary-value half of [`is_cell_val`] — mirrors the exec
/// engines' `is_arrow_like` plus the `Cell`/`__dim__` n-cell shapes
/// `handl_value::cell_dim` recognizes.
fn is_cell_tagged(tv: &TaggedValue) -> bool {
    match tv {
        TaggedValue::Arrow { .. } => true,
        TaggedValue::Obj(m) => {
            let tag = m.get("__tag__").and_then(|t| t.as_str());
            (tag == Some("Arrow")
                && (m.get("first").is_some() || m.get("inputs").is_some()))
                || tag == Some("Cell")
                || tag == Some("Cell2")
                || m.get("__dim__").is_some()
                || m.get("__src__").is_some()
        }
        _ => false,
    }
}

/// Axis-0 composition of two evaluated cell operands (the `Seq{0}`
/// cell route). `Const` payloads compose in the kernel's value model —
/// `handl_value::cell_comp_axis(0, ·)` mints the `Arrow{first, second}`
/// composite (and pastes `Cell`/`__dim__` records at their own
/// height), the same data the exec `flow` arm produces. Domain cells
/// delegate to the `cell_comp_axis` op — the owning checker's
/// `infer_comp_axis` decides the `Hom(1,a,b) ∘ Hom(1,b,c)` boundary
/// law and mints `Former{cell.comp}` (or reports honestly).
pub fn compose_cell_vals(
    axis: u64,
    declared_dim: u64,
    axis_term: &Term,
    a: &Val,
    b: &Val,
    ops: &dyn OpHandler,
) -> Result<Val, EvalError> {
    if let (Val::Const(ta), Val::Const(tb)) = (a, b) {
        return handl_value::cell_comp_axis(axis, declared_dim.max(1), ta, tb)
            .map(Val::Const)
            .map_err(EvalError::Op);
    }
    let lit = serde_json::json!({"axis": axis, "dim": declared_dim});
    ops.op(
        "cell_comp_axis",
        &[a.clone(), b.clone()],
        Some(&lit),
        axis_term,
        Val::Const(TaggedValue::Null),
    )
}

/// Reify a `Val` to the `TaggedValue` boundary encoding — the local
/// inverse of `Val::Const` wrapping, used where a `TaggedValue`
/// payload (pair components, record fields, array elements) is
/// required. Domain-internal variants reify through
/// `handl_value::nbe::quote` against an empty store: a caller
/// embedding a neutral inside boundary data gets its stable tagged
/// encoding, not a silent drop.
fn v_as_tagged(v: Val) -> TaggedValue {
    match v {
        Val::Const(t) => t,
        other => handl_value::nbe::quote(
            &handl_value::nbe::MetaStore::default(),
            0,
            &other,
        ),
    }
}

/// Apply an arrow value — NbE application.
///
/// - `Val::Arrow`: the handler's [`OpHandler::apply_arrow`] hook may
///   interpret the body (the engine owns `body_def` evaluation); the
///   default is the residual closure — extend `binds` with the
///   argument, pop the bound param (currying by binds-extension,
///   smalltt-style lazy application).
/// - `Val::Const(TaggedValue::Arrow)`: a boundary arrow lifts into
///   the domain (`binds` re-enter as `Const` values) and applies by
///   the same rule.
/// - Neutrals (`Rigid`/`Flex`/`Unfold`) extend the spine; `Former`
///   extends its index args; `Nominal`/`Universe`/plain `Const`
///   reject.
pub fn apply(f: &Val, arg: Val, ops: &dyn OpHandler) -> Result<Val, EvalError> {
    match f {
        Val::Rigid { lvl, spine } => {
            let mut s = spine.clone();
            s.push(arg);
            Ok(Val::Rigid {
                lvl: *lvl,
                spine: s,
            })
        }
        Val::Flex { meta, spine } => {
            let mut s = spine.clone();
            s.push(arg);
            Ok(Val::Flex {
                meta: *meta,
                spine: s,
            })
        }
        Val::Unfold { name, spine, value } => {
            let mut s = spine.clone();
            s.push(arg);
            Ok(Val::Unfold {
                name: name.clone(),
                spine: s,
                value: value.clone(),
            })
        }
        Val::Former { head, args } => {
            let mut args = args.clone();
            args.push(arg);
            Ok(Val::Former {
                head: head.clone(),
                args,
            })
        }
        Val::Arrow(a) => apply_arrow_val(a, arg, ops),
        Val::Const(TaggedValue::Arrow {
            params,
            rest,
            array_pattern,
            body_def,
            binds,
            handlers,
        }) => {
            let _ = (rest, array_pattern, handlers);
            let binds = binds
                .iter()
                .map(|(k, v)| (k.clone(), Val::Const(v.clone())))
                .collect();
            let a = ArrowVal {
                params: params.clone(),
                body_def: (**body_def).clone(),
                binds: Rc::new(binds),
            };
            apply_arrow_val(&a, arg, ops)
        }
        _ => Err(EvalError::NotAnArrow),
    }
}

fn apply_arrow_val(a: &ArrowVal, arg: Val, ops: &dyn OpHandler) -> Result<Val, EvalError> {
    if let Some(res) = ops.apply_arrow(a, arg.clone()) {
        return res;
    }
    // Residual: bind the first param to the argument; remaining
    // params stay open.
    let mut binds = (*a.binds).clone();
    if let Some(p) = a.params.first() {
        binds.insert(Rc::from(p.as_str()), arg);
    }
    let params: Rc<[String]> = if a.params.is_empty() {
        a.params.clone()
    } else {
        a.params[1..].to_vec().into()
    };
    Ok(Val::Arrow(Rc::new(ArrowVal {
        params,
        body_def: a.body_def.clone(),
        binds: Rc::new(binds),
    })))
}

/// A trivial op handler for tests: `id`/`const`/`pair` only.
pub struct NativeOnly;
impl OpHandler for NativeOnly {
    fn op(
        &self,
        name: &str,
        args: &[Val],
        _lit: Option<&Value>,
        _dim: &Term,
        input: Val,
    ) -> Result<Val, EvalError> {
        match name {
            "id" => Ok(input),
            "const" => Ok(args.first().cloned().unwrap_or(Val::Const(TaggedValue::Null))),
            "pair" => Ok(Val::Const(TaggedValue::Arr(
                args.into_iter().map(|v| v_as_tagged(v.clone())).collect::<Vec<_>>().into(),
            ))),
            other => Err(EvalError::Op(format!("unknown native op `{other}`"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::term::ArrSource;
    use handl_value::nbe::Spine;

    fn const_t(v: impl Into<Value>) -> Term {
        Term::Const { value: v.into() }
    }

    #[test]
    fn eval_const_ignores_input() {
        let t = const_t(serde_json::json!(42));
        let v = eval(&t, Val::Const(TaggedValue::Null), &NativeOnly).unwrap();
        assert!(matches!(v, Val::Const(TaggedValue::Int(42))));
    }

    #[test]
    fn seq_threads_fst_into_snd() {
        // `const 7 >>> id` — snd sees fst's output as `input`.
        let t = Term::seq(
            Term::op_native("const", vec![const_t(serde_json::json!(7))]),
            Term::op_native("id", vec![]),
        );
        let v = eval(&t, Val::Const(TaggedValue::Null), &NativeOnly).unwrap();
        assert!(matches!(v, Val::Const(TaggedValue::Int(7))));
    }

    #[test]
    fn first_runs_inner_on_pair_left() {
        let t = Term::first(Term::op_native("id", vec![]));
        let input = Val::Const(TaggedValue::Pair(Rc::new((
            TaggedValue::Int(1),
            TaggedValue::Int(2),
        ))));
        let v = eval(&t, input, &NativeOnly).unwrap();
        match v {
            Val::Const(TaggedValue::Pair(p)) => {
                assert!(matches!(p.0, TaggedValue::Int(1)));
                assert!(matches!(p.1, TaggedValue::Int(2)));
            }
            other => panic!("expected pair, got {other:?}"),
        }
    }

    #[test]
    fn choice_routes_either() {
        let t = Term::choice(
            Term::op_native("const", vec![const_t(serde_json::json!("L"))]),
            Term::op_native("const", vec![const_t(serde_json::json!("R"))]),
        );
        let left = Val::Const(TaggedValue::EitherLeft(Rc::new(TaggedValue::Int(0))));
        let v = eval(&t, left, &NativeOnly).unwrap();
        assert!(matches!(v, Val::Const(TaggedValue::Str(ref s)) if &**s == "L"));
        let right = Val::Const(TaggedValue::EitherRight(Rc::new(TaggedValue::Int(0))));
        let v = eval(&t, right, &NativeOnly).unwrap();
        assert!(matches!(v, Val::Const(TaggedValue::Str(ref s)) if &**s == "R"));
    }

    #[test]
    fn app_applies_arrow_residual() {
        // `app` on a boundary arrow value: binds p := arg, params pop.
        let arrow = TaggedValue::Arrow {
            params: vec!["x".to_string(), "y".to_string()].into(),
            rest: None,
            array_pattern: false,
            body_def: Rc::new(TaggedValue::Null),
            binds: Rc::new(Default::default()),
            handlers: Rc::from(Vec::new()),
        };
        let t = Term::app(Term::Arr {
            dim: Term::default_dim(),
            value: Value::from(arrow),
        }, vec![const_t(serde_json::json!(5))]);
        let v = eval(&t, Val::Const(TaggedValue::Null), &NativeOnly).unwrap();
        match v {
            Val::Arrow(a) => {
                assert_eq!(a.params.as_ref(), &["y".to_string()][..]);
                assert!(matches!(
                    a.binds.get("x"),
                    Some(Val::Const(TaggedValue::Int(5)))
                ));
            }
            other => panic!("expected arrow residual, got {other:?}"),
        }
    }

    #[test]
    fn apply_extends_neutral_spine() {
        let f = Val::Rigid {
            lvl: 0,
            spine: Spine::default(),
        };
        let v = apply(&f, Val::Const(TaggedValue::Int(1)), &NativeOnly).unwrap();
        match v {
            Val::Rigid { spine, .. } => assert_eq!(spine.0.len(), 1),
            other => panic!("expected rigid, got {other:?}"),
        }
    }

    #[test]
    fn op_dispatches_to_handler() {
        let t = Term::Op {
            dim: Term::default_dim(),
            name: "id".into(),
            args: vec![const_t(serde_json::json!("x"))],
            source: ArrSource::Judgment,
            lit: None,
        };
        // `id` ignores args, returns input.
        let v = eval(&t, Val::Const(TaggedValue::Int(9)), &NativeOnly).unwrap();
        assert!(matches!(v, Val::Const(TaggedValue::Int(9))));
    }

    #[test]
    fn record_proj_reads_field() {
        let rec = TaggedValue::from(serde_json::json!({"a": 3}));
        let t = Term::RecordProj {
            record: Box::new(Term::Arr {
                dim: Term::default_dim(),
                value: Value::from(rec),
            }),
            field: "a".into(),
        };
        let v = eval(&t, Val::Const(TaggedValue::Null), &NativeOnly).unwrap();
        assert!(matches!(v, Val::Const(TaggedValue::Int(3))));
    }

    #[test]
    fn handle_is_outside_fragment() {
        let t = Term::handle(
            const_t(serde_json::json!(1)),
            "io",
            const_t(serde_json::json!(2)),
        );
        assert!(matches!(
            eval(&t, Val::Const(TaggedValue::Null), &NativeOnly),
            Err(EvalError::NotImplemented("handle"))
        ));
    }

    #[test]
    fn seq_nonzero_axis_delegates_to_handler() {
        // `>>>[1]` on dim-2 arrows is pasting: reaches the handler as
        // `cell_comp_axis` — `NativeOnly` rejects unknown ops.
        let t = Term::seq_axis(
            const_t(serde_json::json!(1)),
            const_t(serde_json::json!("a")),
            const_t(serde_json::json!("b")),
        );
        assert!(matches!(
            eval(&t, Val::Const(TaggedValue::Null), &NativeOnly),
            Err(EvalError::Op(_))
        ));
    }
}
