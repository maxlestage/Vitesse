//! The engine of a thread (a `current_thread` runtime, on Unix: one per core
//! with `thread_per_core`, which Linux uses by default).
//!
//! Its plain HTTP/1.1 connections are watched by one epoll (kqueue) of their
//! own, registered once with tokio: when it is ready, every connection ready
//! with it is read, served and written in one go, without a task, a waker or
//! a timer per connection. That is the common case: a request received whole,
//! whose handler answers without waiting. Anything else (a body still
//! arriving, a handler that waits, a streamed response, an upgrade, HTTP/2,
//! an error to answer) hands the connection over, with its buffers, to the
//! asynchronous engine of `http1.rs`, which picks up where it was.

use std::io::{self, Write};
use std::net::SocketAddr;
use std::os::fd::{FromRawFd, IntoRawFd};
use std::task::{Context, Poll};

use mio::net::TcpStream;
use mio::{Events, Interest, Registry, Token};
use tokio::io::unix::AsyncFd;

use super::*;

/// Events read from the epoll at a time.
const EVENTS: usize = 1024;
/// How long the engine keeps looking for events after a busy turn, before
/// letting the thread sleep: more requests are usually on their way, and
/// waking a sleeping thread costs more than these looks (as Facile does).
const SPIN: Duration = Duration::from_micros(15);

/// A connection of the engine, and how long it has been quiet.
struct Slot {
    conn: Conn<TcpStream>,
    /// Periods of the timer without a request.
    quiet: u32,
    /// Registered for writing too: a response did not fit in the socket.
    writing: bool,
}

/// What [`process`] leaves to the caller.
enum Step {
    /// Every complete request is answered.
    Done,
    /// The write buffer is full: send it, then come back.
    Flush,
    /// The rest is for the asynchronous engine.
    Handoff(Resume),
}

pub(crate) struct Engine {
    /// The epoll of the connections, registered with tokio's.
    poll: AsyncFd<mio::Poll>,
    inner: Inner,
}

struct Inner {
    events: Events,
    slots: Vec<Option<Slot>>,
    free: Vec<usize>,
    len: usize,
    app: &'static AppService,
    state: &'static ServerState,
}

impl Engine {
    /// An engine for the current thread, if it serves its connections alone
    /// (a `current_thread` runtime): on tokio's multi-threaded runtime, one
    /// epoll would put the connections of every core on one of them.
    pub(crate) fn for_this_thread(
        app: &'static AppService,
        state: &'static ServerState,
    ) -> Option<Engine> {
        let flavor = tokio::runtime::Handle::try_current().ok()?.runtime_flavor();
        if flavor != tokio::runtime::RuntimeFlavor::CurrentThread {
            return None;
        }
        // `with_interest` plutôt que `register_with_interest` (tokio 1.54) :
        // les versions de tokio plus anciennes n'ont que lui. Le `Poll` lui
        // appartient, son descripteur vit donc aussi longtemps que lui.
        #[allow(deprecated)]
        let poll =
            AsyncFd::with_interest(mio::Poll::new().ok()?, tokio::io::Interest::READABLE).ok()?;
        Some(Engine {
            poll,
            inner: Inner {
                events: Events::with_capacity(EVENTS),
                slots: Vec::new(),
                free: Vec::new(),
                len: 0,
                app,
                state,
            },
        })
    }

    /// Open connections.
    pub(crate) fn len(&self) -> usize {
        self.inner.len
    }

    /// Takes a connection just accepted.
    pub(crate) fn add(&mut self, stream: tokio::net::TcpStream, peer: Option<SocketAddr>) {
        // Retiré de l'epoll de tokio, ajouté à celui du moteur ; une erreur
        // ferme la connexion, comme une erreur de lecture.
        let Ok(std) = stream.into_std() else { return };
        let mut io = TcpStream::from_std(std);
        let inner = &mut self.inner;
        let index = inner.free.pop().unwrap_or_else(|| {
            inner.slots.push(None);
            inner.slots.len() - 1
        });
        if self
            .poll
            .get_ref()
            .registry()
            .register(&mut io, Token(index), Interest::READABLE)
            .is_err()
        {
            inner.free.push(index);
            return;
        }
        let conn = Conn::new(io, peer, false, inner.app, inner.state);
        inner.slots[index] = Some(Slot {
            conn,
            quiet: 0,
            writing: false,
        });
        inner.len += 1;
    }

    /// Waits until connections are ready, and serves them. Gives back the
    /// connections to hand over to tasks of their own.
    pub(crate) async fn turn(&mut self) -> Vec<Handoff> {
        let Ok(mut guard) = self.poll.readable_mut().await else {
            return Vec::new();
        };
        let inner = &mut self.inner;
        let poll = guard.get_inner_mut();
        let (handoffs, drained) =
            std::future::poll_fn(|cx| Poll::Ready(inner.drain(poll, cx, true))).await;
        if drained {
            guard.clear_ready();
        }
        handoffs
    }

