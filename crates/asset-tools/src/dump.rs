//! Annotated hexadecimal dumps.

use std::fmt::Write;

use formats_rw::rw::{self, RwError, id};

/// Bytes per line of hexadecimal output.
const LINE_WIDTH: usize = 16;

/// Classic hex dump: offset, bytes, printable characters. `base` is the
/// offset of `bytes[0]` in the file.
pub fn hex(bytes: &[u8], base: usize, indent: &str) -> String {
    let mut out = String::new();
    for (line, chunk) in bytes.chunks(LINE_WIDTH).enumerate() {
        let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02x}")).collect();
        let text: String = chunk
            .iter()
            .map(|&b| {
                if b.is_ascii_graphic() || b == b' ' {
                    char::from(b)
                } else {
                    '.'
                }
            })
            .collect();
        let _ = writeln!(
            out,
            "{indent}{:08x}  {:<width$}  {text}",
            base + line * LINE_WIDTH,
            hex.join(" "),
            width = LINE_WIDTH * 3 - 1
        );
    }
    out
}

/// Whether `bytes` starts with a chunk header of a known RenderWare type.
pub fn looks_like_renderware(bytes: &[u8]) -> bool {
    rw::root_chunk(bytes).is_ok_and(|root| rw::chunk_name(root.kind()).is_some())
}

/// Tree of the chunks of a RenderWare stream. The payload of leaf chunks is
/// shown in hexadecimal, `max_bytes` at most per chunk (0: everything).
pub fn renderware_tree(bytes: &[u8], max_bytes: usize) -> Result<String, RwError> {
    let tree = rw::walk(bytes)?;
    let root = tree[0].chunk;
    let root_end = root.data_offset() + root.data.len();
    let mut out = format!(
        "Flux RenderWare {} : {root_end} octets, puis {} octets de remplissage\n",
        root.version(),
        bytes.len() - root_end
    );
    for entry in tree {
        let chunk = entry.chunk;
        let indent = "  ".repeat(entry.depth);
        let name = rw::chunk_name(chunk.kind()).unwrap_or("?");
        let _ = write!(
            out,
            "{:08x}  {indent}{name} ({:#x}), {} octets",
            chunk.offset,
            chunk.kind(),
            chunk.data.len()
        );
        if chunk.version() != root.version() {
            let _ = write!(out, ", version {}", chunk.version());
        }
        if matches!(chunk.kind(), id::STRING | id::NODE_NAME) {
            let text: String = chunk
                .data
                .iter()
                .take_while(|&&b| b != 0)
                .map(|&b| char::from(b))
                .collect();
            let _ = write!(out, " « {text} »");
        }
        out.push('\n');
        if !rw::is_container(chunk.kind()) && !chunk.data.is_empty() {
            let shown = if max_bytes == 0 {
                chunk.data.len()
            } else {
                chunk.data.len().min(max_bytes)
            };
            out.push_str(&hex(
                &chunk.data[..shown],
                chunk.data_offset(),
                &format!("{indent}            "),
            ));
            if shown < chunk.data.len() {
                let _ = writeln!(
                    out,
                    "{indent}            … {} octets de plus",
                    chunk.data.len() - shown
                );
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_dump_shows_offsets_bytes_and_text() {
        let dump = hex(b"Root\x00\x01", 0x20, "");
        assert_eq!(
            dump,
            format!("00000020  52 6f 6f 74 00 01{}  Root..\n", " ".repeat(30))
        );
    }

    #[test]
    fn renderware_tree_lists_chunks_with_names() {
        // Clump > (Struct of 4 bytes, Extension > Node Name "Root").
        let mut bytes = Vec::new();
        for value in [0x10u32, 44, 0x1003_FFFF, 1, 4, 0x1003_FFFF, 7] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        for value in [3u32, 16, 0x1003_FFFF, 0x0253_F2FE, 4, 0x1003_FFFF] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(b"Root");
        bytes.extend([0; 8]);

        assert!(looks_like_renderware(&bytes));
        let tree = renderware_tree(&bytes, 0).unwrap();
        assert!(tree.starts_with("Flux RenderWare 3.4.0.3 : 56 octets, puis 8 octets"));
        assert!(tree.contains("00000000  Clump (0x10), 44 octets\n"));
        assert!(tree.contains("0000000c    Struct (0x1), 4 octets\n"));
        assert!(tree.contains("Node Name (0x253f2fe), 4 octets « Root »"));
        assert!(!looks_like_renderware(b"not a chunk at all"));
    }
}
