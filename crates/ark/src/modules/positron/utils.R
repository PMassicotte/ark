#
# utils.R
#
# Copyright (C) 2022-2025 Posit Software, PBC. All rights reserved.
#
#

#' @export
.ps.inspect <- function(item) {
    .Internal(inspect(item))
}

#' @export
.ps.objectId <- function(object) {
    .ps.Call("ps_object_id", object)
}

#' @export
.ps.recursiveSearch <- function(object, callback, ...) {
    result <- callback(object, ...)
    if (!is.null(result)) {
        return(result)
    }

    if (is.recursive(object)) {
        for (i in seq_along(object)) {
            result <- .ps.recursiveSearch(object[[i]], callback, ...)
            if (!is.null(result)) {
                return(result)
            }
        }
    }
}

#' @export
.ps.ark.version <- function() {
    # Read the version information from Ark
    ark_version <- .ps.Call("ps_ark_version")

    # Format the date into the current timezone for display
    if (nzchar(ark_version['date'])) {
        utc_date <- as.POSIXct(
            ark_version['date'],
            format = "%Y-%m-%dT%H:%M:%SZ",
            tz = "UTC"
        )
        local_date <- format(
            utc_date,
            format = "%Y-%m-%d %H:%M:%S",
            usetz = TRUE,
            tz = Sys.timezone()
        )
        ark_version['date'] <- local_date
    }

    ark_version
}

# Sleep that doesn't check for interrupts to test an unresponsive runtime.
#' @export
.ps.deep_sleep <- function(secs) {
    .ps.Call("ps_deep_sleep", secs)
}

# Extracts a character label from a syntactically valid quoted R expression
as_label <- function(expr) {
    paste(deparse(expr, backtick = TRUE), collapse = "")
}

# Converts an R object to JSON (returned as a string)
#' @export
.ps.to_json <- function(object) {
    .ps.Call("ps_to_json", object)
}

# Evaluate expression in positron's namespace (which includes access to the
# private modules). Any features accessible from `.ps.internal()` are
# subject to change without notice.
#
# If `expr` is a function call, the function is retrieved from the namespace
# but the call (and thus arguments) are evaluated in the calling frame.
#' @export
.ps.internal <- function(expr) {
    expr <- substitute(expr)
    ns <- parent.env(environment())

    if (is.call(expr)) {
        # We evaluate function calls in two different
        # environments.

        # Fist retrieve function from internal namespace
        expr[[1]] <- eval(expr[[1]], ns)

        # Now evaluate call and arguments in calling frame
        eval(expr, parent.frame())
    } else {
        # Simple symbols (and literals) are evaluated
        # in the namespace
        eval(expr, ns)
    }
}

# Alias for the Ark namespace, useful for `.ps.internal(ark_ns)`
ark_ns <- environment()

# From `rlang::env_name()`
#' @export
.ps.env_name <- function(env) {
    if (typeof(env) != "environment") {
        return(NULL)
    }

    if (identical(env, globalenv())) {
        return("global")
    }
    if (identical(env, baseenv())) {
        return("package:base")
    }
    if (identical(env, emptyenv())) {
        return("empty")
    }

    nm <- environmentName(env)

    if (isNamespace(env)) {
        return(paste0("namespace:", nm))
    }

    if (nzchar(nm)) {
        nm
    } else {
        NULL
    }
}

node_poke_cdr <- function(node, cdr) {
    .ps.Call("ark_node_poke_cdr", node, cdr)
}

is_string <- function(x) {
    is.character(x) && length(x) == 1 && !is.na(x)
}

is_http_url <- function(x) {
    is_string(x) && grepl("^https?://", x)
}

obj_address <- function(x) {
    .ps.Call("ps_obj_address", x)
}

paste_line <- function(x) {
    paste(x, collapse = "\n")
}

set_names <- function(x, names = x) {
    names(x) <- x
    x
}

# From rlang
is_on_disk <- function(pkg) {
    system_path(pkg) != ""
}

system_path <- function(pkg) {
    # Important for this to be first because packages loaded with pkgload
    # will have a different path than the one in `.libPaths()` (if any).
    #
    # Note that this will not work for the base package, since we can't call
    # getNamespaceInfo on it.
    if (isNamespaceLoaded(pkg) && !identical(pkg, "base")) {
        return(.getNamespaceInfo(asNamespace(pkg), "path"))
    }

    for (path in file.path(.libPaths(), pkg)) {
        if (file.exists(path)) {
            return(path)
        }
    }

    ""
}
