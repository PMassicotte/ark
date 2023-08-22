//
// dap.rs
//
// Copyright (C) 2023 Posit Software, PBC. All rights reserved.
//
//

use std::sync::{Arc, Mutex};

use amalthea::{comm::comm_channel::CommChannelMsg, language::dap_handler::DapHandler};
use crossbeam::channel::{unbounded, Receiver, Sender};
use harp::session::FrameInfo;
use serde_json::json;
use stdext::{result::ResultOrLog, spawn};

use crate::{dap::dap_server, request::RRequest};

#[derive(Debug, Copy, Clone)]
pub enum DapBackendEvent {
    /// Event sent when a normal (non-browser) prompt marks the end of a
    /// debugging session.
    Terminated,

    /// Event sent when user types `n`, `f`, `c`, or `cont`.
    Continued,

    /// Event sent when a browser prompt is emitted during an existing
    /// debugging session
    Stopped,
}

pub struct Dap {
    /// State shared with the DAP server thread.
    pub state: Arc<Mutex<DapState>>,

    /// Channel for sending events to the DAP frontend.
    pub events_tx: Sender<DapBackendEvent>,

    /// Receiving side of event channel, managed on its own thread.
    events_rx: Receiver<DapBackendEvent>,

    /// Channel for sending events to the comm frontend.
    comm_tx: Option<Sender<CommChannelMsg>>,

    /// Channel for sending debug commands to `read_console()`
    r_request_tx: Sender<RRequest>,
}

pub struct DapState {
    /// Whether the REPL is stopped with a browser prompt.
    pub is_debugging: bool,

    /// Whether the DAP server is connected to a client.
    pub is_connected: bool,

    /// Stack information
    pub stack: Option<Vec<FrameInfo>>,
}

impl DapState {
    pub fn new() -> Self {
        Self {
            is_debugging: false,
            is_connected: false,
            stack: None,
        }
    }
}

impl Dap {
    pub fn new(r_request_tx: Sender<RRequest>) -> Self {
        let (events_tx, events_rx) = unbounded::<DapBackendEvent>();
        Self {
            state: Arc::new(Mutex::new(DapState::new())),
            events_tx,
            events_rx,
            comm_tx: None,
            r_request_tx,
        }
    }

    pub fn start_debug(&self, stack: Vec<FrameInfo>) {
        let mut state = self.state.lock().unwrap();

        state.stack = Some(stack);

        if state.is_debugging {
            if state.is_connected {
                self.send_event(DapBackendEvent::Stopped);
            }
        } else {
            if let Some(tx) = &self.comm_tx {
                // Ask frontend to connect to the DAP
                log::trace!("DAP: Sending `start_debug` event");
                let msg = CommChannelMsg::Data(json!({
                    "msg_type": "start_debug",
                    "content": {}
                }));
                tx.send(msg).unwrap();
            }

            state.is_debugging = true;
        }
    }

    pub fn stop_debug(&self) {
        // Reset state
        let mut state = self.state.lock().unwrap();
        state.stack = None;
        state.is_debugging = false;

        if state.is_connected {
            if let Some(_) = &self.comm_tx {
                // Let frontend know we've quitted the debugger so it can
                // terminate the debugging session and disconnect.
                log::trace!("DAP: Sending `start_debug` event");
                self.send_event(DapBackendEvent::Terminated);
            }
            // else: If not connected to a frontend, the DAP client should
            // have received a `Continued` event already, after a `n`
            // command or similar.
        }
    }

    pub fn send_event(&self, event: DapBackendEvent) {
        self.events_tx
            .send(event)
            .or_log_error(&format!("Couldn't send event {:?}", event));
    }
}

// Handler for Amalthea socket threads
impl DapHandler for Dap {
    fn start(
        &mut self,
        tcp_address: String,
        conn_init_tx: Sender<bool>,
        comm_tx: Sender<CommChannelMsg>,
    ) -> Result<(), amalthea::error::Error> {
        log::info!("DAP: Spawning thread");

        // Create the DAP thread that manages connections and creates a
        // server when connected. This is currently the only way to create
        // this thread but in the future we might provide other ways to
        // connect to the DAP without a Jupyter comm.
        let state_clone = self.state.clone();
        let events_rx_clone = self.events_rx.clone();
        let r_request_tx_clone = self.r_request_tx.clone();
        let comm_tx_clone = comm_tx.clone();
        spawn!("ark-dap", move || {
            dap_server::start_dap(
                tcp_address,
                state_clone,
                conn_init_tx,
                events_rx_clone,
                r_request_tx_clone,
                comm_tx_clone,
            )
        });

        // If `start()` is called we are now connected to a frontend
        self.comm_tx = Some(comm_tx);

        return Ok(());
    }
}
