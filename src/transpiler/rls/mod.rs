//! Row-Level Security (RLS) augmentation for SQL transpilation.
//!
//! This module provides functionality for augmenting SQL queries with
//! Row-Level Security policies. It handles:
//!
//! - Query augmentation with RLS WHERE clauses
//! - Policy creation and management
//! - Role and privilege statement handling
//!
//! # Submodules
//!
//! - [`augment`] - RLS query augmentation logic for SELECT, INSERT, UPDATE, DELETE
//! - [`policy`] - Policy statement parsing (CREATE POLICY, DROP POLICY)
//! - [`utils`] - Role and privilege utilities (CREATE ROLE, GRANT, etc.)
//!
//! # Example
//!
//! ```rust
//! use pgqt::transpiler::rls::transpile_with_rls;
//! use pgqt::rls::RlsContext;
//! use rusqlite::Connection;
//!
//! // Transpile a query with RLS context
//! let sql = "SELECT * FROM documents";
//! let rls_context = RlsContext::new("alice");
//! // let result = transpile_with_rls(sql, &rls_context, &conn);
//! ```

pub mod augment;
pub mod policy;
pub mod utils;

pub use augment::transpile_with_rls;
