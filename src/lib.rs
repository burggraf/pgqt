//! PGQT — PostgreSQL wire-compatible proxy for SQLite
//!
//! This crate provides the library components for PGQT, a middleware server that
//! translates the PostgreSQL wire protocol into SQLite operations.
//!
//! ## Module Overview
//!
//! | Module         | Description                                            |
//! |---------------|--------------------------------------------------------|
//! | [`array`]      | PostgreSQL array functions and operators               |
//! | [`catalog`]    | Shadow catalog (`__pg_meta__`) for type metadata       |
//! | [`copy`]       | `COPY FROM/TO` command support                         |
//! | [`distinct_on`]| `DISTINCT ON` polyfill using window functions          |
//! | [`fts`]        | Full-text search (FTS5-backed)                         |
//! | [`functions`]  | User-defined function (UDF) execution                  |
//! | [`geo`]        | 2D geometric type support                              |
//! | [`plpgsql`]    | PL/pgSQL parser and Lua transpiler                     |
//! | [`range`]      | PostgreSQL range type support                          |
//! | [`rls`]        | Row-Level Security (RLS) via WHERE clause injection    |
//! | [`rls_inject`] | RLS AST injection utilities                            |
//! | [`schema`]     | Schema/namespace support via SQLite ATTACH DATABASE    |
//! | [`transpiler`] | Core SQL transpilation (PostgreSQL → SQLite)           |
//! | [`vector`]     | pgvector-compatible vector similarity search           |

pub mod debug;
pub mod validation;
pub mod array;
pub mod catalog;
pub mod connection_pool;
pub mod copy;
pub mod distinct_on;
pub mod fts;
pub mod functions;
pub mod geo;
pub mod handler;
pub mod plpgsql;
pub mod range;
pub mod rls;
pub mod rls_inject;
pub mod schema;
pub mod rbac;
pub mod stats;
pub mod transpiler;
pub mod vector;
pub mod auth;
