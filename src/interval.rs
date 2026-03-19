//! Interval type support for PGQT
//!
//! Supports PostgreSQL interval input formats:
//! - Standard format: '1 day 2 hours' (component-based)
//! - At-style: '@ 1 minute' (at-prefix format)
//! - ISO 8601: 'P1Y2M3DT4H5M6S' (ISO standard)
//! - Weeks: '1.5 weeks' (decimal weeks)
//! - Months: '5 months' (months only)
//! - Years: '6 years' (years only)
//! - Infinity: 'infinity', '-infinity' (special values)

use std::fmt;
use std::str::FromStr;

/// Error type for interval parsing
#[derive(Debug, Clone, PartialEq)]
pub enum IntervalError {
    InvalidFormat(String),
    InvalidNumber(String),
    DivisionByZero,
}

impl fmt::Display for IntervalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IntervalError::InvalidFormat(msg) => write!(f, "{}", msg),
            IntervalError::InvalidNumber(msg) => write!(f, "{}", msg),
            IntervalError::DivisionByZero => write!(f, "Division by zero"),
        }
    }
}

impl std::error::Error for IntervalError {}

/// Internal representation of a PostgreSQL interval
///
/// PostgreSQL stores intervals as three components:
/// - months: i32 (for year/month components)
/// - days: i32 (for day components)
/// - microseconds: i64 (for time components)
///
/// This separation is necessary because months have variable days
/// and cannot be precisely converted without a reference date.
#[derive(Debug, Clone, PartialEq)]
pub struct Interval {
    /// Months component (can be negative)
    pub months: i32,
    /// Days component (can be negative)
    pub days: i32,
    /// Time component in microseconds (can be negative)
    pub microseconds: i64,
}

/// Constant for microseconds in one second
const MICROSECONDS_PER_SECOND: i64 = 1_000_000;
/// Constant for microseconds in one minute
const MICROSECONDS_PER_MINUTE: i64 = 60_000_000;
/// Constant for microseconds in one hour
const MICROSECONDS_PER_HOUR: i64 = 3_600_000_000;
/// Constant for microseconds in one day
const MICROSECONDS_PER_DAY: i64 = 86_400_000_000;
/// Approximate days per month for conversions (30.44 days)
const DAYS_PER_MONTH: f64 = 30.44;

impl Interval {
    /// Create a new interval with the specified components
    #[allow(dead_code)]
    pub fn new(months: i32, days: i32, microseconds: i64) -> Self {
        Interval {
            months,
            days,
            microseconds,
        }
    }

    /// Create a zero interval
    pub fn zero() -> Self {
        Interval {
            months: 0,
            days: 0,
            microseconds: 0,
        }
    }

    /// Create an infinity interval
    pub fn infinity() -> Self {
        Interval {
            months: i32::MAX,
            days: i32::MAX,
            microseconds: i64::MAX,
        }
    }

    /// Create a negative infinity interval
    pub fn neg_infinity() -> Self {
        Interval {
            months: i32::MIN,
            days: i32::MIN,
            microseconds: i64::MIN,
        }
    }

    /// Check if this is infinity
    #[allow(dead_code)]
    pub fn is_infinity(&self) -> bool {
        self.months == i32::MAX && self.days == i32::MAX && self.microseconds == i64::MAX
    }

    /// Check if this is negative infinity
    #[allow(dead_code)]
    pub fn is_neg_infinity(&self) -> bool {
        self.months == i32::MIN && self.days == i32::MIN && self.microseconds == i64::MIN
    }

    /// Parse an interval from a string
    /// 
    /// Returns PostgreSQL-compatible error messages for invalid inputs.
    pub fn from_str(s: &str) -> Result<Self, IntervalError> {
        let trimmed = s.trim();
        
        if trimmed.is_empty() {
            return Err(IntervalError::InvalidFormat(format!(
                "invalid input syntax for type interval: \"{}\"",
                s
            )));
        }

        // Check for infinity
        if trimmed.eq_ignore_ascii_case("infinity") {
            return Ok(Interval::infinity());
        }
        if trimmed.eq_ignore_ascii_case("-infinity") || trimmed.eq_ignore_ascii_case("@ -infinity") {
            return Ok(Interval::neg_infinity());
        }

        // Try ISO 8601 format (starts with P)
        if trimmed.starts_with('P') || trimmed.starts_with("@ P") {
            let without_at = trimmed.strip_prefix("@ ").unwrap_or(trimmed);
            return Self::parse_iso8601(without_at);
        }

        // Try at-style format (starts with @)
        if trimmed.starts_with('@') {
            return Self::parse_at_style(trimmed);
        }

        // Try standard format
        Self::parse_standard(trimmed)
    }

