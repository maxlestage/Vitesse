//! Vitesse's HTTP/1.1 engine.
//!
//! One connection = one task, with a read buffer and a write buffer that are
//! reused from one request to the next:
//!
//! - the request head is parsed by `httparse` (SIMD) and the headers stay in
//!   the read buffer: only their positions are recorded;
//! - a handler that responds without waiting writes its response straight
//!   into the write buffer: all the responses to a batch of pipelined
//!   requests go out in a single system call;
//! - a single timer per connection (not per request) handles inactivity;
//! - the `Date` header is cached per thread.

use std::cell::RefCell;
use std::future::{Future, poll_fn};
use std::io::{self, IoSlice};
use std::mem::MaybeUninit;
use std::net::SocketAddr;
use std::pin::{Pin, pin};
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicUsize, Ordering::Relaxed};
use std::task::{Context, Poll};
use std::time::{Duration, SystemTime};

use bytes::{Buf, Bytes, BytesMut};
use http::header::{self, HeaderValue};
use http::{Method, StatusCode, Version};
use http_body::{Body as _, Frame, SizeHint};
use http_body_util::BodyExt;
use http_body_util::combinators::UnsyncBoxBody;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::Instant;

use crate::app::AppService;
use crate::body::{Body, BoxError, Kind};
use crate::request::{RawHeaders, ReqBody, Request};
use crate::response::{Response, header_pool};
use crate::server::ResponseFuture;
use crate::util::http_date;

/// Maximum number of headers per request.
const MAX_HEADERS: usize = 64;
/// Maximum size of a request head (positions fit in 16 bits).
const MAX_HEAD: usize = 60 * 1024;
/// Read size.
const READ_SIZE: usize = 8 * 1024;
/// Bodies up to this size are read in full before calling the handler
/// (larger ones are streamed).
const EAGER_BODY: u64 = 64 * 1024;
/// Response bodies up to this size are copied into the write buffer.
const INLINE_BODY: usize = 16 * 1024;
/// Beyond this size, the write buffer is flushed even in the middle of a
/// batch.
const WRITE_HIGH_WATER: usize = 64 * 1024;
/// Period of the inactivity timer.
const TICK: Duration = Duration::from_secs(15);
/// The connection is closed after ~60 s without a request…
const IDLE_TICKS: u32 = 4;
/// … or ~30 s without a complete head.
const HEAD_TICKS: u32 = 2;

/// State shared between the accept loop and its connections.
pub(crate) struct ServerState {
    pub(crate) shutdown: AtomicBool,
    /// Request batches in progress, counted per thread to avoid any
    /// contention between cores (only the sum of the counters is meaningful).
    busy: [Shard; SHARDS],
}

const SHARDS: usize = 32;

#[derive(Default)]
#[repr(align(128))]
struct Shard(AtomicIsize);

impl Default for ServerState {
    fn default() -> Self {
        ServerState {
            shutdown: AtomicBool::new(false),
            busy: std::array::from_fn(|_| Shard::default()),
        }
    }
}

impl ServerState {
    /// Number of connections currently processing requests.
    pub(crate) fn busy(&self) -> isize {
        self.busy.iter().map(|s| s.0.load(Relaxed)).sum()
    }

    #[inline]
    fn shard(&self) -> &AtomicIsize {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        thread_local! {
            static INDEX: usize = NEXT.fetch_add(1, Relaxed) % SHARDS;
        }
        &self.busy[INDEX.with(|i| *i)].0
    }
}

