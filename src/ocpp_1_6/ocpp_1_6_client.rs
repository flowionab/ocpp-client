use std::collections::{BTreeMap};
use std::future::Future;
use std::sync::Arc;
use futures::{SinkExt, StreamExt, TryStreamExt};
use futures::stream::{SplitSink};
use rust_ocpp::v1_6::messages::authorize::{AuthorizeRequest, AuthorizeResponse};
use rust_ocpp::v1_6::messages::boot_notification::{BootNotificationRequest, BootNotificationResponse};
use rust_ocpp::v1_6::messages::cancel_reservation::{CancelReservationRequest, CancelReservationResponse};
use rust_ocpp::v1_6::messages::change_availability::{ChangeAvailabilityRequest, ChangeAvailabilityResponse};
use rust_ocpp::v1_6::messages::change_configuration::{ChangeConfigurationRequest, ChangeConfigurationResponse};
use rust_ocpp::v1_6::messages::clear_cache::{ClearCacheRequest, ClearCacheResponse};
use rust_ocpp::v1_6::messages::clear_charging_profile::{ClearChargingProfileRequest, ClearChargingProfileResponse};
use rust_ocpp::v1_6::messages::data_transfer::{DataTransferRequest, DataTransferResponse};
use rust_ocpp::v1_6::messages::diagnostics_status_notification::{DiagnosticsStatusNotificationRequest, DiagnosticsStatusNotificationResponse};
use rust_ocpp::v1_6::messages::firmware_status_notification::{FirmwareStatusNotificationRequest, FirmwareStatusNotificationResponse};
use rust_ocpp::v1_6::messages::get_composite_schedule::{GetCompositeScheduleRequest, GetCompositeScheduleResponse};
use rust_ocpp::v1_6::messages::get_configuration::{GetConfigurationRequest, GetConfigurationResponse};
use rust_ocpp::v1_6::messages::get_diagnostics::{GetDiagnosticsRequest, GetDiagnosticsResponse};
use rust_ocpp::v1_6::messages::get_local_list_version::{GetLocalListVersionRequest, GetLocalListVersionResponse};
use rust_ocpp::v1_6::messages::heart_beat::{HeartbeatRequest, HeartbeatResponse};
use rust_ocpp::v1_6::messages::meter_values::{MeterValuesRequest, MeterValuesResponse};
use rust_ocpp::v1_6::messages::remote_start_transaction::{RemoteStartTransactionRequest, RemoteStartTransactionResponse};
use rust_ocpp::v1_6::messages::remote_stop_transaction::{RemoteStopTransactionRequest, RemoteStopTransactionResponse};
use rust_ocpp::v1_6::messages::reserve_now::{ReserveNowRequest, ReserveNowResponse};
use rust_ocpp::v1_6::messages::reset::{ResetRequest, ResetResponse};
use rust_ocpp::v1_6::messages::send_local_list::{SendLocalListRequest, SendLocalListResponse};
use rust_ocpp::v1_6::messages::set_charging_profile::{SetChargingProfileRequest, SetChargingProfileResponse};
use rust_ocpp::v1_6::messages::start_transaction::{StartTransactionRequest, StartTransactionResponse};
use rust_ocpp::v1_6::messages::status_notification::{StatusNotificationRequest, StatusNotificationResponse};
use rust_ocpp::v1_6::messages::stop_transaction::{StopTransactionRequest, StopTransactionResponse};
use rust_ocpp::v1_6::messages::trigger_message::{TriggerMessageRequest, TriggerMessageResponse};
use rust_ocpp::v1_6::messages::unlock_connector::{UnlockConnectorRequest, UnlockConnectorResponse};
use rust_ocpp::v1_6::messages::update_firmware::{UpdateFirmwareRequest, UpdateFirmwareResponse};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{Value};
use tokio::net::TcpStream;
use tokio::sync::{Mutex, oneshot};
use tokio::sync::broadcast::Sender;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;
use crate::ocpp_1_6::ocpp_1_6_error::OCPP1_6Error;
use crate::ocpp_1_6::raw_ocpp_1_6_call::RawOcpp1_6Call;
use crate::ocpp_1_6::raw_ocpp_1_6_error::RawOcpp1_6Error;
use crate::ocpp_1_6::raw_ocpp_1_6_result::RawOcpp1_6Result;