    /// Parse standard format like "1 day 2 hours 3 minutes 4 seconds"
    /// 
    /// Returns PostgreSQL-compatible error messages for invalid inputs.
    fn parse_standard(s: &str) -> Result<Self, IntervalError> {
        let mut interval = Interval::zero();
        let mut remaining = s.trim();

        // Handle the "ago" suffix at the end
        let is_ago = remaining.to_lowercase().ends_with(" ago");
        if is_ago {
            remaining = remaining[..remaining.len() - 4].trim();
        }

        // Parse components
        let mut has_components = false;
        
        while !remaining.is_empty() {
            // Skip any leading whitespace or commas
            remaining = remaining.trim_start().trim_start_matches(',').trim_start();
            if remaining.is_empty() {
                break;
            }

            // Parse a number (can be integer or decimal)
            let (value, rest) = match Self::parse_number(remaining) {
                Ok((v, r)) => {
                    has_components = true;
                    (v, r)
                }
                Err(_) => {
                    return Err(IntervalError::InvalidFormat(format!(
                        "invalid input syntax for type interval: \"{}\"",
                        s
                    )));
                }
            };
            remaining = rest.trim_start();

            // Parse the unit
            let (unit, rest) = Self::parse_unit(remaining);
            if unit.is_empty() {
                return Err(IntervalError::InvalidFormat(format!(
                    "invalid input syntax for type interval: \"{}\"",
                    s
                )));
            }
            remaining = rest;

            // Add to interval
            interval.add_component(value, &unit)?;
        }

        // If no components were parsed, the input was invalid
        if !has_components {
            return Err(IntervalError::InvalidFormat(format!(
                "invalid input syntax for type interval: \"{}\"",
                s
            )));
        }

        // Apply "ago" by negating
        if is_ago {
            interval.months = -interval.months;
            interval.days = -interval.days;
            interval.microseconds = -interval.microseconds;
        }

        Ok(interval)
    }

    /// Parse at-style format like "@ 1 minute" or "@ 5 hour ago"
    fn parse_at_style(s: &str) -> Result<Self, IntervalError> {
        let without_at = s.trim_start_matches('@').trim_start();
        Self::parse_standard(without_at)
    }

    /// Parse ISO 8601 duration format: P[n]Y[n]M[n]DT[n]H[n]M[n]S
    /// Examples: P1Y2M3DT4H5M6S, PT1H30M, P1W
    /// 
    /// Returns PostgreSQL-compatible error messages for invalid inputs.
    fn parse_iso8601(s: &str) -> Result<Self, IntervalError> {
        if !s.starts_with('P') {
            return Err(IntervalError::InvalidFormat(format!(
                "invalid input syntax for type interval: \"{}\"",
                s
            )));
        }

        let mut interval = Interval::zero();
        let mut chars = s[1..].chars().peekable();
        let mut current_num = String::new();
        let mut has_time = false;
        let mut has_components = false;

        while let Some(&ch) = chars.peek() {
            if ch.is_ascii_digit() || ch == '.' {
                current_num.push(ch);
                chars.next();
            } else {
                match ch {
                    'Y' => {
                        let val = current_num
                            .parse::<f64>()
                            .map_err(|_| IntervalError::InvalidNumber(current_num.clone()))?;
                        interval.months += (val * 12.0) as i32;
                        has_components = true;
                        current_num.clear();
                        chars.next();
                    }
                    'M' if !has_time => {
                        let val = current_num
                            .parse::<f64>()
                            .map_err(|_| IntervalError::InvalidNumber(current_num.clone()))?;
                        interval.months += val as i32;
                        has_components = true;
                        current_num.clear();
                        chars.next();
                    }
                    'W' => {
                        let val = current_num
                            .parse::<f64>()
                            .map_err(|_| IntervalError::InvalidNumber(current_num.clone()))?;
                        interval.days += (val * 7.0) as i32;
                        has_components = true;
                        current_num.clear();
                        chars.next();
                    }
                    'D' => {
                        let val = current_num
                            .parse::<f64>()
                            .map_err(|_| IntervalError::InvalidNumber(current_num.clone()))?;
                        interval.days += val as i32;
                        has_components = true;
                        current_num.clear();
                        chars.next();
                    }
                    'T' => {
                        has_time = true;
                        current_num.clear();
                        chars.next();
                    }
                    'H' => {
                        let val = current_num
                            .parse::<f64>()
                            .map_err(|_| IntervalError::InvalidNumber(current_num.clone()))?;
                        interval.microseconds += (val * MICROSECONDS_PER_HOUR as f64) as i64;
                        has_components = true;
                        current_num.clear();
                        chars.next();
                    }
                    'M' if has_time => {
                        let val = current_num
                            .parse::<f64>()
                            .map_err(|_| IntervalError::InvalidNumber(current_num.clone()))?;
                        interval.microseconds += (val * MICROSECONDS_PER_MINUTE as f64) as i64;
                        has_components = true;
                        current_num.clear();
                        chars.next();
                    }
                    'S' => {
                        let val = current_num
                            .parse::<f64>()
                            .map_err(|_| IntervalError::InvalidNumber(current_num.clone()))?;
                        interval.microseconds += (val * MICROSECONDS_PER_SECOND as f64) as i64;
                        has_components = true;
                        current_num.clear();
                        chars.next();
                    }
                    _ => {
                        return Err(IntervalError::InvalidFormat(format!(
                            "invalid input syntax for type interval: \"{}\"",
                            s
                        )));
                    }
                }
            }
        }

        // If no components were parsed, the input was invalid
        if !has_components {
            return Err(IntervalError::InvalidFormat(format!(
                "invalid input syntax for type interval: \"{}\"",
                s
            )));
        }

        Ok(interval)
    }

