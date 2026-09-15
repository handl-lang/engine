//! Pure-data interpreter for the arrow-calculus `Term` kernel.
//!
//! This is a proof-of-concept evaluator for lowered `Term` graphs. It returns
//! `Err` for any construct it does not (yet) handle, which lets callers fall
//! back to the existing AST eval engine.

use serde_json::{from_value, Map, Number, Value};

use crate::Term;

/// Interpret a `Term` as an arrow from `input` to a result value.
///
/// Returns `Err` for unsupported `Term` variants or operator names, which is
/// the signal to fall back to another evaluator.
pub fn interpret(term: &Term, input: Value) -> Result<Value, String> {
    match term {
        Term::Arr { value } | Term::Const { value } => Ok(value.clone()),

        Term::Seq { fst, snd } => {
            let v = interpret(fst, input)?;
            interpret(snd, v)
        }

        Term::First { inner } => {
            let (a, b) = pair_components(&input)?;
            let v = interpret(inner, a)?;
            Ok(mk_pair(v, b))
        }

        Term::Choice { l, r } => {
            let (side, value) = either_side(input)?;
            if side == "left" {
                interpret(l, value)
            } else {
                interpret(r, value)
            }
        }

        Term::Loop { body } => {
            let mut state = Value::Null;
            for _ in 0..10_000 {
                let pair = mk_pair(input.clone(), state);
                let out = interpret(body, pair)?;

                if let Some(v) = either_left_value(&out) {
                    return Ok(v);
                }

                if let Some(v) = either_right_value(&out) {
                    state = v;
                    continue;
                }

                // A plain pair is treated as (result, next-state) and terminates.
                if let Ok((fst, _snd)) = pair_components(&out) {
                    return Ok(fst);
                }

                // Anything else is treated as the final result for a simple
                // non-nested loop; this is a conservative fallback.
                return Ok(out);
            }
            eprintln!("[term-interpreter] Term::Loop: exceeded max iterations");
            Err("Loop: exceeded max iterations".to_string())
        }

        Term::App { callee, args } => {
            let arg_vals: Vec<Value> = args
                .iter()
                .map(|a| interpret(a, input.clone()))
                .collect::<Result<_, _>>()?;
            let arg_input = if arg_vals.len() == 1 {
                arg_vals.into_iter().next().unwrap()
            } else {
                Value::Array(arg_vals)
            };

            let callee_val = interpret(callee, input)?;

            // If the callee evaluates to a `Term` value, run it.
            if let Ok(scheme) = from_value::<Term>(callee_val.clone()) {
                return interpret(&scheme, arg_input);
            }

            // String callees are host-function names; the small interpreter
            // has no registry, so report it clearly.
            if callee_val.as_str().is_some() {
                eprintln!(
                    "[term-interpreter] Term::App: host-function callees are not supported"
                );
                return Err("App: host-function callees are not supported".to_string());
            }

            eprintln!("[term-interpreter] Term::App: callee is not an arrow value: {callee_val}");
            Err("App: callee is not an arrow value".to_string())
        }

        Term::Handle { target, .. } => {
            eprintln!("[term-interpreter] Term::Handle for `{target}` is not supported");
            Err(format!("Handle: {target} is not supported"))
        }

        Term::Op { name, args, lit, .. } => eval_op(name, args, lit.as_ref(), input),

        other => {
            eprintln!("[term-interpreter] unsupported Term: {other:?}");
            Err(format!("unsupported Term: {other:?}"))
        }
    }
}

