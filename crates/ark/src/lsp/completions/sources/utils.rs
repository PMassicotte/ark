//
// utils.rs
//
// Copyright (C) 2023-2025 Posit Software, PBC. All rights reserved.
//
//

use harp::error::Error;
use harp::eval::RParseEvalOptions;
use harp::exec::RFunction;
use harp::exec::RFunctionExt;
use harp::object::RObject;
use harp::utils::r_inherits;
use regex::Regex;
use tower_lsp::lsp_types::CompletionItem;
use tree_sitter::Node;
use tree_sitter::Point;

use crate::lsp::completions::completion_item::completion_item_from_data_variable;
use crate::lsp::document_context::DocumentContext;
use crate::lsp::traits::node::NodeExt;
use crate::lsp::traits::point::PointExt;
use crate::lsp::traits::rope::RopeExt;
use crate::treesitter::NodeType;
use crate::treesitter::NodeTypeExt;

pub(crate) fn has_priority_prefix(sort_text: &str) -> bool {
    // If we want to force a completion item to the top of the list,
    // we pre-emptively set its `sort_text` to begin with "0-".
    // We usually don't want to mess with such `sort_text` downstream.
    // Not sure it will remain as simple as "0-", hence this helper.
    sort_text.starts_with("0-")
}

pub(super) fn set_sort_text_by_first_appearance(completions: &mut Vec<CompletionItem>) {
    let size = completions.len();

    // Surely there's a better way to figure out what factor of 10 the `size`
    // fits in, but I can't think of it right now
    let mut width = 1;
    let mut value = 10;

    while size >= value {
        value = value * 10;
        width += 1;
    }

    for (i, item) in completions.iter_mut().enumerate() {
        // Start with existing `sort_text` if one exists
        let text = match &item.sort_text {
            Some(sort_text) => sort_text,
            None => &item.label,
        };
        // Append an integer left padded with `0`s.
        //
        // We don't check if pre-existing `sort_text` has any specific form,
        // because there's no known overlap between the use cases for sorting by
        // first appearance and wanting to hoist a specific completion item to
        // the top of the list.
        let prefix = format!("{:0width$}", i, width = width);
        let sort_text = format!("{prefix}-{text}");
        item.sort_text = Some(sort_text);
    }
}

pub(super) fn set_sort_text_by_words_first(completions: &mut Vec<CompletionItem>) {
    // `_` is considered a word character but we typically want those at the end so:
    // - First `^` for "starts with"
    // - Second `^` for "not the \W_"
    // - `\W_` for "non word characters plus `_`"
    // Result is "starts with any word character except `_`"
    let pattern = Regex::new(r"^[^\W_]").unwrap();

    for item in completions {
        // Start with existing `sort_text` if one exists
        let text = match &item.sort_text {
            Some(sort_text) => {
                // If it looks like the `sort_text` already has a prefix meant
                // to bring this item to the top, don't prepend another prefix.
                if has_priority_prefix(sort_text) {
                    continue;
                }
                sort_text
            },
            None => &item.label,
        };

        if pattern.is_match(text) {
            item.sort_text = Some(format!("1-{text}"));
        } else {
            item.sort_text = Some(format!("2-{text}"));
        }
    }
}

pub(super) fn filter_out_dot_prefixes(
    context: &DocumentContext,
    completions: &mut Vec<CompletionItem>,
) {
    // Remove completions that start with `.` unless the user explicitly requested them
    let user_requested_dot = context
        .document
        .contents
        .node_slice(&context.node)
        .and_then(|x| Ok(x.to_string().starts_with(".")))
        .unwrap_or(false);

    if !user_requested_dot {
        completions.retain(|x| !x.label.starts_with("."));
    }
}

#[derive(PartialEq, Debug)]
pub(super) enum CallNodePositionType {
    Name,
    Value,
    Ambiguous,
    Outside,
    Unknown,
}

