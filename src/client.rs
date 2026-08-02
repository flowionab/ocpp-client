use crate::action::Action;
use crate::envelope::{
    MESSAGE_TYPE_CALL, MESSAGE_TYPE_ERROR, MESSAGE_TYPE_RESULT, RawCall, RawError, RawResult,
};
use crate::error::{ClientError, ProtocolError};
use crate::transport::{TransportEvent, TransportSink, TransportStream};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::{BTreeMap, VecDeque};
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast::Sender as BroadcastSender;
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::time::timeout;
use uuid::Uuid;

type PendingResponses<E> = Arc<Mutex<BTreeMap<Uuid, oneshot::Sender<Result<Value, E>>>>>;
type RequestSenders = Arc<Mutex<BTreeMap<String, mpsc::Sender<(String, Value)>>>>;
type PongWaiters = Arc<Mutex<VecDeque<oneshot::Sender<()>>>>;

/// The OCPP client engine, generic over one version's protocol error type. `OCPP1_6Client`
/// and `OCPP2_0_1Client` are just `Client<OCPP1_6Error>` / `Client<OCPP2_0_1Error>` - the
/// dispatch/timeout/error machinery below is written once and shared by every version.
pub struct Client<E: ProtocolError> {
    sink: Arc<Mutex<Box<dyn TransportSink>>>,
    pending_responses: PendingResponses<E>,
    request_senders: RequestSenders,
    pong_waiters: PongWaiters,
    ping_sender: BroadcastSender<()>,
    timeout: Duration,
}

impl<E: ProtocolError> Clone for Client<E> {
    fn clone(&self) -> Self {
        Self {
            sink: self.sink.clone(),
            pending_responses: self.pending_responses.clone(),
            request_senders: self.request_senders.clone(),
            pong_waiters: self.pong_waiters.clone(),
            ping_sender: self.ping_sender.clone(),
            timeout: self.timeout,
        }
    }
}

impl<E: ProtocolError> Client<E> {
    /// Build a client over any transport - the WebSocket adapter used by `connect_1_6` is
    /// just one implementation of `TransportSink`/`TransportStream`; tests and non-WebSocket
    /// transports (an embedded framed link, an in-memory fake for unit tests) construct a
    /// client the same way.
    pub fn from_transport(
        sink: Box<dyn TransportSink>,
        mut stream: Box<dyn TransportStream>,
        timeout: Duration,
    ) -> Self {
        let sink = Arc::new(Mutex::new(sink));
        let pending_responses: PendingResponses<E> = Arc::new(Mutex::new(BTreeMap::new()));
        let request_senders: RequestSenders = Arc::new(Mutex::new(BTreeMap::new()));
        let pong_waiters: PongWaiters = Arc::new(Mutex::new(VecDeque::new()));
        let (ping_sender, _) = tokio::sync::broadcast::channel(10);

        let read_pending_responses = pending_responses.clone();
        let read_request_senders = request_senders.clone();
        let read_pong_waiters = pong_waiters.clone();
        let read_ping_sender = ping_sender.clone();
        let read_sink = sink.clone();

        tokio::spawn(async move {
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
                        let _ = read_ping_sender.send(());
                        let mut lock = read_sink.lock().await;
                        let _ = lock.pong().await;
                    }
                    Ok(Some(TransportEvent::Pong)) => {
                        let mut lock = read_pong_waiters.lock().await;
                        if let Some(waiter) = lock.pop_front() {
                            let _ = waiter.send(());
                        }
                    }
                    Ok(None) | Err(_) => break,
                }
            }
        });

        Self {
            sink,
            pending_responses,
            request_senders,
            pong_waiters,
            ping_sender,
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
        let (sender, mut recv) = mpsc::channel(1000);
        {
            let mut lock = self.request_senders.lock().await;
            lock.insert(A::NAME.to_string(), sender);
        }

        let client = self.clone();
        tokio::spawn(async move {
            while let Some((message_id, payload)) = recv.recv().await {
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
        });
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
        let (sender, mut recv) = mpsc::channel(1000);
        {
            let mut lock = self.request_senders.lock().await;
            lock.insert(A::NAME.to_string(), sender);
        }

        match timeout(self.timeout, recv.recv()).await {
            Ok(Some((message_id, payload))) => {
                let for_callback: A::Request =
                    serde_json::from_value(payload.clone()).map_err(ClientError::Decode)?;
                let response = callback(for_callback, self.clone()).await;
                self.do_send_response(response, &message_id).await;
                serde_json::from_value(payload).map_err(ClientError::Decode)
            }
            Ok(None) => Err(ClientError::Closed),
            Err(_) => Err(ClientError::Timeout),
        }
    }

    pub async fn send_ping(&self) -> Result<(), ClientError<E>> {
        let (sender, receiver) = oneshot::channel();
        {
            let mut lock = self.pong_waiters.lock().await;
            lock.push_back(sender);
        }
        {
            let mut lock = self.sink.lock().await;
            lock.ping().await.map_err(ClientError::Transport)?;
        }
        timeout(self.timeout, receiver)
            .await
            .map_err(|_| ClientError::Timeout)?
            .map_err(|_| ClientError::Closed)
    }

    pub async fn on_ping<
        F: FnMut(Self) -> FF + Send + Sync + 'static,
        FF: Future<Output = ()> + Send,
    >(
        &self,
        mut callback: F,
    ) {
        let mut recv = self.ping_sender.subscribe();
        let client = self.clone();
        tokio::spawn(async move {
            while recv.recv().await.is_ok() {
                callback(client.clone()).await;
            }
        });
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

        let (sender, receiver) = oneshot::channel();
        {
            let mut lock = self.pending_responses.lock().await;
            lock.insert(message_id, sender);
        }

        {
            let mut lock = self.sink.lock().await;
            lock.send(frame).await.map_err(ClientError::Transport)?;
        }

        let result = timeout(self.timeout, receiver)
            .await
            .map_err(|_| ClientError::Timeout)?
            .map_err(|_| ClientError::Closed)?;

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
                if let Err(err) = lock.send(frame).await {
                    eprintln!("ocpp-client: failed to send response: {err}");
                }
            }
            Err(err) => eprintln!("ocpp-client: failed to encode response: {err}"),
        }
    }
}