/// Marks the connection as busy (for graceful shutdown).
struct Busy(&'static ServerState);

impl Busy {
    #[inline]
    fn new(state: &'static ServerState) -> Self {
        state.shard().fetch_add(1, Relaxed);
        Busy(state)
    }
}

impl Drop for Busy {
    #[inline]
    fn drop(&mut self) {
        self.0.shard().fetch_sub(1, Relaxed);
    }
}

/// Serves a connection until it is closed.
pub(crate) async fn serve_connection(
    io: TcpStream,
    peer: SocketAddr,
    app: &'static AppService,
    state: &'static ServerState,
) {
    let mut conn = Conn {
        io,
        rbuf: BytesMut::with_capacity(READ_SIZE),
        wbuf: Vec::with_capacity(READ_SIZE),
        peer,
        app,
        state,
        linger: false,
        raw: RawHeaders::default(),
    };
    if conn.run().await.is_ok() && conn.linger {
        conn.linger_close().await;
    }
}

struct Conn {
    io: TcpStream,
    rbuf: BytesMut,
    wbuf: Vec<u8>,
    peer: SocketAddr,
    app: &'static AppService,
    state: &'static ServerState,
    /// The client is still sending a body that nobody has read.
    linger: bool,
    /// Positions of the headers of the request being parsed.
    raw: RawHeaders,
}

/// A request whose response is being prepared.
struct Job {
    fut: ResponseFuture,
    is_head: bool,
    version: Version,
    keep_alive: bool,
}

/// What is left to send after the response head.
enum Rest {
    None,
    Bytes(Bytes),
    Stream(UnsyncBoxBody<Bytes, BoxError>, bool),
}

enum Event {
    Read(io::Result<usize>),
    Tick,
}

impl Conn {
    async fn run(&mut self) -> io::Result<()> {
        let mut tick = pin!(tokio::time::sleep(TICK));
        let mut quiet_ticks = 0;
        loop {
            date::refresh();
            // Toutes les requêtes complètes déjà reçues, en un seul lot.
            let mut busy = None;
            loop {
                let head = match parse_head(&self.rbuf, &mut self.raw) {
                    Ok(Some(head)) => head,
                    Ok(None) => break,
                    Err(status) => return self.fail(status).await,
                };
                busy.get_or_insert_with(|| Busy::new(self.state));
                quiet_ticks = 0;
                let mut req = Request::from_wire(
                    &self.rbuf[..head.len],
                    head.method.clone(),
                    head.version,
                    head.target,
                    &self.raw,
                    Some(self.peer),
                    &self.app.shared,
                );
                self.rbuf.advance(head.len);
                let keep_alive = match self.ready_body(head.framing) {
                    Some(body) => {
                        req.put_body(body);
                        let mut job = self.job(req, &head);
                        match &mut poll_once(&mut job.fut).await {
                            // Chemin rapide : le handler a répondu sans attendre.
                            Some(res) => {
                                let (keep_alive, rest) = self.encode(res, &job);
                                if !matches!(rest, Rest::None) {
                                    self.write_rest(rest).await?;
                                }
                                keep_alive
                            }
                            None => self.finish(job).await?,
                        }
                    }
                    None => self.handle_with_body(req, head).await?,
                };
                if !keep_alive {
                    return self.flush().await;
                }
                if self.wbuf.len() >= WRITE_HIGH_WATER {
                    self.flush().await?;
                }
            }
            if !self.wbuf.is_empty() {
                self.flush().await?;
            }
            drop(busy);
            if self.rbuf.len() > MAX_HEAD {
                return self.fail(StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE).await;
            }
            if self.rbuf.capacity() - self.rbuf.len() < 1024 {
                self.rbuf.reserve(READ_SIZE);
            }

            let event = tokio::select! {
                biased;
                read = self.io.read_buf(&mut self.rbuf) => Event::Read(read),
                () = &mut tick => Event::Tick,
            };
            match event {
                Event::Read(Ok(0)) => return Ok(()),
                Event::Read(Ok(_)) => {}
                Event::Read(Err(e)) => return Err(e),
                Event::Tick => {
                    quiet_ticks += 1;
                    let limit = if self.rbuf.is_empty() {
                        IDLE_TICKS
                    } else {
                        HEAD_TICKS
                    };
                    if quiet_ticks >= limit || self.state.shutdown.load(Relaxed) {
                        if !self.rbuf.is_empty() {
                            return self.fail(StatusCode::REQUEST_TIMEOUT).await;
                        }
                        return Ok(());
                    }
                    tick.as_mut().reset(Instant::now() + TICK);
                }
            }
        }
    }

    /// The request body, if it has already been fully received (the common
    /// case).
    #[inline]
    fn ready_body(&mut self, framing: Framing) -> Option<ReqBody> {
        match framing {
            Framing::None => Some(ReqBody::Empty),
            Framing::Length(n) if n <= self.rbuf.len() as u64 => {
                Some(ReqBody::Buffered(self.rbuf.split_to(n as usize).freeze()))
            }
            _ => None,
        }
    }

    #[inline]
    fn job(&self, req: Request, head: &ParsedHead) -> Job {
        Job {
            fut: ResponseFuture::new(self.app, req),
            is_head: head.method == Method::HEAD,
            version: head.version,
            keep_alive: head.keep_alive,
        }
    }

    /// The handler is waiting (database…): send the previous responses
    /// right away, then wait for its own.
    async fn finish(&mut self, mut job: Job) -> io::Result<bool> {
        self.flush().await?;
        let mut res = (&mut job.fut).await;
        date::refresh();
        let (keep_alive, rest) = self.encode(&mut res, &job);
        self.write_rest(rest).await?;
        Ok(keep_alive)
    }

    /// A request whose body has not (fully) arrived yet.
    async fn handle_with_body(&mut self, mut req: Request, head: ParsedHead) -> io::Result<bool> {
        let framing = head.framing;
        if head.expect_continue {
            self.wbuf
                .extend_from_slice(b"HTTP/1.1 100 Continue\r\n\r\n");
            self.flush().await?;
        }
        let mut feeder = None;
        let body = match framing {
            Framing::Length(n) if n <= EAGER_BODY => {
                // Petit corps : on le lit en entier avant d'appeler le handler.
                let n = n as usize;
                while self.rbuf.len() < n {
                    self.rbuf.reserve(n - self.rbuf.len());
                    if self.io.read_buf(&mut self.rbuf).await? == 0 {
                        return Err(io::ErrorKind::UnexpectedEof.into());
                    }
                }
                ReqBody::Buffered(self.rbuf.split_to(n).freeze())
            }
            framing => {
                // Gros corps ou `chunked` : transmis au handler au fil de l'eau.
                let (tx, rx) = mpsc::channel(1);
                let size = match framing {
                    Framing::Length(n) => Some(n),
                    _ => None,
                };
                feeder = Some(Feeder {
                    tx: Some(tx),
                    decoder: Decoder::new(framing),
                });
                ReqBody::Stream(Body::wrap(IncomingBody {
                    rx,
                    remaining: size,
                    done: false,
                }))
            }
        };

        req.put_body(body);
        let mut job = self.job(req, &head);
        let mut res = match &mut feeder {
            None => match poll_once(&mut job.fut).await {
                Some(res) => res,
                None => {
                    self.flush().await?;
                    (&mut job.fut).await
                }
            },
            Some(feeder) => self.drive(&mut job.fut, feeder).await,
        };
        date::refresh();
        if feeder.is_some_and(|f| !f.decoder.is_done()) {
            // Corps non lu jusqu'au bout : impossible de lire la requête suivante.
            job.keep_alive = false;
            self.linger = true;
        }
        let (keep_alive, rest) = self.encode(&mut res, &job);
        self.write_rest(rest).await?;
        Ok(keep_alive)
    }

    /// Drives the handler while feeding it the request body.
    async fn drive(&mut self, fut: &mut ResponseFuture, feeder: &mut Feeder) -> Response {
        if !self.wbuf.is_empty() {
            let _ = self.flush().await;
        }
        loop {
            if feeder.tx.is_none() {
                return fut.await;
            }
            tokio::select! {
                biased;
                res = &mut *fut => return res,
                () = feeder.feed(&mut self.io, &mut self.rbuf) => {}
            }
        }
    }

    /// Gracefully closes a connection whose client is still sending data:
    /// closing right away would trigger an `RST`, which could make the client
    /// lose the response. So the write side is shut down and whatever still
    /// arrives is ignored, for at most 2 s and 8 MiB.
    async fn linger_close(&mut self) {
        if self.io.shutdown().await.is_err() {
            return;
        }
        let deadline = tokio::time::sleep(Duration::from_secs(2));
        let mut deadline = pin!(deadline);
        let mut drained = 0;
        let mut buf = [0u8; 16 * 1024];
        while drained < 8 * 1024 * 1024 {
            tokio::select! {
                read = self.io.read(&mut buf) => match read {
                    Ok(0) | Err(_) => return,
                    Ok(n) => drained += n,
                },
                () = &mut deadline => return,
            }
        }
    }

    /// Responds to an invalid request, then closes the connection.
    async fn fail(&mut self, status: StatusCode) -> io::Result<()> {
        // Le client peut être en train d'envoyer la suite de sa requête.
        self.linger = true;
        write_status(&mut self.wbuf, status);
        date::write(&mut self.wbuf);
        self.wbuf
            .extend_from_slice(b"content-length: 0\r\nconnection: close\r\n\r\n");
        self.flush().await
    }

    async fn flush(&mut self) -> io::Result<()> {
        if !self.wbuf.is_empty() {
            self.io.write_all(&self.wbuf).await?;
            self.wbuf.clear();
            if self.wbuf.capacity() > 1024 * 1024 {
                self.wbuf.shrink_to(READ_SIZE);
            }
        }
        Ok(())
    }

    /// Sends the write buffer followed by `data`, in a single system call if
    /// possible.
    async fn write_with(&mut self, data: &[u8]) -> io::Result<()> {
        let mut head: &[u8] = &self.wbuf;
        let mut data = data;
        while !head.is_empty() || !data.is_empty() {
            let n = if head.is_empty() {
                self.io.write(data).await?
            } else {
                self.io
                    .write_vectored(&[IoSlice::new(head), IoSlice::new(data)])
                    .await?
            };
            if n == 0 {
                return Err(io::ErrorKind::WriteZero.into());
            }
            if n < head.len() {
                head = &head[n..];
            } else {
                data = &data[n - head.len()..];
                head = &[];
            }
        }
        self.wbuf.clear();
        Ok(())
    }

    /// Serializes the response head (and a small body) into the write
    /// buffer. Returns whether to keep the connection alive, and what is left
    /// to send.
    fn encode(&mut self, res: &mut Response, job: &Job) -> (bool, Rest) {
        res.log_server_error();
        let status = res.status;
        let ctype = res.ctype;
        let headers = res.headers.take();
        let body = std::mem::take(&mut res.body);
        let mut keep_alive = job.keep_alive && !self.state.shutdown.load(Relaxed);
        let buf = &mut self.wbuf;

        write_status(buf, status);
        let mut user_length: Option<&HeaderValue> = None;
        match &headers {
            Some(map) => {
                for (name, value) in map.iter() {
                    if name == header::CONTENT_LENGTH {
                        user_length = Some(value);
                        continue;
                    }
                    if name == header::TRANSFER_ENCODING {
                        continue;
                    }
                    if name == header::CONNECTION {
                        if value.as_bytes().eq_ignore_ascii_case(b"close") {
                            keep_alive = false;
                        }
                        continue;
                    }
                    buf.extend_from_slice(name.as_str().as_bytes());
                    buf.extend_from_slice(b": ");
                    buf.extend_from_slice(value.as_bytes());
                    buf.extend_from_slice(b"\r\n");
                }
            }
            // Le cas courant : un type de contenu connu, écrit d'un bloc.
            None => buf.extend_from_slice(ctype.line()),
        }
        date::write(buf);

        let bodyless = status.is_informational()
            || status == StatusCode::NO_CONTENT
            || status == StatusCode::NOT_MODIFIED;
        let mut chunked = false;
        if !bodyless {
            let exact = body.size_hint().exact();
            if let Some(len) = user_length.filter(|_| job.is_head || exact.is_none()) {
                // Taille annoncée par le handler (réponse à HEAD, flux de taille connue).
                buf.extend_from_slice(b"content-length: ");
                buf.extend_from_slice(len.as_bytes());
                buf.extend_from_slice(b"\r\n");
            } else if let Some(n) = exact {
                push_content_length(buf, n);
            } else if job.is_head {
                // Taille inconnue : rien à annoncer.
            } else if job.version == Version::HTTP_11 {
                buf.extend_from_slice(b"transfer-encoding: chunked\r\n");
                chunked = true;
            } else {
                // HTTP/1.0 sans taille connue : la fin du corps = la fermeture.
                keep_alive = false;
            }
        }
        if !keep_alive {
            buf.extend_from_slice(b"connection: close\r\n\r\n");
        } else if job.version == Version::HTTP_10 {
            buf.extend_from_slice(b"connection: keep-alive\r\n\r\n");
        } else {
            buf.extend_from_slice(b"\r\n");
        }

        if let Some(map) = headers {
            header_pool::recycle(map);
        }
        if bodyless || job.is_head {
            return (keep_alive, Rest::None);
        }
        let rest = match body.into_kind() {
            Kind::Empty => Rest::None,
            Kind::Full(bytes) if bytes.len() <= INLINE_BODY => {
                buf.extend_from_slice(&bytes);
                Rest::None
            }
            Kind::Full(bytes) => Rest::Bytes(bytes),
            Kind::Stream(stream) => Rest::Stream(stream, chunked),
        };
        (keep_alive, rest)
    }

    async fn write_rest(&mut self, rest: Rest) -> io::Result<()> {
        match rest {
            Rest::None => Ok(()),
            Rest::Bytes(bytes) => self.write_with(&bytes).await,
            Rest::Stream(stream, chunked) => self.write_stream(stream, chunked).await,
        }
    }

    /// Sends a streamed body, chunk by chunk (each chunk goes out right away:
    /// ideal for files as well as for events).
    async fn write_stream<B>(&mut self, mut body: B, chunked: bool) -> io::Result<()>
    where
        B: http_body::Body<Data = Bytes, Error = BoxError> + Unpin,
    {
        while let Some(frame) = body.frame().await {
            let frame = frame.map_err(io::Error::other)?;
            let Ok(data) = frame.into_data() else {
                continue;
            };
            if data.is_empty() {
                continue;
            }
            if chunked {
                push_hex(&mut self.wbuf, data.len());
                self.wbuf.extend_from_slice(b"\r\n");
            }
            if data.len() <= INLINE_BODY {
                self.wbuf.extend_from_slice(&data);
                if chunked {
                    self.wbuf.extend_from_slice(b"\r\n");
                }
                self.flush().await?;
            } else {
                self.write_with(&data).await?;
                if chunked {
                    self.wbuf.extend_from_slice(b"\r\n");
                }
            }
        }
        if chunked {
            self.wbuf.extend_from_slice(b"0\r\n\r\n");
        }
        Ok(())
    }
}

/// Polls a `Future` exactly once.
#[inline]
fn poll_once<F: Future + Unpin>(fut: &mut F) -> impl Future<Output = Option<F::Output>> + '_ {
    poll_fn(move |cx| match Pin::new(&mut *fut).poll(cx) {
        Poll::Ready(v) => Poll::Ready(Some(v)),
        Poll::Pending => Poll::Ready(None),
    })
}