    /// One period of the inactivity timer: closes the idle connections, and
    /// answers `408` to those whose request head does not come.
    pub(crate) fn tick(&mut self, idle_ticks: u32, head_ticks: u32) -> Vec<Handoff> {
        let registry = self.poll.get_ref().registry();
        let inner = &mut self.inner;
        let shutdown = inner.state.shutdown.load(Relaxed);
        let mut handoffs = Vec::new();
        for index in 0..inner.slots.len() {
            let Some(slot) = inner.slots[index].as_mut() else {
                continue;
            };
            slot.quiet += 1;
            let limit = if slot.conn.rbuf.is_empty() {
                idle_ticks
            } else {
                head_ticks
            };
            if slot.quiet < limit && !shutdown {
                continue;
            }
            if slot.writing {
                // Une réponse part encore : la connexion attendra la suivante.
                continue;
            }
            if slot.conn.rbuf.is_empty() {
                inner.close(registry, index);
            } else if let Some(h) =
                inner.handoff(registry, index, Resume::Fail(StatusCode::REQUEST_TIMEOUT))
            {
                handoffs.push(h);
            }
        }
        handoffs
    }

    /// Closes the connections without a response on the way (graceful
    /// shutdown); true if none is left.
    pub(crate) fn close_idle(&mut self) -> bool {
        let registry = self.poll.get_ref().registry();
        let inner = &mut self.inner;
        for index in 0..inner.slots.len() {
            if inner.slots[index]
                .as_ref()
                .is_some_and(|slot| !slot.writing)
            {
                inner.close(registry, index);
            }
        }
        inner.len == 0
    }
}

