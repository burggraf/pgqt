    /// Register interval functions with SQLite
    fn register_interval_functions(conn: &Connection) -> Result<()> {
        use rusqlite::functions::FunctionFlags;

        // parse_interval - parse interval string from PostgreSQL format
        conn.create_scalar_function(
            "parse_interval",
            1,
            FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
            |ctx| {
                let s: String = ctx.get(0)?;
                let interval = interval::parse_interval(&s)
                    .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
                Ok(interval.to_string())
            },
        )?;

        // interval_add - add two intervals
        conn.create_scalar_function(
            "interval_add",
            2,
            FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
            |ctx| {
                let s1: String = ctx.get(0)?;
                let s2: String = ctx.get(1)?;
                let i1 = interval::parse_interval_storage(&s1)
                    .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
                let i2 = interval::parse_interval_storage(&s2)
                    .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
                Ok(i1.add(&i2).to_string())
            },
        )?;

        // interval_sub - subtract two intervals
        conn.create_scalar_function(
            "interval_sub",
            2,
            FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
            |ctx| {
                let s1: String = ctx.get(0)?;
                let s2: String = ctx.get(1)?;
                let i1 = interval::parse_interval_storage(&s1)
                    .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
                let i2 = interval::parse_interval_storage(&s2)
                    .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
                Ok(i1.sub(&i2).to_string())
            },
        )?;

        // interval_mul - multiply interval by number
        conn.create_scalar_function(
            "interval_mul",
            2,
            FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
            |ctx| {
                let s: String = ctx.get(0)?;
                let factor: f64 = ctx.get(1)?;
                let i = interval::parse_interval_storage(&s)
                    .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
                Ok(i.mul(factor).to_string())
            },
        )?;

        // interval_div - divide interval by number
        conn.create_scalar_function(
            "interval_div",
            2,
            FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
            |ctx| {
                let s: String = ctx.get(0)?;
                let divisor: f64 = ctx.get(1)?;
                let i = interval::parse_interval_storage(&s)
                    .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
                let result = i.div(divisor)
                    .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
                Ok(result.to_string())
            },
        )?;

        // interval_neg - negate interval
        conn.create_scalar_function(
            "interval_neg",
            1,
            FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
            |ctx| {
                let s: String = ctx.get(0)?;
                let i = interval::parse_interval_storage(&s)
                    .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
                Ok(i.neg().to_string())
            },
        )?;

        // interval_eq - check if intervals are equal
        conn.create_scalar_function(
            "interval_eq",
            2,
            FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
            |ctx| {
                let s1: String = ctx.get(0)?;
                let s2: String = ctx.get(1)?;
                let i1 = interval::parse_interval_storage(&s1)
                    .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
                let i2 = interval::parse_interval_storage(&s2)
                    .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
                Ok(i1.eq(&i2))
            },
        )?;

        // interval_lt - check if first interval is less than second
        conn.create_scalar_function(
            "interval_lt",
            2,
            FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
            |ctx| {
                let s1: String = ctx.get(0)?;
                let s2: String = ctx.get(1)?;
                let i1 = interval::parse_interval_storage(&s1)
                    .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
                let i2 = interval::parse_interval_storage(&s2)
                    .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
                Ok(i1.lt(&i2))
            },
        )?;

        // interval_le - check if first interval is less than or equal to second
        conn.create_scalar_function(
            "interval_le",
            2,
            FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
            |ctx| {
                let s1: String = ctx.get(0)?;
                let s2: String = ctx.get(1)?;
                let i1 = interval::parse_interval_storage(&s1)
                    .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
                let i2 = interval::parse_interval_storage(&s2)
                    .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
                Ok(i1.le(&i2))
            },
        )?;

        // interval_gt - check if first interval is greater than second
        conn.create_scalar_function(
            "interval_gt",
            2,
            FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
            |ctx| {
                let s1: String = ctx.get(0)?;
                let s2: String = ctx.get(1)?;
                let i1 = interval::parse_interval_storage(&s1)
                    .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
                let i2 = interval::parse_interval_storage(&s2)
                    .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
                Ok(i1.gt(&i2))
            },
        )?;

        // interval_ge - check if first interval is greater than or equal to second
        conn.create_scalar_function(
            "interval_ge",
            2,
            FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
            |ctx| {
                let s1: String = ctx.get(0)?;
                let s2: String = ctx.get(1)?;
                let i1 = interval::parse_interval_storage(&s1)
                    .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
                let i2 = interval::parse_interval_storage(&s2)
                    .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().into()))?;
                Ok(i1.ge(&i2))
            },
        )?;

        // interval_ne - check if intervals are not equal
        conn.create_scalar_function(
            "interval_ne",
            2,
            FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
            |ctx| {
                let s1: String = ctx.get(0)?;
                let s2: String = ctx.get(1)?;
                let i1 = interval::parse_interval_storage(&s1)
                    .map_err(|e| rusqlite::Error::UserFunctionError(e.to_string().