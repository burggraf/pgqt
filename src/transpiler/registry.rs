use std::collections::HashMap;

pub enum FunctionMapping {
    Simple(&'static str),
    Rewrite(&'static str),
    Complex(fn(args: &[String]) -> String),
    #[allow(dead_code)]
    NoOp,
}

pub struct FunctionRegistry {
    pub mappings: HashMap<String, FunctionMapping>,
}

impl FunctionRegistry {
    pub fn new() -> Self {
        Self {
            mappings: HashMap::new(),
        }
    }

    pub fn register(&mut self, name: &str, mapping: FunctionMapping) {
        self.mappings.insert(name.to_lowercase(), mapping);
    }
}

pub struct TypeRegistry {
    pub mappings: HashMap<String, String>,
}

impl TypeRegistry {
    pub fn new() -> Self {
        Self {
            mappings: HashMap::new(),
        }
    }

    pub fn register(&mut self, pg_type: &str, sqlite_type: &str) {
        self.mappings.insert(pg_type.to_lowercase(), sqlite_type.to_string());
    }

    pub fn rewrite_type(&self, pg_type: &str) -> String {
        let lower = pg_type.to_lowercase();
        
        // Exact match
        if let Some(mapped) = self.mappings.get(&lower) {
            return mapped.clone();
        }

        // Prefix matching for things with parameters like VARCHAR(255)
        let base_type = lower.split('(').next().unwrap_or(&lower).trim();
        if let Some(mapped) = self.mappings.get(base_type) {
            return mapped.clone();
        }

        // Array types
        if lower.ends_with("[]") || lower.starts_with("array") {
            return "text".to_string();
        }
        
        // Fallback for some common prefix matches that might not be easily split
        if lower.starts_with("varchar") || lower.starts_with("character varying") || lower.starts_with("char") || lower.starts_with("character") || lower.starts_with("bpchar") {
            return "text".to_string();
        }

        if lower.starts_with("int") {
            return "integer".to_string();
        }

        if lower.starts_with("float") || lower.starts_with("double") || lower.starts_with("numeric") || lower.starts_with("decimal") {
            return "real".to_string();
        }

        if lower.starts_with("timestamp") || lower.starts_with("time") || lower.starts_with("date") || lower.starts_with("interval") {
            return "text".to_string();
        }

        if lower.starts_with("json") {
            return "text".to_string();
        }

        if lower.starts_with("vector") {
            return "text".to_string();
        }

        if lower.starts_with("bit") || lower.starts_with("varbit") {
            return "text".to_string();
        }

        "text".to_string() // Default
    }
}

pub struct Registry {
    pub types: TypeRegistry,
    pub functions: FunctionRegistry,
    pub stubbing_mode: bool,
}

impl Registry {
    pub fn new() -> Self {
        let mut types = TypeRegistry::new();
        // Populate types
        types.register("serial", "integer primary key autoincrement");
        types.register("smallserial", "integer primary key autoincrement");
        types.register("bigserial", "integer primary key autoincrement");
        types.register("varchar", "text");
        types.register("character varying", "text");
        types.register("char", "text");
        types.register("character", "text");
        types.register("bpchar", "text");
        types.register("text", "text");
        types.register("regclass", "integer");
        types.register("regtype", "integer");
        types.register("regproc", "integer");
        types.register("regprocedure", "integer");
        types.register("varbit", "text");
        types.register("bit varying", "text");
        types.register("int4range", "text");
        types.register("int8range", "text");
        types.register("numrange", "text");
        types.register("tsrange", "text");
        types.register("tstzrange", "text");
        types.register("daterange", "text");
        types.register("int", "integer");
        types.register("integer", "integer");
        types.register("bigint", "integer");
        types.register("smallint", "integer");
        types.register("int2", "integer");
        types.register("int4", "integer");
        types.register("int8", "integer");
        types.register("real", "real");
        types.register("float", "real");
        types.register("float4", "real");
        types.register("float8", "real");
        types.register("double", "real");
        types.register("numeric", "real");
        types.register("decimal", "real");
        types.register("boolean", "integer");
        types.register("bool", "integer");
        types.register("timestamp", "text");
        types.register("date", "text");
        types.register("time", "text");
        types.register("interval", "text");
        types.register("json", "text");
        types.register("jsonb", "text");
        types.register("uuid", "text");
        types.register("bytea", "blob");
        types.register("vector", "text");
        types.register("money", "real");
        types.register("bit", "text");
        types.register("xml", "text");
        types.register("inet", "text");
        types.register("cidr", "text");
        types.register("macaddr", "text");
        types.register("macaddr8", "text");
        types.register("point", "text");
        types.register("line", "text");
        types.register("lseg", "text");
        types.register("box", "text");
        types.register("path", "text");
        types.register("polygon", "text");
        types.register("circle", "text");
        types.register("tsvector", "text");
        types.register("tsquery", "text");

        let mut functions = FunctionRegistry::new();
        // Populate simple functions
        for f in &[
            "floor", "ceil", "abs", "coalesce", "nullif", "length", "lower", "upper", 
            "trim", "ltrim", "rtrim", "substr", "replace", "round", "pg_get_userbyid", 
            "pg_table_is_visible", "pg_type_is_visible", "pg_function_is_visible", 
            "format_type", "current_schema", "current_schemas", "current_database", 
            "current_setting", "pg_my_temp_schema", "pg_get_expr", "pg_get_indexdef", 
            "obj_description", "pg_get_constraintdef", "pg_encoding_to_char", 
            "array_to_string", "array_length", "pg_table_size", "pg_total_relation_size", 
            "pg_size_pretty", "stddev_pop", "stddev_samp", "var_pop", "var_samp", 
            "covar_pop", "covar_samp", "corr", "regr_slope", "regr_intercept", 
            "regr_count", "regr_r2", "regr_avgx", "regr_avgy", "regr_sxx", "regr_syy", 
            "regr_sxy", "to_tsvector", "to_tsquery", "plainto_tsquery", "phraseto_tsquery", 
            "websearch_to_tsquery", "ts_rank", "ts_rank_cd", "ts_headline", "setweight", 
            "strip", "numnode", "querytree", "ts_rewrite", "ts_lexize", "ts_debug", 
            "ts_stat", "array_to_tsvector", "jsonb_to_tsvector", "int4range", "int8range", 
            "numrange", "tsrange", "tstzrange", "daterange", "repeat", "generate_series", 
            "regexp", "regexpi", "make_interval", "justify_interval", "justify_days", "justify_hours"
        ] {
            functions.register(f, FunctionMapping::Simple(*f));
        }

        // Aliases for length()
        functions.register("char_length", FunctionMapping::Simple("length"));
        functions.register("character_length", FunctionMapping::Simple("length"));

        functions.register("now", FunctionMapping::Rewrite("datetime('now')"));
        functions.register("current_timestamp", FunctionMapping::Rewrite("datetime('now')"));
        functions.register("current_date", FunctionMapping::Rewrite("date('now')"));
        functions.register("current_time", FunctionMapping::Rewrite("time('now')"));
        functions.register("clock_timestamp", FunctionMapping::Rewrite("datetime('now')"));
        functions.register("statement_timestamp", FunctionMapping::Rewrite("datetime('now')"));
        functions.register("transaction_timestamp", FunctionMapping::Rewrite("datetime('now')"));
        functions.register("random", FunctionMapping::Rewrite("random()"));
        functions.register("gen_random_uuid", FunctionMapping::Rewrite("lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))), 2) || '-' || substr('89ab', abs(random()) % 4 + 1, 1) || substr(lower(hex(randomblob(2))), 2) || '-' || lower(hex(randomblob(6)))"));
        functions.register("uuid_generate_v4", FunctionMapping::Rewrite("lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))), 2) || '-' || substr('89ab', abs(random()) % 4 + 1, 1) || substr(lower(hex(randomblob(2))), 2) || '-' || lower(hex(randomblob(6)))"));
        functions.register("pg_sleep", FunctionMapping::Rewrite("0"));
        functions.register("any_value", FunctionMapping::Rewrite("min"));
        functions.register("booleq", FunctionMapping::Complex(|args| {
            if args.len() == 2 {
                format!("{} = {}", args[0], args[1])
            } else {
                "NULL".to_string()
            }
        }));
        functions.register("boolne", FunctionMapping::Complex(|args| {
            if args.len() == 2 {
                format!("{} <> {}", args[0], args[1])
            } else {
                "NULL".to_string()
            }
        }));

        functions.register("jsonb_path_exists", FunctionMapping::Complex(|args| {
            if args.len() >= 2 {
                let clean_path = args[1].replace("[*]", "");
                format!("CASE WHEN json_type({}, {}) = 'array' THEN json_array_length(json_extract({}, {})) > 0 ELSE json_type({}, {}) IS NOT NULL END", args[0], clean_path, args[0], clean_path, args[0], clean_path)
            } else {
                format!("json_type({}) IS NOT NULL", args.join(", "))
            }
        }));

        functions.register("jsonb_path_match", FunctionMapping::Complex(|args| {
            if args.len() >= 2 {
                let clean_path = args[1].replace("[*]", "");
                format!("json_extract({}, {}) = 1", args[0], clean_path)
            } else {
                format!("json_extract({}) = 1", args.join(", "))
            }
        }));

        functions.register("jsonb_path_query", FunctionMapping::Complex(|args| {
            if args.len() >= 2 {
                let clean_path = args[1].replace("[*]", "");
                format!("json_extract({}, {})", args[0], clean_path)
            } else {
                format!("json_extract({})", args.join(", "))
            }
        }));

        functions.register("jsonb_path_query_array", FunctionMapping::Complex(|args| {
            if args.len() >= 2 {
                let clean_path = args[1].replace("[*]", "");
                format!("json_extract({}, {})", args[0], clean_path)
            } else {
                format!("json_extract({})", args.join(", "))
            }
        }));

        functions.register("jsonb_path_query_first", FunctionMapping::Complex(|args| {
            if args.len() >= 2 {
                let clean_path = args[1].replace("[*]", "");
                format!("json_extract({}, {})", args[0], clean_path)
            } else {
                format!("json_extract({})", args.join(", "))
            }
        }));

        functions.register("jsonb_typeof", FunctionMapping::Complex(|args| {
            if !args.is_empty() {
                format!("CASE json_type({0}) WHEN 'true' THEN 'boolean' WHEN 'false' THEN 'boolean' WHEN 'integer' THEN 'number' WHEN 'real' THEN 'number' WHEN 'text' THEN 'string' ELSE json_type({0}) END", args[0])
            } else {
                format!("json_type({})", args.join(", "))
            }
        }));

        functions.register("jsonb_object_keys", FunctionMapping::Complex(|args| {
            if !args.is_empty() {
                format!("(SELECT json_group_array(key) FROM json_each({}))", args[0])
            } else {
                format!("(SELECT json_group_array(key) FROM json_each({}))", args.join(", "))
            }
        }));

        functions.register("jsonb_each", FunctionMapping::Complex(|args| {
            if !args.is_empty() {
                format!("json_each({})", args[0])
            } else {
                format!("json_each({})", args.join(", "))
            }
        }));
        
        functions.register("json_each", FunctionMapping::Complex(|args| {
            if !args.is_empty() {
                format!("json_each({})", args[0])
            } else {
                format!("json_each({})", args.join(", "))
            }
        }));

        functions.register("jsonb_array_elements", FunctionMapping::Complex(|args| {
            if !args.is_empty() {
                format!("json_each({})", args[0])
            } else {
                format!("json_each({})", args.join(", "))
            }
        }));

        functions.register("jsonb_extract_path", FunctionMapping::Complex(|args| {
            if args.len() >= 2 {
                format!("json_extract({}, {})", args[0], args[1..].join(", "))
            } else {
                format!("json_extract({})", args.join(", "))
            }
        }));

        functions.register("jsonb_extract_path_text", FunctionMapping::Complex(|args| {
            if args.len() >= 2 {
                format!("json_extract({}, {})", args[0], args[1..].join(", "))
            } else {
                format!("json_extract({})", args.join(", "))
            }
        }));

        // JSON builder functions - map PostgreSQL json_build_object/jsonb_build_object to SQLite json_object
        functions.register("json_build_object", FunctionMapping::Complex(|args| {
            // Build json_object(key1, value1, key2, value2, ...)
            format!("json_object({})", args.join(", "))
        }));

        functions.register("jsonb_build_object", FunctionMapping::Complex(|args| {
            // jsonb_build_object maps to same json_object in SQLite
            format!("json_object({})", args.join(", "))
        }));

        // JSON array builder functions - map PostgreSQL json_build_array/jsonb_build_array to SQLite json_array
        functions.register("json_build_array", FunctionMapping::Complex(|args| {
            // Build json_array(value1, value2, ...)
            format!("json_array({})", args.join(", "))
        }));

        functions.register("jsonb_build_array", FunctionMapping::Complex(|args| {
            // jsonb_build_array maps to same json_array in SQLite
            format!("json_array({})", args.join(", "))
        }));

        functions.register("pg_input_is_valid", FunctionMapping::Complex(|_args| {
            "1".to_string()
        }));

        functions.register("timezone", FunctionMapping::Complex(|args| {
            if args.len() >= 2 {
                args[1].clone()
            } else {
                "0".to_string()
            }
        }));

        functions.register("extract", FunctionMapping::Complex(|args| {
            if args.len() >= 2 {
                let field = args[0].trim_matches('\'');
                let source = &args[1];
                match field.to_lowercase().as_str() {
                    "year" => format!("cast(strftime('%Y', {}) as integer)", source),
                    "month" => format!("cast(strftime('%m', {}) as integer)", source),
                    "day" => format!("cast(strftime('%d', {}) as integer)", source),
                    "hour" => format!("cast(strftime('%H', {}) as integer)", source),
                    "minute" => format!("cast(strftime('%M', {}) as integer)", source),
                    "second" => format!("cast(strftime('%S', {}) as integer)", source),
                    "dow" => format!("cast(strftime('%w', {}) as integer)", source),
                    "doy" => format!("cast(strftime('%j', {}) as integer)", source),
                    "week" => format!("cast(strftime('%W', {}) as integer)", source),
                    "quarter" => format!("(cast(strftime('%m', {}) as integer) + 2) / 3", source),
                    "epoch" => format!("strftime('%s', {})", source),
                    "millennium" => format!("(cast(strftime('%Y', {}) as integer) + 999) / 1000", source),
                    "century" => format!("(cast(strftime('%Y', {}) as integer) + 99) / 100", source),
                    "decade" => format!("cast(strftime('%Y', {}) as integer) / 10", source),
                    _ => format!("strftime('{}', {})", field, source),
                }
            } else {
                format!("extract({})", args.join(", "))
            }
        }));

        // Math functions: log, ln, sqrt, exp, div
        functions.register("log", FunctionMapping::Complex(|args| {
            if args.len() == 1 {
                // log(x) -> log10(x) in SQLite (PostgreSQL log is base 10)
                format!("log10({})", args[0])
            } else if args.len() == 2 {
                // log(base, x) -> log(x) / log(base) using change of base formula
                // SQLite's log() is natural log, so we use change of base: log_base(x) = ln(x) / ln(base)
                format!("(log({}) / log({}))", args[1], args[0])
            } else {
                format!("log({})", args.join(", "))
            }
        }));

        functions.register("ln", FunctionMapping::Complex(|args| {
            if args.len() == 1 {
                // ln(x) -> log(x) in SQLite (SQLite's log is natural log)
                format!("log({})", args[0])
            } else {
                format!("ln({})", args.join(", "))
            }
        }));

        functions.register("sqrt", FunctionMapping::Complex(|args| {
            if args.len() == 1 {
                format!("sqrt({})", args[0])
            } else {
                format!("sqrt({})", args.join(", "))
            }
        }));

        functions.register("exp", FunctionMapping::Complex(|args| {
            if args.len() == 1 {
                format!("exp({})", args[0])
            } else {
                format!("exp({})", args.join(", "))
            }
        }));

        functions.register("div", FunctionMapping::Complex(|args| {
            if args.len() == 2 {
                // PostgreSQL div() truncates toward zero
                // SQLite / operator with integers also truncates toward zero
                format!("CAST({} / {} AS INTEGER)", args[0], args[1])
            } else {
                format!("div({})", args.join(", "))
            }
        }));

        // btrim - trim characters from both ends (PostgreSQL compatibility)
        functions.register("btrim", FunctionMapping::Complex(|args| {
            if args.len() == 1 {
                format!("trim({})", args[0])
            } else if args.len() == 2 {
                format!("trim({}, {})", args[0], args[1])
            } else {
                format!("btrim({})", args.join(", "))
            }
        }));

        // position - find position of substring in string
        functions.register("position", FunctionMapping::Complex(|args| {
            if args.len() == 2 {
                format!("instr({}, {})", args[1], args[0])
            } else {
                format!("position({})", args.join(", "))
            }
        }));

        // overlay - overlay replacement string at position
        functions.register("overlay", FunctionMapping::Complex(|args| {
            if args.len() >= 3 {
                let string = &args[0];
                let replacement = &args[1];
                let start = &args[2];
                if args.len() >= 4 {
                    let length = &args[3];
                    format!("substr({}, 1, {} - 1) || {} || substr({}, {} + {})", 
                        string, start, replacement, string, start, length)
                } else {
                    format!("substr({}, 1, {} - 1) || {} || substr({}, {} + length({}))", 
                        string, start, replacement, string, start, replacement)
                }
            } else {
                format!("overlay({})", args.join(", "))
            }
        }));

        // decode - decode bytea from various encodings
        functions.register("decode", FunctionMapping::Complex(|args| {
            if args.len() == 2 {
                let data = &args[0];
                let encoding = args[1].trim_matches('\'').to_lowercase();
                match encoding.as_str() {
                    "hex" => format!("hex_decode({})", data),
                    "escape" => format!("escape_decode({})", data),
                    "base64" => format!("base64_decode({})", data),
                    _ => format!("decode({}, {})", data, args[1])
                }
            } else {
                format!("decode({})", args.join(", "))
            }
        }));

        // encode - encode blob to various encodings
        functions.register("encode", FunctionMapping::Complex(|args| {
            if args.len() == 2 {
                let data = &args[0];
                let encoding = args[1].trim_matches('\'').to_lowercase();
                match encoding.as_str() {
                    "hex" => format!("hex({})", data),
                    "escape" => format!("escape_encode({})", data),
                    "base64" => format!("base64_encode({})", data),
                    _ => format!("encode({}, {})", data, args[1])
                }
            } else {
                format!("encode({})", args.join(", "))
            }
        }));

        // trim_scale - trim trailing zeros from numeric
        functions.register("trim_scale", FunctionMapping::Complex(|args| {
            if args.len() == 1 {
                format!("CAST(regexp_replace(CAST({} AS TEXT), '\\.0*$|\\.([0-9]*[1-9])0*$', '\\.\\1') AS REAL)", args[0])
            } else {
                format!("trim_scale({})", args.join(", "))
            }
        }));

        // string_to_array - handled by SQLite scalar function in handler/mod.rs

        // regexp_replace - replace substring matching pattern
        functions.register("regexp_replace", FunctionMapping::Complex(|args| {
            match args.len() {
                3 => format!("regexp_replace({}, {}, {})", args[0], args[1], args[2]),
                4 => format!("regexp_replace({}, {}, {}, {})", args[0], args[1], args[2], args[3]),
                5 => format!("regexp_replace({}, {}, {}, {})", args[0], args[1], args[2], args[4]),
                6 => format!("regexp_replace({}, {}, {}, {})", args[0], args[1], args[2], args[5]),
                _ => format!("regexp_replace({})", args.join(", "))
            }
        }));

        Self {
            types,
            functions,
            stubbing_mode: true, // We'll enable it for testing dynamic stubbing
        }
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}
