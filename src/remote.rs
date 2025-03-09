use std::sync::mpsc;

pub enum RemoteUpdate {
    Fetched { index: usize, content: String },
}

#[derive(Clone)]
pub struct RemoteUrl {
    pub url: String,
    pub content: Option<String>,
    pub include: bool,
}

pub struct Remote {
    pub remote_urls: Vec<RemoteUrl>,
    pub new_url: String,
    pub remote_update_rx: mpsc::Receiver<RemoteUpdate>,
    pub remote_update_tx: mpsc::Sender<RemoteUpdate>,
}

impl Default for Remote {
    fn default() -> Self {
        let (remote_tx, remote_rx) = mpsc::channel();
        Self {
            remote_urls: Vec::new(),
            new_url: String::new(),
            remote_update_rx: remote_rx,
            remote_update_tx: remote_tx,
        }
    }
}