/// OCPP 1.6 client
#[derive(Clone)]
pub struct OCPP1_6Client {
    sink: Arc<Mutex<SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>>>,
    response_channels: Arc<Mutex<BTreeMap<Uuid, oneshot::Sender<Result<Value, OCPP1_6Error>>>>>,
    request_sender: Sender<RawOcpp1_6Call>,
}

impl OCPP1_6Client {
    pub(crate) fn new(stream: WebSocketStream<MaybeTlsStream<TcpStream>>) -> Self {
        let (sink, stream) = stream.split();

        let response_channels = Arc::new(Mutex::new(BTreeMap::<Uuid, oneshot::Sender<Result<Value, OCPP1_6Error>>>::new()));
        let response_channels2 = Arc::clone(&response_channels);

        let (request_sender, _) = tokio::sync::broadcast::channel(1000);

        let request_sender2 = request_sender.clone();
        tokio::spawn(async move {
            stream
                .map_err(|e| Box::<dyn std::error::Error + Send + Sync>::from(e))
                .try_for_each(|message| {
                    let request_sender = request_sender2.clone();
                    let response_channels2 = response_channels2.clone();
                    let request_sender = request_sender.clone();{
                    let value = request_sender.clone();
                    async move {
                        match message {
                            Message::Text(raw_payload) => {
                                let raw_value = serde_json::from_str(&raw_payload)?;

                                match raw_value {
                                    Value::Array(list) => {
                                        if let Some(message_type_item) = list.get(0) {
                                            if let Value::Number(message_type_raw) = message_type_item {
                                                if let Some(message_type) = message_type_raw.as_u64() {
                                                    match message_type {
                                                        // CALL
                                                        2 => {
                                                            let call: RawOcpp1_6Call =
                                                                serde_json::from_str(&raw_payload).unwrap();
                                                            if let Err(err) = value.send(call) {
                                                                println!("Error sending request: {:?}", err);
                                                            };

                                                        },
                                                        // RESPONSE
                                                        3 => {
                                                            let result: RawOcpp1_6Result =
                                                                serde_json::from_str(&raw_payload).unwrap();
                                                            let mut lock = response_channels2.lock().await;
                                                            if let Some(sender) = lock.remove(&Uuid::parse_str(&result.1)?) {
                                                                sender.send(Ok(result.2)).unwrap();
                                                            }
                                                        },
                                                        // ERROR
                                                        4 => {
                                                            let error: RawOcpp1_6Error =
                                                                serde_json::from_str(&raw_payload)?;
                                                            let mut lock = response_channels2.lock().await;
                                                            if let Some(sender) = lock.remove(&Uuid::parse_str(&error.1)?) {
                                                                sender.send(Err(error.into())).unwrap();
                                                            }
                                                        },
                                                        _ => println!("Unknown message type"),
                                                    }
                                                } else {
                                                    println!("The message type has to be an integer, it cant have decimals")
                                                }
                                            } else {
                                                println!("The first item in the array was not a number")
                                            }
                                        } else {
                                            println!("The root list was empty")
                                        }
                                    }
                                    _ => println!("A message should be an array of items"),
                                }

                            }
                            _ => {}
                        }
                        Ok(())
                    }
                    }
                }).await?;
            Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
        });

        Self {
            sink: Arc::new(Mutex::new(sink)),
            response_channels,
            request_sender
        }
    }

    /// Disconnect from the server
    pub async fn disconnect(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut lock = self.sink.lock().await;
        lock.close().await?;
        Ok(())
    }

    pub async fn send_authorize(&self, request: AuthorizeRequest) -> Result<Result<AuthorizeResponse, OCPP1_6Error>, Box<dyn std::error::Error + Send + Sync>> {
        self.do_send_request(request, "Authorize").await
    }

    pub async fn send_boot_notification(&self, request: BootNotificationRequest) -> Result<Result<BootNotificationResponse, OCPP1_6Error>, Box<dyn std::error::Error + Send + Sync>> {
        self.do_send_request(request, "BootNotification").await
    }

