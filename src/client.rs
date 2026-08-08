use crate::action::{Action, SendAction};
use crate::envelope::{
    MESSAGE_TYPE_CALL, MESSAGE_TYPE_ERROR, MESSAGE_TYPE_RESULT, MESSAGE_TYPE_SEND, RawCall,
    RawError, RawResult, RawSend,
};
use crate::error::{ClientError, ProtocolError};
use crate::keepalive::{KeepaliveBehavior, KeepalivePolicy};
use crate::reconnect::{ReconnectPolicy, Reconnector};
use crate::runtime::{Executor, Timer, with_cancel, with_timeout};
use crate::sync::{BroadcastRegistry, Chan, Notify, OneShot, SharedMutex};
use crate::transport::{TransportEvent, TransportSink, TransportStream};
use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::future::Future;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use core::time::Duration;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use uuid::Uuid;

type PendingResponses<E> = Arc<SharedMutex<BTreeMap<Uuid, OneShot<Result<Value, E>>>>>;
type RequestSenders = Arc<SharedMutex<BTreeMap<String, Chan<(String, Value)>>>>;
type NotificationSenders = Arc<SharedMutex<BTreeMap<String, Chan<Value>>>>;
type PongWaiters = Arc<SharedMutex<PongState>>;

/// Outstanding pings, keyed by the correlation token written into each ping's payload. RFC 6455
/// requires a pong to echo that payload back, so a pong resolves the exact ping that produced it
/// rather than whichever one happened to be at the front of a queue.
///
/// This replaced a `VecDeque<OneShot<()>>` matched positionally, which had two failure modes: a
/// ping that timed out left its waiter in the queue forever, permanently offsetting every later
/// ping's pong by one, and an unsolicited pong (which RFC 6455 permits) did the same. Both were
/// mostly unreachable while pings were only ever sent by hand; a keepalive loop pinging on a
/// timer makes them routine.
#[derive(Default)]
struct PongState {
    next_token: u64,
    waiters: BTreeMap<u64, OneShot<()>>,
}

/// Why the read loop stopped reading the current transport, which decides what happens next.
enum LoopExit {
    /// The transport ended or errored on its own. Redial if a reconnector is configured.
    Eof,
    /// Keepalive (or `Client::force_reconnect`) gave up on an unresponsive peer. Redial.
    Forced,
    /// `Client::disconnect` was called. Stop entirely - do not redial.
    Shutdown,
}

/// The keepalive interval, mutable at runtime so a CSMS writing `WebSocketPingInterval` takes
/// effect on a live connection, plus the fixed policy governing each ping.
///
/// Stored as milliseconds in an `AtomicU32` rather than behind the client's mutex so
/// `Client::ping_interval`/`set_ping_interval` can stay non-`async` - a `GetVariables` handler
/// reporting the value shouldn't have to await a lock. `u32` milliseconds caps at ~49 days, far
/// beyond any sane ping interval, and 32-bit atomics exist on every target this crate builds for
/// (`AtomicU64` does not - notably not on `thumbv7em-none-eabihf`).
struct KeepaliveState {
    interval_millis: AtomicU32,
    changed: Notify,
    policy: KeepalivePolicy,
}

impl KeepaliveState {
    /// The current interval, or `None` when keepalive is off. Zero is the disabled
    /// representation, matching OCPP's `WebSocketPingInterval` semantics.
    fn interval(&self) -> Option<Duration> {
        match self.interval_millis.load(Ordering::Relaxed) {
            0 => None,
            millis => Some(Duration::from_millis(millis as u64)),
        }
    }

    fn set_interval(&self, interval: Option<Duration>) {
        let millis = interval
            .map(|d| d.as_millis().min(u32::MAX as u128) as u32)
            .unwrap_or(0);
        self.interval_millis.store(millis, Ordering::Relaxed);
        self.changed.notify();
    }
}

/// Everything `Client::from_transport_with_config` needs beyond the transport halves and the
/// runtime. Introduced because the option set had outgrown positional parameters -
/// `from_transport_with_reconnect` already took seven arguments, and keepalive would have made it
/// eight or forced a third constructor.
///
/// Defaults are deliberately inert: no reconnector, no keepalive. `ConnectOptions` (the
/// WebSocket convenience path) opts into both, but a caller assembling a client from raw
/// transport halves gets exactly the behavior they asked for and nothing more.
pub struct ClientConfig {
    /// How long to wait for a CALLRESULT/CALLERROR, and the default pong deadline.
    pub timeout: Duration,
    /// Redials when the transport closes. `None` means the read loop exits on disconnect.
    pub reconnector: Option<Box<dyn Reconnector>>,
    /// Backoff between failed reconnect attempts. Ignored when `reconnector` is `None`.
    pub reconnect_policy: ReconnectPolicy,
    /// Whether to ping the peer on a schedule, and what to do when it stops answering.
    pub keepalive: KeepaliveBehavior,
}

impl ClientConfig {
    /// A config with `timeout` and nothing else enabled.
    pub fn new(timeout: Duration) -> Self {
        Self {
            timeout,
            reconnector: None,
            reconnect_policy: ReconnectPolicy::default(),
            keepalive: KeepaliveBehavior::Disabled,
        }
    }