// ----- Analyse de la tête --------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Framing {
    None,
    Length(u64),
    Chunked,
}

struct ParsedHead {
    /// Size of the head in the read buffer.
    len: usize,
    method: Method,
    version: Version,
    target: (usize, usize),
    framing: Framing,
    keep_alive: bool,
    expect_continue: bool,
}

/// Parses the head of a request if it is complete; the positions of the
/// headers are written into `raw`. The buffer is not modified.
fn parse_head(rbuf: &[u8], raw: &mut RawHeaders) -> Result<Option<ParsedHead>, StatusCode> {
    if rbuf.is_empty() {
        return Ok(None);
    }
    let mut headers = [const { MaybeUninit::<httparse::Header<'_>>::uninit() }; MAX_HEADERS];
    let mut req = httparse::Request::new(&mut []);
    let len = match req.parse_with_uninit_headers(rbuf, &mut headers) {
        Ok(httparse::Status::Complete(len)) => len,
        Ok(httparse::Status::Partial) => return Ok(None),
        Err(httparse::Error::TooManyHeaders) => {
            return Err(StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE);
        }
        Err(_) => return Err(StatusCode::BAD_REQUEST),
    };
    if len > MAX_HEAD {
        return Err(StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE);
    }

    let base = rbuf.as_ptr() as usize;
    let offset = |s: &[u8]| s.as_ptr() as usize - base;

    let method = Method::from_bytes(req.method.unwrap_or_default().as_bytes())
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let version = if req.version == Some(0) {
        Version::HTTP_10
    } else {
        Version::HTTP_11
    };
    let path = req.path.unwrap_or("/").as_bytes();
    let target = (offset(path), offset(path) + path.len());

    raw.clear();
    let mut length: Option<u64> = None;
    let mut transfer_encoding = false;
    let mut chunked = false;
    let (mut close, mut keep) = (false, false);
    let mut expect_continue = false;
    for h in req.headers.iter() {
        let name = h.name;
        raw.push(
            offset(name.as_bytes()),
            name.len(),
            offset(h.value),
            h.value.len(),
        );
        match name.len() {
            14 if name.eq_ignore_ascii_case("content-length") => {
                let n = parse_decimal(h.value).ok_or(StatusCode::BAD_REQUEST)?;
                if length.is_some_and(|l| l != n) {
                    return Err(StatusCode::BAD_REQUEST);
                }
                length = Some(n);
            }
            17 if name.eq_ignore_ascii_case("transfer-encoding") => {
                transfer_encoding = true;
                // Seul le dernier codage compte, et il doit être `chunked`.
                chunked = h
                    .value
                    .rsplit(|&b| b == b',')
                    .next()
                    .is_some_and(|last| last.trim_ascii().eq_ignore_ascii_case(b"chunked"));
            }
            10 if name.eq_ignore_ascii_case("connection") => {
                for token in h.value.split(|&b| b == b',') {
                    let token = token.trim_ascii();
                    close |= token.eq_ignore_ascii_case(b"close");
                    keep |= token.eq_ignore_ascii_case(b"keep-alive");
                }
            }
            6 if name.eq_ignore_ascii_case("expect") => {
                expect_continue = h.value.trim_ascii().eq_ignore_ascii_case(b"100-continue");
            }
            _ => {}
        }
    }

    let framing = if transfer_encoding {
        // `Transfer-Encoding` + `Content-Length` : refusé (contrebande de requêtes).
        if length.is_some() {
            return Err(StatusCode::BAD_REQUEST);
        }
        if !chunked {
            return Err(StatusCode::NOT_IMPLEMENTED);
        }
        Framing::Chunked
    } else {
        match length {
            Some(0) | None => Framing::None,
            Some(n) => Framing::Length(n),
        }
    };
    let keep_alive = match version {
        Version::HTTP_11 => !close,
        _ => keep && !close,
    };

    Ok(Some(ParsedHead {
        len,
        method,
        version,
        target,
        framing,
        keep_alive,
        expect_continue,
    }))
}

fn parse_decimal(value: &[u8]) -> Option<u64> {
    let value = value.trim_ascii();
    if value.is_empty() || value.len() > 19 {
        return None;
    }
    value.iter().try_fold(0u64, |acc, &b| {
        b.is_ascii_digit().then(|| acc * 10 + u64::from(b - b'0'))
    })
}

// ----- Corps des requêtes en flux ------------------------------------------

/// A message sent from the engine to the body read by the handler.
enum Msg {
    Data(Bytes),
    End,
    Error(io::Error),
}

/// Reads the body from the network and forwards it to the handler.
struct Feeder {
    tx: Option<mpsc::Sender<Msg>>,
    decoder: Decoder,
}

impl Feeder {
    /// Forwards a chunk (or the end) of the body.
    async fn feed(&mut self, io: &mut TcpStream, rbuf: &mut BytesMut) {
        let Some(tx) = &self.tx else { return };
        let msg = loop {
            match self.decoder.decode(rbuf) {
                Ok(Decoded::Data(chunk)) => break Msg::Data(chunk),
                Ok(Decoded::Done) => break Msg::End,
                Ok(Decoded::NeedMore) => {
                    if rbuf.capacity() - rbuf.len() < 1024 {
                        rbuf.reserve(READ_SIZE);
                    }
                    match io.read_buf(rbuf).await {
                        Ok(0) => break Msg::Error(io::ErrorKind::UnexpectedEof.into()),
                        Ok(_) => {}
                        Err(e) => break Msg::Error(e),
                    }
                }
                Err(e) => break Msg::Error(e),
            }
        };
        let last = !matches!(msg, Msg::Data(_));
        // Si le handler a lâché le corps, on arrête de le lire.
        if tx.send(msg).await.is_err() || last {
            self.tx = None;
        }
    }
}

/// The body of a request, as seen by the handler.
struct IncomingBody {
    rx: mpsc::Receiver<Msg>,
    remaining: Option<u64>,
    done: bool,
}

impl http_body::Body for IncomingBody {
    type Data = Bytes;
    type Error = io::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, io::Error>>> {
        if self.done {
            return Poll::Ready(None);
        }
        let msg = std::task::ready!(self.rx.poll_recv(cx));
        Poll::Ready(match msg {
            Some(Msg::Data(data)) => {
                if let Some(remaining) = &mut self.remaining {
                    *remaining = remaining.saturating_sub(data.len() as u64);
                }
                Some(Ok(Frame::data(data)))
            }
            Some(Msg::End) => {
                self.done = true;
                None
            }
            Some(Msg::Error(e)) => {
                self.done = true;
                Some(Err(e))
            }
            None => {
                self.done = true;
                Some(Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    "connection aborted",
                )))
            }
        })
    }

    fn is_end_stream(&self) -> bool {
        self.done
    }

    fn size_hint(&self) -> SizeHint {
        match self.remaining {
            Some(n) => SizeHint::with_exact(n),
            None => SizeHint::default(),
        }
    }
}

