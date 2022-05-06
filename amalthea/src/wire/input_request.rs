/*
 * input_request.rs
 *
 * Copyright (C) 2022 by RStudio, PBC
 *
 */

use crate::wire::jupyter_message::MessageType;
use serde::{Deserialize, Serialize};

/// Represents a request from the kernel to the front end to prompt the user for
/// input
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InputRequest {
    /// The prompt to display to the user
    pub prompt: String,

    /// Whether the string being requested is a password (and should therefore
    /// be obscured)
    pub password: bool,
}

impl MessageType for InputRequest {
    fn message_type() -> String {
        String::from("input_request")
    }
}