    /// Redial through `reconnector`, backing off per `policy`, when the transport closes.
    pub fn with_reconnect(
        mut self,
        reconnector: Box<dyn Reconnector>,
        policy: ReconnectPolicy,
    ) -> Self {
        self.reconnector = Some(reconnector);
        self.reconnect_policy = policy;
        self
    }

    /// Ping the peer per `keepalive`.
    pub fn with_keepalive(mut self, keepalive: KeepaliveBehavior) -> Self {
        self.keepalive = keepalive;
        self
    }
}

/// The OCPP client engine, generic over one version's protocol error type. `OCPP1_6Client`
/// and `OCPP2_0_1Client` are just `Client<OCPP1_6Error>` / `Client<OCPP2_0_1Error>` - the
/// dispatch/timeout/error machinery below is written once and shared by every version.
pub struct Client<E: ProtocolError> {
    sink: Arc<SharedMutex<Box<dyn TransportSink>>>,
    pending_responses: PendingResponses<E>,
    request_senders: RequestSenders,
    notification_senders: NotificationSenders,
    pong_waiters: PongWaiters,
    ping_registry: Arc<BroadcastRegistry>,
    reconnect_registry: Arc<BroadcastRegistry>,
    keepalive: Arc<KeepaliveState>,
    force_reconnect: Notify,
    /// Sticky: set by `disconnect()` and never cleared. Distinguishes "the caller shut this
    /// client down" from "the connection dropped", which the read loop must treat oppositely.
    closed: Arc<AtomicBool>,
    executor: Arc<dyn Executor>,
    timer: Arc<dyn Timer>,
    timeout: Duration,
}

impl<E: ProtocolError> Clone for Client<E> {
    fn clone(&self) -> Self {
        Self {
            sink: self.sink.clone(),
            pending_responses: self.pending_responses.clone(),
            request_senders: self.request_senders.clone(),
            notification_senders: self.notification_senders.clone(),
            pong_waiters: self.pong_waiters.clone(),
            ping_registry: self.ping_registry.clone(),
            reconnect_registry: self.reconnect_registry.clone(),
            keepalive: self.keepalive.clone(),
            force_reconnect: self.force_reconnect.clone(),
            closed: self.closed.clone(),
            executor: self.executor.clone(),
            timer: self.timer.clone(),
            timeout: self.timeout,
        }
    }
}

impl<E: ProtocolError> Client<E> {
    /// Build a client over any transport - the WebSocket adapter used by `connect_1_6` is
    /// just one implementation of `TransportSink`/`TransportStream`; tests and non-WebSocket
    /// transports (an embedded framed link, an in-memory fake for unit tests) construct a
    /// client the same way. `executor`/`timer` are likewise pluggable: the `tokio-runtime`
    /// feature provides `TokioExecutor`/`TokioTimer`; embedded users supply their own (e.g.
    /// backed by `embassy-executor`/`embassy-time`).
    pub fn from_transport(
        sink: Box<dyn TransportSink>,
        stream: Box<dyn TransportStream>,
        timeout: Duration,
        executor: Box<dyn Executor>,
        timer: Box<dyn Timer>,
    ) -> Self {
        Self::from_transport_with_config(sink, stream, executor, timer, ClientConfig::new(timeout))
    }

    /// Same as [`Client::from_transport`], but with automatic reconnect: when the transport
    /// closes (`TransportStream::recv` returns `Ok(None)`/`Err(_)`), the background read loop
    /// calls `reconnector.connect()` (backing off per `reconnect_policy` between failed
    /// attempts) instead of exiting, and swaps in the new transport once one succeeds.
    /// `reconnector: None` reproduces `from_transport`'s behavior - the read loop exits on
    /// disconnect and the client goes quiet. `connect_1_6`/`connect_2_0_1`/`connect_2_1` use
    /// this constructor with a WebSocket-backed `Reconnector`.
    pub fn from_transport_with_reconnect(
        sink: Box<dyn TransportSink>,
        stream: Box<dyn TransportStream>,
        timeout: Duration,
        executor: Box<dyn Executor>,
        timer: Box<dyn Timer>,
        reconnector: Option<Box<dyn Reconnector>>,
        reconnect_policy: ReconnectPolicy,
    ) -> Self {
        let mut config = ClientConfig::new(timeout);
        config.reconnector = reconnector;
        config.reconnect_policy = reconnect_policy;
        Self::from_transport_with_config(sink, stream, executor, timer, config)
    }