enum Decoded {
    Data(Bytes),
    NeedMore,
    Done,
}

/// Decodes a body delimited by `Content-Length` or sent as `chunked`.
#[derive(Debug)]
enum Decoder {
    Length(u64),
    ChunkSize,
    ChunkData(u64),
    ChunkEnd,
    Trailers,
    Done,
}

impl Decoder {
    fn new(framing: Framing) -> Self {
        match framing {
            Framing::None => Decoder::Done,
            Framing::Length(n) => Decoder::Length(n),
            Framing::Chunked => Decoder::ChunkSize,
        }
    }

    fn is_done(&self) -> bool {
        matches!(self, Decoder::Done)
    }

    fn decode(&mut self, buf: &mut BytesMut) -> io::Result<Decoded> {
        fn invalid() -> io::Error {
            io::Error::new(io::ErrorKind::InvalidData, "invalid chunked body")
        }
        loop {
            match *self {
                Decoder::Done => return Ok(Decoded::Done),
                Decoder::Length(0) => *self = Decoder::Done,
                Decoder::Length(remaining) | Decoder::ChunkData(remaining) => {
                    if buf.is_empty() {
                        return Ok(Decoded::NeedMore);
                    }
                    let n = remaining.min(buf.len() as u64);
                    let left = remaining - n;
                    *self = match *self {
                        Decoder::Length(_) => Decoder::Length(left),
                        _ if left == 0 => Decoder::ChunkEnd,
                        _ => Decoder::ChunkData(left),
                    };
                    return Ok(Decoded::Data(buf.split_to(n as usize).freeze()));
                }
                Decoder::ChunkSize => {
                    let Some(end) = find_crlf(buf) else {
                        return if buf.len() > 1024 {
                            Err(invalid())
                        } else {
                            Ok(Decoded::NeedMore)
                        };
                    };
                    let line = &buf[..end];
                    let digits = line
                        .split(|&b| b == b';')
                        .next()
                        .unwrap_or_default()
                        .trim_ascii();
                    if digits.is_empty() || digits.len() > 15 {
                        return Err(invalid());
                    }
                    let size = digits.iter().try_fold(0u64, |acc, &b| {
                        (b as char).to_digit(16).map(|d| acc * 16 + u64::from(d))
                    });
                    let size = size.ok_or_else(invalid)?;
                    buf.advance(end + 2);
                    *self = if size == 0 {
                        Decoder::Trailers
                    } else {
                        Decoder::ChunkData(size)
                    };
                }
                Decoder::ChunkEnd => {
                    if buf.len() < 2 {
                        return Ok(Decoded::NeedMore);
                    }
                    if &buf[..2] != b"\r\n" {
                        return Err(invalid());
                    }
                    buf.advance(2);
                    *self = Decoder::ChunkSize;
                }
                Decoder::Trailers => {
                    let Some(end) = find_crlf(buf) else {
                        return if buf.len() > 8192 {
                            Err(invalid())
                        } else {
                            Ok(Decoded::NeedMore)
                        };
                    };
                    buf.advance(end + 2);
                    if end == 0 {
                        *self = Decoder::Done;
                    }
                }
            }
        }
    }
}

