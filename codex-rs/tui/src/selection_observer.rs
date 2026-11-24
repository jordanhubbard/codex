//! Optional, one-way selection observation for the process that launched this TUI.
//!
//! The launcher passes a private write-only pipe, rather than scraping terminal
//! output or treating a shared daemon's most recent thread as this viewer's
//! selection. The pipe carries no prompts, paths, credentials or control requests.

use std::sync::Mutex;

// run_main may be retried inside this same process after transport recovery.
// Descriptor ownership belongs to the launcher invocation, not one UI attempt.
static REGISTRY: Mutex<Registry> = Mutex::new(Registry {
    descriptor: None,
    observer: None,
});

struct Registry {
    descriptor: Option<i32>,
    observer: Option<Observer>,
}

impl Registry {
    fn initialize(&mut self, fd: Option<i32>) -> std::io::Result<()> {
        if self.descriptor.is_some() {
            if self.descriptor != fd {
                return Err(std::io::Error::other(
                    "selection observation channel changed during retry",
                ));
            }
            // Never construct a second File for the same raw descriptor. If a
            // failed writer was already dropped, its number may now belong to
            // an unrelated file; disabled observation stays disabled.
        } else if let Some(fd) = fd {
            self.observer = Some(Observer::from_descriptor(fd)?);
            self.descriptor = Some(fd);
        }
        self.publish(None);
        Ok(())
    }

    fn publish(&mut self, thread_id: Option<String>) {
        if let Some(writer) = self.observer.as_mut()
            && writer.publish(thread_id).is_err()
        {
            self.observer = None;
        }
    }
}

pub(crate) fn initialize(fd: Option<i32>) -> std::io::Result<()> {
    REGISTRY
        .lock()
        .map_err(|_| std::io::Error::other("selection observer unavailable"))?
        .initialize(fd)
}

pub(crate) fn enabled() -> bool {
    REGISTRY
        .lock()
        .is_ok_and(|registry| registry.observer.is_some())
}

pub(crate) fn publish(thread_id: Option<String>) {
    if let Ok(mut registry) = REGISTRY.lock() {
        // Observation loss must never stop coding or redirect output to the PTY.
        registry.publish(thread_id);
    }
}

#[cfg(unix)]
pub(crate) fn same_host(target: &crate::AppServerTarget) -> bool {
    use crate::AppServerTarget;
    use crate::RemoteAppServerEndpoint;
    use std::os::unix::fs::MetadataExt;
    match target {
        AppServerTarget::Embedded => true,
        AppServerTarget::LocalDaemon { endpoint, .. } => match endpoint {
            RemoteAppServerEndpoint::UnixSocket { socket_path } => {
                use std::os::unix::fs::FileTypeExt;
                std::fs::metadata(socket_path.as_path()).is_ok_and(|metadata| {
                    metadata.file_type().is_socket()
                        && metadata.uid() == unsafe { libc::geteuid() }
                        && metadata.mode() & 0o077 == 0
                })
            }
            RemoteAppServerEndpoint::WebSocket { .. } => false,
        },
        // An explicitly supplied Unix socket may be a forward to another host.
        AppServerTarget::Remote { .. } | AppServerTarget::Cloud => false,
    }
}

#[cfg(not(unix))]
pub(crate) fn same_host(_: &crate::AppServerTarget) -> bool {
    false
}

#[cfg(unix)]
struct Observer {
    pipe: std::fs::File,
    sequence: u64,
    last: Option<Option<String>>,
}

