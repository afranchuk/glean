// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Handling the Glean upload logic.
//!
//! This doesn't perform the actual upload but rather handles
//! retries, upload limitations and error tracking.

use std::sync::{
    atomic::{AtomicU8, Ordering},
    Arc, Mutex,
};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use glean_core::upload::PingUploadTask;
pub use glean_core::upload::{PingRequest, UploadResult, UploadTaskAction};

pub use http_uploader::*;

mod http_uploader;

/// A description of a component used to upload pings.
pub trait PingUploader: std::fmt::Debug + Send + Sync {
    /// Uploads a ping to a server.
    ///
    /// # Arguments
    ///
    /// * `url` - the URL path to upload the data to.
    /// * `body` - the serialized text data to send.
    /// * `headers` - a vector of tuples containing the headers to send with
    ///   the request, i.e. (Name, Value).
    fn upload(&self, url: String, body: Vec<u8>, headers: Vec<(String, String)>) -> UploadResult;
}

/// The logic for uploading pings: this leaves the actual upload mechanism as
/// a detail of the user-provided object implementing [`PingUploader`].
#[derive(Debug)]
pub(crate) struct UploadManager {
    inner: Arc<Inner>,
}

const STATE_STOPPED: u8 = 0;
const STATE_RUNNING: u8 = 1;
const STATE_SHUTTING_DOWN: u8 = 2;

#[derive(Debug)]
struct Inner {
    server_endpoint: String,
    uploader: Box<dyn PingUploader + 'static>,
    thread_running: AtomicU8,
    handle: Mutex<Option<JoinHandle<()>>>,
}

impl UploadManager {
    /// Create a new instance of the upload manager.
    ///
    /// # Arguments
    ///
    /// * `server_endpoint` -  the server pings are sent to.
    /// * `new_uploader` - the instance of the uploader used to send pings.
    pub(crate) fn new(
        server_endpoint: String,
        new_uploader: Box<dyn PingUploader + 'static>,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                server_endpoint,
                uploader: new_uploader,
                thread_running: AtomicU8::new(STATE_STOPPED),
                handle: Mutex::new(None),
            }),
        }
    }

    /// Signals Glean to upload pings at the next best opportunity.
    pub(crate) fn trigger_upload(&self) {
        // If no other upload proces is running, we're the one starting it.
        // Need atomic compare/exchange to avoid any further races
        // or we can end up with 2+ uploader threads.
        if self
            .inner
            .thread_running
            .compare_exchange(
                STATE_STOPPED,
                STATE_RUNNING,
                Ordering::SeqCst,
                Ordering::SeqCst,
            )
            .is_err()
        {
            return;
        }

        let inner = Arc::clone(&self.inner);

        // Need to lock before we start so that noone thinks we're not running.
        let mut handle = self.inner.handle.lock().unwrap();
        let thread = thread::Builder::new()
            .name("glean.upload".into())
            .spawn(move || {
                log::trace!("Started glean.upload thread");
                loop {
                    let incoming_task = glean_core::glean_get_upload_task();

                    match incoming_task {
                        PingUploadTask::Upload { request } => {
                            log::trace!("Received upload task with request {:?}", request);
                            let doc_id = request.document_id.clone();
                            let upload_url = format!("{}{}", inner.server_endpoint, request.path);
                            let headers: Vec<(String, String)> =
                                request.headers.into_iter().collect();
                            let result = inner.uploader.upload(upload_url, request.body, headers);
                            // Process the upload response.
                            match glean_core::glean_process_ping_upload_response(doc_id, result) {
                                UploadTaskAction::Next => continue,
                                UploadTaskAction::End => break,
                            }
                        }
                        PingUploadTask::Wait { time } => {
                            log::trace!("Instructed to wait for {:?}ms", time);
                            thread::sleep(Duration::from_millis(time));
                        }
                        PingUploadTask::Done { .. } => {
                            log::trace!("Received PingUploadTask::Done. Exiting.");
                            // Nothing to do here, break out of the loop.
                            break;
                        }
                    }

                    let status = inner.thread_running.load(Ordering::SeqCst);
                    // asked to shut down. let's do it.
                    if status == STATE_SHUTTING_DOWN {
                        break;
                    }
                }

                // Clear the running flag to signal that this thread is done,
                // but only if there's no shutdown thread.
                let _ = inner.thread_running.compare_exchange(
                    STATE_RUNNING,
                    STATE_STOPPED,
                    Ordering::SeqCst,
                    Ordering::SeqCst,
                );
            })
            .expect("Failed to spawn Glean's uploader thread");
        *handle = Some(thread);
    }

    pub(crate) fn shutdown(&self) {
        // mark as shutting down.
        self.inner
            .thread_running
            .store(STATE_SHUTTING_DOWN, Ordering::SeqCst);

        // take the thread handle out.
        let mut handle = self.inner.handle.lock().unwrap();
        let thread = handle.take();

        if let Some(thread) = thread {
            thread
                .join()
                .expect("couldn't join on the uploader thread.");
        }
    }
}