fn find_crlf(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|w| w == b"\r\n")
}

// ----- Écriture ------------------------------------------------------------

fn write_status(buf: &mut Vec<u8>, status: StatusCode) {
    // Les statuts courants, d'un seul bloc.
    let line: &[u8] = match status.as_u16() {
        200 => b"HTTP/1.1 200 OK\r\n",
        201 => b"HTTP/1.1 201 Created\r\n",
        204 => b"HTTP/1.1 204 No Content\r\n",
        206 => b"HTTP/1.1 206 Partial Content\r\n",
        301 => b"HTTP/1.1 301 Moved Permanently\r\n",
        302 => b"HTTP/1.1 302 Found\r\n",
        304 => b"HTTP/1.1 304 Not Modified\r\n",
        400 => b"HTTP/1.1 400 Bad Request\r\n",
        401 => b"HTTP/1.1 401 Unauthorized\r\n",
        403 => b"HTTP/1.1 403 Forbidden\r\n",
        404 => b"HTTP/1.1 404 Not Found\r\n",
        405 => b"HTTP/1.1 405 Method Not Allowed\r\n",
        500 => b"HTTP/1.1 500 Internal Server Error\r\n",
        _ => {
            buf.extend_from_slice(b"HTTP/1.1 ");
            buf.extend_from_slice(status.as_str().as_bytes());
            buf.push(b' ');
            buf.extend_from_slice(status.canonical_reason().unwrap_or("").as_bytes());
            buf.extend_from_slice(b"\r\n");
            return;
        }
    };
    buf.extend_from_slice(line);
}