pub(super) fn call_node_position_type(node: &Node, point: Point) -> CallNodePositionType {
    match node.node_type() {
        NodeType::Arguments => return CallNodePositionType::Name,
        NodeType::Anonymous(kind) if kind == "(" => {
            if point.is_before_or_equal(node.start_position()) {
                // Before the `(`
                return CallNodePositionType::Outside;
            } else {
                // Must be a name position
                return CallNodePositionType::Name;
            }
        },
        NodeType::Anonymous(kind) if kind == ")" => {
            if point.is_after_or_equal(node.end_position()) {
                // After the `)`
                return CallNodePositionType::Outside;
            } else {
                // Let previous leaf determine type (i.e. did the `)`
                // follow a `=` or a `,`?)
                return call_prev_leaf_position_type(&node, false);
            }
        },
        NodeType::Comma => return CallNodePositionType::Name,
        NodeType::Anonymous(kind) if kind == "=" => return CallNodePositionType::Value,
        // Like `fn(arg<tab>)` or `fn(x = 1, arg<tab>)` (which are ambiguous)
        // or `fn(x = arg<tab>)` (which is clearly a `Value`)
        NodeType::Identifier => return call_prev_leaf_position_type(&node, true),
        _ => {
            // Probably a complex node inside `()`. Typically a `Value`
            // unless we are at the very beginning of the node.

            // For things like `vctrs::vec_sort(x = 1, |2)` where you typed
            // the argument value but want to go back and fill in the name.
            if point == node.start_position() {
                return call_prev_leaf_position_type(&node, false);
            }

            return CallNodePositionType::Value;
        },
    }
}

fn call_prev_leaf_position_type(node: &Node, allow_ambiguous: bool) -> CallNodePositionType {
    let Some(previous) = node.prev_leaf() else {
        // We expect a previous leaf to exist anywhere we use this, so if it
        // doesn't exist then we return this marker type that tells us we should
        // probably investigate our heuristics.
        log::warn!(
            "Expected `node` to have a previous leaf. Is `call_node_position_type()` written correctly?"
        );
        return CallNodePositionType::Unknown;
    };

    let after_open_parenthesis_or_comma = if allow_ambiguous {
        // i.e. `fn(arg<tab>)` or `fn(x, arg<tab>)` where it can be
        // ambiguous whether we are on a `Name` or a `Value`.
        CallNodePositionType::Ambiguous
    } else {
        CallNodePositionType::Name
    };

    match previous.node_type() {
        NodeType::Comma => after_open_parenthesis_or_comma,
        NodeType::Anonymous(kind) if kind == "(" => after_open_parenthesis_or_comma,
        NodeType::Anonymous(kind) if kind == "=" => CallNodePositionType::Value,
        _ => CallNodePositionType::Value,
    }
}

pub(super) fn completions_from_evaluated_object_names(
    name: &str,
    enquote: bool,
    node_type: NodeType,
) -> anyhow::Result<Option<Vec<CompletionItem>>> {
    log::trace!("completions_from_evaluated_object_names({name:?})");

    let options = RParseEvalOptions {
        forbid_function_calls: true,
        ..Default::default()
    };

    // Try to evaluate the object
    let object = harp::parse_eval(name, options);

    // If we get an `UnsafeEvaluationError` here from setting
    // `forbid_function_calls`, we don't even log that one, as that is
    // expected to happen with complex inputs.
    // If we get a `TryCatchError`, that is typically an 'object not found' error resulting
    // from the user typing pseudocode. Log those at info level without a full backtrace.
    let object = match object {
        Ok(object) => object,
        Err(err) => match err {
            Error::UnsafeEvaluationError(_) => return Ok(None),
            Error::TryCatchError { message, .. } => {
                log::trace!("Can't evaluate object: {message}");
                return Ok(None);
            },
            _ => {
                log::error!("Can't evaluate object: {err}");
                return Ok(None);
            },
        },
    };

    let completions = if harp::utils::r_is_matrix(object.sexp) {
        // Special case just for 2D arrays
        completions_from_object_colnames(object, name, enquote)?
    } else if r_inherits(object.sexp, "data.table") {
        // The `[` method for data.table uses NSE so we don't enquote the names
        // https://github.com/posit-dev/positron/issues/3140
        let enquote = match node_type {
            NodeType::Subset => false,
            NodeType::Subset2 => true,
            _ => enquote,
        };
        completions_from_object_names(object, name, enquote)?
    } else {
        completions_from_object_names(object, name, enquote)?
    };

    Ok(Some(completions))
}

