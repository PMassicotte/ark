// 
// completions.rs
// 
// Copyright (C) 2022 by RStudio, PBC
// 
// 

use std::collections::HashSet;

use tower_lsp::lsp_types::CompletionItem;
use tower_lsp::lsp_types::CompletionParams;
use tree_sitter::Node;
use tree_sitter::Point;

use crate::lsp::document::Document;
use crate::lsp::logger::log_push;
use crate::lsp::macros::expect;
use crate::lsp::macros::unwrap;
use crate::lsp::traits::cursor::TreeCursorExt;
use crate::lsp::traits::node::NodeExt;
use crate::lsp::traits::point::PointExt;
use crate::lsp::traits::position::PositionExt;

fn completion_from_identifier(node: &Node, source: &str) -> CompletionItem {
    let label = node.utf8_text(source.as_bytes()).expect("empty assignee");
    let detail = format!("Defined on line {}", node.start_position().row + 1);
    CompletionItem::new_simple(label.to_string(), detail)
}

struct CompletionData {
    source: String,
    position: Point,
    visited: HashSet<usize>,
}

fn call_uses_nse(node: &Node, data: &CompletionData) -> bool {

    // get the callee
    let lhs = unwrap!(node.child(0), {
        return false;
    });

    // validate we have an identifier or a string
    match lhs.kind() {
        "identifier" | "string" => {},
        _ => { return false; }
    }

    // check for a function whose evaluation occurs in a local scope
    let value = expect!(lhs.utf8_text(data.source.as_bytes()), {
        return false;
    });

    match value {
        "expression" | "local" | "quote" | "enquote" | "substitute" | "with" | "within" => { return true; },
        _ => { return false; }
    }

}

fn append_defined_variables(node: &Node, data: &mut CompletionData, completions: &mut Vec<CompletionItem>) {

    log_push!("append_defined_variables(): Dumping AST. {}", node.dump(data.source.as_str()));
    let mut cursor = node.walk();
    cursor.recurse(|node| {

        // skip nodes that exist beyond the completion position
        if node.start_position().is_after(data.position) {
            log_push!("append_defined_variables(): Halting recursion after point {}.", data.position);
            return false;
        }

        // skip nodes that were already visited
        if data.visited.contains(&node.id()) {
            return false;
        }

        log_push!("append_defined_variables(): {:#?}", node);
        match node.kind() {

            "left_assignment" | "super_assignment" | "equals_assignment" => {

                // TODO: Should we de-quote symbols and strings, or insert them as-is?
                let assignee = node.child(0).unwrap();
                if assignee.kind() == "identifier" || assignee.kind() == "string" {
                    completions.push(completion_from_identifier(&assignee, &data.source));
                }

                // return true in case we have nested assignments
                return true;

            }

            "right_assignment" | "super_right_assignment" => {

                // return true for nested assignments
                return true;

            }

            "call" => {

                // don't recurse into calls for certain functions
                return !call_uses_nse(&node, &data);

            }

            "function_definition" => {

                // don't recurse into function definitions, as these create as new scope
                // for variable definitions (and so such definitions are no longer visible)
                log_push!("append_defined_variables(): Halting recursion (found 'function_definition').");
                return false;

            }

            _ => {
                return true;
            }

        }

    });

}

fn append_function_parameters(node: &Node, data: &mut CompletionData, completions: &mut Vec<CompletionItem>) {

    let mut cursor = node.walk();
    
    if !cursor.goto_first_child() {
        log_push!("append_function_completions(): goto_first_child() failed");
        return;
    }

    if !cursor.goto_next_sibling() {
        log_push!("append_function_completions(): goto_next_sibling() failed");
        return;
    }

    let kind = cursor.node().kind();
    if kind != "formal_parameters" {
        log_push!("append_function_completions(): unexpected node kind {}", kind);
        return;
    }

    if !cursor.goto_first_child() {
        log_push!("append_function_completions(): goto_first_child() failed");
        return;
    }

    // The R tree-sitter grammar doesn't parse an R function's formals list into
    // a tree; instead, it's just held as a sequence of tokens. that said, the
    // only way an identifier could / should show up here is if it is indeed a
    // function parameter, so just search direct children here for identifiers.
    while cursor.goto_next_sibling() {
        let node = cursor.node();
        if node.kind() == "identifier" {
            completions.push(completion_from_identifier(&node, data.source.as_str()));
        }
    }

}


pub(crate) fn append_document_completions(document: &mut Document, params: &CompletionParams, completions: &mut Vec<CompletionItem>) {

    // get reference to AST
    let ast = unwrap!(&mut document.ast, {
        log_push!("append_completions(): No AST available.");
        return;
    });

    // try to find child for point
    let point = params.text_document_position.position.as_point();
    let mut node = unwrap!(ast.root_node().descendant_for_point_range(point, point), {
        log_push!("append_completions(): Couldn't find node for point {}", point);
        return;
    });

    // build completion data
    let mut data = CompletionData {
        source: document.contents.to_string(),
        position: point,
        visited: HashSet::new(),
    };

    log_push!("append_completions(): Found node {:?} at [{}, {}]", node, point.row, point.column);
    loop {

        // If this is a brace list, or the document root, recurse to find identifiers.
        if node.kind() == "brace_list" || node.parent() == None {
            log_push!("append_defined_variables(): Entering scope. ({:?})", node);
            append_defined_variables(&node, &mut data, completions);
        }

        // If this is a function definition, add parameter names.
        if node.kind() == "function_definition" {
            log_push!("append_defined_variables(): Adding function parameters. ({:?})", node);
            append_function_parameters(&node, &mut data, completions);
        }

        // Mark this node as visited.
        data.visited.insert(node.id());

        // Keep going.
        node = match node.parent() {
            Some(node) => node,
            None => break,
        };

    }

}