//! A blocking client of the `lirtup serve` socket: `lirtup ctl` and tests.
//! The GUI keeps its own async client (`CoreLink`, round 3).

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use crate::proto::{Command, EngineSnapshot, Event, PROTOCOL_VERSION, decode_line, encode_line};
use crate::{CoreError, Result};

/// One connection to `lirtup serve`.
pub struct Client {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    line: String,
    closed: bool,
}

impl Client {
    /// Connect to the socket (does not send `Hello`).
    pub fn connect(socket: &Path) -> Result<Client> {
        let writer = UnixStream::connect(socket)?;
        let reader = BufReader::new(writer.try_clone()?);
        Ok(Client {
            reader,
            writer,
            line: String::new(),
            closed: false,
        })
    }

    /// Send `Hello` and wait for the `Snapshot` (events before it are
    /// skipped).
    pub fn hello(&mut self, client: &str) -> Result<EngineSnapshot> {
        self.send(&Command::Hello {
            version: PROTOCOL_VERSION,
            client: client.into(),
        })?;
        loop {
            match self.recv()? {
                Some(Event::Snapshot(snapshot)) => return Ok(snapshot),
                Some(_) => continue,
                None => return Err(CoreError::Disconnected),
            }
        }
    }

    /// Send one command.
    pub fn send(&mut self, command: &Command) -> Result<()> {
        self.writer.write_all(encode_line(command).as_bytes())?;
        self.writer.flush()?;
        Ok(())
    }

    /// Wait for the next event; `None` when the server closed the
    /// connection.
    pub fn recv(&mut self) -> Result<Option<Event>> {
        self.writer.set_read_timeout(None)?;
        self.read_event()
    }

    /// [`Client::recv`] with a timeout; `Ok(None)` on timeout or when the
    /// server closed the connection (then [`Client::is_closed`] is true).
    pub fn recv_timeout(&mut self, timeout: Duration) -> Result<Option<Event>> {
        self.writer
            .set_read_timeout(Some(timeout.max(Duration::from_millis(1))))?;
        match self.read_event() {
            Err(CoreError::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                Ok(None)
            }
            other => other,
        }
    }

    /// The server closed the connection (seen by the last receive).
    pub fn is_closed(&self) -> bool {
        self.closed
    }

    fn read_event(&mut self) -> Result<Option<Event>> {
        loop {
            // A timeout may leave half a line in `self.line`: keep it.
            let n = self.reader.read_line(&mut self.line)?;
            if n == 0 {
                self.closed = true;
                return Ok(None);
            }
            if !self.line.ends_with('\n') {
                continue;
            }
            let line = std::mem::take(&mut self.line);
            if line.trim().is_empty() {
                continue;
            }
            return Ok(Some(decode_line(&line)?));
        }
    }
}