    /// Parse a number from the beginning of a string
    /// 
    /// Returns PostgreSQL-compatible error messages for invalid inputs.
    fn parse_number(s: &str) -> Result<(f64, &str), IntervalError> {
        let mut end = 0;
        let mut has_dot = false;

        // Handle optional sign
        if s.starts_with('-') || s.starts_with('+') {
            end = 1;
        }

        // Track the start offset for calculating absolute positions
        let start_offset = end;

        for (i, ch) in s[start_offset..].chars().enumerate() {
            if ch.is_ascii_digit() {
                end = start_offset + i + 1;
            } else if ch == '.' && !has_dot {
                has_dot = true;
                end = start_offset + i + 1;
            } else {
                break;
            }
        }

        if end == 0 || (end == 1 && (s.starts_with('-') || s.starts_with('+'))) {
            return Err(IntervalError::InvalidFormat(format!(
                "invalid input syntax for type interval: \"{}\"",
                s
            )));
        }

        let num_str = &s[..end];
        let value = num_str
            .parse::<f64>()
            .map_err(|_| IntervalError::InvalidNumber(num_str.to_string()))?;

        Ok((value, &s[end..]))
    }

    /// Parse a unit from the beginning of a string
    fn parse_unit(s: &str) -> (String, &str) {
        // List of valid units (longest first to avoid partial matches like "month" matching "months")
        let units = [
            "microseconds",
            "microsecond",
            "milliseconds",
            "millisecond",
            "centuries",
            "century",
            "decades",
            "decade",
            "millennia",
            "millennium",
            "seconds",
            "second",
            "minutes",
            "minute",
            "hours",
            "hour",
            "days",
            "day",
            "weeks",
            "week",
            "months",
            "month",
            "years",
            "year",
        ];

        let lower = s.to_lowercase();
        for unit in units {
            if lower.starts_with(unit) {
                // Return the original case from s
                let len = unit.len();
                return (s[..len].to_lowercase(), &s[len..]);
            }
        }

        (String::new(), s)
    }

