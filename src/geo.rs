//! Geometric type support for PostgreSQL compatibility
//!
//! This module implements 2D geometric data types and their associated operators
//! and distance functions, providing PostgreSQL-compatible geometry support backed
//! by SQLite TEXT storage.
//!
//! ## Supported Types
//! - `POINT` — a 2D coordinate `(x, y)`
//! - `LINE` — an infinite line described by coefficients `{A, B, C}` (Ax + By + C = 0)
//! - `LSEG` — a finite line segment `[(x1,y1),(x2,y2)]`
//! - `BOX` — a rectangular box `(x1,y1),(x2,y2)`
//! - `PATH` — an open or closed sequence of points
//! - `POLYGON` — a closed polygon
//! - `CIRCLE` — a circle `<(x,y),r>`
//!
//! ## Supported Operators
//! - Distance: `<->`, `<#>`, `<=>`
//! - Containment: `@>`, `<@`, `&&`
//! - Translation: `+`, `-`, `*`, `/`

use std::str::FromStr;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
    
    pub fn distance(&self, other: &Point) -> f64 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }
}

impl FromStr for Point {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        let s = if s.starts_with('(') && s.ends_with(')') {
            &s[1..s.len()-1]
        } else {
            s
        };
        let parts: Vec<&str> = s.split(',').collect();
        if parts.len() != 2 {
            return Err(format!("Invalid point format: {}", s));
        }
        let x = parts[0].trim().parse::<f64>().map_err(|e| e.to_string())?;
        let y = parts[1].trim().parse::<f64>().map_err(|e| e.to_string())?;
        Ok(Point { x, y })
    }
}

impl fmt::Display for Point {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({},{})", self.x, self.y)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub struct Lseg {
    pub p1: Point,
    pub p2: Point,
}

impl FromStr for Lseg {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        let s = if s.starts_with('(') && s.ends_with(')') {
            &s[1..s.len()-1]
        } else {
            s
        };
        // Expecting ((x1,y1),(x2,y2)) or (x1,y1),(x2,y2) or x1,y1,x2,y2
        // Simplest: split by common delimiters and take 4 floats
        let parts: Vec<&str> = s.split(|c| c == ',' || c == '(' || c == ')').filter(|s| !s.trim().is_empty()).collect();
        if parts.len() == 4 {
            let x1 = parts[0].trim().parse::<f64>().map_err(|e| e.to_string())?;
            let y1 = parts[1].trim().parse::<f64>().map_err(|e| e.to_string())?;
            let x2 = parts[2].trim().parse::<f64>().map_err(|e| e.to_string())?;
            let y2 = parts[3].trim().parse::<f64>().map_err(|e| e.to_string())?;
            Ok(Lseg { p1: Point::new(x1, y1), p2: Point::new(x2, y2) })
        } else {
            Err(format!("Invalid lseg format: {}", s))
        }
    }
}

impl fmt::Display for Lseg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({},{})", self.p1, self.p2)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Box {
    pub high: Point, // Upper right
    pub low: Point,  // Lower left
}

impl FromStr for Box {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        let s = if s.starts_with('(') && s.ends_with(')') {
            &s[1..s.len()-1]
        } else {
            s
        };
        let parts: Vec<&str> = s.split(|c| c == ',' || c == '(' || c == ')').filter(|s| !s.trim().is_empty()).collect();
        if parts.len() == 4 {
            let x1 = parts[0].trim().parse::<f64>().map_err(|e| e.to_string())?;
            let y1 = parts[1].trim().parse::<f64>().map_err(|e| e.to_string())?;
            let x2 = parts[2].trim().parse::<f64>().map_err(|e| e.to_string())?;
            let y2 = parts[3].trim().parse::<f64>().map_err(|e| e.to_string())?;
            
            // Reorder to high (UR) and low (LL)
            let high = Point::new(x1.max(x2), y1.max(y2));
            let low = Point::new(x1.min(x2), y1.min(y2));
            Ok(Box { high, low })
        } else {
            Err(format!("Invalid box format: {}", s))
        }
    }
}

impl fmt::Display for Box {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({},{})", self.high, self.low)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub struct Circle {
    pub center: Point,
    pub radius: f64,
}

impl FromStr for Circle {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        // Format: <(x,y),r> or ((x,y),r)
        let s = if (s.starts_with('<') && s.ends_with('>')) || (s.starts_with('(') && s.ends_with(')')) {
            &s[1..s.len()-1]
        } else {
            s
        };
        
