use std::collections::{BTreeMap, VecDeque};
use std::future::Future;
use std::sync::Arc;
use futures::stream::SplitSink;
use futures::{SinkExt, StreamExt, TryStreamExt};
use rust_ocpp::v2_0_1::messages::authorize::{AuthorizeRequest, AuthorizeResponse};
use rust_ocpp::v2_0_1::messages::boot_notification::{BootNotificationRequest, BootNotificationResponse};
use rust_ocpp::v2_0_1::messages::cancel_reservation::{CancelReservationRequest, CancelReservationResponse};
use rust_ocpp::v2_0_1::messages::certificate_signed::{CertificateSignedRequest, CertificateSignedResponse};
use rust_ocpp::v2_0_1::messages::change_availability::{ChangeAvailabilityRequest, ChangeAvailabilityResponse};
use rust_ocpp::v2_0_1::messages::clear_cache::{ClearCacheRequest, ClearCacheResponse};
use rust_ocpp::v2_0_1::messages::clear_charging_profile::{ClearChargingProfileRequest, ClearChargingProfileResponse};
use rust_ocpp::v2_0_1::messages::clear_display_message::{ClearDisplayMessageRequest, ClearDisplayMessageResponse};
use rust_ocpp::v2_0_1::messages::clear_variable_monitoring::{ClearVariableMonitoringRequest, ClearVariableMonitoringResponse};
use rust_ocpp::v2_0_1::messages::cleared_charging_limit::{ClearedChargingLimitRequest, ClearedChargingLimitResponse};
use rust_ocpp::v2_0_1::messages::cost_updated::{CostUpdatedRequest, CostUpdatedResponse};
use rust_ocpp::v2_0_1::messages::customer_information::{CustomerInformationRequest, CustomerInformationResponse};
use rust_ocpp::v2_0_1::messages::datatransfer::{DataTransferRequest, DataTransferResponse};
use rust_ocpp::v2_0_1::messages::delete_certificate::{DeleteCertificateRequest, DeleteCertificateResponse};
use rust_ocpp::v2_0_1::messages::firmware_status_notification::{FirmwareStatusNotificationRequest, FirmwareStatusNotificationResponse};
use rust_ocpp::v2_0_1::messages::get_15118ev_certificate::{Get15118EVCertificateRequest, Get15118EVCertificateResponse};
use rust_ocpp::v2_0_1::messages::get_base_report::{GetBaseReportRequest, GetBaseReportResponse};
use rust_ocpp::v2_0_1::messages::get_certificate_status::{GetCertificateStatusRequest, GetCertificateStatusResponse};
use rust_ocpp::v2_0_1::messages::get_charging_profiles::{GetChargingProfilesRequest, GetChargingProfilesResponse};
use rust_ocpp::v2_0_1::messages::get_composite_schedule::{GetCompositeScheduleRequest, GetCompositeScheduleResponse};
use rust_ocpp::v2_0_1::messages::get_display_message::{GetDisplayMessagesRequest, GetDisplayMessagesResponse};
use rust_ocpp::v2_0_1::messages::get_installed_certificate_ids::{GetInstalledCertificateIdsRequest, GetInstalledCertificateIdsResponse};
use rust_ocpp::v2_0_1::messages::get_local_list_version::{GetLocalListVersionRequest, GetLocalListVersionResponse};
use rust_ocpp::v2_0_1::messages::get_log::{GetLogRequest, GetLogResponse};
use rust_ocpp::v2_0_1::messages::get_monitoring_report::{GetMonitoringReportRequest, GetMonitoringReportResponse};
use rust_ocpp::v2_0_1::messages::get_report::{GetReportRequest, GetReportResponse};
use rust_ocpp::v2_0_1::messages::get_transaction_status::{GetTransactionStatusRequest, GetTransactionStatusResponse};
use rust_ocpp::v2_0_1::messages::get_variables::{GetVariablesRequest, GetVariablesResponse};
use rust_ocpp::v2_0_1::messages::heartbeat::{HeartbeatRequest, HeartbeatResponse};
use rust_ocpp::v2_0_1::messages::install_certificate::{InstallCertificateRequest, InstallCertificateResponse};
use rust_ocpp::v2_0_1::messages::log_status_notification::{LogStatusNotificationRequest, LogStatusNotificationResponse};
use rust_ocpp::v2_0_1::messages::meter_values::{MeterValuesRequest, MeterValuesResponse};
use rust_ocpp::v2_0_1::messages::notify_charging_limit::{NotifyChargingLimitRequest, NotifyChargingLimitResponse};
use rust_ocpp::v2_0_1::messages::notify_customer_information::{NotifyCustomerInformationRequest, NotifyCustomerInformationResponse};
use rust_ocpp::v2_0_1::messages::notify_display_messages::{NotifyDisplayMessagesRequest, NotifyDisplayMessagesResponse};
use rust_ocpp::v2_0_1::messages::notify_ev_charging_needs::{NotifyEVChargingNeedsRequest, NotifyEVChargingNeedsResponse};
use rust_ocpp::v2_0_1::messages::notify_ev_charging_schedule::{NotifyEVChargingScheduleRequest, NotifyEVChargingScheduleResponse};
use rust_ocpp::v2_0_1::messages::notify_event::{NotifyEventRequest, NotifyEventResponse};
use rust_ocpp::v2_0_1::messages::notify_monitoring_report::{NotifyMonitoringReportRequest, NotifyMonitoringReportResponse};
use rust_ocpp::v2_0_1::messages::notify_report::{NotifyReportRequest, NotifyReportResponse};
use rust_ocpp::v2_0_1::messages::publish_firmware::{PublishFirmwareRequest, PublishFirmwareResponse};
use rust_ocpp::v2_0_1::messages::publish_firmware_status_notification::{PublishFirmwareStatusNotificationRequest, PublishFirmwareStatusNotificationResponse};
use rust_ocpp::v2_0_1::messages::report_charging_profiles::{ReportChargingProfilesRequest, ReportChargingProfilesResponse};
use rust_ocpp::v2_0_1::messages::request_start_transaction::{RequestStartTransactionRequest, RequestStartTransactionResponse};
use rust_ocpp::v2_0_1::messages::request_stop_transaction::{RequestStopTransactionRequest, RequestStopTransactionResponse};
use rust_ocpp::v2_0_1::messages::reservation_status_update::{ReservationStatusUpdateRequest, ReservationStatusUpdateResponse};
use rust_ocpp::v2_0_1::messages::reserve_now::{ReserveNowRequest, ReserveNowResponse};
use rust_ocpp::v2_0_1::messages::reset::{ResetRequest, ResetResponse};
use rust_ocpp::v2_0_1::messages::send_local_list::{SendLocalListRequest, SendLocalListResponse};
use rust_ocpp::v2_0_1::messages::set_charging_profile::{SetChargingProfileRequest, SetChargingProfileResponse};
use rust_ocpp::v2_0_1::messages::set_display_message::{SetDisplayMessageRequest, SetDisplayMessageResponse};
use rust_ocpp::v2_0_1::messages::set_monitoring_base::{SetMonitoringBaseRequest, SetMonitoringBaseResponse};
use rust_ocpp::v2_0_1::messages::set_monitoring_level::{SetMonitoringLevelRequest, SetMonitoringLevelResponse};
use rust_ocpp::v2_0_1::messages::set_network_profile::{SetNetworkProfileRequest, SetNetworkProfileResponse};
use rust_ocpp::v2_0_1::messages::set_variable_monitoring::{SetVariableMonitoringRequest, SetVariableMonitoringResponse};
use rust_ocpp::v2_0_1::messages::set_variables::{SetVariablesRequest, SetVariablesResponse};
use rust_ocpp::v2_0_1::messages::sign_certificate::{SignCertificateRequest, SignCertificateResponse};
use rust_ocpp::v2_0_1::messages::status_notification::{StatusNotificationRequest, StatusNotificationResponse};
use rust_ocpp::v2_0_1::messages::transaction_event::{TransactionEventRequest, TransactionEventResponse};
use rust_ocpp::v2_0_1::messages::trigger_message::{TriggerMessageRequest, TriggerMessageResponse};
use rust_ocpp::v2_0_1::messages::unlock_connector::{UnlockConnectorRequest, UnlockConnectorResponse};
use rust_ocpp::v2_0_1::messages::unpublish_firmware::{UnpublishFirmwareRequest, UnpublishFirmwareResponse};
use rust_ocpp::v2_0_1::messages::update_firmware::{UpdateFirmwareRequest, UpdateFirmwareResponse};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use tokio::net::TcpStream;
use tokio::sync::{Mutex, oneshot};
use tokio::sync::broadcast::Sender;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;
use crate::ocpp_2_0_1::OCPP2_0_1Error;
use crate::ocpp_2_0_1::raw_ocpp_2_0_1_call::RawOcpp2_0_1Call;
use crate::ocpp_2_0_1::raw_ocpp_2_0_1_error::RawOcpp2_0_1Error;
use crate::ocpp_2_0_1::raw_ocpp_2_0_1_result::RawOcpp2_0_1Result;

