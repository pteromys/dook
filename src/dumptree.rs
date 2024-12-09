/// dump the structure of a `tree_sitter::Tree` to standard output.
pub fn dump_tree<'tree, T: tree_sitter::TextProvider<'tree>>(
    tree: &tree_sitter::Tree,
    mut text_provider: T,
    use_color: bool,
) {
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
        print!("{}", String::from(" ").repeat(depth));
        // parent's field name if it's there
        if let Some(parent) = node.parent() {
            if let Some(field_name) = parent
                .field_name_for_child(*sibling_idx.last().unwrap() as u32 /* mod 2**32 */)
            {
                print!(
                    "{}{}{}:{} ",
                    color_fieldname, field_name, color_eq, color_end
                );
            }
        }
        if node.child_count() > 0 {
            println!(
                "{}({}{}{}",
                color_paren,
                color_nodekind,
                node.kind(),
                color_end
            );
        } else {
            let node_content = String::from_utf8(
                text_provider
                    .text(node)
                    .collect::<std::vec::Vec<&[u8]>>()
                    .concat(),
            )
            .unwrap();
            if node.is_named() {
                println!(
                    "{}({}{}{} = {}{:?}{}){}",
                    color_paren,
                    color_nodekind,
                    node.kind(),
                    color_eq,
                    color_literal,
                    node_content,
                    color_paren,
                    color_end
                );
            } else {
                println!("{}{:?}{}", color_literal, node_content, color_end);
            }
        }
        // depth first traversal
        if !cursor.goto_first_child() {
            while !cursor.goto_next_sibling() {
                println!(
                    "{}{}){}",
                    String::from(" ").repeat(depth),
                    color_paren,
                    color_end
                );
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
}
