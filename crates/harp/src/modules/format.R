#
# format.R
#
# Copyright (C) 2023 Posit Software, PBC. All rights reserved.
#
#

# Works around possibly unconforming methods in `base::format()`. Tries
# hard to recover from failed assumptions, including by unclassing and
# reformatting with the default method.
harp_format_vec <- function(x, ...) {
    if (is.object(x)) {
        format_oo(x, ...)
    } else {
        base::format(x, ...)
    }
}

format_oo <- function(x, ...) {
    out <- base::format(x, ...)

    if (!is.character(out)) {
        log_trace(sprintf(
            "`format()` method for <%s> should return a character vector.",
            class_collapsed(x)
        ))
        return(format_fallback(x, ...))
    }

    if (length(x) != length(out)) {
        log_trace(sprintf(
            "`format()` method for <%s> should return the same number of elements.",
            class_collapsed(x)
        ))
        return(format_fallback(x, ...))
    }

    # Try to recover if dimensions don't agree (for example `format.Surv()`
    # doesn't preserve dimensions, see https://github.com/posit-dev/positron/issues/1862)
    if (!identical(dim(x), dim(out))) {
        log_trace(sprintf(
            "`format()` method for <%s> should return conforming dimensions.",
            class_collapsed(x)
        ))

        dim(out) <- dim(x)
    }

    out
}

# Try without dispatch
format_fallback <- function(x, ...) {
    out <- base::format(unclass(x), ...)

    # Shouldn't happen but just in case
    if (!is.character(out)) {
        stop("Unexpected type from `base::format()`.")
    }
    if (length(x) != length(out)) {
        stop("Unexpected length from `base::format()`.")
    }
    if (!identical(dim(x), dim(out))) {
        stop("Unexpected dimensions from `base::format()`.")
    }

    out
}

# Register unconforming methods for tests
init_test_format <- function() {
    format.test_unconforming_dims <- function(x) as.character(x)
    .S3method("format", "test_unconforming_dims", format.test_unconforming_dims)

    format.test_unconforming_type <- function(x) as.double(x)
    .S3method("format", "test_unconforming_type", format.test_unconforming_type)

    format.test_unconforming_length <- function(x) as.character(x)[-1]
    .S3method(
        "format",
        "test_unconforming_length",
        format.test_unconforming_length
    )

    unconforming_dims <- matrix(1:4, 2)
    class(unconforming_dims) <- "test_unconforming_dims"

    unconforming_type <- structure(1:2, class = "test_unconforming_type")
    unconforming_length <- structure(1:2, class = "test_unconforming_length")

    environment()
}

harp_format_s4 <- function(x, ...) {
    # For S4 values we assume that the formatted value is a character vector of length 1,
    # even if the value has length > 1. This is because the formatted value is typically
    # the class of the object, which is a single string.
    # If for some reason the formatted value is not a character vector of length 1, we
    # check if the result is a character vector the same length as `value`.
    out <- base::format(x, ...)

    if (length(out) != 1 && length(out) != length(x)) {
        log_trace(sprintf(
            "`format()` method for <%s> should return a character vector of length 1 or the same length as the object.",
            class_collapsed(x)
        ))
        return(format_fallback_s4(x))
    }

    if (!is.character(out)) {
        log_trace(sprintf(
            "`format()` method for <%s> should return a character vector.",
            class_collapsed(x)
        ))
        return(format_fallback_s4(x))
    }

    out
}

format_fallback_s4 <- function(x) {
    paste0("<S4 class '", class_collapsed(x), "'>")
}