fn log_send_error(err: serde_json::Error) {
    eprintln!("ocpp-client: failed to encode response payload: {err}");
}

async fn handle_frame<E: ProtocolError>(
    frame: &str,
    pending_responses: &PendingResponses<E>,
    request_senders: &RequestSenders,
    sink: &Arc<Mutex<Box<dyn TransportSink>>>,
) {
    let value: Value = match serde_json::from_str(frame) {
        Ok(v) => v,
        Err(err) => {
            eprintln!("ocpp-client: received malformed frame: {err}");
            return;
        }
    };

    let Value::Array(items) = value else {
        eprintln!("ocpp-client: a message should be a JSON array");
        return;
    };
    let Some(Value::Number(message_type)) = items.first() else {
        eprintln!("ocpp-client: missing message type id");
        return;
    };
    let Some(message_type) = message_type.as_u64() else {
        eprintln!("ocpp-client: message type id must be an integer");
        return;
    };

    match message_type {
        MESSAGE_TYPE_CALL => {
            let call: RawCall = match serde_json::from_str(frame) {
                Ok(c) => c,
                Err(err) => {
                    eprintln!("ocpp-client: failed to parse CALL: {err}");
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
                    let _ = sender.send((call.1, call.3)).await;
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
                    eprintln!("ocpp-client: failed to parse CALLRESULT: {err}");
                    return;
                }
            };
            let Ok(id) = Uuid::parse_str(&result.1) else {
                return;
            };
            let mut lock = pending_responses.lock().await;
            if let Some(sender) = lock.remove(&id) {
                let _ = sender.send(Ok(result.2));
            }
        }
        MESSAGE_TYPE_ERROR => {
            let error: RawError = match serde_json::from_str(frame) {
                Ok(e) => e,
                Err(err) => {
                    eprintln!("ocpp-client: failed to parse CALLERROR: {err}");
                    return;
                }
            };
            let Ok(id) = Uuid::parse_str(&error.1) else {
                return;
            };
            let mut lock = pending_responses.lock().await;
            if let Some(sender) = lock.remove(&id) {
                let _ = sender.send(Err(E::from_wire(&error.2, &error.3, error.4)));
            }
        }
        other => eprintln!("ocpp-client: unknown message type id {other}"),
    }
}