    /// Add a value with a unit to this interval
    fn add_component(&mut self, value: f64, unit: &str) -> Result<(), IntervalError> {
        match unit {
            "microsecond" | "microseconds" => {
                self.microseconds += value as i64;
            }
            "millisecond" | "milliseconds" => {
                self.microseconds += (value * 1_000.0) as i64;
            }
            "second" | "seconds" => {
                self.microseconds += (value * MICROSECONDS_PER_SECOND as f64) as i64;
            }
            "minute" | "minutes" => {
                self.microseconds += (value * MICROSECONDS_PER_MINUTE as f64) as i64;
            }
            "hour" | "hours" => {
                self.microseconds += (value * MICROSECONDS_PER_HOUR as f64) as i64;
            }
            "day" | "days" => {
                // Store integer days and convert fractional part to microseconds
                self.days += value as i32;
                let frac = value.fract();
                if frac.abs() > f64::EPSILON {
                    self.microseconds += (frac * MICROSECONDS_PER_DAY as f64) as i64;
                }
            }
            "week" | "weeks" => {
                // Convert weeks to days first, then handle fractional days
                let total_days = value * 7.0;
                self.days += total_days as i32;
                let frac_days = total_days.fract();
                if frac_days.abs() > f64::EPSILON {
                    self.microseconds += (frac_days * MICROSECONDS_PER_DAY as f64) as i64;
                }
            }
            "month" | "months" => {
                // Store integer months and convert fractional part to days (approximate)
                self.months += value as i32;
                let frac = value.fract();
                if frac.abs() > f64::EPSILON {
                    let frac_days = frac * DAYS_PER_MONTH;
                    self.days += frac_days as i32;
                    let frac_day_remainder = frac_days.fract();
                    if frac_day_remainder.abs() > f64::EPSILON {
                        self.microseconds += (frac_day_remainder * MICROSECONDS_PER_DAY as f64) as i64;
                    }
                }
            }
            "year" | "years" => {
                // Convert years to months first
                let total_months = value * 12.0;
                self.months += total_months as i32;
                let frac = total_months.fract();
                if frac.abs() > f64::EPSILON {
                    let frac_days = frac * DAYS_PER_MONTH;
                    self.days += frac_days as i32;
                    let frac_day_remainder = frac_days.fract();
                    if frac_day_remainder.abs() > f64::EPSILON {
                        self.microseconds += (frac_day_remainder * MICROSECONDS_PER_DAY as f64) as i64;
                    }
                }
            }
            "decade" | "decades" => {
                let total_months = value * 120.0;
                self.months += total_months as i32;
                let frac = total_months.fract();
                if frac.abs() > f64::EPSILON {
                    let frac_days = frac * DAYS_PER_MONTH;
                    self.days += frac_days as i32;
                    let frac_day_remainder = frac_days.fract();
                    if frac_day_remainder.abs() > f64::EPSILON {
                        self.microseconds += (frac_day_remainder * MICROSECONDS_PER_DAY as f64) as i64;
                    }
                }
            }
            "century" | "centuries" => {
                let total_months = value * 1200.0;
                self.months += total_months as i32;
                let frac = total_months.fract();
                if frac.abs() > f64::EPSILON {
                    let frac_days = frac * DAYS_PER_MONTH;
                    self.days += frac_days as i32;
                    let frac_day_remainder = frac_days.fract();
                    if frac_day_remainder.abs() > f64::EPSILON {
                        self.microseconds += (frac_day_remainder * MICROSECONDS_PER_DAY as f64) as i64;
                    }
                }
            }
            "millennium" | "millennia" => {
                let total_months = value * 12_000.0;
                self.months += total_months as i32;
                let frac = total_months.fract();
                if frac.abs() > f64::EPSILON {
                    let frac_days = frac * DAYS_PER_MONTH;
                    self.days += frac_days as i32;
                    let frac_day_remainder = frac_days.fract();
                    if frac_day_remainder.abs() > f64::EPSILON {
                        self.microseconds += (frac_day_remainder * MICROSECONDS_PER_DAY as f64) as i64;
                    }
                }
            }
            _ => {
                return Err(IntervalError::InvalidFormat(format!(
                    "Unknown unit: '{}'",
                    unit
                )));
            }
        }
        Ok(())
    }

    /// Normalize for comparison (approximate total microseconds)
    /// This is an approximation since months have variable days
    fn normalize_for_compare(&self) -> i128 {
        // Use approximate conversions for comparison
        // 1 month ≈ 30.44 days
        // 1 day = 24 * 60 * 60 * 1_000_000 microseconds
        let month_micros = (self.months as i128) * (DAYS_PER_MONTH as i128) * MICROSECONDS_PER_DAY as i128;
        let day_micros = (self.days as i128) * MICROSECONDS_PER_DAY as i128;
        month_micros + day_micros + (self.microseconds as i128)
    }

    /// Add two intervals
    pub fn add(&self, other: &Interval) -> Interval {
        Interval {
            months: self.months + other.months,
            days: self.days + other.days,
            microseconds: self.microseconds + other.microseconds,
        }
    }

    /// Subtract two intervals
    pub fn sub(&self, other: &Interval) -> Interval {
        Interval {
            months: self.months - other.months,
            days: self.days - other.days,
            microseconds: self.microseconds - other.microseconds,
        }
    }

