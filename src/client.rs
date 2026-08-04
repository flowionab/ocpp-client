use crate::action::Action;
use crate::envelope::{
    MESSAGE_TYPE_CALL, MESSAGE_TYPE_ERROR, MESSAGE_TYPE_RESULT, RawCall, RawError, RawResult,
};
use crate::error::{ClientError, ProtocolError};
use crate::reconnect::{ReconnectPolicy, Reconnector};
use crate::runtime::{Executor, Timer, with_timeout};
use crate::sync::{BroadcastRegistry, Chan, OneShot, SharedMutex};
use crate::transport::{TransportEvent, TransportSink, TransportStream};
use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::collections::{BTreeMap, VecDeque};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use core::future::Future;
use core::time::Duration;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use uuid::Uuid;

type PendingResponses<E> = Arc<SharedMutex<BTreeMap<Uuid, OneShot<Result<Value, E>>>>>;
type RequestSenders = Arc<SharedMutex<BTreeMap<String, Chan<(String, Value)>>>>;
type PongWaiters = Arc<SharedMutex<VecDeque<OneShot<()>>>>;

/// The OCPP client engine, generic over one version's protocol error type. `OCPP1_6Client`
/// and `OCPP2_0_1Client` are just `Client<OCPP1_6Error>` / `Client<OCPP2_0_1Error>` - the
/// dispatch/timeout/error machinery below is written once and shared by every version.
pub struct Client<E: ProtocolError> {
    sink: Arc<SharedMutex<Box<dyn TransportSink>>>,
    pending_responses: PendingResponses<E>,
    request_senders: RequestSenders,
    pong_waiters: PongWaiters,
    ping_registry: Arc<BroadcastRegistry>,
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
            pong_waiters: self.pong_waiters.clone(),
            ping_registry: self.ping_registry.clone(),
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
        Self::from_transport_with_reconnect(
            sink,
            stream,
            timeout,
            executor,
            timer,
            None,
            ReconnectPolicy::default(),
        )
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
        mut stream: Box<dyn TransportStream>,
        timeout: Duration,
        executor: Box<dyn Executor>,
        timer: Box<dyn Timer>,
        reconnector: Option<Box<dyn Reconnector>>,
        reconnect_policy: ReconnectPolicy,
    ) -> Self {
        let sink = Arc::new(SharedMutex::new(sink));
        let pending_responses: PendingResponses<E> = Arc::new(SharedMutex::new(BTreeMap::new()));
        let request_senders: RequestSenders = Arc::new(SharedMutex::new(BTreeMap::new()));
        let pong_waiters: PongWaiters = Arc::new(SharedMutex::new(VecDeque::new()));
        let ping_registry = Arc::new(BroadcastRegistry::new());
        let executor: Arc<dyn Executor> = Arc::from(executor);
        let timer: Arc<dyn Timer> = Arc::from(timer);

        let read_pending_responses = pending_responses.clone();
        let read_request_senders = request_senders.clone();
        let read_pong_waiters = pong_waiters.clone();
        let read_ping_registry = ping_registry.clone();
        let read_sink = sink.clone();
        let read_timer = timer.clone();

        executor.spawn(Box::pin(async move {
            loop {
                loop {
                    match stream.recv().await {
                        Ok(Some(TransportEvent::Frame(frame))) => {
                            handle_frame::<E>(
                                &frame,
                                &read_pending_responses,
                                &read_request_senders,
                                &read_sink,
                            )
                            .await;
                        }
                        Ok(Some(TransportEvent::Ping)) => {
                            read_ping_registry.notify_all().await;
                            let mut lock = read_sink.lock().await;
                            let _ = lock.pong().await;
                        }
                        Ok(Some(TransportEvent::Pong)) => {
                            let mut lock = read_pong_waiters.lock().await;
                            if let Some(waiter) = lock.pop_front() {
                                waiter.send(());
                            }
                        }
                        Ok(None) | Err(_) => break,
                    }
                }

                let Some(reconnector) = reconnector.as_ref() else {
                    break;
                };

                let mut attempt = 0u32;
                loop {
                    match reconnector.connect().await {
                        Ok((new_sink, new_stream)) => {
                            *read_sink.lock().await = new_sink;
                            stream = new_stream;
                            break;
                        }
                        Err(_err) => {
                            #[cfg(feature = "std")]
                            std::eprintln!("ocpp-client: reconnect attempt failed: {_err}");
                            read_timer.delay(reconnect_policy.delay_for(attempt)).await;
                            attempt = attempt.saturating_add(1);
                        }
                    }
                }
            }
        }));

        Self {
            sink,
            pending_responses,
            request_senders,
            pong_waiters,
            ping_registry,
            executor,
            timer,
            timeout,
        }
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
            lock.insert(A::NAME.to_string(), chan.clone());
        }

        let client = self.clone();
        self.executor.spawn(Box::pin(async move {
            loop {
                let (message_id, payload) = chan.recv().await;
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
            lock.insert(A::NAME.to_string(), chan.clone());
        }

        match with_timeout(self.timer.as_ref(), self.timeout, chan.recv()).await {
            Ok((message_id, payload)) => {
                let for_callback: A::Request =
                    serde_json::from_value(payload.clone()).map_err(ClientError::Decode)?;
                let response = callback(for_callback, self.clone()).await;
                self.do_send_response(response, &message_id).await;
                serde_json::from_value(payload).map_err(ClientError::Decode)
            }
            Err(_) => Err(ClientError::Timeout),
        }
    }

    pub async fn send_ping(&self) -> Result<(), ClientError<E>> {
        let waiter = OneShot::new();
        {
            let mut lock = self.pong_waiters.lock().await;
            lock.push_back(waiter.clone());
        }
        {
            let mut lock = self.sink.lock().await;
            lock.ping().await.map_err(ClientError::Transport)?;
        }
        with_timeout(self.timer.as_ref(), self.timeout, waiter.wait())
            .await
            .map(|_| ())
            .map_err(|_| ClientError::Timeout)
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

    pub async fn disconnect(&self) -> Result<(), ClientError<E>> {
        let mut lock = self.sink.lock().await;
        lock.close().await.map_err(ClientError::Transport)
    }

    async fn do_send_request<P: Serialize, R: DeserializeOwned>(
        &self,
        request: P,
        action: &str,
    ) -> Result<R, ClientError<E>> {
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

        {
            let mut lock = self.sink.lock().await;
            lock.send(frame).await.map_err(ClientError::Transport)?;
        }

        let result = with_timeout(self.timer.as_ref(), self.timeout, waiter.wait())
            .await
            .map_err(|_| ClientError::Timeout)?;

        match result {
            Ok(value) => serde_json::from_value(value).map_err(ClientError::Decode),
            Err(e) => Err(ClientError::Protocol(e)),
        }
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
                if let Err(_err) = lock.send(frame).await {
                    #[cfg(feature = "std")]
                    std::eprintln!("ocpp-client: failed to send response: {_err}");
                }
            }
            Err(_err) => {
                #[cfg(feature = "std")]
                std::eprintln!("ocpp-client: failed to encode response: {_err}");
            }
        }
    }
}