impl Inner {
    /// Serves every connection ready. Also says whether the epoll was emptied.
    ///
    /// After a busy turn, with `spin`, it looks again for [`SPIN`]: work that
    /// arrives then is served, and the epoll is said not emptied, so that
    /// tokio comes back right after its other tasks.
    fn drain(
        &mut self,
        poll: &mut mio::Poll,
        cx: &mut Context<'_>,
        spin: bool,
    ) -> (Vec<Handoff>, bool) {
        let mut handoffs = Vec::new();
        let mut ready = Vec::new();
        let mut busy = false;
        let mut spin_from = None;
        loop {
            match poll.poll(&mut self.events, Some(Duration::ZERO)) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => return (handoffs, true),
            }
            ready.clear();
            ready.extend(self.events.iter().map(|event| event.token().0));
            if ready.is_empty() {
                if !spin || !busy {
                    return (handoffs, true);
                }
                let start = *spin_from.get_or_insert_with(std::time::Instant::now);
                if start.elapsed() >= SPIN {
                    return (handoffs, true);
                }
                std::hint::spin_loop();
                continue;
            }
            for &index in &ready {
                self.serve(poll.registry(), index, cx, &mut handoffs);
            }
            if spin_from.is_some() {
                // Du travail arrivé pendant l'attente : tokio reprend la main
                // pour ses autres tâches, et revient aussitôt.
                return (handoffs, false);
            }
            busy = true;
            if ready.len() < EVENTS && !spin {
                return (handoffs, true);
            }
        }
    }

    /// Serves a connection: what it has to send, the requests it has
    /// received, then what it sends next, until its socket is empty.
    fn serve(
        &mut self,
        registry: &Registry,
        index: usize,
        cx: &mut Context<'_>,
        handoffs: &mut Vec<Handoff>,
    ) {
        let mut drained = false;
        loop {
            match self.flush(registry, index) {
                Ok(true) => {}
                Ok(false) => return,
                Err(_) => return self.close(registry, index),
            }
            let Some(slot) = self.slots[index].as_mut() else {
                return;
            };
            match process(&mut slot.conn, &mut slot.quiet, cx) {
                Step::Done => {}
                Step::Flush => continue,
                Step::Handoff(resume) => {
                    handoffs.extend(self.handoff(registry, index, resume));
                    return;
                }
            }
            match self.flush(registry, index) {
                Ok(true) => {}
                Ok(false) => return,
                Err(_) => return self.close(registry, index),
            }
            if drained {
                return;
            }
            let Some(slot) = self.slots[index].as_mut() else {
                return;
            };
            let conn = &mut slot.conn;
            if conn.rbuf.len() > MAX_HEAD {
                // Le moteur asynchrone répond `431`.
                handoffs.extend(self.handoff(registry, index, Resume::Parse));
                return;
            }
            if conn.rbuf.capacity() - conn.rbuf.len() < 1024 {
                conn.rbuf.reserve(READ_SIZE);
            }
            let spare = conn.rbuf.spare_capacity_mut();
            let room = spare.len();
            match socket2::SockRef::from(&conn.io).recv(spare) {
                Ok(0) => return self.close(registry, index),
                Ok(n) => {
                    // SAFETY: `recv` a initialisé les `n` premiers octets de la réserve.
                    unsafe { conn.rbuf.set_len(conn.rbuf.len() + n) };
                    // Lu moins que la place : le socket est vide (epoll en
                    // mode front préviendra des octets suivants).
                    drained = n < room;
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => return,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(_) => return self.close(registry, index),
            }
        }
    }

    /// Sends the write buffer. `Ok(false)`: the socket is full, the
    /// connection now waits until it can write.
    fn flush(&mut self, registry: &Registry, index: usize) -> io::Result<bool> {
        let Some(slot) = self.slots[index].as_mut() else {
            return Ok(false);
        };
        let conn = &mut slot.conn;
        let mut sent = 0;
        while sent < conn.wbuf.len() {
            match conn.io.write(&conn.wbuf[sent..]) {
                Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
                Ok(n) => sent += n,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        conn.wbuf.drain(..sent);
        let done = conn.wbuf.is_empty();
        if done && conn.wbuf.capacity() > 1024 * 1024 {
            conn.wbuf.shrink_to(READ_SIZE);
        }
        if done != !slot.writing {
            slot.writing = !done;
            let interest = if done {
                Interest::READABLE
            } else {
                Interest::READABLE | Interest::WRITABLE
            };
            registry.reregister(&mut slot.conn.io, Token(index), interest)?;
        }
        Ok(done)
    }

    /// Hands a connection over to the asynchronous engine; `None` if tokio
    /// cannot take it (the connection is then closed).
    fn handoff(&mut self, registry: &Registry, index: usize, resume: Resume) -> Option<Handoff> {
        let mut slot = self.slots[index].take()?;
        self.free.push(index);
        self.len -= 1;
        let _ = registry.deregister(&mut slot.conn.io);
        let conn = slot
            .conn
            .try_map_io(|io| {
                // SAFETY: le descripteur est celui d'un socket TCP, dont on prend la propriété.
                let std = unsafe { std::net::TcpStream::from_raw_fd(io.into_raw_fd()) };
                tokio::net::TcpStream::from_std(std)
            })
            .ok()?;
        Some(Handoff(conn, resume))
    }

    fn close(&mut self, registry: &Registry, index: usize) {
        if let Some(mut slot) = self.slots[index].take() {
            let _ = registry.deregister(&mut slot.conn.io);
            self.free.push(index);
            self.len -= 1;
        }
    }
}

/// Answers the complete requests of the read buffer whose handler answers
/// at once, like the inner loop of [`Conn::run`]. `quiet` starts again at
/// each request.
fn process(conn: &mut Conn<TcpStream>, quiet: &mut u32, cx: &mut Context<'_>) -> Step {
    date::refresh();
    let mut busy = None;
    loop {
        let head = match parse_head(&conn.rbuf, &mut conn.raw) {
            Ok(Some(head)) => head,
            Ok(None) => return Step::Done,
            // Une erreur, ou le préambule de HTTP/2 : le moteur asynchrone s'en charge.
            Err(_) => return Step::Handoff(Resume::Parse),
        };
        let body_ready = match head.framing {
            Framing::None => true,
            Framing::Length(n) => n <= (conn.rbuf.len() - head.len) as u64,
            Framing::Chunked => false,
        };
        if !body_ready {
            return Step::Handoff(Resume::Parse);
        }
        conn.served += 1;
        *quiet = 0;
        busy.get_or_insert_with(|| Busy::new(conn.state));
        let mut req = Request::from_wire(
            &conn.rbuf[..head.len],
            head.method.clone(),
            head.version,
            head.target,
            &conn.raw,
            conn.peer,
            conn.secure,
            &conn.app.shared,
        );
        if head.conditional {
            req.mark_conditional();
        }
        conn.rbuf.advance(head.len);
        let Some(body) = conn.ready_body(head.framing) else {
            unreachable!("the body is in the buffer");
        };
        req.put_body(body);
        let mut job = conn.job(req, &head);
        let mut res = match Pin::new(&mut job.fut).poll(cx) {
            Poll::Ready(res) => res,
            // Le handler attend (base de données…) : une tâche à lui.
            Poll::Pending => return Step::Handoff(Resume::Job(job)),
        };
        let (keep_alive, rest) = conn.encode(&mut res, &job);
        match rest {
            Rest::None => {}
            Rest::Bytes(bytes) if bytes.len() <= WRITE_HIGH_WATER => {
                conn.wbuf.extend_from_slice(&bytes)
            }
            rest => return Step::Handoff(Resume::Rest(rest, keep_alive)),
        }
        if !keep_alive || conn.upgrade.is_some() {
            // Fermer proprement (ou changer de protocole) : le moteur asynchrone.
            return Step::Handoff(Resume::Rest(Rest::None, false));
        }
        if conn.wbuf.len() >= WRITE_HIGH_WATER {
            return Step::Flush;
        }
    }
}
