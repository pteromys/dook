/// dump the structure of a `tree_sitter::Tree` to standard output.
pub fn dump_tree<I: AsRef<[u8]>, T: tree_sitter::TextProvider<I>>(
    tree: &tree_sitter::Tree,
    mut text_provider: T,
    use_color: bool,
) -> std::io::Result<()> {
    use std::io::Write;
    let mut stdout = std::io::stdout();
    let mut depth: usize = 0;
    let mut sibling_idx = std::vec::Vec::<usize>::new();
    let mut cursor = tree.walk();
    let color_end = if use_color { "\x1b[m" } else { "" };
    let color_paren = if use_color { "\x1b[1;37m" } else { "" };
    let color_nodekind = if use_color { "\x1b[1;34m" } else { "" };
    let color_fieldname = if use_color { "\x1b[0;36m" } else { "" };
    let color_eq = if use_color { "\x1b[1;33m" } else { "" };
    let color_literal = if use_color { "\x1b[0;32m" } else { "" };
    'treewalk: loop {
        let node = cursor.node();
        // indent
        write!(stdout, "{}", String::from(" ").repeat(depth))?;
        // parent's field name if it's there
        if let Some(parent) = node.parent() {
            if let Some(field_name) = parent
                .field_name_for_child(*sibling_idx.last().unwrap() as u32 /* mod 2**32 */)
            {
                write!(
                    stdout,
                    "{}{}{}:{} ",
                    color_fieldname, field_name, color_eq, color_end
                )?;
            }
        }
        if node.child_count() > 0 {
            writeln!(
                stdout,
                "{}({}{}{}",
                color_paren,
                color_nodekind,
                node.kind(),
                color_end
            )?;
        } else {
            let node_content = text_provider
                .text(node)
                .map(|t| String::from(std::str::from_utf8(t.as_ref()).unwrap()))
                .collect::<Vec<_>>()
                .concat();
            if node.is_named() {
                writeln!(
                    stdout,
                    "{}({}{}{} = {}{:?}{}){}",
                    color_paren,
                    color_nodekind,
                    node.kind(),
                    color_eq,
                    color_literal,
                    node_content,
                    color_paren,
                    color_end
                )?;
            } else {
                writeln!(stdout, "{}{:?}{}", color_literal, node_content, color_end)?;
            }
        }
        // depth first traversal
        if !cursor.goto_first_child() {
            while !cursor.goto_next_sibling() {
                writeln!(
                    stdout,
                    "{}{}){}",
                    String::from(" ").repeat(depth),
                    color_paren,
                    color_end
                )?;
                if !cursor.goto_parent() {
                    break 'treewalk;
                } else {
                    depth = depth.saturating_sub(1);
                    sibling_idx.pop();
                }
            }
            if let Some(last) = sibling_idx.last_mut() {
                *last += 1
            }
        } else {
            depth = depth.saturating_add(1);
            sibling_idx.push(0)
        }
    }
    Ok(())
}