    pub async fn send_data_transfer(&self, request: DataTransferRequest) -> Result<Result<DataTransferResponse, OCPP1_6Error>, Box<dyn std::error::Error + Send + Sync>> {
        self.do_send_request(request, "DataTransfer").await
    }

    pub async fn send_diagnostics_status_notification(&self, request: DiagnosticsStatusNotificationRequest) -> Result<Result<DiagnosticsStatusNotificationResponse, OCPP1_6Error>, Box<dyn std::error::Error + Send + Sync>> {
        self.do_send_request(request, "DiagnosticsStatusNotification").await
    }

    pub async fn send_firmware_status_notification(&self, request: FirmwareStatusNotificationRequest) -> Result<Result<FirmwareStatusNotificationResponse, OCPP1_6Error>, Box<dyn std::error::Error + Send + Sync>> {
        self.do_send_request(request, "FirmwareStatusNotification").await
    }

    pub async fn send_heartbeat(&self, request: HeartbeatRequest) -> Result<Result<HeartbeatResponse, OCPP1_6Error>, Box<dyn std::error::Error + Send + Sync>> {
        self.do_send_request(request, "Heartbeat").await
    }

    pub async fn send_meter_values(&self, request: MeterValuesRequest) -> Result<Result<MeterValuesResponse, OCPP1_6Error>, Box<dyn std::error::Error + Send + Sync>> {
        self.do_send_request(request, "MeterValues").await
    }

    pub async fn send_start_transaction(&self, request: StartTransactionRequest) -> Result<Result<StartTransactionResponse, OCPP1_6Error>, Box<dyn std::error::Error + Send + Sync>> {
        self.do_send_request(request, "StartTransaction").await
    }

    pub async fn send_status_notification(&self, request: StatusNotificationRequest) -> Result<Result<StatusNotificationResponse, OCPP1_6Error>, Box<dyn std::error::Error + Send + Sync>> {
        self.do_send_request(request, "StatusNotification").await
    }

    pub async fn send_stop_transaction(&self, request: StopTransactionRequest) -> Result<Result<StopTransactionResponse, OCPP1_6Error>, Box<dyn std::error::Error + Send + Sync>> {
        self.do_send_request(request, "StopTransaction").await
    }

