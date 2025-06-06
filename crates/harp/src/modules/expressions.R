#
# expressions.R
#
# Copyright (C) 2024 Posit Software, PBC. All rights reserved.
#
#

expr_deparse_collapse <- function(
    expr,
    width.cutoff = 500L,
    nlines = -1L,
    collapse = " "
) {
    # TODO: take inspiration from .rs.deparse() in rstudio
    deparsed <- deparse(
        expr,
        width.cutoff = width.cutoff,
        nlines = nlines
    )
    paste(deparsed, collapse = collapse)
}