/// OCPP 2.0.1 client
#[derive(Clone)]
pub struct OCPP2_0_1Client {
    sink: Arc<Mutex<SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>>>,
    response_channels: Arc<Mutex<BTreeMap<Uuid, oneshot::Sender<Result<Value, OCPP2_0_1Error>>>>>,
    request_sender: Sender<RawOcpp2_0_1Call>,
    pong_channels: Arc<Mutex<VecDeque<oneshot::Sender<()>>>>,
    ping_sender: Sender<()>,
}

impl OCPP2_0_1Client {
    pub(crate) fn new(stream: WebSocketStream<MaybeTlsStream<TcpStream>>) -> Self {
        let (sink, stream) = stream.split();

        let response_channels = Arc::new(Mutex::new(BTreeMap::<Uuid, oneshot::Sender<Result<Value, OCPP2_0_1Error>>>::new()));
        let response_channels2 = Arc::clone(&response_channels);

        let pong_channels = Arc::new(Mutex::new(VecDeque::<oneshot::Sender<()>>::new()));
        let pong_channels2 = Arc::clone(&pong_channels);

        let (request_sender, _) = tokio::sync::broadcast::channel(1000);

        let request_sender2 = request_sender.clone();

        let (ping_sender, _) = tokio::sync::broadcast::channel(10);
        let ping_sender2 = ping_sender.clone();

        tokio::spawn(async move {
            stream
                .map_err(|e| Box::<dyn std::error::Error + Send + Sync>::from(e))
                .try_for_each(|message| {
                    let request_sender = request_sender2.clone();
                    let response_channels2 = response_channels2.clone();
                    let ping_sender = ping_sender2.clone();
                    let pong_channels2 = pong_channels2.clone();
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
                                                                let call: RawOcpp2_0_1Call =
                                                                    serde_json::from_str(&raw_payload).unwrap();
                                                                if let Err(err) = value.send(call) {
                                                                    println!("Error sending request: {:?}", err);
                                                                };

                                                            },
                                                            // RESPONSE
                                                            3 => {
                                                                let result: RawOcpp2_0_1Result =
                                                                    serde_json::from_str(&raw_payload).unwrap();
                                                                let mut lock = response_channels2.lock().await;
                                                                if let Some(sender) = lock.remove(&Uuid::parse_str(&result.1)?) {
                                                                    sender.send(Ok(result.2)).unwrap();
                                                                }
                                                            },
                                                            // ERROR
                                                            4 => {
                                                                let error: RawOcpp2_0_1Error =
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
                                Message::Ping(_) => {
                                    if ping_sender.receiver_count() > 0 {
                                        if let Err(err) = ping_sender.send(()) {
                                            println!("Error sending websocket ping: {:?}", err);
                                        };
                                    }
                                }
                                Message::Pong(_) => {
                                    let mut lock = pong_channels2.lock().await;
                                    if let Some(sender) = lock.pop_back() {
                                        sender.send(()).unwrap();
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
            request_sender,
            pong_channels,
            ping_sender,
        }
    }

    /// Disconnect from the server
    pub async fn disconnect(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut lock = self.sink.lock().await;
        lock.close().await?;
        Ok(())
    }

    pub async fn send_authorize(&self, request: AuthorizeRequest) -> Result<Result<AuthorizeResponse, OCPP2_0_1Error>, Box<dyn std::error::Error + Send + Sync>> {
        self.do_send_request(request, "Authorize").await
    }

    pub async fn send_boot_notification(&self, request: BootNotificationRequest) -> Result<Result<BootNotificationResponse, OCPP2_0_1Error>, Box<dyn std::error::Error + Send + Sync>> {
        self.do_send_request(request, "BootNotification").await
    }

    pub async fn send_cleared_charging_limit_request(&self, request: ClearedChargingLimitRequest) -> Result<Result<ClearedChargingLimitResponse, OCPP2_0_1Error>, Box<dyn std::error::Error + Send + Sync>> {
        self.do_send_request(request, "ClearedChargingLimit").await
    }

    pub async fn send_data_transfer(&self, request: DataTransferRequest) -> Result<Result<DataTransferResponse, OCPP2_0_1Error>, Box<dyn std::error::Error + Send + Sync>> {
        self.do_send_request(request, "DataTransfer").await
    }

    pub async fn send_firmware_status_notification(&self, request: FirmwareStatusNotificationRequest) -> Result<Result<FirmwareStatusNotificationResponse, OCPP2_0_1Error>, Box<dyn std::error::Error + Send + Sync>> {
        self.do_send_request(request, "FirmwareStatusNotification").await
    }

    pub async fn send_get_15118_ev_certificate(&self, request: Get15118EVCertificateRequest) -> Result<Result<Get15118EVCertificateResponse, OCPP2_0_1Error>, Box<dyn std::error::Error + Send + Sync>> {
        self.do_send_request(request, "Get15118EVCertificate").await
    }

    pub async fn send_get_certificate_status(&self, request: GetCertificateStatusRequest) -> Result<Result<GetCertificateStatusResponse, OCPP2_0_1Error>, Box<dyn std::error::Error + Send + Sync>> {
        self.do_send_request(request, "GetCertificateStatus").await
    }

    pub async fn send_heartbeat(&self, request: HeartbeatRequest) -> Result<Result<HeartbeatResponse, OCPP2_0_1Error>, Box<dyn std::error::Error + Send + Sync>> {
        self.do_send_request(request, "Heartbeat").await
    }

    pub async fn send_log_status_notification(&self, request: LogStatusNotificationRequest) -> Result<Result<LogStatusNotificationResponse, OCPP2_0_1Error>, Box<dyn std::error::Error + Send + Sync>> {
        self.do_send_request(request, "LogStatusNotification").await
    }

    pub async fn send_meter_values(&self, request: MeterValuesRequest) -> Result<Result<MeterValuesResponse, OCPP2_0_1Error>, Box<dyn std::error::Error + Send + Sync>> {
        self.do_send_request(request, "MeterValues").await
    }

    pub async fn send_notify_charging_limit(&self, request: NotifyChargingLimitRequest) -> Result<Result<NotifyChargingLimitResponse, OCPP2_0_1Error>, Box<dyn std::error::Error + Send + Sync>> {
        self.do_send_request(request, "NotifyChargingLimit").await
    }

    pub async fn send_notify_customer_information(&self, request: NotifyCustomerInformationRequest) -> Result<Result<NotifyCustomerInformationResponse, OCPP2_0_1Error>, Box<dyn std::error::Error + Send + Sync>> {
        self.do_send_request(request, "NotifyCustomerInformation").await
    }

    pub async fn send_notify_display_messages(&self, request: NotifyDisplayMessagesRequest) -> Result<Result<NotifyDisplayMessagesResponse, OCPP2_0_1Error>, Box<dyn std::error::Error + Send + Sync>> {
        self.do_send_request(request, "NotifyDisplayMessages").await
    }

    pub async fn send_notify_ev_charging_needs(&self, request: NotifyEVChargingNeedsRequest) -> Result<Result<NotifyEVChargingNeedsResponse, OCPP2_0_1Error>, Box<dyn std::error::Error + Send + Sync>> {
        self.do_send_request(request, "NotifyEVChargingNeeds").await
    }

    pub async fn send_notify_ev_charging_schedule(&self, request: NotifyEVChargingScheduleRequest) -> Result<Result<NotifyEVChargingScheduleResponse, OCPP2_0_1Error>, Box<dyn std::error::Error + Send + Sync>> {
        self.do_send_request(request, "NotifyEVChargingSchedule").await
    }

    pub async fn send_notify_event(&self, request: NotifyEventRequest) -> Result<Result<NotifyEventResponse, OCPP2_0_1Error>, Box<dyn std::error::Error + Send + Sync>> {
        self.do_send_request(request, "NotifyEvent").await
    }

    pub async fn send_notify_monitoring_report(&self, request: NotifyMonitoringReportRequest) -> Result<Result<NotifyMonitoringReportResponse, OCPP2_0_1Error>, Box<dyn std::error::Error + Send + Sync>> {
        self.do_send_request(request, "NotifyMonitoringReport").await
    }

    pub async fn send_notify_report(&self, request: NotifyReportRequest) -> Result<Result<NotifyReportResponse, OCPP2_0_1Error>, Box<dyn std::error::Error + Send + Sync>> {
        self.do_send_request(request, "NotifyReport").await
    }

    pub async fn send_publish_firmware_status_notification(&self, request: PublishFirmwareStatusNotificationRequest) -> Result<Result<PublishFirmwareStatusNotificationResponse, OCPP2_0_1Error>, Box<dyn std::error::Error + Send + Sync>> {
        self.do_send_request(request, "PublishFirmwareStatusNotification").await
    }

    pub async fn send_report_charging_profiles(&self, request: ReportChargingProfilesRequest) -> Result<Result<ReportChargingProfilesResponse, OCPP2_0_1Error>, Box<dyn std::error::Error + Send + Sync>> {
        self.do_send_request(request, "ReportChargingProfiles").await
    }

    pub async fn send_request_start_transaction(&self, request: RequestStartTransactionRequest) -> Result<Result<RequestStartTransactionResponse, OCPP2_0_1Error>, Box<dyn std::error::Error + Send + Sync>> {
        self.do_send_request(request, "RequestStartTransaction").await
    }

    pub async fn send_request_stop_transaction(&self, request: RequestStopTransactionRequest) -> Result<Result<RequestStopTransactionResponse, OCPP2_0_1Error>, Box<dyn std::error::Error + Send + Sync>> {
        self.do_send_request(request, "RequestStopTransaction").await
    }

    pub async fn send_reservation_status_update(&self, request: ReservationStatusUpdateRequest) -> Result<Result<ReservationStatusUpdateResponse, OCPP2_0_1Error>, Box<dyn std::error::Error + Send + Sync>> {
        self.do_send_request(request, "ReservationStatusUpdate").await
    }

    pub async fn send_sign_certificate(&self, request: SignCertificateRequest) -> Result<Result<SignCertificateResponse, OCPP2_0_1Error>, Box<dyn std::error::Error + Send + Sync>> {
        self.do_send_request(request, "SignCertificate").await
    }

    pub async fn send_status_notification(&self, request: StatusNotificationRequest) -> Result<Result<StatusNotificationResponse, OCPP2_0_1Error>, Box<dyn std::error::Error + Send + Sync>> {
        self.do_send_request(request, "StatusNotification").await
    }

    pub async fn send_transaction_event(&self, request: TransactionEventRequest) -> Result<Result<TransactionEventResponse, OCPP2_0_1Error>, Box<dyn std::error::Error + Send + Sync>> {
        self.do_send_request(request, "TransactionEvent").await
    }

    pub async fn send_ping(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        {
            let mut lock = self.sink.lock().await;
            lock.send(Message::Ping(vec![])).await?;
        }

        let (s, r) = oneshot::channel();
        {
            let mut pong_channels = self.pong_channels.lock().await;
            pong_channels.push_front(s);
        }

        r.await?;
        Ok(())
    }

    pub async fn inspect_raw_message<F: FnMut(String, Value) -> FF + Send + Sync + 'static, FF: Future<Output=()> + Send + Sync>(&self, mut callback: F){
        let mut recv = self.request_sender.subscribe();
        tokio::spawn(async move {
            while let Ok(call) = recv.recv().await {
                callback(call.2.to_string(), call.3.to_owned()).await;
            }
        });
    }

    pub async fn on_cancel_reservation<F: FnMut(CancelReservationRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<CancelReservationResponse, OCPP2_0_1Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "CancelReservation").await
    }

    pub async fn on_certificate_signed<F: FnMut(CertificateSignedRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<CertificateSignedResponse, OCPP2_0_1Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "CertificateSigned").await
    }

    pub async fn on_change_availability<F: FnMut(ChangeAvailabilityRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<ChangeAvailabilityResponse, OCPP2_0_1Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "ChangeAvailability").await
    }

    pub async fn on_clear_cache<F: FnMut(ClearCacheRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<ClearCacheResponse, OCPP2_0_1Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "ClearCache").await
    }