    /// The constructor the other two delegate to: everything optional lives in [`ClientConfig`]
    /// instead of a growing positional parameter list.
    ///
    /// Spawns two background tasks on `executor`: the read loop, and a keepalive task. The
    /// keepalive task is spawned even when `config.keepalive` is `Disabled`, where it simply
    /// parks until someone calls [`Client::set_ping_interval`] - otherwise a client built with
    /// keepalive off could never have it turned on later, which is exactly what a CSMS writing
    /// `WebSocketPingInterval` needs to do.
    pub fn from_transport_with_config(
        sink: Box<dyn TransportSink>,
        mut stream: Box<dyn TransportStream>,
        executor: Box<dyn Executor>,
        timer: Box<dyn Timer>,
        config: ClientConfig,
    ) -> Self {
        let ClientConfig {
            timeout,
            reconnector,
            reconnect_policy,
            keepalive,
        } = config;

        let sink = Arc::new(SharedMutex::new(sink));
        let pending_responses: PendingResponses<E> = Arc::new(SharedMutex::new(BTreeMap::new()));
        let request_senders: RequestSenders = Arc::new(SharedMutex::new(BTreeMap::new()));
        let notification_senders: NotificationSenders = Arc::new(SharedMutex::new(BTreeMap::new()));
        let pong_waiters: PongWaiters = Arc::new(SharedMutex::new(PongState::default()));
        let ping_registry = Arc::new(BroadcastRegistry::new());
        let reconnect_registry = Arc::new(BroadcastRegistry::new());
        let keepalive_state = Arc::new(KeepaliveState {
            interval_millis: AtomicU32::new(
                keepalive
                    .initial_interval()
                    .map(|d| d.as_millis().min(u32::MAX as u128) as u32)
                    .unwrap_or(0),
            ),
            changed: Notify::new(),
            policy: keepalive.policy(),
        });
        let force_reconnect = Notify::new();
        let closed = Arc::new(AtomicBool::new(false));
        let executor: Arc<dyn Executor> = Arc::from(executor);
        let timer: Arc<dyn Timer> = Arc::from(timer);

        let read_pending_responses = pending_responses.clone();
        let read_request_senders = request_senders.clone();
        let read_notification_senders = notification_senders.clone();
        let read_pong_waiters = pong_waiters.clone();
        let read_ping_registry = ping_registry.clone();
        let read_reconnect_registry = reconnect_registry.clone();
        let read_sink = sink.clone();
        let read_timer = timer.clone();
        let read_force_reconnect = force_reconnect.clone();
        let read_closed = closed.clone();

        // Honoring a forced reconnect means abandoning the current transport. With no
        // reconnector there is nothing to abandon it *for*, and breaking the read loop would
        // leave a permanently deaf client - strictly worse than an unanswered ping. So keepalive
        // can only ever escalate to a redial when redialling is actually configured. An explicit
        // `disconnect()` is different: it wants the loop gone, reconnector or not.
        let honor_force_reconnect = reconnector.is_some();

        executor.spawn(Box::pin(async move {
            // Persists across connections on purpose. Resetting it per connection is what made a
            // peer that accepts-then-immediately-closes a zero-delay hot loop: every dial
            // "succeeded", so the backoff never advanced past its first step. It is reset by
            // evidence that a connection actually works (see the inbound-event arm below), not by
            // the mere fact that a dial completed.
            let mut attempt = 0u32;

            'connection: loop {
                let mut reason = LoopExit::Eof;
                loop {
                    // `recv` is always raced against the wake signal, even with no reconnector,
                    // so `disconnect()` can pull the loop out of a `recv` that would otherwise
                    // park until the OS TCP timeout. This is why the cancel-safety contract on
                    // `TransportStream::recv` is unconditional.
                    let event = match with_cancel(stream.recv(), read_force_reconnect.wait()).await
                    {
                        Ok(event) => event,
                        Err(_) => {
                            // Woken on purpose. An explicit shutdown outranks everything.
                            if read_closed.load(Ordering::SeqCst) {
                                reason = LoopExit::Shutdown;
                                break;
                            }
                            if honor_force_reconnect {
                                reason = LoopExit::Forced;
                                break;
                            }
                            // Nothing to redial with, so keep reading rather than going deaf.
                            continue;
                        }
                    };

                    let event = match event {
                        Ok(Some(event)) => event,
                        Ok(None) | Err(_) => break,
                    };

                    // Anything arriving proves this connection is real and not an
                    // accept-then-close, so stop escalating the backoff. Dialling successfully is
                    // deliberately *not* treated as proof - that is exactly what the hot-loop bug
                    // mistook for a healthy connection.
                    attempt = 0;

                    match event {
                        TransportEvent::Frame(frame) => {
                            handle_frame::<E>(
                                &frame,
                                &read_pending_responses,
                                &read_request_senders,
                                &read_notification_senders,
                                &read_sink,
                            )
                            .await;
                        }
                        TransportEvent::Ping(payload) => {
                            read_ping_registry.notify_all().await;
                            let mut lock = read_sink.lock().await;
                            // RFC 6455: a pong must echo the triggering ping's payload.
                            let _ = lock.pong(payload).await;
                        }
                        TransportEvent::Pong(payload) => {
                            let token = <[u8; 8]>::try_from(payload.as_slice())
                                .map(u64::from_be_bytes)
                                .ok();
                            let mut lock = read_pong_waiters.lock().await;
                            match token.and_then(|token| lock.waiters.remove(&token)) {
                                Some(waiter) => waiter.send(()),
                                // Either an unsolicited pong, or one whose ping already timed
                                // out. Both are dropped rather than resolving some other
                                // outstanding ping, which is what the old positional matching
                                // did wrong.
                                None => tracing::debug!(
                                    "ocpp-client: pong matched no outstanding ping"
                                ),
                            }
                        }
                    }
                }

                // The EOF path lands here too, and `disconnect()` produces one: it closes the
                // sink, which on a real transport ends the stream. Without this check that EOF
                // is indistinguishable from a dropped connection, and the reconnector undoes the
                // shutdown the caller just asked for.
                if matches!(reason, LoopExit::Shutdown) || read_closed.load(Ordering::SeqCst) {
                    tracing::info!("ocpp-client: read loop stopped after an explicit disconnect");
                    read_pong_waiters.lock().await.waiters.clear();
                    break 'connection;
                }

                let Some(reconnector) = reconnector.as_ref() else {
                    break 'connection;
                };

                if matches!(reason, LoopExit::Forced) {
                    // Courtesy close so a peer that *is* still listening sees a clean shutdown
                    // rather than a vanished socket. Bounded, because the whole reason we got
                    // here is that this socket may be dead - an unbounded close could park the
                    // read loop for as long as the OS TCP timeout, which is precisely what
                    // forcing a reconnect was meant to avoid.
                    let mut lock = read_sink.lock().await;
                    let _ = with_timeout(read_timer.as_ref(), timeout, lock.close()).await;
                }

                // Outstanding pings belong to the connection that just died; a pong can never
                // arrive for them now. Leaving them would also mean the keepalive task's first
                // ping on the new connection competes with corpses from the old one.
                read_pong_waiters.lock().await.waiters.clear();

                loop {
                    // Wait *before* dialling, not only after a failed dial. The old order meant a
                    // dial that succeeded and then instantly dropped never waited at all.
                    let delay = reconnect_policy.jittered_delay_for(attempt);
                    attempt = attempt.saturating_add(1);

                    // Interruptible, so `disconnect()` during a long backoff takes effect now
                    // rather than after up to `max_delay`. A wake that isn't a shutdown (keepalive
                    // giving up on the connection we are already replacing) just shortens this
                    // one wait - it can't recur faster than the keepalive interval, so it can't
                    // reopen the hot loop.
                    if with_cancel(read_timer.delay(delay), read_force_reconnect.wait())
                        .await
                        .is_err()
                        && read_closed.load(Ordering::SeqCst)
                    {
                        tracing::info!("ocpp-client: reconnect abandoned after a disconnect");
                        break 'connection;
                    }

                    if read_closed.load(Ordering::SeqCst) {
                        break 'connection;
                    }

                    match reconnector.connect().await {
                        Ok((new_sink, new_stream)) => {
                            *read_sink.lock().await = new_sink;
                            stream = new_stream;
                            tracing::info!(attempt, "ocpp-client: reconnected");
                            read_reconnect_registry.notify_all().await;
                            break;
                        }
                        Err(err) => {
                            tracing::warn!(attempt, error = %err, "ocpp-client: reconnect attempt failed");
                        }
                    }
                }
            }
        }));