#[cfg(unix)]
impl Observer {
    fn from_descriptor(fd: i32) -> std::io::Result<Self> {
        use std::os::fd::FromRawFd;
        if fd < 3 {
            return Err(std::io::Error::other(
                "selection observer requires a private pipe",
            ));
        }
        // Validate before assuming ownership. In particular, never change stdout,
        // a regular file, socket, or a read end supplied by a mistaken invocation.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
        if flags < 0 || unsafe { libc::fstat(fd, stat.as_mut_ptr()) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        let stat = unsafe { stat.assume_init() };
        if flags & libc::O_ACCMODE != libc::O_WRONLY || stat.st_mode & libc::S_IFMT != libc::S_IFIFO
        {
            return Err(std::io::Error::other(
                "selection observer requires a write-only pipe",
            ));
        }
        let pipe = unsafe { std::fs::File::from_raw_fd(fd) };
        // Do this before app-server bootstrap. Neither the independent daemon nor
        // commands that it executes may inherit this viewer's observation channel.
        if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } < 0
            || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
        {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Self {
            pipe,
            sequence: 0,
            last: None,
        })
    }

    fn publish(&mut self, thread_id: Option<String>) -> std::io::Result<()> {
        use std::io::Write;
        if self.last.as_ref() == Some(&thread_id) {
            return Ok(());
        }
        if thread_id
            .as_ref()
            .is_some_and(|id| uuid::Uuid::parse_str(id).is_err())
        {
            return Err(std::io::Error::other("invalid selected thread"));
        }
        let sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| std::io::Error::other("selection sequence exhausted"))?;
        let mut record = serde_json::to_vec(&serde_json::json!({
            "schema": "codex.tui.selection.v1",
            "pid": std::process::id(),
            "sequence": sequence,
            "thread_id": thread_id,
        }))?;
        record.push(b'\n');
        // POSIX guarantees atomic pipe writes up to at least 512 bytes. A full
        // pipe defers this observation; it cannot block the UI or split a record.
        if record.len() > 512 {
            return Err(std::io::Error::other("selection record too large"));
        }
        match self.pipe.write(&record) {
            Ok(written) if written == record.len() => {
                self.sequence = sequence;
                self.last = Some(thread_id);
                Ok(())
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(()),
            Err(error) => Err(error),
            _ => Err(std::io::Error::other("incomplete selection record")),
        }
    }
}

#[cfg(not(unix))]
struct Observer;

#[cfg(not(unix))]
impl Observer {
    fn from_descriptor(_: i32) -> std::io::Result<Self> {
        Err(std::io::Error::other(
            "selection observation requires a Unix pipe",
        ))
    }
    fn publish(&mut self, _: Option<String>) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::io::BufRead;
    use std::os::fd::AsRawFd;
    use std::os::fd::IntoRawFd;

    #[test]
    fn ui_retry_retains_descriptor_ownership_and_invalidates_old_selection() -> std::io::Result<()>
    {
        let (reader, writer) = std::io::pipe()?;
        let fd = writer.into_raw_fd();
        let mut registry = Registry {
            descriptor: None,
            observer: None,
        };
        registry.initialize(Some(fd))?;
        registry.publish(Some(uuid::Uuid::new_v4().to_string()));
        registry.initialize(Some(fd))?;
        assert!(registry.initialize(None).is_err());
        assert!(registry.initialize(Some(fd + 10000)).is_err());
        let next = uuid::Uuid::new_v4().to_string();
        registry.publish(Some(next.clone()));
        drop(registry);
        let records: Vec<serde_json::Value> = std::io::BufReader::new(reader)
            .lines()
            .map(|line| serde_json::from_str(&line.unwrap()).unwrap())
            .collect();
        assert_eq!(records.len(), 4);
        assert!(records[0]["thread_id"].is_null());
        assert!(records[2]["thread_id"].is_null());
        assert_eq!(records[3]["thread_id"], next);
        Ok(())
    }

