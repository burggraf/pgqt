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

pub mod augment;
pub mod policy;
pub mod utils;

pub use utils::{
    reconstruct_create_role_stmt,
    reconstruct_alter_role_stmt,
    reconstruct_alter_role_set_stmt,
    reconstruct_drop_role_stmt,
    reconstruct_grant_stmt,
    reconstruct_grant_role_stmt,
    reconstruct_alter_default_privileges_stmt,
    reconstruct_alter_owner_stmt,
};