        let client = Self {
            sink,
            pending_responses,
            request_senders,
            notification_senders,
            pong_waiters,
            ping_registry,
            reconnect_registry,
            keepalive: keepalive_state,
            force_reconnect,
            closed,
            executor: executor.clone(),
            timer,
            timeout,
        };

        let keepalive_client = client.clone();
        executor.spawn(Box::pin(
            async move { keepalive_loop(keepalive_client).await },
        ));

        client
    }

    /// Send a CALL for `A` and wait for the matching CALLRESULT/CALLERROR.
    pub async fn call<A: Action>(
        &self,
        request: A::Request,
    ) -> Result<A::Response, ClientError<E>> {
        let response = self.do_send_request(request, A::NAME).await?;
        Ok(response)
    }

    /// Register a handler for CALLs the other side sends for action `A`. Replaces any
    /// previously registered handler for the same action.
    pub async fn on<A, F, FF>(&self, mut callback: F)
    where
        A: Action,
        F: FnMut(A::Request, Self) -> FF + Send + Sync + 'static,
        FF: Future<Output = Result<A::Response, E>> + Send,
    {
        let chan: Chan<(String, Value)> = Chan::new();
        {
            let mut lock = self.request_senders.lock().await;
            // Retire the handler being replaced. Overwriting the map entry alone only made the
            // old task unreachable, not finished - it stayed parked on a channel nothing could
            // ever deliver to, leaking one task per re-registration.
            if let Some(previous) = lock.insert(A::NAME.to_string(), chan.clone()) {
                previous.close();
            }
        }

        let client = self.clone();
        self.executor.spawn(Box::pin(async move {
            while let Some((message_id, payload)) = chan.recv().await {
                match serde_json::from_value::<A::Request>(payload) {
                    Ok(request) => {
                        let response = callback(request, client.clone()).await;
                        client.do_send_response(response, &message_id).await;
                    }
                    Err(_) => {
                        let error =
                            E::not_implemented(&format!("Failed to parse payload for {}", A::NAME));
                        client
                            .do_send_response::<A::Response>(Err(error), &message_id)
                            .await;
                    }
                }
            }
        }));
    }

    /// Wait for exactly one CALL for action `A` (bounded by the client's timeout), answer
    /// it with `callback`, and return the parsed request. Only useful in tests.
    ///
    /// The registration is removed again on the way out, whichever way that is. Leaving it in
    /// place left the action bound to a channel with no reader, so any *later* CALL for it was
    /// queued and silently forgotten - the peer got no CALLRESULT and no CALLERROR either, which
    /// looks exactly like the client having hung.
    ///
    /// Note this does not restore a handler that [`Client::on`] had registered for the same
    /// action beforehand; registering replaces, as `on`'s own docs say.
    #[cfg(feature = "test")]
    pub async fn wait_for<A, F, FF>(&self, mut callback: F) -> Result<A::Request, ClientError<E>>
    where
        A: Action,
        F: FnMut(A::Request, Self) -> FF + Send + Sync + 'static,
        FF: Future<Output = Result<A::Response, E>> + Send,
    {
        let chan: Chan<(String, Value)> = Chan::new();
        {
            let mut lock = self.request_senders.lock().await;
            if let Some(previous) = lock.insert(A::NAME.to_string(), chan.clone()) {
                previous.close();
            }
        }

        let outcome = match with_timeout(self.timer.as_ref(), self.timeout, chan.recv()).await {
            Ok(Some((message_id, payload))) => {
                match serde_json::from_value::<A::Request>(payload.clone()) {
                    Ok(for_callback) => {
                        let response = callback(for_callback, self.clone()).await;
                        self.do_send_response(response, &message_id).await;
                        serde_json::from_value(payload).map_err(ClientError::Decode)
                    }
                    Err(err) => Err(ClientError::Decode(err)),
                }
            }
            // The channel was closed out from under us - another registration for the same
            // action superseded this one.
            Ok(None) => Err(ClientError::Closed),
            Err(_) => Err(ClientError::Timeout),
        };

        {
            let mut lock = self.request_senders.lock().await;
            // Only remove our own registration: something else may have replaced it while we
            // were waiting, and tearing that out would break whoever installed it.
            if lock
                .get(A::NAME)
                .is_some_and(|current| current.is_same(&chan))
            {
                lock.remove(A::NAME);
            }
        }

        outcome
    }

    /// Send a `SEND` (OCPP-J 2.1 only) fire-and-forget message: writes the frame and returns as
    /// soon as the transport accepts it - no waiter, no timeout, since the spec forbids the
    /// receiver from ever replying to a `SEND`.
    pub async fn send_notification<A: SendAction>(
        &self,
        payload: A::Payload,
    ) -> Result<(), ClientError<E>> {
        if self.is_closed() {
            return Err(ClientError::Closed);
        }
        let message_id = Uuid::new_v4();
        let payload = serde_json::to_value(&payload).map_err(ClientError::Decode)?;
        let send = RawSend(
            MESSAGE_TYPE_SEND,
            message_id.to_string(),
            A::NAME.to_string(),
            payload,
        );
        let frame = serde_json::to_string(&send).map_err(ClientError::Decode)?;

        let mut lock = self.sink.lock().await;
        lock.send(frame).await.map_err(ClientError::Transport)
    }

    /// Register a handler for `SEND` (OCPP-J 2.1 only) messages of action `A`. Unlike
    /// [`Client::on`], `callback` returns nothing - the spec forbids replying to a `SEND`, so
    /// there's no response to send back. Replaces any previously registered handler for the
    /// same action.
    pub async fn on_notification<A, F, FF>(&self, mut callback: F)
    where
        A: SendAction,
        F: FnMut(A::Payload, Self) -> FF + Send + Sync + 'static,
        FF: Future<Output = ()> + Send,
    {
        let chan: Chan<Value> = Chan::new();
        {
            let mut lock = self.notification_senders.lock().await;
            if let Some(previous) = lock.insert(A::NAME.to_string(), chan.clone()) {
                previous.close();
            }
        }

        let client = self.clone();
        self.executor.spawn(Box::pin(async move {
            while let Some(payload) = chan.recv().await {
                match serde_json::from_value::<A::Payload>(payload) {
                    Ok(payload) => callback(payload, client.clone()).await,
                    Err(err) => {
                        tracing::warn!(error = %err, action = A::NAME, "ocpp-client: failed to parse SEND payload");
                    }
                }
            }
        }));
    }

    /// Send one ping and wait for the matching pong, bounded by the client's timeout.
    ///
    /// The pong is matched by correlation token, not arrival order: the ping carries an
    /// 8-byte token as its payload and only a pong echoing that exact payload resolves this
    /// call. RFC 6455 requires peers to echo ping payloads, so this is exact against any
    /// compliant server; a pong that echoes something else is ignored, and this call times out.
    ///
    /// This is the manual, one-shot ping. For scheduled keepalive - including detecting a peer
    /// that has stopped answering and forcing a redial - see [`Client::set_ping_interval`] and
    /// `KeepaliveBehavior`.
    pub async fn send_ping(&self) -> Result<(), ClientError<E>> {
        self.send_ping_with_timeout(self.timeout).await
    }

    /// [`Client::send_ping`] with an explicit pong deadline, so keepalive can use
    /// `KeepalivePolicy::timeout` instead of the client's request timeout.
    async fn send_ping_with_timeout(&self, timeout: Duration) -> Result<(), ClientError<E>> {
        if self.is_closed() {
            return Err(ClientError::Closed);
        }
        let waiter = OneShot::new();
        let token = {
            let mut lock = self.pong_waiters.lock().await;
            let token = lock.next_token;
            lock.next_token = lock.next_token.wrapping_add(1);
            lock.waiters.insert(token, waiter.clone());
            token
        };

        let sent = {
            let mut lock = self.sink.lock().await;
            lock.ping(Vec::from(token.to_be_bytes())).await
        };
        if let Err(err) = sent {
            self.forget_ping(token).await;
            return Err(ClientError::Transport(err));
        }

        match with_timeout(self.timer.as_ref(), timeout, waiter.wait()).await {
            Ok(()) => Ok(()),
            Err(_) => {
                // Drop our own waiter. Skipping this is what used to poison the client: an
                // abandoned waiter sat in the table forever, and (under the old positional
                // matching) stole the next ping's pong.
                self.forget_ping(token).await;
                Err(ClientError::Timeout)
            }
        }
    }

    async fn forget_ping(&self, token: u64) {
        self.pong_waiters.lock().await.waiters.remove(&token);
    }

    /// How many requests are still waiting for a CALLRESULT/CALLERROR.
    ///
    /// Test-only instrumentation: this table is bookkeeping that should return to zero once every
    /// request has either been answered or given up, and a leak in it is otherwise invisible from
    /// outside - it shows up only as memory growth on a charge point that has been running for
    /// weeks. `tests/ocpp_1_6_bookkeeping.rs` asserts on it.
    #[cfg(feature = "test")]
    pub async fn pending_request_count(&self) -> usize {
        self.pending_responses.lock().await.len()
    }

    /// How many pings are still waiting for a pong. Test-only, same rationale as
    /// [`Client::pending_request_count`].
    #[cfg(feature = "test")]
    pub async fn pending_ping_count(&self) -> usize {
        self.pong_waiters.lock().await.waiters.len()
    }

    /// The keepalive ping interval currently in force, or `None` when keepalive is off.
    ///
    /// This is the value to report for `OCPPCommCtrlr.WebSocketPingInterval` (2.0.1/2.1) or the
    /// `WebSocketPingInterval` configuration key (1.6) - `None` maps to the spec's `0`. Cheap
    /// and non-blocking, so a `GetVariables`/`GetConfiguration` handler can call it directly.
    pub fn ping_interval(&self) -> Option<Duration> {
        self.keepalive.interval()
    }

    /// Change the keepalive ping interval on a live connection, for a CSMS writing
    /// `WebSocketPingInterval` via `SetVariables`/`ChangeConfiguration`.
    ///
    /// `None` - or `Some(Duration::ZERO)`, matching the spec's `0` - disables pinging. Takes
    /// effect immediately: the keepalive task is woken rather than finishing the interval it was
    /// already waiting out, so shortening a 1-hour interval doesn't take up to an hour to apply.
    /// Enabling works even on a client built with `KeepaliveBehavior::Disabled`.
    pub fn set_ping_interval(&self, interval: Option<Duration>) {
        let interval = interval.filter(|d| !d.is_zero());
        self.keepalive.set_interval(interval);
        tracing::info!(
            interval_millis = interval.map(|d| d.as_millis() as u64).unwrap_or(0),
            "ocpp-client: keepalive ping interval updated"
        );
    }

    /// Abandon the current transport and redial, without waiting for it to notice it is dead.
    ///
    /// This is what keepalive escalates to after `KeepalivePolicy::max_missed` unanswered pings,
    /// exposed because a caller with its own liveness signal (an application-level heartbeat
    /// going unanswered, say) has the same problem. A half-open TCP connection can otherwise
    /// keep the read loop parked until the OS timeout, which no amount of protocol-level
    /// bookkeeping can shorten.
    ///
    /// No-op when the client was built without a reconnector: there would be nothing to redial
    /// with, and dropping the current connection anyway would just make the client deaf.
    pub fn force_reconnect(&self) {
        if self.is_closed() {
            return;
        }
        self.force_reconnect.notify();
    }

    pub async fn on_ping<
        F: FnMut(Self) -> FF + Send + Sync + 'static,
        FF: Future<Output = ()> + Send,
    >(
        &self,
        mut callback: F,
    ) {
        let signal = self.ping_registry.subscribe().await;
        let client = self.clone();
        self.executor.spawn(Box::pin(async move {
            loop {
                signal.wait().await;
                callback(client.clone()).await;
            }
        }));
    }

    /// Register a callback that fires every time the background read loop redials
    /// successfully after a disconnect (see [`Client::from_transport_with_reconnect`]). Never
    /// fires for the initial connection, only for later reconnects - the initial `Client` is
    /// already handed back post-connect, so callers run their own post-connect setup (e.g.
    /// `BootNotification`) right after `connect_1_6`/`from_transport_with_reconnect` returns.
    /// This is the hook for redoing that setup (or resyncing any other session state) after a
    /// dropped-and-restored connection; this crate does not re-run `BootNotification` or replay
    /// any state on its own.
    pub async fn on_reconnect<
        F: FnMut(Self) -> FF + Send + Sync + 'static,
        FF: Future<Output = ()> + Send,
    >(
        &self,
        mut callback: F,
    ) {
        let signal = self.reconnect_registry.subscribe().await;
        let client = self.clone();
        self.executor.spawn(Box::pin(async move {
            loop {
                signal.wait().await;
                callback(client.clone()).await;
            }
        }));
    }

    /// Shut this client down for good: close the transport, stop the read loop, stop keepalive,
    /// and do **not** redial.
    ///
    /// The shutdown is sticky and takes precedence over every automatic recovery path. That
    /// matters because closing the transport looks exactly like a dropped connection from the read
    /// loop's side - it previously produced an EOF the reconnector dutifully redialled, so on the
    /// default [`crate::ConnectOptions`] (reconnect enabled) there was no way to stop a client at
    /// all. After this returns:
    ///
    /// - the read loop has been told to exit rather than redial, whether it was parked in `recv`
    ///   or sees the EOF from the close;
    /// - the keepalive task stops pinging, and [`Client::set_ping_interval`] cannot restart it;
    /// - [`Client::force_reconnect`] is a no-op;
    /// - further `call`/`send_*`/`send_ping` return [`ClientError::Closed`] instead of writing to
    ///   a dead transport and waiting out the timeout.
    ///
    /// Idempotent: calling it again is a no-op returning `Ok(())`. Reconnecting afterwards means
    /// building a new `Client`.
    ///
    /// This only covers *deliberate* shutdown. An unrequested drop is still redialled as before.
    pub async fn disconnect(&self) -> Result<(), ClientError<E>> {
        if self.closed.swap(true, Ordering::SeqCst) {
            return Ok(());
        }

        // Both loops re-read `closed` as soon as they wake, so the flag has to be set first.
        self.force_reconnect.notify();
        self.keepalive.changed.notify();

        let mut lock = self.sink.lock().await;
        lock.close().await.map_err(ClientError::Transport)
    }

    /// Whether [`Client::disconnect`] has been called.
    ///
    /// This reflects deliberate shutdown only - it stays `false` while a connection is dropped and
    /// being redialled, because such a client is still live and will resume on its own. There is
    /// deliberately no "is the socket up right now" accessor: it would be stale the moment it
    /// returned, and [`Client::on_reconnect`] is the reliable way to observe reconnection.
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    async fn do_send_request<P: Serialize, R: DeserializeOwned>(
        &self,
        request: P,
        action: &str,
    ) -> Result<R, ClientError<E>> {
        // Fail fast rather than writing to a closed transport and then waiting out the full
        // request timeout for a response that cannot arrive.
        if self.is_closed() {
            return Err(ClientError::Closed);
        }
        let message_id = Uuid::new_v4();
        let payload = serde_json::to_value(&request).map_err(ClientError::Decode)?;
        let call = RawCall(
            MESSAGE_TYPE_CALL,
            message_id.to_string(),
            action.to_string(),
            payload,
        );
        let frame = serde_json::to_string(&call).map_err(ClientError::Decode)?;

        let waiter = OneShot::new();
        {
            let mut lock = self.pending_responses.lock().await;
            lock.insert(message_id, waiter.clone());
        }

        let sent = {
            let mut lock = self.sink.lock().await;
            lock.send(frame).await
        };
        if let Err(err) = sent {
            // Never reached the wire, so no response can ever arrive to clear this entry.
            self.forget_pending(message_id).await;
            return Err(ClientError::Transport(err));
        }

        let result = match with_timeout(self.timer.as_ref(), self.timeout, waiter.wait()).await {
            Ok(result) => result,
            Err(_) => {
                // Drop our own waiter. `handle_frame` only removes entries when a response
                // actually arrives, so without this every timed-out request left one behind
                // permanently - unbounded growth on a charge point that has been up for weeks
                // with an intermittent CSMS. Same failure the pong table used to have.
                self.forget_pending(message_id).await;
                return Err(ClientError::Timeout);
            }
        };

        match result {
            Ok(value) => serde_json::from_value(value).map_err(ClientError::Decode),
            Err(e) => Err(ClientError::Protocol(e)),
        }
    }

    async fn forget_pending(&self, message_id: Uuid) {
        self.pending_responses.lock().await.remove(&message_id);
    }

    async fn do_send_response<R: Serialize>(&self, response: Result<R, E>, message_id: &str) {
        let frame = match response {
            Ok(r) => match serde_json::to_value(r) {
                Ok(value) => serde_json::to_string(&RawResult(
                    MESSAGE_TYPE_RESULT,
                    message_id.to_string(),
                    value,
                )),
                Err(e) => return log_send_error(e),
            },
            Err(e) => serde_json::to_string(&RawError(
                MESSAGE_TYPE_ERROR,
                message_id.to_string(),
                e.code().to_string(),
                e.description().to_string(),
                e.details().to_owned(),
            )),
        };

        match frame {
            Ok(frame) => {
                let mut lock = self.sink.lock().await;
                if let Err(err) = lock.send(frame).await {
                    tracing::warn!(error = %err, "ocpp-client: failed to send response");
                }
            }
            Err(err) => {
                tracing::error!(error = %err, "ocpp-client: failed to encode response");
            }
        }
    }
}

