/// dump the structure of a `tree_sitter::Tree` to standard output.
pub fn dump_tree<'tree, T: tree_sitter::TextProvider<'tree>>(
    tree: &tree_sitter::Tree,
    mut text_provider: T,
) {
    let mut depth: usize = 0;
    let mut sibling_idx = std::vec::Vec::<usize>::new();
    let mut cursor = tree.walk();
    'treewalk: loop {
        // collect ranges to print
        let node = cursor.node();
        let field_name = match node.parent() {
            Some(parent) => parent
                .field_name_for_child(*sibling_idx.last().unwrap() as u32 /* mod 2**32 */)
                .unwrap_or(""),
            None => "",
        };
        if node.child_count() > 0 {
            println!(
                "{}{}: {}",
                String::from(" ").repeat(depth),
                field_name,
                node.kind()
            );
        } else {
            println!(
                "{}{}: {} = \"{}\"",
                String::from(" ").repeat(depth),
                field_name,
                node.kind(),
                std::str::from_utf8(
                    text_provider
                        .text(node)
                        .collect::<std::vec::Vec<&[u8]>>()
                        .concat()
                        .as_slice()
                )
                .unwrap()
            );
        }
        // depth first traversal
        if !cursor.goto_first_child() {
            while !cursor.goto_next_sibling() {
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