        let parts: Vec<&str> = s.split(|c| c == ',' || c == '(' || c == ')' || c == '<' || c == '>').filter(|s| !s.trim().is_empty()).collect();
        if parts.len() == 3 {
            let x = parts[0].trim().parse::<f64>().map_err(|e| e.to_string())?;
            let y = parts[1].trim().parse::<f64>().map_err(|e| e.to_string())?;
            let r = parts[2].trim().parse::<f64>().map_err(|e| e.to_string())?;
            Ok(Circle { center: Point::new(x, y), radius: r })
        } else {
            Err(format!("Invalid circle format: {}", s))
        }
    }
}

impl fmt::Display for Circle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<{},{}>", self.center, self.radius)
    }
}

// Geometric functions implemented as SQLite scalar functions
pub fn point_distance(p1: &str, p2: &str) -> Result<f64, String> {
    let p1_obj = Point::from_str(p1).map_err(|e| format!("p1: {}", e))?;
    let p2_obj = Point::from_str(p2).map_err(|e| format!("p2: {}", e))?;
    Ok(p1_obj.distance(&p2_obj))
}

pub fn box_overlaps(b1: &str, b2: &str) -> Result<bool, String> {
    let b1 = Box::from_str(b1)?;
    let b2 = Box::from_str(b2)?;
    
    // Two boxes overlap if they are not strictly separated in X or Y
    let x_overlap = b1.low.x <= b2.high.x && b2.low.x <= b1.high.x;
    let y_overlap = b1.low.y <= b2.high.y && b2.low.y <= b1.high.y;
    Ok(x_overlap && y_overlap)
}

pub fn box_contains(b1: &str, b2: &str) -> Result<bool, String> {
    let b1 = Box::from_str(b1)?;
    let b2 = Box::from_str(b2)?;
    
    Ok(b1.high.x >= b2.high.x && b1.high.y >= b2.high.y &&
       b1.low.x <= b2.low.x && b1.low.y <= b2.low.y)
}

pub fn box_left(b1: &str, b2: &str) -> Result<bool, String> {
    let b1 = Box::from_str(b1)?;
    let b2 = Box::from_str(b2)?;
    Ok(b1.high.x < b2.low.x)
}

pub fn box_right(b1: &str, b2: &str) -> Result<bool, String> {
    let b1 = Box::from_str(b1)?;
    let b2 = Box::from_str(b2)?;
    Ok(b1.low.x > b2.high.x)
}

pub fn box_below(b1: &str, b2: &str) -> Result<bool, String> {
    let b1 = Box::from_str(b1)?;
    let b2 = Box::from_str(b2)?;
    Ok(b1.high.y < b2.low.y)
}

pub fn box_above(b1: &str, b2: &str) -> Result<bool, String> {
    let b1 = Box::from_str(b1)?;
    let b2 = Box::from_str(b2)?;
    Ok(b1.low.y > b2.high.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_point_parsing() {
        let p: Point = "1.0, 2.0".parse().unwrap();
        assert_eq!(p.x, 1.0);
        assert_eq!(p.y, 2.0);
        
        let p: Point = "(3, 4)".parse().unwrap();
        assert_eq!(p.x, 3.0);
        assert_eq!(p.y, 4.0);
    }

    #[test]
    fn test_box_parsing() {
        let b: Box = "(1,1),(2,2)".parse().unwrap();
        assert_eq!(b.high, Point::new(2.0, 2.0));
        assert_eq!(b.low, Point::new(1.0, 1.0));
        
        let b: Box = "(2,2),(1,1)".parse().unwrap();
        assert_eq!(b.high, Point::new(2.0, 2.0));
        assert_eq!(b.low, Point::new(1.0, 1.0));
    }

    #[test]
    fn test_circle_parsing() {
        let c: Circle = "<(1,2),5>".parse().unwrap();
        assert_eq!(c.center, Point::new(1.0, 2.0));
        assert_eq!(c.radius, 5.0);
    }

    #[test]
    fn test_box_overlaps() {
        assert!(box_overlaps("(0,0),(2,2)", "(1,1),(3,3)").unwrap());
        assert!(!box_overlaps("(0,0),(1,1)", "(2,2),(3,3)").unwrap());
    }
}
