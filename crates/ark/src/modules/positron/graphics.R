#
# graphics.R
#
# Copyright (C) 2022-2025 by Posit Software, PBC
#
#

# Set up "before plot new" hooks. This is our cue for
# saving up the state of a plot before it gets wiped out.
setHook("before.plot.new", action = "replace", function(...) {
    .ps.Call("ps_graphics_before_plot_new", "before.plot.new")
})
setHook("before.grid.newpage", action = "replace", function(...) {
    .ps.Call("ps_graphics_before_plot_new", "before.grid.newpage")
})

# A persistent list mapping plot `id`s to their display list recording.
# Used for replaying recordings under a new device or new width/height/resolution.
RECORDINGS <- list()

# Retrieves a recording by its `id`
#
# Returns `NULL` if no recording exists
get_recording <- function(id) {
    RECORDINGS[[id]]
}

add_recording <- function(id, recording) {
    RECORDINGS[[id]] <<- recording
}

# Called when a plot comm is closed by the frontend
remove_recording <- function(id) {
    RECORDINGS[[id]] <<- NULL
}

render_directory <- function() {
    directory <- file.path(tempdir(), "positron-plot-renderings")
    ensure_directory(directory)
    directory
}

render_path <- function(id) {
    directory <- render_directory()
    file <- paste0("render-", id, ".png")
    file.path(directory, file)
}

#' @export
.ps.graphics.create_device <- function() {
    name <- "Ark Graphics Device"

    # TODO: Remove the "shadow" device in favor of implementing our own
    # minimal graphics device like {devoid}. That would allow us to remove
    # all of the awkwardness here around:
    # - A `filename` that we never look at
    # - A `res` that isn't scaled by `pixel_ratio`
    # - The fact that the `png` device is forcing double the work to happen,
    #   as it is drawing graphics that we never look at.
    # - The fact that `locator()` doesn't work b/c `png` doesn't support it.
    directory <- render_directory()
    filename <- file.path(directory, "current-plot.png")
    type <- default_device_type()
    res <- default_resolution_in_pixels_per_inch()

    # Create the graphics device that we are going to shadow
    withCallingHandlers(
        grDevices::png(
            filename = filename,
            type = type,
            res = res
        ),
        warning = function(w) {
            stop("Error creating graphics device: ", conditionMessage(w))
        }
    )

    # Update the device name + description in the base environment.
    index <- grDevices::dev.cur()
    oldDevice <- .Devices[[index]]
    newDevice <- name

    # Copy device attributes. Usually, this is just the file path.
    attributes(newDevice) <- attributes(oldDevice)

    # Set other device properties.
    attr(newDevice, "type") <- type
    attr(newDevice, "res") <- res

    # Update the devices list.
    .Devices[[index]] <- newDevice

    # Replace bindings.
    env_bind_force(baseenv(), ".Devices", .Devices)
    env_bind_force(baseenv(), ".Device", newDevice)

    # Also set ourselves as a known interactive device.
    # Used by `dev.interactive()`, which is used in `stats:::plot.lm()`
    # to determine if `devAskNewPage(TRUE)` should be set to prompt before
    # each new plot is drawn.
    grDevices::deviceIsInteractive(name)
}

# Create a recording of the current plot.
#
# This saves the plot's display list, so it can be used to re-render plots as
# necessary.
#' @export
.ps.graphics.record_plot <- function(id) {
    # Create the plot recording
    recording <- grDevices::recordPlot()

    # Add the recording to the persistent list
    add_recording(id, recording)

    invisible(NULL)
}

#' @export
.ps.graphics.render_plot_from_recording <- function(
    id,
    width,
    height,
    pixel_ratio,
    format
) {
    path <- render_path(id)
    recording <- get_recording(id)

    if (is.null(recording)) {
        stop(sprintf(
            "Failed to render plot for plot `id` %s. Recording is missing.",
            id
        ))
    }

    # Replay the plot with the specified device.
    with_graphics_device(path, width, height, pixel_ratio, format, {
        suppressWarnings(grDevices::replayPlot(recording))
    })

    # Return path to generated plot file.
    invisible(path)
}