    #[test]
    fn selection_switch_and_loss_are_ordered_without_replaying_unchanged_state()
    -> std::io::Result<()> {
        let (reader, writer) = std::io::pipe()?;
        let mut observer = Observer::from_descriptor(writer.into_raw_fd())?;
        let first = uuid::Uuid::new_v4().to_string();
        let second = uuid::Uuid::new_v4().to_string();
        observer.publish(Some(first.clone()))?;
        observer.publish(Some(first.clone()))?;
        observer.publish(None)?;
        observer.publish(Some(second.clone()))?;
        drop(observer);
        let records: Vec<serde_json::Value> = std::io::BufReader::new(reader)
            .lines()
            .map(|line| serde_json::from_str(&line.unwrap()).unwrap())
            .collect();
        assert_eq!(records.len(), 3);
        assert_eq!(records[0]["thread_id"], first);
        assert!(records[1]["thread_id"].is_null());
        assert_eq!(records[2]["thread_id"], second);
        for (index, record) in records.iter().enumerate() {
            assert_eq!(record["sequence"], index + 1);
            assert_eq!(record["pid"], std::process::id());
        }
        Ok(())
    }

    #[test]
    fn pipe_is_private_nonblocking_and_not_inherited_by_commands() -> std::io::Result<()> {
        let (_reader, writer) = std::io::pipe()?;
        let observer = Observer::from_descriptor(writer.into_raw_fd())?;
        let fd = observer.pipe.as_raw_fd();
        assert_ne!(
            unsafe { libc::fcntl(fd, libc::F_GETFD) } & libc::FD_CLOEXEC,
            0
        );
        assert_ne!(
            unsafe { libc::fcntl(fd, libc::F_GETFL) } & libc::O_NONBLOCK,
            0
        );
        let status = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(format!("test ! -e /dev/fd/{fd}"))
            .status()?;
        assert!(status.success());
        assert!(Observer::from_descriptor(1).is_err());
        let file = tempfile::tempfile()?;
        assert!(Observer::from_descriptor(file.as_raw_fd()).is_err());
        // Rejection did not take ownership of the unrelated file descriptor.
        assert!(file.metadata().is_ok());
        Ok(())
    }

    #[test]
    fn observer_backpressure_and_reader_loss_do_not_block_coding() -> std::io::Result<()> {
        use std::io::Read;
        use std::io::Write;
        let (mut reader, writer) = std::io::pipe()?;
        let mut observer = Observer::from_descriptor(writer.into_raw_fd())?;
        let block = [b'x'; 512];
        let mut filled = 0;
        loop {
            match observer.pipe.write(&block) {
                Ok(count) => filled += count,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) => return Err(error),
            }
        }
        let id = uuid::Uuid::new_v4().to_string();
        observer.publish(Some(id.clone()))?;
        assert_eq!(observer.sequence, 0);
        let mut drain = vec![0; filled];
        reader.read_exact(&mut drain)?;
        observer.publish(Some(id.clone()))?;
        drop(observer);
        let mut record = String::new();
        reader.read_to_string(&mut record)?;
        let record: serde_json::Value = serde_json::from_str(&record)?;
        assert_eq!(record["thread_id"], id);
        assert_eq!(record["sequence"], 1);

        let (reader, writer) = std::io::pipe()?;
        let mut observer = Observer::from_descriptor(writer.into_raw_fd())?;
        drop(reader);
        assert!(observer.publish(None).is_err());
        Ok(())
    }

    #[test]
    fn selection_is_local_only_for_owned_private_execution_sockets() -> std::io::Result<()> {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("execution.sock");
        let _listener = std::os::unix::net::UnixListener::bind(&path)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        let endpoint = crate::RemoteAppServerEndpoint::UnixSocket {
            socket_path: path.clone().try_into().expect("absolute test socket"),
        };
        assert!(!same_host(&crate::AppServerTarget::Remote {
            endpoint: endpoint.clone(),
        }));
        let target = crate::AppServerTarget::LocalDaemon {
            endpoint,
            allow_embedded_fallback: false,
        };
        assert!(same_host(&target));
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666))?;
        assert!(!same_host(&target));
        assert!(!same_host(&crate::AppServerTarget::Cloud));
        assert!(!same_host(&crate::AppServerTarget::Remote {
            endpoint: crate::RemoteAppServerEndpoint::WebSocket {
                websocket_url: "ws://127.0.0.1:1234".to_string(),
                auth_token: None,
            },
        }));
        Ok(())
    }
}