pub(super) fn completions_from_object_names(
    object: RObject,
    name: &str,
    enquote: bool,
) -> anyhow::Result<Vec<CompletionItem>> {
    completions_from_object_names_impl(object, name, enquote, "names")
}

pub(super) fn completions_from_object_colnames(
    object: RObject,
    name: &str,
    enquote: bool,
) -> anyhow::Result<Vec<CompletionItem>> {
    completions_from_object_names_impl(object, name, enquote, "colnames")
}

fn completions_from_object_names_impl(
    object: RObject,
    name: &str,
    enquote: bool,
    function: &str,
) -> anyhow::Result<Vec<CompletionItem>> {
    log::trace!("completions_from_object_names_impl({object:?})");

    let mut completions = vec![];

    unsafe {
        let element_names = RFunction::new("base", function)
            .add(object)
            .call()?
            .to::<Vec<String>>()?;

        for element_name in element_names {
            match completion_item_from_data_variable(&element_name, name, enquote) {
                Ok(item) => completions.push(item),
                Err(err) => log::error!("{err:?}"),
            }
        }
    }

    Ok(completions)
}

#[cfg(test)]
mod tests {
    use harp::eval::parse_eval_global;

    use crate::fixtures::package_is_installed;
    use crate::fixtures::point_from_cursor;
    use crate::lsp::completions::sources::utils::call_node_position_type;
    use crate::lsp::completions::sources::utils::completions_from_evaluated_object_names;
    use crate::lsp::completions::sources::utils::CallNodePositionType;
    use crate::lsp::document_context::DocumentContext;
    use crate::lsp::documents::Document;
    use crate::r_task;
    use crate::treesitter::NodeType;
    use crate::treesitter::NodeTypeExt;

