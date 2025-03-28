//
// composite.rs
//
// Copyright (C) 2023-2025 Posit Software, PBC. All rights reserved.
//
//

mod call;
mod document;
mod keyword;
pub(crate) mod pipe;
mod search_path;
mod snippets;
mod subset;
mod workspace;

use std::collections::HashSet;

use stdext::*;
use tower_lsp::lsp_types::CompletionItem;
use tower_lsp::lsp_types::CompletionItemKind;
use tree_sitter::Node;

use crate::lsp::completions::completion_context::CompletionContext;
use crate::lsp::completions::sources::push_completions;
use crate::treesitter::NodeType;
use crate::treesitter::NodeTypeExt;

/// Gets completions from all composite sources, with deduplication and sorting
pub(crate) fn get_completions(
    completion_context: &CompletionContext,
) -> anyhow::Result<Option<Vec<CompletionItem>>> {
    log::info!("Getting completions from composite sources");

    let mut completions: Vec<CompletionItem> = vec![];

    // Call, pipe, and subset completions should show up no matter what when
    // the user requests completions. This allows them to "tab" their way
    // through completions effectively without typing anything.

    // argument completions
    push_completions(call::CallSource, completion_context, &mut completions)?;

    // pipe completions, such as column names of a data frame
    push_completions(pipe::PipeSource, completion_context, &mut completions)?;

    // subset completions (`[` or `[[`)
    push_completions(subset::SubsetSource, completion_context, &mut completions)?;

    // For the rest of the general completions, we require an identifier to
    // begin showing anything.
    if is_identifier_like(completion_context.document_context.node) {
        // Consulted settings.json
        push_completions(keyword::KeywordSource, completion_context, &mut completions)?;

        push_completions(
            snippets::SnippetSource,
            completion_context,
            &mut completions,
        )?;

        push_completions(
            search_path::SearchPathSource,
            completion_context,
            &mut completions,
        )?;

        push_completions(
            document::DocumentSource,
            completion_context,
            &mut completions,
        )?;

        push_completions(
            workspace::WorkspaceSource,
            completion_context,
            &mut completions,
        )?;
    }

    // Remove duplicates
    let mut uniques = HashSet::new();
    completions.retain(|x| uniques.insert(x.label.clone()));

    // Sort completions by providing custom 'sort' text to be used when
    // ordering completion results. we use some placeholders at the front
    // to 'bin' different completion types differently; e.g. we place parameter
    // completions at the front, followed by variable completions (like pipe
    // completions and subset completions), followed by anything else.
    for item in &mut completions {
        // Start with existing `sort_text` if one exists
        let sort_text = item.sort_text.take();
        let sort_text = match sort_text {
            Some(sort_text) => sort_text,
            None => item.label.clone(),
        };

        case! {
            // Argument name
            item.kind == Some(CompletionItemKind::FIELD) => {
                item.sort_text = Some(join!["1-", sort_text]);
            }
            // Something like pipe completions, or data frame column names
            item.kind == Some(CompletionItemKind::VARIABLE) => {
                item.sort_text = Some(join!["2-", sort_text]);
            }
            // Package names generally have higher preference than function
            // names. Particularly useful for `dev|` to get to `devtools::`,
            // as that has a lot of base R functions with similar names.
            item.kind == Some(CompletionItemKind::MODULE) => {
                item.sort_text = Some(join!["3-", sort_text]);
            }
            => {
                item.sort_text = Some(join!["4-", sort_text]);
            }
        }
    }

    Ok(Some(completions))
}

fn is_identifier_like(x: Node) -> bool {
    if x.is_identifier() {
        // Obvious case
        return true;
    }

    // If the user exactly types these keywords, then they end up matching
    // anonymous nodes in the tree-sitter grammar, so they show up as
    // non-`identifier` kinds. However, we do still want to provide completions
    // here, especially in two cases:
    // - `for<tab>` should provide completions for things like `forcats`
    // - `for<tab>` should provide snippet completions for the `for` snippet
    // The keywords here come from matching snippets in `r.code-snippets`.
    if matches!(x.node_type(), NodeType::Anonymous(kind) if matches!(kind.as_str(), "if" | "for" | "while"))
    {
        return true;
    }

    return false;
}

#[cfg(test)]
mod tests {
    use tree_sitter::Point;

    use crate::lsp::completions::sources::composite::is_identifier_like;
    use crate::lsp::document_context::DocumentContext;
    use crate::lsp::documents::Document;
    use crate::r_task;
    use crate::treesitter::NodeType;
    use crate::treesitter::NodeTypeExt;

    #[test]
    fn test_completions_on_anonymous_node_keywords() {
        r_task(|| {
            // `if`, `for`, and `while` in particular are both tree-sitter
            // anonymous nodes and snippet keywords, so they need to look like
            // identifiers that we provide completions for
            for keyword in ["if", "for", "while"] {
                let point = Point { row: 0, column: 0 };
                let document = Document::new(keyword, None);
                let context = DocumentContext::new(&document, point, None);
                assert!(is_identifier_like(context.node));
                assert_eq!(
                    context.node.node_type(),
                    NodeType::Anonymous(keyword.to_string())
                );
            }
        })
    }
}
