#' @export
.ps.reticulate_open <- function(input = "") {
    .ps.Call("ps_reticulate_open", input)
}

#' Called by the front-end right before starting the reticulate session.
#'
#' At this point it should be fine to load Python if it's not loaded, and
#' check if it can be started and if necessary packages are installed.
#' @export
.ps.rpc.reticulate_check_prerequisites <- function() {
    # This should return a list with the following fields:
    # python: NULL or string
    # venv: NULL or string
    # ipykernel: NULL or string
    # error: NULL or string

    config <- tryCatch(
        {
            reticulate::py_discover_config()
        },
        error = function(err) {
            err
        }
    )

    if (inherits(config, "error")) {
        # py_discover_config() can fail if the user forced a Python session
        # via RETICULATE_PYTHON, but this version doesn't exist.
        return(list(error = conditionMessage(config)))
    }

    # Starting with reticulate v1.41, `py_discover_config()` is NULL if reticulate
    # didn't find a forced python environment and will eventually use `uv` to
    # install and manage environments.
    version <- utils::packageVersion("reticulate")
    if (version <= "1.40.0" && (is.null(config) || is.null(config$python))) {
        # The front-end will offer to install Python.
        return(list(python = NULL, error = NULL))
    }

    # Check that python can be loaded, if it can't we will throw
    # an error, which is unrecoverable.
    config <- tryCatch(
        {
            # With reticulate >= v1.41.0, (if the previous config was NULL) this will trigger
            # an installation of `uv` and the creation of a temporary virtual environment for
            # the current session.
            # This may take a while, if `uv` has a lot of work to do (like downloading a bunch of packages).
            # Positron UI will display a progress bar and handle the timeout.
            reticulate::py_config()
        },
        error = function(err) {
            err
        }
    )

    python <- config$python
    venv <- config$virtualenv

    if (inherits(config, "error")) {
        return(list(
            python = python,
            venv = venv,
            error = conditionMessage(config)
        ))
    }

    # Now check ipykernel
    ipykernel <- tryCatch(
        {
            reticulate::py_module_available("ipykernel")
        },
        error = function(err) {
            err
        }
    )

    if (inherits(ipykernel, "error")) {
        return(list(
            python = python,
            venv = venv,
            error = conditionMessage(ipykernel)
        ))
    }

    list(
        python = config$python,
        venv = venv,
        ipykernel = ipykernel,
        error = NULL
    )
}

#' @export
.ps.rpc.reticulate_start_kernel <- function(
    kernelPath,
    connectionFile,
    logFile,
    logLevel
) {
    # Starts an IPykernel in a separate thread with information provided by
    # the caller.
    # It it's essentially executing the kernel startup script:
    # https://github.com/posit-dev/positron/blob/main/extensions/positron-python/python_files/positron/positron_language_server.py
    # and passing the communication files that Positron Jupyter's Adapter sets up.
    tryCatch(
        {
            reticulate:::py_run_file_on_thread(
                file = kernelPath,
                args = c(
                    "-f",
                    connectionFile,
                    "--logfile",
                    logFile,
                    "--loglevel",
                    logLevel,
                    "--session-mode",
                    "console"
                )
            )
            # Empty string means that no error happened.
            ""
        },
        error = function(err) {
            conditionMessage(err)
        }
    )
}

#' @export
.ps.rpc.reticulate_id <- function() {
    .ps.Call("ps_reticulate_id")
}