/// The scheduled-ping task spawned by [`Client::from_transport_with_config`].
///
/// Runs for the client's whole life, including across reconnects - `send_ping` writes through
/// `Client`'s shared sink handle, which the read loop swaps in place on redial, so nothing here
/// has to know a reconnect happened.
///
/// Sleeping is `with_timeout(timer, interval, changed.wait())` rather than a plain delay: it is
/// already exactly "wait out the interval, but wake early if reconfigured", so `set_ping_interval`
/// applies immediately without a second timer or a polling granularity.
async fn keepalive_loop<E: ProtocolError>(client: Client<E>) {
    let state = client.keepalive.clone();
    let policy = state.policy;
    let misses_allowed = policy.misses_allowed();
    let ping_timeout = policy.timeout.unwrap_or(client.timeout);
    let mut missed = 0u32;

    loop {
        // `disconnect()` sets this and then notifies `changed`, so both waits below wake up here.
        if client.is_closed() {
            return;
        }

        let Some(interval) = state.interval() else {
            // Keepalive off: park until someone turns it on (or the client shuts down).
            state.changed.wait().await;
            missed = 0;
            continue;
        };

        if with_timeout(client.timer.as_ref(), interval, state.changed.wait())
            .await
            .is_ok()
        {
            // Reconfigured mid-wait; re-read the interval rather than pinging on the old one.
            missed = 0;
            continue;
        }

        // The interval could have been zeroed - or the client shut down - between the wait
        // ending and here.
        if client.is_closed() {
            return;
        }
        if state.interval().is_none() {
            continue;
        }

        match client.send_ping_with_timeout(ping_timeout).await {
            Ok(()) => missed = 0,
            Err(err) => {
                missed = missed.saturating_add(1);
                tracing::warn!(
                    missed,
                    misses_allowed,
                    error = %err,
                    "ocpp-client: keepalive ping went unanswered"
                );
                if missed >= misses_allowed {
                    missed = 0;
                    tracing::error!(
                        misses_allowed,
                        "ocpp-client: peer stopped answering pings, forcing a redial"
                    );
                    client.force_reconnect();
                }
            }
        }
    }
}