/// Writes `content-length: n` in one block.
fn push_content_length(buf: &mut Vec<u8>, n: u64) {
    const PREFIX: &[u8] = b"content-length: ";
    let mut line = [0u8; PREFIX.len() + 20 + 2];
    line[..PREFIX.len()].copy_from_slice(PREFIX);
    let digits = if n == 0 { 1 } else { n.ilog10() as usize + 1 };
    let mut n = n;
    for slot in line[PREFIX.len()..PREFIX.len() + digits].iter_mut().rev() {
        *slot = b'0' + (n % 10) as u8;
        n /= 10;
    }
    let end = PREFIX.len() + digits;
    line[end..end + 2].copy_from_slice(b"\r\n");
    buf.extend_from_slice(&line[..end + 2]);
}

fn push_hex(buf: &mut Vec<u8>, mut n: usize) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut tmp = [0u8; 16];
    let mut i = tmp.len();
    loop {
        i -= 1;
        tmp[i] = HEX[n & 0xf];
        n >>= 4;
        if n == 0 {
            break;
        }
    }
    buf.extend_from_slice(&tmp[i..]);
}

/// The `Date` header, recomputed at most once per second and per thread.
mod date {
    use super::*;

    struct Cache {
        secs: u64,
        line: [u8; 37],
    }

