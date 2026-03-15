use rusqlite::functions::{Aggregate, Context, FunctionFlags};
use rusqlite::{Connection, Result};
use rusqlite::types::Value;
use std::collections::HashSet;

#[derive(Default)]
struct HypotheticalRankState {
    hypothetical_value: Option<Value>,
    count_less: i64,
    count_equal: i64,
    total_rows: i64,
    distinct_less: HashSet<String>, 
}

fn compare_values(a: &Value, b: &Value) -> std::cmp::Ordering {
    match (a, b) {
        (Value::Integer(ai), Value::Integer(bi)) => ai.cmp(bi),
        (Value::Integer(ai), Value::Real(bf)) => (*ai as f64).partial_cmp(bf).unwrap_or(std::cmp::Ordering::Equal),
        (Value::Real(af), Value::Integer(bi)) => af.partial_cmp(&(*bi as f64)).unwrap_or(std::cmp::Ordering::Equal),
        (Value::Real(af), Value::Real(bf)) => af.partial_cmp(bf).unwrap_or(std::cmp::Ordering::Equal),
        (Value::Text(as_), Value::Text(bs)) => as_.cmp(bs),
        (Value::Null, Value::Null) => std::cmp::Ordering::Equal,
        (Value::Null, _) => std::cmp::Ordering::Less,
        (_, Value::Null) => std::cmp::Ordering::Greater,
        (Value::Blob(ab), Value::Blob(bb)) => ab.cmp(bb),
        _ => std::cmp::Ordering::Equal,
    }
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::Null => "NULL".to_string(),
        Value::Integer(i) => format!("I:{}", i),
        Value::Real(f) => format!("R:{}", f),
        Value::Text(s) => format!("T:{}", s),
        Value::Blob(b) => format!("B:{}", hex::encode(b)),
    }
}

pub struct HypotheticalRank;
pub struct HypotheticalDenseRank;
pub struct HypotheticalPercentRank;
pub struct HypotheticalCumeDist;

impl Aggregate<HypotheticalRankState, i64> for HypotheticalRank {
    fn init(&self, _ctx: &mut Context<'_>) -> Result<HypotheticalRankState> {
        Ok(HypotheticalRankState::default())
    }

    fn step(&self, ctx: &mut Context<'_>, acc: &mut HypotheticalRankState) -> Result<()> {
        let hyp_val: Value = ctx.get(0)?;
        let table_val: Value = ctx.get(1)?;
        if acc.hypothetical_value.is_none() { acc.hypothetical_value = Some(hyp_val.clone()); }
        acc.total_rows += 1;
        match compare_values(&table_val, &hyp_val) {
            std::cmp::Ordering::Less => acc.count_less += 1,
            std::cmp::Ordering::Equal => acc.count_equal += 1,
            _ => {}
        }
        Ok(())
    }

    fn finalize(&self, _ctx: &mut Context<'_>, acc: Option<HypotheticalRankState>) -> Result<i64> {
        match acc {
            Some(state) => Ok(state.count_less + 1),
            None => Ok(1),
        }
    }
}

impl Aggregate<HypotheticalRankState, i64> for HypotheticalDenseRank {
    fn init(&self, _ctx: &mut Context<'_>) -> Result<HypotheticalRankState> {
        Ok(HypotheticalRankState::default())
    }

    fn step(&self, ctx: &mut Context<'_>, acc: &mut HypotheticalRankState) -> Result<()> {
        let hyp_val: Value = ctx.get(0)?;
        let table_val: Value = ctx.get(1)?;
        if acc.hypothetical_value.is_none() { acc.hypothetical_value = Some(hyp_val.clone()); }
        acc.total_rows += 1;
        match compare_values(&table_val, &hyp_val) {
            std::cmp::Ordering::Less => {
                acc.distinct_less.insert(value_to_string(&table_val));
            }
            std::cmp::Ordering::Equal => acc.count_equal += 1,
            _ => {}
        }
        Ok(())
    }

    fn finalize(&self, _ctx: &mut Context<'_>, acc: Option<HypotheticalRankState>) -> Result<i64> {
        match acc {
            Some(state) => Ok(state.distinct_less.len() as i64 + 1),
            None => Ok(1),
        }
    }
}

impl Aggregate<HypotheticalRankState, f64> for HypotheticalPercentRank {
    fn init(&self, _ctx: &mut Context<'_>) -> Result<HypotheticalRankState> {
        Ok(HypotheticalRankState::default())
    }

    fn step(&self, ctx: &mut Context<'_>, acc: &mut HypotheticalRankState) -> Result<()> {
        let hyp_val: Value = ctx.get(0)?;
        let table_val: Value = ctx.get(1)?;
        if acc.hypothetical_value.is_none() { acc.hypothetical_value = Some(hyp_val.clone()); }
        acc.total_rows += 1;
        if compare_values(&table_val, &hyp_val) == std::cmp::Ordering::Less {
            acc.count_less += 1;
        }
        Ok(())
    }

    fn finalize(&self, _ctx: &mut Context<'_>, acc: Option<HypotheticalRankState>) -> Result<f64> {
        match acc {
            Some(state) => {
                if state.total_rows == 0 {
                    Ok(0.0)
                } else {
                    Ok(state.count_less as f64 / state.total_rows as f64)
                }
            }
            None => Ok(0.0),
        }
    }
}

impl Aggregate<HypotheticalRankState, f64> for HypotheticalCumeDist {
    fn init(&self, _ctx: &mut Context<'_>) -> Result<HypotheticalRankState> {
        Ok(HypotheticalRankState::default())
    }

    fn step(&self, ctx: &mut Context<'_>, acc: &mut HypotheticalRankState) -> Result<()> {
        let hyp_val: Value = ctx.get(0)?;
        let table_val: Value = ctx.get(1)?;
        if acc.hypothetical_value.is_none() { acc.hypothetical_value = Some(hyp_val.clone()); }
        acc.total_rows += 1;
        match compare_values(&table_val, &hyp_val) {
            std::cmp::Ordering::Less | std::cmp::Ordering::Equal => acc.count_less += 1,
            _ => {}
        }
        Ok(())
    }

    fn finalize(&self, _ctx: &mut Context<'_>, acc: Option<HypotheticalRankState>) -> Result<f64> {
        match acc {
            Some(state) => Ok((state.count_less + 1) as f64 / (state.total_rows + 1) as f64),
            None => Ok(1.0),
        }
    }
}

pub fn register_hypothetical_functions(conn: &Connection) -> Result<()> {
    let flags = FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC;
    conn.create_aggregate_function("__pg_hypothetical_rank", 2, flags, HypotheticalRank)?;
    conn.create_aggregate_function("__pg_hypothetical_dense_rank", 2, flags, HypotheticalDenseRank)?;
    conn.create_aggregate_function("__pg_hypothetical_percent_rank", 2, flags, HypotheticalPercentRank)?;
    conn.create_aggregate_function("__pg_hypothetical_cume_dist", 2, flags, HypotheticalCumeDist)?;
    Ok(())
}