    pub async fn on_clear_charging_profile<F: FnMut(ClearChargingProfileRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<ClearChargingProfileResponse, OCPP2_0_1Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "ClearChargingProfile").await
    }

    pub async fn on_clear_display_message<F: FnMut(ClearDisplayMessageRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<ClearDisplayMessageResponse, OCPP2_0_1Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "ClearDisplayMessage").await
    }

    pub async fn on_clear_variable_monitoring<F: FnMut(ClearVariableMonitoringRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<ClearVariableMonitoringResponse, OCPP2_0_1Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "ClearVariableMonitoring").await
    }

    pub async fn on_cost_updated<F: FnMut(CostUpdatedRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<CostUpdatedResponse, OCPP2_0_1Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "CostUpdated").await
    }

    pub async fn on_customer_information<F: FnMut(CustomerInformationRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<CustomerInformationResponse, OCPP2_0_1Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "CustomerInformation").await
    }

    pub async fn on_data_transfer<F: FnMut(DataTransferRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<DataTransferResponse, OCPP2_0_1Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "DataTransfer").await
    }

    pub async fn on_delete_certificate<F: FnMut(DeleteCertificateRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<DeleteCertificateResponse, OCPP2_0_1Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "DeleteCertificate").await
    }

    pub async fn on_get_base_report<F: FnMut(GetBaseReportRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<GetBaseReportResponse, OCPP2_0_1Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "GetBaseReport").await
    }

    pub async fn on_get_charging_profiles<F: FnMut(GetChargingProfilesRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<GetChargingProfilesResponse, OCPP2_0_1Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "GetChargingProfiles").await
    }

    pub async fn on_get_composite_schedule<F: FnMut(GetCompositeScheduleRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<GetCompositeScheduleResponse, OCPP2_0_1Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "GetCompositeSchedule").await
    }

    pub async fn on_get_display_messages<F: FnMut(GetDisplayMessagesRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<GetDisplayMessagesResponse, OCPP2_0_1Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "GetDisplayMessages").await
    }

    pub async fn on_get_installed_certificate_ids<F: FnMut(GetInstalledCertificateIdsRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<GetInstalledCertificateIdsResponse, OCPP2_0_1Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "GetInstalledCertificateIds").await
    }

    pub async fn on_get_local_list_version<F: FnMut(GetLocalListVersionRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<GetLocalListVersionResponse, OCPP2_0_1Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "GetLocalListVersion").await
    }

    pub async fn on_get_log<F: FnMut(GetLogRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<GetLogResponse, OCPP2_0_1Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "GetLog").await
    }

    pub async fn on_get_monitoring_report<F: FnMut(GetMonitoringReportRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<GetMonitoringReportResponse, OCPP2_0_1Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "GetMonitoringReport").await
    }

    pub async fn on_get_report<F: FnMut(GetReportRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<GetReportResponse, OCPP2_0_1Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "GetReport").await
    }

    pub async fn on_get_transaction_status<F: FnMut(GetTransactionStatusRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<GetTransactionStatusResponse, OCPP2_0_1Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "GetTransactionStatus").await
    }

    pub async fn on_get_variables<F: FnMut(GetVariablesRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<GetVariablesResponse, OCPP2_0_1Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "GetVariables").await
    }

    pub async fn on_install_certificate<F: FnMut(InstallCertificateRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<InstallCertificateResponse, OCPP2_0_1Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "InstallCertificate").await
    }

    pub async fn on_publish_firmware<F: FnMut(PublishFirmwareRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<PublishFirmwareResponse, OCPP2_0_1Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "PublishFirmware").await
    }

    pub async fn on_reserve_now<F: FnMut(ReserveNowRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<ReserveNowResponse, OCPP2_0_1Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "ReserveNow").await
    }

    pub async fn on_reset<F: FnMut(ResetRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<ResetResponse, OCPP2_0_1Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "Reset").await
    }

    pub async fn on_send_local_list<F: FnMut(SendLocalListRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<SendLocalListResponse, OCPP2_0_1Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "SendLocalList").await
    }

    pub async fn on_set_charging_profile<F: FnMut(SetChargingProfileRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<SetChargingProfileResponse, OCPP2_0_1Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "SetChargingProfile").await
    }

    pub async fn on_set_display_message<F: FnMut(SetDisplayMessageRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<SetDisplayMessageResponse, OCPP2_0_1Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "SetDisplayMessage").await
    }

    pub async fn on_set_monitoring_base<F: FnMut(SetMonitoringBaseRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<SetMonitoringBaseResponse, OCPP2_0_1Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "SetMonitoringBase").await
    }

    pub async fn on_set_monitoring_level<F: FnMut(SetMonitoringLevelRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<SetMonitoringLevelResponse, OCPP2_0_1Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "SetMonitoringLevel").await
    }

    pub async fn on_set_network_profile<F: FnMut(SetNetworkProfileRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<SetNetworkProfileResponse, OCPP2_0_1Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "SetNetworkProfile").await
    }

    pub async fn on_set_variable_monitoring<F: FnMut(SetVariableMonitoringRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<SetVariableMonitoringResponse, OCPP2_0_1Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "SetVariableMonitoring").await
    }

    pub async fn on_set_variables<F: FnMut(SetVariablesRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<SetVariablesResponse, OCPP2_0_1Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "SetVariables").await
    }

    pub async fn on_trigger_message<F: FnMut(TriggerMessageRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<TriggerMessageResponse, OCPP2_0_1Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "TriggerMessage").await
    }

    pub async fn on_unlock_connector<F: FnMut(UnlockConnectorRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<UnlockConnectorResponse, OCPP2_0_1Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "UnlockConnector").await
    }

    pub async fn on_unpublish_firmware<F: FnMut(UnpublishFirmwareRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<UnpublishFirmwareResponse, OCPP2_0_1Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "UnpublishFirmware").await
    }

    pub async fn on_update_firmware<F: FnMut(UpdateFirmwareRequest, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<UpdateFirmwareResponse, OCPP2_0_1Error>> + Send + Sync>(&self, callback: F) {
        self.handle_on_request(callback, "UpdateFirmware").await
    }

    pub async fn on_ping<F: FnMut(Self) -> FF + Send + Sync + 'static, FF: Future<Output=()> + Send + Sync>(&self, mut callback: F) {
        let mut recv = self.ping_sender.subscribe();

        let s = self.clone();
        tokio::spawn(async move {
            while let Ok(()) = recv.recv().await {
                callback(s.clone()).await;
            }
        });
    }

    async fn handle_on_request<P: DeserializeOwned + Send + Sync, R: Serialize + Send + Sync, F: FnMut(P, Self) -> FF + Send + Sync + 'static, FF: Future<Output=Result<R, OCPP2_0_1Error>> + Send + Sync>(&self, mut callback: F, action: &'static str) {
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

    async fn do_send_response<R: Serialize>(&self, response: Result<R, OCPP2_0_1Error>, message_id: &str) {
        let payload = match response {
            Ok(r) => {
                serde_json::to_string(&RawOcpp2_0_1Result(3, message_id.to_string(), serde_json::to_value(r).unwrap())).unwrap()
            }
            Err(e) => {
                serde_json::to_string(&RawOcpp2_0_1Error(4, message_id.to_string(), e.code().to_string(), e.description().to_string(), e.details().to_owned())).unwrap()
            }
        };

        let mut lock = self.sink.lock().await;
        if let Err(err) = lock.send(Message::Text(payload)).await {
            println!("Failed to send response: {:?}", err)
        }
    }

    async fn do_send_request<P: Serialize, R: DeserializeOwned>(&self, request: P, action: &str) -> Result<Result<R, OCPP2_0_1Error>, Box<dyn std::error::Error + Send + Sync>> {
        let message_id = Uuid::new_v4();

        let call = RawOcpp2_0_1Call(2, message_id.to_string(), action.to_string(), serde_json::to_value(&request)?);

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