    /// Multiply interval by a factor
    pub fn mul(&self, factor: f64) -> Interval {
        Interval {
            months: (self.months as f64 * factor) as i32,
            days: (self.days as f64 * factor) as i32,
            microseconds: (self.microseconds as f64 * factor) as i64,
        }
    }

    /// Divide interval by a divisor
    pub fn div(&self, divisor: f64) -> Result<Interval, IntervalError> {
        if divisor == 0.0 {
            return Err(IntervalError::DivisionByZero);
        }
        Ok(Interval {
            months: (self.months as f64 / divisor) as i32,
            days: (self.days as f64 / divisor) as i32,
            microseconds: (self.microseconds as f64 / divisor) as i64,
        })
    }

    /// Negate the interval
    pub fn neg(&self) -> Interval {
        Interval {
            months: -self.months,
            days: -self.days,
            microseconds: -self.microseconds,
        }
    }

    /// Check equality
    pub fn eq(&self, other: &Interval) -> bool {
        self.months == other.months
            && self.days == other.days
            && self.microseconds == other.microseconds
    }

    /// Check less than
    pub fn lt(&self, other: &Interval) -> bool {
        self.normalize_for_compare() < other.normalize_for_compare()
    }

    /// Check less than or equal
    pub fn le(&self, other: &Interval) -> bool {
        self.eq(other) || self.lt(other)
    }

    /// Check greater than
    pub fn gt(&self, other: &Interval) -> bool {
        !self.le(other)
    }

    /// Check greater than or equal
    pub fn ge(&self, other: &Interval) -> bool {
        !self.lt(other)
    }

    /// Extract a field from the interval
    pub fn extract(&self, field: &str) -> f64 {
        match field.to_uppercase().as_str() {
            "EPOCH" => {
                let month_seconds = (self.months as f64) * DAYS_PER_MONTH * 24.0 * 3600.0;
                let day_seconds = (self.days as f64) * 24.0 * 3600.0;
                let micros_seconds = (self.microseconds as f64) / MICROSECONDS_PER_SECOND as f64;
                month_seconds + day_seconds + micros_seconds
            }
            "CENTURY" => (self.months as f64) / 1200.0,
            "DECADE" => (self.months as f64) / 120.0,
            "YEAR" => (self.months as f64) / 12.0,
            "MONTH" => (self.months % 12) as f64,
            "DAY" => self.days as f64,
            "HOUR" => (self.microseconds / MICROSECONDS_PER_HOUR) as f64,
            "MINUTE" => ((self.microseconds % MICROSECONDS_PER_HOUR) / MICROSECONDS_PER_MINUTE) as f64,
            "SECOND" => {
                ((self.microseconds % MICROSECONDS_PER_MINUTE) as f64) / MICROSECONDS_PER_SECOND as f64
            }
            "MILLISECOND" => {
                ((self.microseconds % MICROSECONDS_PER_SECOND) as f64) / 1000.0
                    + ((self.microseconds / MICROSECONDS_PER_SECOND) % 60) as f64 * 1000.0
            }
            "MICROSECOND" => (self.microseconds % MICROSECONDS_PER_SECOND) as f64,
            _ => 0.0,
        }
    }
}

impl FromStr for Interval {
    type Err = IntervalError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Interval::from_str(s)
    }
}

impl fmt::Display for Interval {
    /// Format interval as a delimited string for storage: "months|days|microseconds"
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}|{}|{}", self.months, self.days, self.microseconds)
    }
}

/// Parse an interval from a string
/// This is the main entry point for SQL interval parsing
pub fn parse_interval(input: &str) -> Result<Interval, IntervalError> {
    Interval::from_str(input)
}

