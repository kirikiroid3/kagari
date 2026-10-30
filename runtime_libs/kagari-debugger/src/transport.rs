//! Moving protocol messages: a [`Link`] is the thread-safe pair of channels between an endpoint
//! and the threads (or the peer) that carry its messages; a [`Connection`] is one endpoint's
//! view of it, with sequence numbers and a backlog of messages put back for later.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::io::{self, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::thread;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;

use crate::protocol::{Message, event, read_frame, write_frame};

/// One endpoint's channels: what arrives and where to send.
pub struct Link {
    pub incoming: Receiver<Message>,
    pub outgoing: Sender<Message>,
}

impl Link {
    /// Two endpoints connected to each other (an in-process backend).
    pub fn pair() -> (Link, Link) {
        let (a_tx, a_rx) = mpsc::channel();
        let (b_tx, b_rx) = mpsc::channel();
        (
            Link {
                incoming: a_rx,
                outgoing: b_tx,
            },
            Link {
                incoming: b_rx,
                outgoing: a_tx,
            },
        )
    }

    /// An endpoint over a byte stream: a reader thread parses frames, a writer thread writes
    /// them. The link closes when the stream ends or a frame is malformed.
    pub fn over(reader: impl Read + Send + 'static, writer: impl Write + Send + 'static) -> Link {
        let (in_tx, in_rx) = mpsc::channel();
        let (out_tx, out_rx) = mpsc::channel::<Message>();
        thread::spawn(move || {
            let mut reader = BufReader::new(reader);
            while let Ok(Some(body)) = read_frame(&mut reader) {
                match serde_json::from_slice::<Message>(&body) {
                    Ok(message) => {
                        if in_tx.send(message).is_err() {
                            break;
                        }
                    }
                    Err(error) => {
                        log::warn!(target: "kagari::debug", "a malformed message: {error}");
                    }
                }
            }
        });
        thread::spawn(move || {
            let mut writer = writer;
            for message in out_rx {
                let Ok(body) = serde_json::to_vec(&message) else {
                    continue;
                };
                if write_frame(&mut writer, &body).is_err() {
                    break;
                }
            }
        });
        Link {
            incoming: in_rx,
            outgoing: out_tx,
        }
    }

    /// An endpoint over a TCP stream.
    pub fn tcp(stream: TcpStream) -> io::Result<Link> {
        let reader = stream.try_clone()?;
        Ok(Link::over(reader, stream))
    }
}

/// Listens on `127.0.0.1:port` (0 picks a free port) and hands every accepted client over as a
/// link. Returns the bound address.
pub fn listen(port: u16) -> io::Result<(SocketAddr, Receiver<Link>)> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    let address = listener.local_addr()?;
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let link = match stream.and_then(Link::tcp) {
                Ok(link) => link,
                Err(error) => {
                    log::warn!(target: "kagari::debug", "a debugger could not connect: {error}");
                    continue;
                }
            };
            if tx.send(link).is_err() {
                break;
            }
        }
    });
    Ok((address, rx))
}

/// One endpoint: sends with fresh sequence numbers, receives with a backlog.
pub struct Connection {
    link: Link,
    seq: Cell<i64>,
    backlog: RefCell<VecDeque<Message>>,
    closed: Cell<bool>,
}

impl Connection {
    pub fn new(link: Link) -> Connection {
        Connection {
            link,
            seq: Cell::new(0),
            backlog: RefCell::new(VecDeque::new()),
            closed: Cell::new(false),
        }
    }

    /// Whether the peer is gone.
    pub fn closed(&self) -> bool {
        self.closed.get()
    }

    fn next_seq(&self) -> i64 {
        let seq = self.seq.get().saturating_add(1);
        self.seq.set(seq);
        seq
    }

    fn send(&self, message: Message) {
        if self.link.outgoing.send(message).is_err() {
            self.closed.set(true);
        }
    }

    /// Sends a request; returns its sequence number.
    pub fn request(&self, command: &str, arguments: Value) -> i64 {
        let seq = self.next_seq();
        self.send(Message::Request {
            seq,
            command: command.to_owned(),
            arguments,
        });
        seq
    }

    /// Answers the request `request_seq` of `command`.
    pub fn respond(&self, request_seq: i64, command: &str, result: Result<Value, String>) {
        let seq = self.next_seq();
        let (success, message, body) = match result {
            Ok(body) => (true, None, body),
            Err(message) => (false, Some(message), Value::Null),
        };
        self.send(Message::Response {
            seq,
            request_seq,
            success,
            command: command.to_owned(),
            message,
            body,
        });
    }

    pub fn event(&self, name: &str, body: impl Serialize) {
        let seq = self.next_seq();
        self.send(Message::Event {
            seq,
            event: name.to_owned(),
            body: serde_json::to_value(body).unwrap_or(Value::Null),
        });
    }

    /// An `output` event.
    pub fn output(&self, text: impl Into<String>) {
        self.event(
            event::OUTPUT,
            crate::protocol::OutputBody { text: text.into() },
        );
    }

    /// The next message without waiting.
    pub fn try_recv(&self) -> Option<Message> {
        if let Some(message) = self.backlog.borrow_mut().pop_front() {
            return Some(message);
        }
        match self.link.incoming.try_recv() {
            Ok(message) => Some(message),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.closed.set(true);
                None
            }
        }
    }

    /// The next message, waiting for it; `None` once the peer is gone.
    pub fn recv(&self) -> Option<Message> {
        if let Some(message) = self.backlog.borrow_mut().pop_front() {
            return Some(message);
        }
        match self.link.incoming.recv() {
            Ok(message) => Some(message),
            Err(_) => {
                self.closed.set(true);
                None
            }
        }
    }

    /// The next message within `timeout`.
    pub fn recv_timeout(&self, timeout: Duration) -> Option<Message> {
        if let Some(message) = self.backlog.borrow_mut().pop_front() {
            return Some(message);
        }
        match self.link.incoming.recv_timeout(timeout) {
            Ok(message) => Some(message),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => {
                self.closed.set(true);
                None
            }
        }
    }

    /// Puts a message back, to be received after the ones already put back.
    pub fn defer(&self, message: Message) {
        self.backlog.borrow_mut().push_back(message);
    }

    /// Takes the first waiting request for `command` (leaving the others in order); its
    /// sequence number.
    pub fn take_request(&self, command: &str) -> Option<i64> {
        loop {
            match self.link.incoming.try_recv() {
                Ok(message) => self.backlog.borrow_mut().push_back(message),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.closed.set(true);
                    break;
                }
            }
        }
        let mut backlog = self.backlog.borrow_mut();
        let at = backlog
            .iter()
            .position(|m| matches!(m, Message::Request { command: c, .. } if c == command))?;
        match backlog.remove(at)? {
            Message::Request { seq, .. } => Some(seq),
            _ => None,
        }
    }
}