    thread_local! {
        static CACHE: RefCell<Cache> = const { RefCell::new(Cache { secs: u64::MAX, line: [0; 37] }) };
    }

    /// Updates the cache if the second has changed.
    pub(super) fn refresh() {
        let now = SystemTime::now();
        let secs = now
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            if cache.secs != secs {
                let date = http_date(now);
                let line = &mut cache.line;
                line[..6].copy_from_slice(b"date: ");
                line[6..35].copy_from_slice(&date.as_bytes()[..29]);
                line[35..].copy_from_slice(b"\r\n");
                cache.secs = secs;
            }
        });
    }

    pub(super) fn write(buf: &mut Vec<u8>) {
        CACHE.with(|cache| {
            let cache = cache.borrow();
            if cache.secs == u64::MAX {
                drop(cache);
                refresh();
                return write(buf);
            }
            buf.extend_from_slice(&cache.line);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_all(input: &[u8]) -> io::Result<Vec<u8>> {
        let mut buf = BytesMut::from(input);
        let mut decoder = Decoder::new(Framing::Chunked);
        let mut out = Vec::new();
        loop {
            match decoder.decode(&mut buf)? {
                Decoded::Data(d) => out.extend_from_slice(&d),
                Decoded::Done => return Ok(out),
                Decoded::NeedMore => return Err(io::ErrorKind::UnexpectedEof.into()),
            }
        }
    }

    #[test]
    fn chunked() {
        assert_eq!(
            decode_all(b"3\r\nabc\r\n2;ext=1\r\nde\r\n0\r\n\r\n").unwrap(),
            b"abcde"
        );
        assert_eq!(
            decode_all(b"A\r\n0123456789\r\n0\r\nX-Trailer: 1\r\n\r\n").unwrap(),
            b"0123456789"
        );
        assert!(decode_all(b"zz\r\n").is_err());
        assert!(decode_all(b"3\r\nabcXX").is_err());
        assert!(decode_all(b"3\r\nab").is_err());
    }

    #[test]
    fn head_parsing() {
        let mut raw = RawHeaders::default();
        let mut buf = BytesMut::from(
            &b"POST /a?b=1 HTTP/1.1\r\nHost: x\r\nContent-Length: 5\r\n\r\nhelloGET / HTTP/1.0\r\n\r\n"[..],
        );
        let head = parse_head(&buf, &mut raw).unwrap().unwrap();
        assert_eq!(head.method, Method::POST);
        assert_eq!(head.framing, Framing::Length(5));
        assert!(head.keep_alive);
        assert_eq!(&buf[head.target.0..head.target.1], b"/a?b=1");
        buf.advance(head.len);
        assert_eq!(&buf[..5], b"hello");
        buf.advance(5);
        let head = parse_head(&buf, &mut raw).unwrap().unwrap();
        assert_eq!(head.version, Version::HTTP_10);
        assert!(!head.keep_alive);
        buf.advance(head.len);
        assert!(buf.is_empty());

        let partial = BytesMut::from(&b"GET / HTTP/1.1\r\nHost:"[..]);
        assert!(parse_head(&partial, &mut raw).unwrap().is_none());

        let smuggle = BytesMut::from(
            &b"POST / HTTP/1.1\r\nContent-Length: 3\r\nTransfer-Encoding: chunked\r\n\r\n"[..],
        );
        assert_eq!(
            parse_head(&smuggle, &mut raw).err(),
            Some(StatusCode::BAD_REQUEST)
        );
        let bad_len = BytesMut::from(&b"POST / HTTP/1.1\r\nContent-Length: 1x\r\n\r\n"[..]);
        assert_eq!(
            parse_head(&bad_len, &mut raw).err(),
            Some(StatusCode::BAD_REQUEST)
        );
        let garbage = BytesMut::from(&b"\x01\x02 / HTTP/1.1\r\n\r\n"[..]);
        assert_eq!(
            parse_head(&garbage, &mut raw).err(),
            Some(StatusCode::BAD_REQUEST)
        );
    }

    #[test]
    fn numbers() {
        let mut v = Vec::new();
        push_content_length(&mut v, 0);
        push_content_length(&mut v, 1234567890);
        push_hex(&mut v, 0x1a2b);
        assert_eq!(
            v,
            b"content-length: 0\r\ncontent-length: 1234567890\r\n1a2b"
        );
        assert_eq!(parse_decimal(b" 42 "), Some(42));
        assert_eq!(parse_decimal(b"-1"), None);
    }
}
