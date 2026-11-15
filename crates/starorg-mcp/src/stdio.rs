//! The stdio transport: one JSON-RPC message per line on stdin, one answer per line on
//! stdout, nothing else on stdout (logs go to stderr). Ends at end of input.

use crate::protocol::{Handled, Server};
use std::io::{BufRead, Write};

/// Serve until `input` ends. `on_request` sees each handled request (for logs).
pub fn serve(
    server: &Server,
    input: impl BufRead,
    mut output: impl Write,
    mut on_request: impl FnMut(&Handled),
) -> std::io::Result<()> {
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let mut log = Vec::new();
        let answer = server.handle_text(&line, &mut log);
        for h in &log {
            on_request(h);
        }
        if let Some(a) = answer {
            // serde_json never puts a raw newline inside a JSON text.
            writeln!(output, "{a}")?;
            output.flush()?;
        }
    }
    Ok(())
}
