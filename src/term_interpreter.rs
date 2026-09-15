//! Pure-data interpreter for the arrow-calculus `Term` kernel.
//!
//! This is a proof-of-concept evaluator for lowered `Term` graphs. It returns
//! `Err` for any construct it does not (yet) handle, which lets callers fall
//! back to the existing AST eval engine.

use serde_json::{Map, Number, Value};

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
            let arr = input.as_array().ok_or_else(|| "First: input is not an array")?;
            if arr.len() < 2 {
                return Err("First: input array too short".into());
            }
            let a = arr[0].clone();
            let b = arr[1].clone();
            let v = interpret(inner, a)?;
            Ok(Value::Array(vec![v, b]))
        }

        Term::Op { name, args, lit, .. } => eval_op(name, args, lit.as_ref(), input),

        other => Err(format!("unsupported Term: {other:?}")),
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

        "math.add" | "math.mul" | "math.sub" | "math.div" => {
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
                    "math.add" => result += n,
                    "math.mul" => result *= n,
                    "math.sub" => result -= n,
                    "math.div" => result /= n,
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

        "product" | "fanout" => {
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

        other => Err(format!("unsupported op: {other}")),
    }
}