    pub async fn inspect_raw_message<F: FnMut(String, Value) -> FF + Send + Sync + 'static, FF: Future<Output=()> + Send + Sync>(&self, mut callback: F){
        let mut recv = self.request_sender.subscribe();
        tokio::spawn(async move {
            while let Ok(call) = recv.recv().await {
                callback(call.2.to_string(), call.3.to_owned()).await;
            }
        });
    }

    pub async fn on_cancel_reservation<F: FnMut(CancelReservationRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<CancelReservationResponse, OCPP1_6Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "CancelReservation").await
    }

    pub async fn on_change_availability<F: FnMut(ChangeAvailabilityRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<ChangeAvailabilityResponse, OCPP1_6Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "ChangeAvailability").await
    }

    pub async fn on_change_configuration<F: FnMut(ChangeConfigurationRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<ChangeConfigurationResponse, OCPP1_6Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "ChangeConfiguration").await
    }

    pub async fn on_clear_cache<F: FnMut(ClearCacheRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<ClearCacheResponse, OCPP1_6Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "ClearCache").await
    }

    pub async fn on_clear_charging_profile<F: FnMut(ClearChargingProfileRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<ClearChargingProfileResponse, OCPP1_6Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "ClearChargingProfile").await
    }

    pub async fn on_data_transfer<F: FnMut(DataTransferRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<DataTransferResponse, OCPP1_6Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "DataTransfer").await
    }

    pub async fn on_get_composite_schedule<F: FnMut(GetCompositeScheduleRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<GetCompositeScheduleResponse, OCPP1_6Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "GetCompositeSchedule").await
    }

    pub async fn on_get_configuration<F: FnMut(GetConfigurationRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<GetConfigurationResponse, OCPP1_6Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "GetConfiguration").await
    }

    pub async fn on_get_diagnostics<F: FnMut(GetDiagnosticsRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<GetDiagnosticsResponse, OCPP1_6Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "GetDiagnostics").await
    }

    pub async fn on_get_local_list_version<F: FnMut(GetLocalListVersionRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<GetLocalListVersionResponse, OCPP1_6Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "GetLocalListVersion").await
    }

    pub async fn on_remote_start_transaction<F: FnMut(RemoteStartTransactionRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<RemoteStartTransactionResponse, OCPP1_6Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "RemoteStartTransaction").await
    }

    pub async fn on_remote_stop_transaction<F: FnMut(RemoteStopTransactionRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<RemoteStopTransactionResponse, OCPP1_6Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "RemoteStopTransaction").await
    }

    pub async fn on_reserve_now<F: FnMut(ReserveNowRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<ReserveNowResponse, OCPP1_6Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "ReserveNow").await
    }

    pub async fn on_reset<F: FnMut(ResetRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<ResetResponse, OCPP1_6Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "Reset").await
    }

    pub async fn on_send_local_list<F: FnMut(SendLocalListRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<SendLocalListResponse, OCPP1_6Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "SendLocalList").await
    }

    pub async fn on_set_charging_profile<F: FnMut(SetChargingProfileRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<SetChargingProfileResponse, OCPP1_6Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "SetChargingProfile").await
    }

    pub async fn on_trigger_message<F: FnMut(TriggerMessageRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<TriggerMessageResponse, OCPP1_6Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "TriggerMessage").await
    }

    pub async fn on_unlock_connector<F: FnMut(UnlockConnectorRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<UnlockConnectorResponse, OCPP1_6Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "UnlockConnector").await
    }

    pub async fn on_update_firmware<F: FnMut(UpdateFirmwareRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<UpdateFirmwareResponse, OCPP1_6Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "UpdateFirmware").await
    }

    async fn handle_on_request<P: DeserializeOwned + Send + Sync, R: Serialize + Send + Sync, F: FnMut(P, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<R, OCPP1_6Error>> + Send + Sync>(&self, mut callback: F, action: &'static str) {
        let mut recv = self.request_sender.subscribe();

        let s = self.clone();
        tokio::spawn(async move {
            while let Ok(call) = recv.recv().await {
                if call.2 != action {
                    continue;
                }

                match serde_json::from_value(call.3) {
                    Ok(payload) => {
                        let response = callback(payload, s.clone()).await;
                        s.do_send_response(response, &call.1).await
                    }
                    Err(err) => {
                        println!("Failed to parse payload: {:?}", err)
                    }
                }
            }
        });
    }

    async fn do_send_response<R: Serialize>(&self, response: Result<R, OCPP1_6Error>, message_id: &str) {
        let payload = match response {
            Ok(r) => {
                serde_json::to_string(&RawOcpp1_6Result(3, message_id.to_string(), serde_json::to_value(r).unwrap())).unwrap()
            }
            Err(e) => {
                serde_json::to_string(&RawOcpp1_6Error(4, message_id.to_string(), e.code().to_string(), e.description().to_string(), e.details().to_owned())).unwrap()
            }
        };

        let mut lock = self.sink.lock().await;
        if let Err(err) = lock.send(Message::Text(payload)).await {
            println!("Failed to send response: {:?}", err)
        }
    }

    async fn do_send_request<P: Serialize, R: DeserializeOwned>(&self, request: P, action: &str) -> Result<Result<R, OCPP1_6Error>, Box<dyn std::error::Error + Send + Sync>> {
        let message_id = Uuid::new_v4();

        let call = RawOcpp1_6Call(2, message_id.to_string(), action.to_string(), serde_json::to_value(&request)?);

        {
            let mut lock = self.sink.lock().await;
            lock.send(Message::Text(serde_json::to_string(&call)?)).await?;
        }

        let (s, r) = oneshot::channel();
        {
            let mut response_channels = self.response_channels.lock().await;
            response_channels.insert(message_id, s);
        }

         match r.await? {
             Ok(value) => {
                 Ok(Ok(serde_json::from_value(value)?))
             }
             Err(e) => Ok(Err(e))
         }
    }
}