#' Run an expression with the specificed device activated.
#'
#' The device is guaranteed to close after the expression has run.
#'
#' @param path The file path to render output to.
#' @param width The plot width, in pixels.
#' @param height The plot height, in pixels.
#' @param pixel_ratio The device pixel ratio (e.g. 1 for standard displays, 2
#'   for retina displays)
#' @param format The output format (and therefore graphics device) to use.
#'   One of: `"png"`, `"svg"`, `"pdf"`, `"jpeg"`, or `"tiff"`.
with_graphics_device <- function(
    path,
    width,
    height,
    pixel_ratio,
    format,
    expr
) {
    # Store handle to current device (i.e. us)
    old_dev <- grDevices::dev.cur()

    args <- finalize_device_arguments(format, width, height, pixel_ratio)
    width <- args$width
    height <- args$height
    res <- args$res
    type <- args$type

    # Create a new graphics device.
    # TODO: Use 'ragg' if available?
    switch(
        format,
        "png" = grDevices::png(
            filename = path,
            width = width,
            height = height,
            res = res,
            type = type
        ),
        "svg" = grDevices::svg(
            filename = path,
            width = width,
            height = height,
        ),
        "pdf" = grDevices::pdf(
            file = path,
            width = width,
            height = height
        ),
        "jpeg" = grDevices::jpeg(
            filename = path,
            width = width,
            height = height,
            res = res,
            type = type
        ),
        "tiff" = grDevices::tiff(
            filename = path,
            width = width,
            height = height,
            res = res,
            type = type
        ),
        stop("Internal error: Unknown plot `format`.")
    )

    # Ensure we turn off the device on the way out, this:
    # - Commits the plot to disk
    # - Resets us back as being the current device
    defer(utils::capture.output({
        grDevices::dev.off()
        if (old_dev > 1) {
            grDevices::dev.set(old_dev)
        }
    }))

    expr
}

finalize_device_arguments <- function(format, width, height, pixel_ratio) {
    if (format == "png" || format == "jpeg" || format == "tiff") {
        # These devices require `width` and `height` in pixels, which is what
        # they are provided in already. For pixel based devices, all relevant
        # values are upscaled by `pixel_ratio`.
        #
        # `res` is nominal resolution specified in pixels-per-inch (ppi).
        return(list(
            type = default_device_type(),
            res = default_resolution_in_pixels_per_inch() * pixel_ratio,
            width = width * pixel_ratio,
            height = height * pixel_ratio
        ))
    }

    if (format == "svg" || format == "pdf") {
        # These devices require `width` and `height` in inches, but they are
        # provided to us in pixels, so we have to perform a conversion here.
        # For vector based devices, providing the size in inches implicitly
        # tells the device the relative size to use for things like text,
        # since that is the absolute unit (pts are based on inches).
        #
        # Thomas says the math for `width` and `height` here are correct, i.e.
        # we don't also multiply `default_resolution_in_pixels_per_inch()` by
        # `pixel_ratio` like we do above, which would have made it cancel out of
        # the equation below.
        #
        # There is no `type` or `res` argument for these devices.
        return(list(
            type = NULL,
            res = NULL,
            width = width *
                pixel_ratio /
                default_resolution_in_pixels_per_inch(),
            height = height *
                pixel_ratio /
                default_resolution_in_pixels_per_inch()
        ))
    }

    stop("Internal error: Unknown plot `format`.")
}

#' Default OS resolution in PPI (pixels per inch)
#'
#' Thomas thinks these are "more correct than any other numbers." Specifically,
#' macOS uses 96 DPI for its internal scaling, but this is user definable on
#' Windows.
#'
#' This corresponds to a scaling factor that tries to make things that appear
#' "on screen" be as close to the size in which they are actually printed at,
#' which has always been tricky.
default_resolution_in_pixels_per_inch <- function() {
    if (Sys.info()[["sysname"]] == "Darwin") {
        96L
    } else {
        72L
    }
}

default_device_type <- function() {
    if (has_aqua()) {
        "quartz"
    } else if (has_cairo()) {
        "cairo"
    } else if (has_x11()) {
        "Xlib"
    } else {
        stop("This version of R wasn't built with plotting capabilities")
    }
}