fn log_send_error(err: serde_json::Error) {
    tracing::error!(error = %err, "ocpp-client: failed to encode response payload");
}

async fn handle_frame<E: ProtocolError>(
    frame: &str,
    pending_responses: &PendingResponses<E>,
    request_senders: &RequestSenders,
    notification_senders: &NotificationSenders,
    sink: &Arc<SharedMutex<Box<dyn TransportSink>>>,
) {
    let value: Value = match serde_json::from_str(frame) {
        Ok(v) => v,
        Err(err) => {
            tracing::warn!(error = %err, "ocpp-client: received malformed frame");
            return;
        }
    };

    let Value::Array(items) = value else {
        tracing::warn!("ocpp-client: a message should be a JSON array");
        return;
    };
    let Some(Value::Number(message_type)) = items.first() else {
        tracing::warn!("ocpp-client: missing message type id");
        return;
    };
    let Some(message_type) = message_type.as_u64() else {
        tracing::warn!("ocpp-client: message type id must be an integer");
        return;
    };

    match message_type {
        MESSAGE_TYPE_CALL => {
            let call: RawCall = match serde_json::from_str(frame) {
                Ok(c) => c,
                Err(err) => {
                    tracing::warn!(error = %err, "ocpp-client: failed to parse CALL");
                    return;
                }
            };
            let action = &call.2;
            let sender = {
                let lock = request_senders.lock().await;
                lock.get(action).cloned()
            };
            match sender {
                Some(sender) => {
                    sender.send((call.1, call.3)).await;
                }
                None => {
                    let error =
                        E::not_implemented(&format!("Action '{action}' is not implemented"));
                    let payload = RawError(
                        MESSAGE_TYPE_ERROR,
                        call.1,
                        error.code().to_string(),
                        error.description().to_string(),
                        error.details().to_owned(),
                    );
                    if let Ok(frame) = serde_json::to_string(&payload) {
                        let mut lock = sink.lock().await;
                        let _ = lock.send(frame).await;
                    }
                }
            }
        }
        MESSAGE_TYPE_RESULT => {
            let result: RawResult = match serde_json::from_str(frame) {
                Ok(r) => r,
                Err(err) => {
                    tracing::warn!(error = %err, "ocpp-client: failed to parse CALLRESULT");
                    return;
                }
            };
            let Ok(id) = Uuid::parse_str(&result.1) else {
                return;
            };
            let mut lock = pending_responses.lock().await;
            if let Some(sender) = lock.remove(&id) {
                sender.send(Ok(result.2));
            }
        }
        MESSAGE_TYPE_ERROR => {
            let error: RawError = match serde_json::from_str(frame) {
                Ok(e) => e,
                Err(err) => {
                    tracing::warn!(error = %err, "ocpp-client: failed to parse CALLERROR");
                    return;
                }
            };
            let Ok(id) = Uuid::parse_str(&error.1) else {
                return;
            };
            let mut lock = pending_responses.lock().await;
            if let Some(sender) = lock.remove(&id) {
                sender.send(Err(E::from_wire(&error.2, &error.3, error.4)));
            }
        }
        MESSAGE_TYPE_SEND => {
            let send: RawSend = match serde_json::from_str(frame) {
                Ok(s) => s,
                Err(err) => {
                    tracing::warn!(error = %err, "ocpp-client: failed to parse SEND");
                    return;
                }
            };
            let action = &send.2;
            let sender = {
                let lock = notification_senders.lock().await;
                lock.get(action).cloned()
            };
            match sender {
                Some(sender) => sender.send(send.3).await,
                None => {
                    tracing::warn!(action = %action, "ocpp-client: SEND for unhandled action");
                }
            }
        }
        other => {
            tracing::warn!(message_type = other, "ocpp-client: unknown message type id");
        }
    }
}