    #[test]
    fn test_call_node_position_type() {
        // Before `(`, but on it
        let (text, point) = point_from_cursor("fn @()");
        let document = Document::new(text.as_str(), None);
        let context = DocumentContext::new(&document, point, None);
        assert_eq!(
            context.node.node_type(),
            NodeType::Anonymous(String::from("("))
        );
        assert_eq!(
            call_node_position_type(&context.node, context.point),
            CallNodePositionType::Outside
        );

        // After `)`, but on it
        let (text, point) = point_from_cursor("fn()@");
        let document = Document::new(text.as_str(), None);
        let context = DocumentContext::new(&document, point, None);
        assert_eq!(
            context.node.node_type(),
            NodeType::Anonymous(String::from(")"))
        );
        assert_eq!(
            call_node_position_type(&context.node, context.point),
            CallNodePositionType::Outside
        );

        // After `(`, but on it
        let (text, point) = point_from_cursor("fn(@)");
        let document = Document::new(text.as_str(), None);
        let context = DocumentContext::new(&document, point, None);
        assert_eq!(
            context.node.node_type(),
            NodeType::Anonymous(String::from("("))
        );
        assert_eq!(
            call_node_position_type(&context.node, context.point),
            CallNodePositionType::Name
        );

        // After `x`
        let (text, point) = point_from_cursor("fn(x@)");
        let document = Document::new(text.as_str(), None);
        let context = DocumentContext::new(&document, point, None);
        assert_eq!(
            call_node_position_type(&context.node, context.point),
            CallNodePositionType::Ambiguous
        );

        // After `x`
        let (text, point) = point_from_cursor("fn(1, x@)");
        let document = Document::new(text.as_str(), None);
        let context = DocumentContext::new(&document, point, None);
        assert_eq!(
            call_node_position_type(&context.node, context.point),
            CallNodePositionType::Ambiguous
        );

        // Directly after `,`
        let (text, point) = point_from_cursor("fn(x,@ )");
        let document = Document::new(text.as_str(), None);
        let context = DocumentContext::new(&document, point, None);
        assert_eq!(context.node.node_type(), NodeType::Comma);
        assert_eq!(
            call_node_position_type(&context.node, context.point),
            CallNodePositionType::Name
        );

        // After `,`, but on `)`
        let (text, point) = point_from_cursor("fn(x, @)");
        let document = Document::new(text.as_str(), None);
        let context = DocumentContext::new(&document, point, None);
        assert_eq!(
            context.node.node_type(),
            NodeType::Anonymous(String::from(")"))
        );
        assert_eq!(
            call_node_position_type(&context.node, context.point),
            CallNodePositionType::Name
        );

        // After `=`
        let (text, point) = point_from_cursor("fn(x =@ )");
        let document = Document::new(text.as_str(), None);
        let context = DocumentContext::new(&document, point, None);
        assert_eq!(
            context.node.node_type(),
            NodeType::Anonymous(String::from("="))
        );
        assert_eq!(
            call_node_position_type(&context.node, context.point),
            CallNodePositionType::Value
        );

        // In an expression
        let (text, point) = point_from_cursor("fn(1@ + 1)");
        let document = Document::new(text.as_str(), None);
        let context = DocumentContext::new(&document, point, None);
        assert_eq!(context.node.node_type(), NodeType::Float);
        assert_eq!(
            call_node_position_type(&context.node, context.point),
            CallNodePositionType::Value
        );

        let (text, point) = point_from_cursor("fn(1 + 1@)");
        let document = Document::new(text.as_str(), None);
        let context = DocumentContext::new(&document, point, None);
        assert_eq!(context.node.node_type(), NodeType::Float);
        assert_eq!(
            call_node_position_type(&context.node, context.point),
            CallNodePositionType::Value
        );

        // Right before an expression
        // (special case where we still provide argument completions)
        let (text, point) = point_from_cursor("fn(1, @1 + 1)");
        let document = Document::new(text.as_str(), None);
        let context = DocumentContext::new(&document, point, None);
        assert_eq!(context.node.node_type(), NodeType::Float);
        assert_eq!(
            call_node_position_type(&context.node, context.point),
            CallNodePositionType::Name
        );

        // After an identifier, before the `)`, with whitespace between them,
        // but on the `)`
        let (text, point) = point_from_cursor("fn(x @)");
        let document = Document::new(text.as_str(), None);
        let context = DocumentContext::new(&document, point, None);
        assert_eq!(
            context.node.node_type(),
            NodeType::Anonymous(String::from(")"))
        );
        assert_eq!(
            call_node_position_type(&context.node, context.point),
            CallNodePositionType::Value
        );

        // After an identifier, before the `)`, with whitespace between them,
        // but on the identifier
        let (text, point) = point_from_cursor("fn(x@ )");
        let document = Document::new(text.as_str(), None);
        let context = DocumentContext::new(&document, point, None);
        assert!(context.node.is_identifier());
        assert_eq!(
            call_node_position_type(&context.node, context.point),
            CallNodePositionType::Ambiguous
        );

        // After `(`, and on own line
        let (text, point) = point_from_cursor("fn(\n  @\n)");
        let document = Document::new(&text, None);
        let context = DocumentContext::new(&document, point, None);

        assert_eq!(context.node.node_type(), NodeType::Arguments);
        assert_eq!(
            call_node_position_type(&context.node, context.point),
            CallNodePositionType::Name
        );
    }

