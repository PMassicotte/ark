/*---------------------------------------------------------------------------------------------
 *  Copyright (C) 2024 Posit Software, PBC. All rights reserved.
 *--------------------------------------------------------------------------------------------*/

//
// AUTO-GENERATED from frontend.json; do not edit.
//

use serde::Deserialize;
use serde::Serialize;

/// Items in Params
pub type Param = serde_json::Value;

/// The method result
pub type CallMethodResult = serde_json::Value;

/// Parameters for the CallMethod method.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct CallMethodParams {
    /// The method to call inside the interpreter
    pub method: String,

    /// The parameters for `method`
    pub params: Vec<Param>,
}

/// Parameters for the Busy method.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct BusyParams {
    /// Whether the backend is busy
    pub busy: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FrontendRpcRequest {
    pub method: String,
    pub params: Vec<Value>, // Should we use Value::Object() instead?
}

/// Parameters for the OpenEditor method.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct OpenEditorParams {
    /// The path of the file to open
    pub file: String,

    /// The line number to jump to
    pub line: i64,

    /// The column number to jump to
    pub column: i64,
}

/// Parameters for the ShowMessage method.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ShowMessageParams {
    /// The message to show to the user.
    pub message: String,
}

/// Parameters for the PromptState method.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct PromptStateParams {
    /// Prompt for primary input.
    pub input_prompt: String,

    /// Prompt for incomplete input.
    pub continuation_prompt: String,
}

/// Parameters for the WorkingDirectory method.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct WorkingDirectoryParams {
    /// The new working directory
    pub directory: String,
}

/**
 * RPC request types for the frontend comm
 */
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "method", content = "params")]
pub enum FrontendRpcRequest {
    /// Run a method in the interpreter and return the result to the frontend
    ///
    /// Unlike other RPC methods, `call_method` calls into methods implemented
    /// in the interpreter and returns the result back to the frontend using
    /// an implementation-defined serialization scheme.
    #[serde(rename = "call_method")]
    CallMethod(CallMethodParams),
}

/**
 * RPC Reply types for the frontend comm
 */
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "method", content = "result")]
pub enum FrontendRpcReply {
    /// The method result
    CallMethodReply(CallMethodResult),
}

/**
 * Front-end events for the frontend comm
 */
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "method", content = "params")]
pub enum FrontendEvent {
    #[serde(rename = "busy")]
    Busy(BusyParams),

    #[serde(rename = "clear_console")]
    ClearConsole,

    #[serde(rename = "open_editor")]
    OpenEditor(OpenEditorParams),

    #[serde(rename = "show_message")]
    ShowMessage(ShowMessageParams),

    #[serde(rename = "prompt_state")]
    PromptState(PromptStateParams),

    #[serde(rename = "working_directory")]
    WorkingDirectory(WorkingDirectoryParams),
}
