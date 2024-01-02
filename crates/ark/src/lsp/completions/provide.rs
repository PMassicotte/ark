//
// provide.rs
//
// Copyright (C) 2023 Posit Software, PBC. All rights reserved.
//
//

use anyhow::Result;
use tower_lsp::lsp_types::CompletionItem;

use crate::lsp::backend::Backend;
use crate::lsp::completions::sources::completions_from_composite_sources;
use crate::lsp::completions::sources::completions_from_unique_sources;
use crate::lsp::document_context::DocumentContext;

// Entry point for completions.
// Must be within an `r_task()`.
pub fn provide_completions(
    backend: &Backend,
    context: &DocumentContext,
) -> Result<Vec<CompletionItem>> {
    log::info!("provide_completions()");

    if let Some(completions) = completions_from_unique_sources(context)? {
        return Ok(completions);
    };

    // At this point we aren't in a "unique" completion case, so just return a
    // set of reasonable completions based on loaded packages, the open
    // document, the current workspace, and any call related arguments
    completions_from_composite_sources(backend, context)
}