    #[test]
    fn test_completions_from_evaluated_object_names() {
        r_task(|| {
            // Vector with names
            parse_eval_global("x <- 1:2").unwrap();
            parse_eval_global("names(x) <- c('a', 'b')").unwrap();

            let completions = completions_from_evaluated_object_names("x", false, NodeType::Subset)
                .unwrap()
                .unwrap();
            assert_eq!(completions.len(), 2);
            assert_eq!(completions.get(0).unwrap().label, String::from("a"));
            assert_eq!(completions.get(1).unwrap().label, String::from("b"));

            parse_eval_global("remove(x)").unwrap();

            // Data frame
            parse_eval_global("x <- data.frame(a = 1, b = 2, c = 3)").unwrap();

            let completions = completions_from_evaluated_object_names("x", false, NodeType::Subset)
                .unwrap()
                .unwrap();
            assert_eq!(completions.len(), 3);
            assert_eq!(completions.get(0).unwrap().label, String::from("a"));
            assert_eq!(completions.get(1).unwrap().label, String::from("b"));
            assert_eq!(completions.get(2).unwrap().label, String::from("c"));

            parse_eval_global("remove(x)").unwrap();

            // 1D array with names
            parse_eval_global("x <- array(1:2)").unwrap();
            parse_eval_global("names(x) <- c('a', 'b')").unwrap();

            let completions = completions_from_evaluated_object_names("x", false, NodeType::Subset)
                .unwrap()
                .unwrap();
            assert_eq!(completions.len(), 2);
            assert_eq!(completions.get(0).unwrap().label, String::from("a"));
            assert_eq!(completions.get(1).unwrap().label, String::from("b"));

            parse_eval_global("remove(x)").unwrap();

            // Matrix with column names
            parse_eval_global("x <- array(1, dim = c(1, 1))").unwrap();
            parse_eval_global("rownames(x) <- 'a'").unwrap();
            parse_eval_global("colnames(x) <- 'b'").unwrap();

            let completions = completions_from_evaluated_object_names("x", false, NodeType::Subset)
                .unwrap()
                .unwrap();
            assert_eq!(completions.len(), 1);
            assert_eq!(completions.get(0).unwrap().label, String::from("b"));

            parse_eval_global("remove(x)").unwrap();

            // 3D array with column names
            // We currently decide not to return any names here. It is typically quite
            // ambiguous which axis's names you'd want when working with >=3D arrays.
            // But we did find an object, so we return an empty vector.
            parse_eval_global("x <- array(1, dim = c(1, 1, 1))").unwrap();
            parse_eval_global("rownames(x) <- 'a'").unwrap();
            parse_eval_global("colnames(x) <- 'b'").unwrap();

            let completions = completions_from_evaluated_object_names("x", false, NodeType::Subset)
                .unwrap()
                .unwrap();
            assert!(completions.is_empty());

            parse_eval_global("remove(x)").unwrap();
        })
    }

    #[test]
    fn test_data_table_completions() {
        r_task(|| {
            // Skip test if data.table is not installed
            if !package_is_installed("data.table") {
                return;
            }

            parse_eval_global("x <- data.table::as.data.table(mtcars)").unwrap();

            // Subset completions
            let completions = completions_from_evaluated_object_names("x", false, NodeType::Subset)
                .unwrap()
                .unwrap();

            assert_eq!(completions.len(), 11);
            assert_eq!(completions.get(0).unwrap().label, String::from("mpg"));
            assert_eq!(completions.get(0).unwrap().insert_text, None);

            // Subset2 completions
            let completions =
                completions_from_evaluated_object_names("x", false, NodeType::Subset2)
                    .unwrap()
                    .unwrap();

            assert_eq!(completions.len(), 11);
            assert_eq!(completions.get(0).unwrap().label, String::from("mpg"));
            assert_eq!(
                completions.get(0).unwrap().insert_text,
                Some("\"mpg\"".to_string())
            );

            parse_eval_global("remove(x)").unwrap();
        })
    }
}