/// Parse an interval from its storage format (months|days|microseconds)
pub fn parse_interval_storage(input: &str) -> Result<Interval, IntervalError> {
    let parts: Vec<&str> = input.split('|').collect();
    if parts.len() != 3 {
        return Err(IntervalError::InvalidFormat(format!(
            "Expected 'months|days|microseconds' format, got: {}",
            input
        )));
    }

    let months = parts[0]
        .parse::<i32>()
        .map_err(|_| IntervalError::InvalidNumber(parts[0].to_string()))?;
    let days = parts[1]
        .parse::<i32>()
        .map_err(|_| IntervalError::InvalidNumber(parts[1].to_string()))?;
    let microseconds = parts[2]
        .parse::<i64>()
        .map_err(|_| IntervalError::InvalidNumber(parts[2].to_string()))?;

    Ok(Interval {
        months,
        days,
        microseconds,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_standard_format() {
        let test_cases = vec![
            ("1 day", Interval::new(0, 1, 0)),
            ("2 days", Interval::new(0, 2, 0)),
            ("1 hour", Interval::new(0, 0, MICROSECONDS_PER_HOUR)),
            ("2 hours", Interval::new(0, 0, 2 * MICROSECONDS_PER_HOUR)),
            ("1 minute", Interval::new(0, 0, MICROSECONDS_PER_MINUTE)),
            ("1 second", Interval::new(0, 0, MICROSECONDS_PER_SECOND)),
            (
                "1 day 2 hours",
                Interval::new(0, 1, 2 * MICROSECONDS_PER_HOUR),
            ),
            (
                "2 hours 30 minutes",
                Interval::new(0, 0, 2 * MICROSECONDS_PER_HOUR + 30 * MICROSECONDS_PER_MINUTE),
            ),
            (
                "1 day 2 hours 3 minutes 4 seconds",
                Interval::new(
                    0,
                    1,
                    2 * MICROSECONDS_PER_HOUR + 3 * MICROSECONDS_PER_MINUTE + 4 * MICROSECONDS_PER_SECOND,
                ),
            ),
            ("1 year", Interval::new(12, 0, 0)),
            ("2 years", Interval::new(24, 0, 0)),
            ("1 month", Interval::new(1, 0, 0)),
            ("6 months", Interval::new(6, 0, 0)),
            ("1 year 6 months", Interval::new(18, 0, 0)),
            ("1 week", Interval::new(0, 7, 0)),
            ("2 weeks", Interval::new(0, 14, 0)),
            ("1.5 weeks", Interval::new(0, 10, 43200000000)), // 10.5 days = 10 days + 0.5*86400000000 microseconds
            (
                "1 decade",
                Interval::new(120, 0, 0),
            ),
            (
                "1 century",
                Interval::new(1200, 0, 0),
            ),
            (
                "1 millennium",
                Interval::new(12000, 0, 0),
            ),
        ];

        for (input, expected) in test_cases {
            let parsed = Interval::from_str(input).unwrap();
            assert_eq!(parsed, expected, "Failed for input: {}", input);
        }
    }

    #[test]
    fn test_at_style_format() {
        let test_cases = vec![
            ("@ 1 minute", Interval::new(0, 0, MICROSECONDS_PER_MINUTE)),
            ("@ 1 hour", Interval::new(0, 0, MICROSECONDS_PER_HOUR)),
            ("@ 1 day", Interval::new(0, 1, 0)),
            ("@ 1 week", Interval::new(0, 7, 0)),
        ];

        for (input, expected) in test_cases {
            let parsed = Interval::from_str(input).unwrap();
            assert_eq!(parsed, expected, "Failed for input: {}", input);
        }
    }

    #[test]
    fn test_iso8601_format() {
        let test_cases = vec![
            ("P1Y", Interval::new(12, 0, 0)),
            ("P2M", Interval::new(2, 0, 0)),
            ("P3D", Interval::new(0, 3, 0)),
            ("P1W", Interval::new(0, 7, 0)),
            (
                "PT1H",
                Interval::new(0, 0, MICROSECONDS_PER_HOUR),
            ),
            (
                "PT1M",
                Interval::new(0, 0, MICROSECONDS_PER_MINUTE),
            ),
            (
                "PT1S",
                Interval::new(0, 0, MICROSECONDS_PER_SECOND),
            ),
            (
                "P1Y2M3DT4H5M6S",
                Interval::new(
                    14,
                    3,
                    4 * MICROSECONDS_PER_HOUR + 5 * MICROSECONDS_PER_MINUTE + 6 * MICROSECONDS_PER_SECOND,
                ),
            ),
            (
                "PT1H30M",
                Interval::new(0, 0, MICROSECONDS_PER_HOUR + 30 * MICROSECONDS_PER_MINUTE),
            ),
            ("@ P1D", Interval::new(0, 1, 0)),
        ];

        for (input, expected) in test_cases {
            let parsed = Interval::from_str(input).unwrap();
            assert_eq!(parsed, expected, "Failed for input: {}", input);
        }
    }

    #[test]
    fn test_weeks_format() {
        let test_cases = vec![
            ("1 week", Interval::new(0, 7, 0)),
            ("2 weeks", Interval::new(0, 14, 0)),
            ("1.5 weeks", Interval::new(0, 10, 43200000000)),
        ];

        for (input, expected) in test_cases {
            let parsed = Interval::from_str(input).unwrap();
            assert_eq!(parsed, expected, "Failed for input: {}", input);
        }
    }

    #[test]
    fn test_months_format() {
        let test_cases = vec![
            ("5 months", Interval::new(5, 0, 0)),
            ("1 month", Interval::new(1, 0, 0)),
        ];

        for (input, expected) in test_cases {
            let parsed = Interval::from_str(input).unwrap();
            assert_eq!(parsed, expected, "Failed for input: {}", input);
        }
    }

    #[test]
    fn test_years_format() {
        let test_cases = vec![
            ("6 years", Interval::new(72, 0, 0)),
            ("1 year", Interval::new(12, 0, 0)),
        ];

        for (input, expected) in test_cases {
            let parsed = Interval::from_str(input).unwrap();
            assert_eq!(parsed, expected, "Failed for input: {}", input);
        }
    }

    #[test]
    fn test_infinity() {
        let inf = Interval::from_str("infinity").unwrap();
        assert!(inf.is_infinity());

        let neg_inf = Interval::from_str("-infinity").unwrap();
        assert!(neg_inf.is_neg_infinity());
    }

    #[test]
    fn test_ago_suffix() {
        let test_cases = vec![
            (
                "1 day ago",
                Interval::new(0, -1, 0),
            ),
            (
                "2 hours ago",
                Interval::new(0, 0, -2 * MICROSECONDS_PER_HOUR),
            ),
            (
                "1 hour 30 minutes ago",
                Interval::new(0, 0, -(MICROSECONDS_PER_HOUR + 30 * MICROSECONDS_PER_MINUTE)),
            ),
        ];

        for (input, expected) in test_cases {
            let parsed = Interval::from_str(input).unwrap();
            assert_eq!(parsed, expected, "Failed for input: {}", input);
        }
    }

    #[test]
    fn test_storage_format() {
        let interval = Interval::new(14, 3, 14706000000);
        let storage = interval.to_string();
        assert_eq!(storage, "14|3|14706000000");

        let parsed = parse_interval_storage(&storage).unwrap();
        assert_eq!(parsed, interval);
    }

    #[test]
    fn test_negative_values() {
        let test_cases = vec![
            ("-1 day", Interval::new(0, -1, 0)),
            ("-2 hours", Interval::new(0, 0, -2 * MICROSECONDS_PER_HOUR)),
            (
                "-1 day -2 hours",
                Interval::new(0, -1, -2 * MICROSECONDS_PER_HOUR),
            ),
        ];

        for (input, expected) in test_cases {
            let parsed = Interval::from_str(input).unwrap();
            assert_eq!(parsed, expected, "Failed for input: {}", input);
        }
    }

    #[test]
    fn test_empty_string() {
        // Empty string should now return an error (PostgreSQL-compatible behavior)
        let result = Interval::from_str("");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("invalid input syntax for type interval"));
    }

    #[test]
    fn test_whitespace_only() {
        // Whitespace-only string should now return an error (PostgreSQL-compatible behavior)
        let result = Interval::from_str("   ");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("invalid input syntax for type interval"));
    }

    #[test]
    fn test_display_trait() {
        let interval = Interval::new(1, 2, 3);
        let s = format!("{}", interval);
        assert_eq!(s, "1|2|3");
    }

    #[test]
    fn test_arithmetic() {
        let i1 = Interval::new(1, 2, 3);
        let i2 = Interval::new(1, 1, 1);

        let sum = i1.add(&i2);
        assert_eq!(sum, Interval::new(2, 3, 4));

        let diff = i1.sub(&i2);
        assert_eq!(diff, Interval::new(0, 1, 2));

        let neg = i1.neg();
        assert_eq!(neg, Interval::new(-1, -2, -3));

        let mul = i1.mul(2.0);
        assert_eq!(mul, Interval::new(2, 4, 6));

        let div = i1.div(2.0).unwrap();
        assert_eq!(div, Interval::new(0, 1, 1));
    }

    #[test]
    fn test_comparison() {
        let i1 = Interval::new(1, 0, 0); // 1 month
        let i2 = Interval::new(0, 30, 0); // 30 days (approximately 1 month)
        let i3 = Interval::new(2, 0, 0); // 2 months

        assert!(i1.lt(&i3));
        assert!(i3.gt(&i1));
        assert!(i1.le(&i1));
        assert!(i1.ge(&i1));
        assert!(i1.eq(&i1));
    }

    #[test]
    fn test_extract() {
        let interval = Interval::new(14, 3, 14706000000); // 14 months, 3 days, ~4 hours 5 min 6 sec

        assert_eq!(interval.extract("YEAR"), 14.0 / 12.0);
        assert_eq!(interval.extract("MONTH"), 2.0); // 14 % 12 = 2
        assert_eq!(interval.extract("DAY"), 3.0);
        assert_eq!(interval.extract("HOUR"), 4.0);
        assert_eq!(interval.extract("MINUTE"), 5.0);
        assert!((interval.extract("SECOND") - 6.0).abs() < 0.001);
    }

    #[test]
    fn test_invalid_formats() {
        // Invalid format should return an error
        assert!(Interval::from_str("invalid").is_err());
        assert!(Interval::from_str("1").is_err()); // Missing unit
        assert!(Interval::from_str("day").is_err()); // Missing number
    }

    #[test]
    fn test_division_by_zero() {
        let interval = Interval::new(1, 2, 3);
        assert!(interval.div(0.0).is_err());
    }

    #[test]
    fn test_storage_parsing_errors() {
        assert!(parse_interval_storage("invalid").is_err());
        assert!(parse_interval_storage("1|2").is_err()); // Missing field
        assert!(parse_interval_storage("a|b|c").is_err()); // Non-numeric
    }

    #[test]
    fn test_invalid_interval_strings_postgresql_compatible() {
        // Test that invalid intervals return PostgreSQL-compatible error messages
        let invalid_cases = vec![
            "invalid",
            "xyz",
            "not an interval",
            "1",           // Missing unit
            "day",         // Missing number
            "5.0.0 days",  // Invalid number format
            "",            // Empty string should now error
            "   ",         // Whitespace only should now error
        ];

        for input in invalid_cases {
            let result = Interval::from_str(input);
            assert!(result.is_err(), "Should reject: {}", input);
            
            let err_msg = result.unwrap_err().to_string();
            assert!(
                err_msg.contains("invalid input syntax for type interval"),
                "Error message should be PostgreSQL-compatible for '{}': got '{}'",
                input,
                err_msg
            );
        }
    }

    #[test]
    fn test_valid_intervals_still_work() {
        // Ensure valid intervals still parse correctly
        let valid_cases = vec![
            ("1 day", Interval::new(0, 1, 0)),
            ("2 hours", Interval::new(0, 0, 2 * MICROSECONDS_PER_HOUR)),
            ("1 year 6 months", Interval::new(18, 0, 0)),
            ("@ 1 minute", Interval::new(0, 0, MICROSECONDS_PER_MINUTE)),
            ("P1Y2M3D", Interval::new(14, 3, 0)),
            ("PT1H30M", Interval::new(0, 0, MICROSECONDS_PER_HOUR + 30 * MICROSECONDS_PER_MINUTE)),
            ("infinity", Interval::infinity()),
            ("-infinity", Interval::neg_infinity()),
        ];

        for (input, expected) in valid_cases {
            let result = Interval::from_str(input);
            assert!(result.is_ok(), "Should accept '{}': {:?}", input, result);
            assert_eq!(result.unwrap(), expected, "Failed for input: {}", input);
        }
    }

    #[test]
    fn test_validate_interval_function() {
        // Test the validate_interval function
        assert!(validate_interval("1 day").is_ok());
        assert!(validate_interval("2 hours").is_ok());
        assert!(validate_interval("invalid").is_err());
        assert!(validate_interval("xyz").is_err());
        
        // Check error message format
        let err = validate_interval("invalid").unwrap_err();
        assert!(err.contains("invalid input syntax for type interval"));
    }
}

/// Validates that a string is a valid interval format
/// 
/// Returns Ok(()) if valid, or Err with a PostgreSQL-compatible error message if invalid.
/// 
/// # Examples
/// 
/// ```
/// use pgqt::interval::validate_interval;
/// 
/// assert!(validate_interval("1 day").is_ok());
/// assert!(validate_interval("2 hours 30 minutes").is_ok());
/// assert!(validate_interval("invalid").is_err());
/// ```
#[allow(dead_code)]
pub fn validate_interval(s: &str) -> Result<(), String> {
    match Interval::from_str(s) {
        Ok(_) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}
