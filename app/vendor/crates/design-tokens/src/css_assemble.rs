// CSS text assembly (item C): turns structured (name, value) pairs, baked
// by build.rs from the resolved token values, into the final
// `:root{...}` / `<dark-selector>{...}` stylesheet text at CLI runtime,
// where the `--prefix`/`--dark-selector` flags are known. The prefix is
// spliced into each declaration NAME while the line is being built, never
// by a string-replace over already-rendered CSS text -- a value can never
// gain a prefix by accident, however many "--" or digit sequences it
// contains (Idea 1's risk note, plan 094 item C). Self-contained (no
// `crate::` imports) so it can be a normal public module, callable from
// both `main.rs` and integration tests.

/// Every declaration line, `--{prefix}{name}: {value};`, for one block,
/// in the given order (build.rs bakes them in the schema's field order).
fn push_declarations(out: &mut String, pairs: &[(&str, &str)], prefix: &str) {
    for (name, value) in pairs {
        out.push_str(&format!("  --{prefix}{name}: {value};\n"));
    }
}

/// Full `:root{...}` `<dark_selector>{...}` stylesheet text. Always emits
/// `color-scheme: light`/`color-scheme: dark` (a real CSS property, never
/// prefixed) as the first line of each block.
pub fn assemble_stylesheet(
    light_pairs: &[(&str, &str)],
    dark_pairs: &[(&str, &str)],
    prefix: &str,
    dark_selector: &str,
) -> String {
    let mut root = String::from("  color-scheme: light;\n");
    push_declarations(&mut root, light_pairs, prefix);

    let mut dark_block = String::from("  color-scheme: dark;\n");
    push_declarations(&mut dark_block, dark_pairs, prefix);

    format!(":root {{\n{root}}}\n{dark_selector} {{\n{dark_block}}}\n")
}