fn log_send_error(_err: serde_json::Error) {
    #[cfg(feature = "std")]
    std::eprintln!("ocpp-client: failed to encode response payload: {_err}");
}

async fn handle_frame<E: ProtocolError>(
    frame: &str,
    pending_responses: &PendingResponses<E>,
    request_senders: &RequestSenders,
    sink: &Arc<SharedMutex<Box<dyn TransportSink>>>,
) {
    let value: Value = match serde_json::from_str(frame) {
        Ok(v) => v,
        Err(_err) => {
            #[cfg(feature = "std")]
            std::eprintln!("ocpp-client: received malformed frame: {_err}");
            return;
        }
    };

    let Value::Array(items) = value else {
        #[cfg(feature = "std")]
        std::eprintln!("ocpp-client: a message should be a JSON array");
        return;
    };
    let Some(Value::Number(message_type)) = items.first() else {
        #[cfg(feature = "std")]
        std::eprintln!("ocpp-client: missing message type id");
        return;
    };
    let Some(message_type) = message_type.as_u64() else {
        #[cfg(feature = "std")]
        std::eprintln!("ocpp-client: message type id must be an integer");
        return;
    };

    match message_type {
        MESSAGE_TYPE_CALL => {
            let call: RawCall = match serde_json::from_str(frame) {
                Ok(c) => c,
                Err(_err) => {
                    #[cfg(feature = "std")]
                    std::eprintln!("ocpp-client: failed to parse CALL: {_err}");
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
                Err(_err) => {
                    #[cfg(feature = "std")]
                    std::eprintln!("ocpp-client: failed to parse CALLRESULT: {_err}");
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
                Err(_err) => {
                    #[cfg(feature = "std")]
                    std::eprintln!("ocpp-client: failed to parse CALLERROR: {_err}");
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
        #[allow(unused_variables)]
        other => {
            #[cfg(feature = "std")]
            std::eprintln!("ocpp-client: unknown message type id {other}");
        }
    }
}
