//! Transport abstractions for MCP communication.
//!
//! Currently supports:
//! - STDIO: subprocess communication via stdin/stdout (local sensory tools)
//!
//! Future:
//! - Streamable HTTP: for remote sensory tools and multi-client servers

// Transport is handled by rmcp's built-in transport types:
// - rmcp::transport::TokioChildProcess for STDIO subprocess
// - rmcp::transport::stdio() for serving on stdin/stdout
//
// This module exists as a place for any custom transport adapters
// we may need in the future.
