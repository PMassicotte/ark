use harp::call::r_expr_quote;
use harp::environment::r_ns_env;
use harp::environment::Binding;
use harp::exec::RFunction;
use harp::exec::RFunctionExt;
use harp::object::r_length;
use harp::object::RObject;
use harp::r_symbol;
use harp::utils::r_typeof;
use libr::*;

use crate::interface::RMain;
use crate::modules::ARK_ENVS;
use crate::r_task;
use crate::variables::variable::is_binding_fancy;
use crate::variables::variable::plain_binding_force_with_rollback;

#[tracing::instrument(level = "trace", skip_all)]
pub(crate) fn resource_namespaces(pkgs: Vec<String>) -> anyhow::Result<()> {
    // Generate only one task and loop inside it to preserve the order of `pkgs`
    r_task::spawn_idle(|| async move {
        for pkg in pkgs.into_iter() {
            if let Err(err) = ns_populate_srcref(pkg.clone()).await {
                log::error!("Can't populate srcrefs for `{pkg}`: {err:?}");
            }
        }
    });

    Ok(())
}

pub(crate) fn resource_loaded_namespaces() -> anyhow::Result<()> {
    let loaded = RFunction::new("base", "loadedNamespaces").call()?;
    let loaded: Vec<String> = loaded.try_into()?;
    resource_namespaces(loaded)
}

#[harp::register]
unsafe extern "C-unwind" fn ps_resource_namespaces(pkgs: SEXP) -> anyhow::Result<SEXP> {
    let pkgs: Vec<String> = RObject::view(pkgs).try_into()?;
    resource_namespaces(pkgs)?;
    Ok(harp::r_null())
}

#[harp::register]
unsafe extern "C-unwind" fn ps_ns_populate_srcref(ns_name: SEXP) -> anyhow::Result<SEXP> {
    let ns_name: String = RObject::view(ns_name).try_into()?;
    futures::executor::block_on(ns_populate_srcref(ns_name))?;
    Ok(harp::r_null())
}

pub(crate) async fn ns_populate_srcref(ns_name: String) -> anyhow::Result<()> {
    let span = tracing::trace_span!("ns_populate_srcref", ns = ns_name);
    let mut tick = std::time::Instant::now();

    let ns = r_ns_env(&ns_name)?;

    let uri_path = format!("namespace:{ns_name}.R");
    let uri = format!("ark:{uri_path}");

    let mut vdoc: Vec<String> = vec![
        format!("# Virtual namespace of package {ns_name}."),
        format!("# This source is generated by Ark and is approximate."),
        format!(""),
        format!("declare(ark(diagnostics(enable = FALSE)))"),
        String::from(""),
    ];

    let mut n_ok = 0;
    let mut n_bad = 0;
    let mut n_skipped = 0;

    for b in ns.iter().filter_map(Result::ok) {
        span.in_scope(|| {
            match generate_source(&b, vdoc.len(), &uri) {
                Ok(Some(mut lines)) => {
                    n_ok = n_ok + 1;

                    vdoc.append(&mut lines);

                    // Add some separation
                    vdoc.push(String::from(""));
                },
                Err(_err) => {
                    n_bad = n_bad + 1;

                    // log::error!(
                    //     "Can't populate srcref for {} in namespace {ns_name}: {_err}",
                    //     b.name
                    // )
                },
                _ => {
                    n_skipped = n_skipped + 1;
                },
            }
        });

        if tick.elapsed() > std::time::Duration::from_millis(10) {
            tick = std::time::Instant::now();
            tokio::task::yield_now().await;
        }
    }

    log::trace!(
        "Populated virtual namespace for {ns_name}: \
         {} lines, {n_ok} ok, {n_bad} bad, {n_skipped} skipped",
        vdoc.len()
    );

    let contents = vdoc.join("\n");

    // Notify LSP of the opened virtual document so the LSP can function as a
    // text document content provider of the virtual document contents, which is
    // used by the debugger.
    RMain::with_mut(|main| main.did_open_virtual_document(uri_path, contents));

    Ok(())
}

#[tracing::instrument(level = "trace", skip_all, fields(name = %binding.name))]
fn generate_source(
    binding: &Binding,
    line: usize,
    uri: &String,
) -> anyhow::Result<Option<Vec<String>>> {
    if is_binding_fancy(binding) {
        return Ok(None);
    }

    // Only update regular functions
    let old = plain_binding_force_with_rollback(binding)?;
    if old.kind() != CLOSXP {
        return Ok(None);
    }

    // These don't deparse to a `function` call!
    if harp::utils::r_is_s4(old.sexp) {
        return Ok(None);
    }

    // Ignore functions that already have sources
    if let Some(_) = old.get_attribute("srcref") {
        return Ok(None);
    }

    let reparsed = RFunction::new("", "reparse_with_srcref")
        .add(old.clone())
        .param("name", r_expr_quote(binding.name.sexp))
        .param("uri", uri.clone())
        .param("line", (line + 1) as i32)
        .call_in(ARK_ENVS.positron_ns)?;

    let (new, text) = (
        harp::list_get(reparsed.sexp, 0),
        harp::list_get(reparsed.sexp, 1),
    );

    // Inject source references in functions. This is slightly unsafe but we
    // couldn't think of a dire failure mode.
    unsafe {
        // First replace the body which contains expressions tagged with srcrefs
        // such as calls to `{`. Compiled functions are a little more tricky.

        let body = BODY(old.sexp);
        if r_typeof(body) == BCODESXP {
            // This is a compiled function. We could recompile the fresh
            // function we just created but the compiler is very slow. Instead,
            // update the expression stored in the bytecode. This expression is
            // used by `eval()` when stepping with the debugger.

            // Get the constant pool: BCODE_CONSTS = CDR
            let consts = CDR(body);

            // The original body expression is stored as first element
            // of the constant pool
            if r_length(consts) > 0 {
                // Inject new body instrumented with source references
                SET_VECTOR_ELT(consts, 0, R_ClosureExpr(new));
            }
        } else {
            SET_BODY(old.sexp, BODY(new));
        }

        // Finally push the srcref attribute for the whole function
        Rf_setAttrib(
            old.sexp,
            r_symbol!("srcref"),
            Rf_getAttrib(new, r_symbol!("srcref")),
        );
    }

    let text: Vec<String> = RObject::view(text).try_into()?;
    Ok(Some(text))
}

#[harp::register]
pub extern "C-unwind" fn ark_zap_srcref(x: SEXP) -> anyhow::Result<SEXP> {
    Ok(harp::attrib::zap_srcref(x).sexp)
}
