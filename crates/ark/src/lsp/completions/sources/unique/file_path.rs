//
// file_path.rs
//
// Copyright (C) 2023-2025 Posit Software, PBC. All rights reserved.
//
//

use std::env::current_dir;
use std::path::PathBuf;

use harp::object::RObject;
use harp::string::r_string_decode;
use harp::utils::r_normalize_path;
use stdext::unwrap;
use stdext::IntoResult;
use tower_lsp::lsp_types::CompletionItem;
use tree_sitter::Node;

use crate::lsp::completions::completion_item::completion_item_from_direntry;
use crate::lsp::completions::sources::utils::set_sort_text_by_words_first;
use crate::lsp::document_context::DocumentContext;
use crate::lsp::traits::rope::RopeExt;

pub(super) fn completions_from_string_file_path(
    node: &Node,
    context: &DocumentContext,
) -> anyhow::Result<Vec<CompletionItem>> {
    log::trace!("completions_from_string_file_path()");

    let mut completions: Vec<CompletionItem> = vec![];

    // Get the contents of the string token.
    //
    // NOTE: This includes the quotation characters on the string, and so
    // also includes any internal escapes! We need to decode the R string
    // before searching the path entries.
    let token = context.document.contents.node_slice(&node)?.to_string();
    let contents = unsafe { r_string_decode(token.as_str()).into_result()? };
    log::trace!("String value (decoded): {}", contents);

    // Use R to normalize the path.
    let path = r_normalize_path(RObject::from(contents))?;

    // parse the file path and get the directory component
    let mut path = PathBuf::from(path.as_str());
    log::trace!("Normalized path: {}", path.display());

    // if this path doesn't have a root, add it on
    if !path.has_root() {
        let root = current_dir()?;
        path = root.join(path);
    }

    // if this isn't a directory, get the parent path
    if !path.is_dir() {
        if let Some(parent) = path.parent() {
            path = parent.to_path_buf();
        }
    }

    // look for files in this directory
    log::trace!("Reading directory: {}", path.display());
    let entries = std::fs::read_dir(path)?;

    for entry in entries.into_iter() {
        let entry = unwrap!(entry, Err(error) => {
            log::error!("{}", error);
            continue;
        });

        let item = unwrap!(completion_item_from_direntry(entry), Err(error) => {
            log::error!("{}", error);
            continue;
        });

        completions.push(item);
    }

    // Push path completions starting with non-word characters to the bottom of
    // the sort list (like those starting with `.`)
    set_sort_text_by_words_first(&mut completions);

    Ok(completions)
}
