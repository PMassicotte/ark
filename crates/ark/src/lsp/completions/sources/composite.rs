//
// composite.rs
//
// Copyright (C) 2023 Posit Software, PBC. All rights reserved.
//
//

mod call;
mod document;
mod keyword;
mod pipe;
mod search_path;
mod snippets;
mod subset;
mod workspace;

use std::collections::HashSet;

use anyhow::Result;
use call::completions_from_call;
use document::completions_from_document;
use keyword::completions_from_keywords;
use pipe::completions_from_pipe;
use pipe::find_pipe_root;
use search_path::completions_from_search_path;
use snippets::completions_from_snippets;
use stdext::*;
use subset::completions_from_subset;
use tower_lsp::lsp_types::CompletionItem;
use tower_lsp::lsp_types::CompletionItemKind;
use workspace::completions_from_workspace;

use crate::lsp::backend::Backend;
use crate::lsp::document_context::DocumentContext;

pub fn completions_from_composite_sources(
    backend: &Backend,
    context: &DocumentContext,
) -> Result<Vec<CompletionItem>> {
    log::info!("completions_from_composite_sources()");

    let mut completions: Vec<CompletionItem> = vec![];

    let root = find_pipe_root(context);

    // Try argument completions
    if let Some(mut additional_completions) = completions_from_call(context, root.clone())? {
        completions.append(&mut additional_completions);
    }

    // Try pipe completions
    if let Some(mut additional_completions) = completions_from_pipe(root.clone())? {
        completions.append(&mut additional_completions);
    }

    // Try subset completions (`[` or `[[`)
    if let Some(mut additional_completions) = completions_from_subset(context)? {
        completions.append(&mut additional_completions);
    }

    // Call, pipe, and subset completions should show up no matter what when
    // the user requests completions (this allows them to Tab their way through
    // completions effectively without typing anything). For the rest of the
    // general completions, we require an identifier to begin showing
    // anything.
    if context.node.kind() == "identifier" {
        completions.append(&mut completions_from_keywords());
        completions.append(&mut completions_from_snippets());
        completions.append(&mut completions_from_search_path(context)?);

        if let Some(mut additional_completions) = completions_from_document(context)? {
            completions.append(&mut additional_completions);
        }

        if let Some(mut additional_completions) = completions_from_workspace(backend, context)? {
            completions.append(&mut additional_completions);
        }
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

    Ok(completions)
}