fn eval_op(name: &str, args: &[Term], lit: Option<&Value>, input: Value) -> Result<Value, String> {
    match name {
        "id" => Ok(input),

        "proj_child" => {
            let idx = lit
                .and_then(Value::as_u64)
                .ok_or_else(|| "proj_child: missing integer literal")? as usize;
            input
                .as_array()
                .and_then(|a| a.get(idx).cloned())
                .ok_or_else(|| "proj_child: index out of bounds".into())
        }

        "add" | "mul" | "sub" | "div" => {
            if args.is_empty() {
                return Err("math op: no arguments".into());
            }
            let mut values = Vec::with_capacity(args.len());
            for arg in args {
                values.push(interpret(arg, input.clone())?);
            }
            let mut iter = values.into_iter();
            let first = iter.next().unwrap();
            let mut result = first
                .as_f64()
                .ok_or_else(|| format!("{name}: non-numeric argument"))?;
            for v in iter {
                let n = v
                    .as_f64()
                    .ok_or_else(|| format!("{name}: non-numeric argument"))?;
                match name {
                    "add" => result += n,
                    "mul" => result *= n,
                    "sub" => result -= n,
                    "div" => result /= n,
                    _ => unreachable!(),
                }
            }
            // Preserve integers when the result is whole.
            if result == result.trunc()
                && result <= i64::MAX as f64
                && result >= i64::MIN as f64
            {
                Ok(Value::Number(Number::from(result as i64)))
            } else {
                Number::from_f64(result)
                    .map(Value::Number)
                    .ok_or_else(|| format!("{name}: invalid numeric result"))
            }
        }

        "product" | "fn.fanout_fn" | "fanout" => {
            let mut out = Vec::with_capacity(args.len());
            for arg in args {
                out.push(interpret(arg, input.clone())?);
            }
            Ok(Value::Array(out))
        }

        "record" => {
            let keys = lit
                .and_then(Value::as_array)
                .ok_or_else(|| "record: missing key list")?;
            if keys.len() != args.len() {
                return Err("record: key/value count mismatch".into());
            }
            let mut obj = Map::new();
            for (i, arg) in args.iter().enumerate() {
                let key = keys[i]
                    .as_str()
                    .ok_or_else(|| "record: key is not a string")?;
                obj.insert(key.to_string(), interpret(arg, input.clone())?);
            }
            Ok(Value::Object(obj))
        }

        "field_access" => {
            let target = args
                .first()
                .ok_or_else(|| "field_access: missing target")?;
            let v = interpret(target, input)?;
            match lit {
                Some(Value::Number(n)) if n.is_u64() => {
                    let idx = n.as_u64().unwrap() as usize;
                    v.as_array()
                        .and_then(|a| a.get(idx).cloned())
                        .ok_or_else(|| "field_access: index out of bounds".into())
                }
                Some(Value::String(s)) => v
                    .get(s)
                    .cloned()
                    .ok_or_else(|| format!("field_access: no field {s}")),
                _ => Err("field_access: missing/unsupported field literal".into()),
            }
        }

        other => {
            eprintln!("[term-interpreter] unsupported op: {other}");
            Err(format!("unsupported op: {other}"))
        }
    }
}

fn pair_components(value: &Value) -> Result<(Value, Value), String> {
    if let Some(arr) = value.as_array() {
        if arr.len() == 2 {
            return Ok((arr[0].clone(), arr[1].clone()));
        }
    }
    if let Some(obj) = value.as_object() {
        if let Some(pair) = obj.get("__pair__").and_then(Value::as_array) {
            if pair.len() == 2 {
                return Ok((pair[0].clone(), pair[1].clone()));
            }
        }
    }
    Err("expected a pair (array of length 2 or __pair__)".to_string())
}

fn mk_pair(a: Value, b: Value) -> Value {
    Value::Array(vec![a, b])
}

fn either_side(value: Value) -> Result<(&'static str, Value), String> {
    if let Some(obj) = value.as_object() {
        if let Some(side) = obj.get("__either__").and_then(Value::as_str) {
            let inner = obj
                .get("value")
                .cloned()
                .ok_or_else(|| "Either: missing value")?;
            return match side {
                "left" => Ok(("left", inner)),
                "right" => Ok(("right", inner)),
                _ => Err(format!("Either: unknown side {side}")),
            };
        }
        if let Some(side) = obj.get("__tag__").and_then(Value::as_str) {
            let inner = obj
                .get("value")
                .cloned()
                .unwrap_or(Value::Null);
            return match side {
                "Left" => Ok(("left", inner)),
                "Right" => Ok(("right", inner)),
                _ => Err(format!("Choice: unknown tag {side}")),
            };
        }
    }
    Err("Choice: input is not an Either".to_string())
}

fn either_left_value(value: &Value) -> Option<Value> {
    if let Some(obj) = value.as_object() {
        if obj.get("__either__").and_then(Value::as_str) == Some("left") {
            return obj.get("value").cloned();
        }
        if obj.get("__tag__").and_then(Value::as_str) == Some("Left") {
            return obj.get("value").cloned();
        }
    }
    None
}

fn either_right_value(value: &Value) -> Option<Value> {
    if let Some(obj) = value.as_object() {
        if obj.get("__either__").and_then(Value::as_str) == Some("right") {
            return obj.get("value").cloned();
        }
        if obj.get("__tag__").and_then(Value::as_str) == Some("Right") {
            return obj.get("value").cloned();
        }
    }
    None
}
