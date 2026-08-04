use crate::action::{Action, SendAction};
use crate::error::ClientError;
use crate::ocpp_2_1::OCPP2_1Client;
use crate::ocpp_2_1::error::OCPP2_1Error;
use core::future::Future;
use ocpp_types::v21::{
    AFRRSignalRequest, AFRRSignalResponse, AdjustPeriodicEventStreamRequest,
    AdjustPeriodicEventStreamResponse, AuthorizeRequest, AuthorizeResponse, BatterySwapRequest,
    BatterySwapResponse, BootNotificationRequest, BootNotificationResponse,
    CancelReservationRequest, CancelReservationResponse, CertificateSignedRequest,
    CertificateSignedResponse, ChangeAvailabilityRequest, ChangeAvailabilityResponse,
    ChangeTransactionTariffRequest, ChangeTransactionTariffResponse, ClearCacheRequest,
    ClearCacheResponse, ClearChargingProfileRequest, ClearChargingProfileResponse,
    ClearDERControlRequest, ClearDERControlResponse, ClearDisplayMessageRequest,
    ClearDisplayMessageResponse, ClearTariffsRequest, ClearTariffsResponse,
    ClearVariableMonitoringRequest, ClearVariableMonitoringResponse, ClearedChargingLimitRequest,
    ClearedChargingLimitResponse, ClosePeriodicEventStreamRequest,
    ClosePeriodicEventStreamResponse, CostUpdatedRequest, CostUpdatedResponse,
    CustomerInformationRequest, CustomerInformationResponse, DataTransferRequest,
    DataTransferResponse, DeleteCertificateRequest, DeleteCertificateResponse,
    FirmwareStatusNotificationRequest, FirmwareStatusNotificationResponse,
    Get15118EVCertificateRequest, Get15118EVCertificateResponse, GetBaseReportRequest,
    GetBaseReportResponse, GetCertificateChainStatusRequest, GetCertificateChainStatusResponse,
    GetCertificateStatusRequest, GetCertificateStatusResponse, GetChargingProfilesRequest,
    GetChargingProfilesResponse, GetCompositeScheduleRequest, GetCompositeScheduleResponse,
    GetDisplayMessagesRequest, GetDisplayMessagesResponse, GetInstalledCertificateIdsRequest,
    GetInstalledCertificateIdsResponse, GetLocalListVersionRequest, GetLocalListVersionResponse,
    GetLogRequest, GetLogResponse, GetMonitoringReportRequest, GetMonitoringReportResponse,
    GetPeriodicEventStreamRequest, GetPeriodicEventStreamResponse, GetReportRequest,
    GetReportResponse, GetTariffsRequest, GetTariffsResponse, GetTransactionStatusRequest,
    GetTransactionStatusResponse, GetVariablesRequest, GetVariablesResponse, HeartbeatRequest,
    HeartbeatResponse, InstallCertificateRequest, InstallCertificateResponse,
    LogStatusNotificationRequest, LogStatusNotificationResponse, MeterValuesRequest,
    MeterValuesResponse, NotifyAllowedEnergyTransferRequest, NotifyAllowedEnergyTransferResponse,
    NotifyChargingLimitRequest, NotifyChargingLimitResponse, NotifyCustomerInformationRequest,
    NotifyCustomerInformationResponse, NotifyDERAlarmRequest, NotifyDERAlarmResponse,
    NotifyDERStartStopRequest, NotifyDERStartStopResponse, NotifyDisplayMessagesRequest,
    NotifyDisplayMessagesResponse, NotifyEVChargingNeedsRequest, NotifyEVChargingNeedsResponse,
    NotifyEVChargingScheduleRequest, NotifyEVChargingScheduleResponse, NotifyEventRequest,
    NotifyEventResponse, NotifyMonitoringReportRequest, NotifyMonitoringReportResponse,
    NotifyPeriodicEventStream, NotifyPriorityChargingRequest, NotifyPriorityChargingResponse,
    NotifyReportRequest, NotifyReportResponse, NotifySettlementRequest, NotifySettlementResponse,
    NotifyWebPaymentStartedRequest, NotifyWebPaymentStartedResponse,
    OpenPeriodicEventStreamRequest, OpenPeriodicEventStreamResponse, PublishFirmwareRequest,
    PublishFirmwareResponse, PublishFirmwareStatusNotificationRequest,
    PublishFirmwareStatusNotificationResponse, PullDynamicScheduleUpdateRequest,
    PullDynamicScheduleUpdateResponse, ReportChargingProfilesRequest,
    ReportChargingProfilesResponse, ReportDERControlRequest, ReportDERControlResponse,
    RequestBatterySwapRequest, RequestBatterySwapResponse, RequestStartTransactionRequest,
    RequestStartTransactionResponse, RequestStopTransactionRequest, RequestStopTransactionResponse,
    ReservationStatusUpdateRequest, ReservationStatusUpdateResponse, ReserveNowRequest,
    ReserveNowResponse, ResetRequest, ResetResponse, SecurityEventNotificationRequest,
    SecurityEventNotificationResponse, SendLocalListRequest, SendLocalListResponse,
    SetChargingProfileRequest, SetChargingProfileResponse, SetDefaultTariffRequest,
    SetDefaultTariffResponse, SetMonitoringBaseRequest, SetMonitoringBaseResponse,
    SetMonitoringLevelRequest, SetMonitoringLevelResponse, SetNetworkProfileRequest,
    SetNetworkProfileResponse, SetVariableMonitoringRequest, SetVariableMonitoringResponse,
    SetVariablesRequest, SetVariablesResponse, SignCertificateRequest, SignCertificateResponse,
    StatusNotificationRequest, StatusNotificationResponse, TransactionEventRequest,
    TransactionEventResponse, UnlockConnectorRequest, UnlockConnectorResponse,
    UnpublishFirmwareRequest, UnpublishFirmwareResponse, UpdateFirmwareRequest,
    UpdateFirmwareResponse, UsePriorityChargingRequest, UsePriorityChargingResponse,
    VatNumberValidationRequest, VatNumberValidationResponse,
};

/// Same pattern as `ocpp_2_0_1_action!` (see `src/ocpp_2_0_1/actions.rs`): one marker type
/// implementing [`Action`] plus `send_x`/`on_x`/`wait_for_x` convenience methods on
/// [`OCPP2_1Client`], generated from one macro line per action.
macro_rules! ocpp_2_1_action {
    ($name:ident, $req:ty, $res:ty, $action:literal, $send:ident, $on:ident, $wait_for:ident) => {
        #[doc = concat!("Marker type for the `", $action, "` action.")]
        pub struct $name;

        impl Action for $name {
            const NAME: &'static str = $action;
            type Request = $req;
            type Response = $res;
        }

        impl OCPP2_1Client {
            pub async fn $send(&self, request: $req) -> Result<$res, ClientError<OCPP2_1Error>> {
                self.call::<$name>(request).await
            }

            pub async fn $on<F, FF>(&self, callback: F)
            where
                F: FnMut($req, Self) -> FF + Send + Sync + 'static,
                FF: Future<Output = Result<$res, OCPP2_1Error>> + Send,
            {
                self.on::<$name, F, FF>(callback).await
            }

            #[cfg(feature = "test")]
            pub async fn $wait_for<F, FF>(
                &self,
                callback: F,
            ) -> Result<$req, ClientError<OCPP2_1Error>>
            where
                F: FnMut($req, Self) -> FF + Send + Sync + 'static,
                FF: Future<Output = Result<$res, OCPP2_1Error>> + Send,
            {
                self.wait_for::<$name, F, FF>(callback).await
            }
        }
    };
}

/// Like `ocpp_2_1_action!`, but for OCPP-J 2.1's `SEND` (fire-and-forget) message type
/// instead of a CALL/CALLRESULT pair: `$payload` is a single struct (e.g.
/// `NotifyPeriodicEventStream`), not a Request/Response pair, and the generated `$on` callback
/// returns nothing, since the spec forbids replying to a `SEND`.
macro_rules! ocpp_2_1_send_action {
    ($name:ident, $payload:ty, $action:literal, $send:ident, $on:ident) => {
        #[doc = concat!("Marker type for the `", $action, "` SEND message.")]
        pub struct $name;

        impl SendAction for $name {
            const NAME: &'static str = $action;
            type Payload = $payload;
        }

        impl OCPP2_1Client {
            pub async fn $send(&self, payload: $payload) -> Result<(), ClientError<OCPP2_1Error>> {
                self.send_notification::<$name>(payload).await
            }

            pub async fn $on<F, FF>(&self, callback: F)
            where
                F: FnMut($payload, Self) -> FF + Send + Sync + 'static,
                FF: Future<Output = ()> + Send,
            {
                self.on_notification::<$name, F, FF>(callback).await
            }
        }
    };
}

ocpp_2_1_action!(
    AdjustPeriodicEventStream,
    AdjustPeriodicEventStreamRequest,
    AdjustPeriodicEventStreamResponse,
    "AdjustPeriodicEventStream",
    send_adjust_periodic_event_stream,
    on_adjust_periodic_event_stream,
    wait_for_adjust_periodic_event_stream
);
ocpp_2_1_action!(
    AFRRSignal,
    AFRRSignalRequest,
    AFRRSignalResponse,
    "AFRRSignal",
    send_afrr_signal,
    on_afrr_signal,
    wait_for_afrr_signal
);
ocpp_2_1_action!(
    Authorize,
    AuthorizeRequest,
    AuthorizeResponse,
    "Authorize",
    send_authorize,
    on_authorize,
    wait_for_authorize
);
ocpp_2_1_action!(
    BatterySwap,
    BatterySwapRequest,
    BatterySwapResponse,
    "BatterySwap",
    send_battery_swap,
    on_battery_swap,
    wait_for_battery_swap
);
ocpp_2_1_action!(
    BootNotification,
    BootNotificationRequest,
    BootNotificationResponse,
    "BootNotification",
    send_boot_notification,
    on_boot_notification,
    wait_for_boot_notification
);
ocpp_2_1_action!(
    CancelReservation,
    CancelReservationRequest,
    CancelReservationResponse,
    "CancelReservation",
    send_cancel_reservation,
    on_cancel_reservation,
    wait_for_cancel_reservation
);
ocpp_2_1_action!(
    CertificateSigned,
    CertificateSignedRequest,
    CertificateSignedResponse,
    "CertificateSigned",
    send_certificate_signed,
    on_certificate_signed,
    wait_for_certificate_signed
);
ocpp_2_1_action!(
    ChangeAvailability,
    ChangeAvailabilityRequest,
    ChangeAvailabilityResponse,
    "ChangeAvailability",
    send_change_availability,
    on_change_availability,
    wait_for_change_availability
);
ocpp_2_1_action!(
    ChangeTransactionTariff,
    ChangeTransactionTariffRequest,
    ChangeTransactionTariffResponse,
    "ChangeTransactionTariff",
    send_change_transaction_tariff,
    on_change_transaction_tariff,
    wait_for_change_transaction_tariff
);
ocpp_2_1_action!(
    ClearCache,
    ClearCacheRequest,
    ClearCacheResponse,
    "ClearCache",
    send_clear_cache,
    on_clear_cache,
    wait_for_clear_cache
);
ocpp_2_1_action!(
    ClearChargingProfile,
    ClearChargingProfileRequest,
    ClearChargingProfileResponse,
    "ClearChargingProfile",
    send_clear_charging_profile,
    on_clear_charging_profile,
    wait_for_clear_charging_profile
);
ocpp_2_1_action!(
    ClearDERControl,
    ClearDERControlRequest,
    ClearDERControlResponse,
    "ClearDERControl",
    send_clear_der_control,
    on_clear_der_control,
    wait_for_clear_der_control
);
ocpp_2_1_action!(
    ClearDisplayMessage,
    ClearDisplayMessageRequest,
    ClearDisplayMessageResponse,
    "ClearDisplayMessage",
    send_clear_display_message,
    on_clear_display_message,
    wait_for_clear_display_message
);
ocpp_2_1_action!(
    ClearTariffs,
    ClearTariffsRequest,
    ClearTariffsResponse,
    "ClearTariffs",
    send_clear_tariffs,
    on_clear_tariffs,
    wait_for_clear_tariffs
);
ocpp_2_1_action!(
    ClearVariableMonitoring,
    ClearVariableMonitoringRequest,
    ClearVariableMonitoringResponse,
    "ClearVariableMonitoring",
    send_clear_variable_monitoring,
    on_clear_variable_monitoring,
    wait_for_clear_variable_monitoring
);
ocpp_2_1_action!(
    ClearedChargingLimit,
    ClearedChargingLimitRequest,
    ClearedChargingLimitResponse,
    "ClearedChargingLimit",
    send_cleared_charging_limit,
    on_cleared_charging_limit,
    wait_for_cleared_charging_limit
);
ocpp_2_1_action!(
    ClosePeriodicEventStream,
    ClosePeriodicEventStreamRequest,
    ClosePeriodicEventStreamResponse,
    "ClosePeriodicEventStream",
    send_close_periodic_event_stream,
    on_close_periodic_event_stream,
    wait_for_close_periodic_event_stream
);
ocpp_2_1_action!(
    CostUpdated,
    CostUpdatedRequest,
    CostUpdatedResponse,
    "CostUpdated",
    send_cost_updated,
    on_cost_updated,
    wait_for_cost_updated
);
ocpp_2_1_action!(
    CustomerInformation,
    CustomerInformationRequest,
    CustomerInformationResponse,
    "CustomerInformation",
    send_customer_information,
    on_customer_information,
    wait_for_customer_information
);
ocpp_2_1_action!(
    DataTransfer,
    DataTransferRequest,
    DataTransferResponse,
    "DataTransfer",
    send_data_transfer,
    on_data_transfer,
    wait_for_data_transfer
);
ocpp_2_1_action!(
    DeleteCertificate,
    DeleteCertificateRequest,
    DeleteCertificateResponse,
    "DeleteCertificate",
    send_delete_certificate,
    on_delete_certificate,
    wait_for_delete_certificate
);
ocpp_2_1_action!(
    FirmwareStatusNotification,
    FirmwareStatusNotificationRequest,
    FirmwareStatusNotificationResponse,
    "FirmwareStatusNotification",
    send_firmware_status_notification,
    on_firmware_status_notification,
    wait_for_firmware_status_notification
);
ocpp_2_1_action!(
    Get15118EVCertificate,
    Get15118EVCertificateRequest,
    Get15118EVCertificateResponse,
    "Get15118EVCertificate",
    send_get_15118_ev_certificate,
    on_get_15118_ev_certificate,
    wait_for_get_15118_ev_certificate
);
ocpp_2_1_action!(
    GetBaseReport,
    GetBaseReportRequest,
    GetBaseReportResponse,
    "GetBaseReport",
    send_get_base_report,
    on_get_base_report,
    wait_for_get_base_report
);
ocpp_2_1_action!(
    GetCertificateChainStatus,
    GetCertificateChainStatusRequest,
    GetCertificateChainStatusResponse,
    "GetCertificateChainStatus",
    send_get_certificate_chain_status,
    on_get_certificate_chain_status,
    wait_for_get_certificate_chain_status
);
ocpp_2_1_action!(
    GetCertificateStatus,
    GetCertificateStatusRequest,
    GetCertificateStatusResponse,
    "GetCertificateStatus",
    send_get_certificate_status,
    on_get_certificate_status,
    wait_for_get_certificate_status
);
ocpp_2_1_action!(
    GetChargingProfiles,
    GetChargingProfilesRequest,
    GetChargingProfilesResponse,
    "GetChargingProfiles",
    send_get_charging_profiles,
    on_get_charging_profiles,
    wait_for_get_charging_profiles
);
ocpp_2_1_action!(
    GetCompositeSchedule,
    GetCompositeScheduleRequest,
    GetCompositeScheduleResponse,
    "GetCompositeSchedule",
    send_get_composite_schedule,
    on_get_composite_schedule,
    wait_for_get_composite_schedule
);
ocpp_2_1_action!(
    GetDisplayMessages,
    GetDisplayMessagesRequest,
    GetDisplayMessagesResponse,
    "GetDisplayMessages",
    send_get_display_messages,
    on_get_display_messages,
    wait_for_get_display_messages
);
ocpp_2_1_action!(
    GetInstalledCertificateIds,
    GetInstalledCertificateIdsRequest,
    GetInstalledCertificateIdsResponse,
    "GetInstalledCertificateIds",
    send_get_installed_certificate_ids,
    on_get_installed_certificate_ids,
    wait_for_get_installed_certificate_ids
);
ocpp_2_1_action!(
    GetLocalListVersion,
    GetLocalListVersionRequest,
    GetLocalListVersionResponse,
    "GetLocalListVersion",
    send_get_local_list_version,
    on_get_local_list_version,
    wait_for_get_local_list_version
);
ocpp_2_1_action!(
    GetLog,
    GetLogRequest,
    GetLogResponse,
    "GetLog",
    send_get_log,
    on_get_log,
    wait_for_get_log
);
ocpp_2_1_action!(
    GetMonitoringReport,
    GetMonitoringReportRequest,
    GetMonitoringReportResponse,
    "GetMonitoringReport",
    send_get_monitoring_report,
    on_get_monitoring_report,
    wait_for_get_monitoring_report
);
ocpp_2_1_action!(
    GetPeriodicEventStream,
    GetPeriodicEventStreamRequest,
    GetPeriodicEventStreamResponse,
    "GetPeriodicEventStream",
    send_get_periodic_event_stream,
    on_get_periodic_event_stream,
    wait_for_get_periodic_event_stream
);
ocpp_2_1_action!(
    GetReport,
    GetReportRequest,
    GetReportResponse,
    "GetReport",
    send_get_report,
    on_get_report,
    wait_for_get_report
);
ocpp_2_1_action!(
    GetTariffs,
    GetTariffsRequest,
    GetTariffsResponse,
    "GetTariffs",
    send_get_tariffs,
    on_get_tariffs,
    wait_for_get_tariffs
);
ocpp_2_1_action!(
    GetTransactionStatus,
    GetTransactionStatusRequest,
    GetTransactionStatusResponse,
    "GetTransactionStatus",
    send_get_transaction_status,
    on_get_transaction_status,
    wait_for_get_transaction_status
);
ocpp_2_1_action!(
    GetVariables,
    GetVariablesRequest,
    GetVariablesResponse,
    "GetVariables",
    send_get_variables,
    on_get_variables,
    wait_for_get_variables
);
ocpp_2_1_action!(
    Heartbeat,
    HeartbeatRequest,
    HeartbeatResponse,
    "Heartbeat",
    send_heartbeat,
    on_heartbeat,
    wait_for_heartbeat
);
ocpp_2_1_action!(
    InstallCertificate,
    InstallCertificateRequest,
    InstallCertificateResponse,
    "InstallCertificate",
    send_install_certificate,
    on_install_certificate,
    wait_for_install_certificate
);
ocpp_2_1_action!(
    LogStatusNotification,
    LogStatusNotificationRequest,
    LogStatusNotificationResponse,
    "LogStatusNotification",
    send_log_status_notification,
    on_log_status_notification,
    wait_for_log_status_notification
);
ocpp_2_1_action!(
    MeterValues,
    MeterValuesRequest,
    MeterValuesResponse,
    "MeterValues",
    send_meter_values,
    on_meter_values,
    wait_for_meter_values
);
ocpp_2_1_action!(
    NotifyAllowedEnergyTransfer,
    NotifyAllowedEnergyTransferRequest,
    NotifyAllowedEnergyTransferResponse,
    "NotifyAllowedEnergyTransfer",
    send_notify_allowed_energy_transfer,
    on_notify_allowed_energy_transfer,
    wait_for_notify_allowed_energy_transfer
);
ocpp_2_1_action!(
    NotifyChargingLimit,
    NotifyChargingLimitRequest,
    NotifyChargingLimitResponse,
    "NotifyChargingLimit",
    send_notify_charging_limit,
    on_notify_charging_limit,
    wait_for_notify_charging_limit
);
ocpp_2_1_action!(
    NotifyCustomerInformation,
    NotifyCustomerInformationRequest,
    NotifyCustomerInformationResponse,
    "NotifyCustomerInformation",
    send_notify_customer_information,
    on_notify_customer_information,
    wait_for_notify_customer_information
);
ocpp_2_1_action!(
    NotifyDERAlarm,
    NotifyDERAlarmRequest,
    NotifyDERAlarmResponse,
    "NotifyDERAlarm",
    send_notify_der_alarm,
    on_notify_der_alarm,
    wait_for_notify_der_alarm
);
ocpp_2_1_action!(
    NotifyDERStartStop,
    NotifyDERStartStopRequest,
    NotifyDERStartStopResponse,
    "NotifyDERStartStop",
    send_notify_der_start_stop,
    on_notify_der_start_stop,
    wait_for_notify_der_start_stop
);
ocpp_2_1_action!(
    NotifyDisplayMessages,
    NotifyDisplayMessagesRequest,
    NotifyDisplayMessagesResponse,
    "NotifyDisplayMessages",
    send_notify_display_messages,
    on_notify_display_messages,
    wait_for_notify_display_messages
);
ocpp_2_1_action!(
    NotifyEVChargingNeeds,
    NotifyEVChargingNeedsRequest,
    NotifyEVChargingNeedsResponse,
    "NotifyEVChargingNeeds",
    send_notify_ev_charging_needs,
    on_notify_ev_charging_needs,
    wait_for_notify_ev_charging_needs
);
ocpp_2_1_action!(
    NotifyEVChargingSchedule,
    NotifyEVChargingScheduleRequest,
    NotifyEVChargingScheduleResponse,
    "NotifyEVChargingSchedule",
    send_notify_ev_charging_schedule,
    on_notify_ev_charging_schedule,
    wait_for_notify_ev_charging_schedule
);
ocpp_2_1_action!(
    NotifyEvent,
    NotifyEventRequest,
    NotifyEventResponse,
    "NotifyEvent",
    send_notify_event,
    on_notify_event,
    wait_for_notify_event
);
ocpp_2_1_action!(
    NotifyMonitoringReport,
    NotifyMonitoringReportRequest,
    NotifyMonitoringReportResponse,
    "NotifyMonitoringReport",
    send_notify_monitoring_report,
    on_notify_monitoring_report,
    wait_for_notify_monitoring_report
);
ocpp_2_1_action!(
    NotifyReport,
    NotifyReportRequest,
    NotifyReportResponse,
    "NotifyReport",
    send_notify_report,
    on_notify_report,
    wait_for_notify_report
);
// `NotifyPeriodicEventStream` is genuinely SEND-only in the OCPP-J 2.1 spec (no CALLRESULT) -
// `ocpp_types` models it as one struct, not a Request/Response pair, unlike every other action
// here. Wired up as a real `SEND` (message type 6) via `ocpp_2_1_send_action!`/
// `Client::send_notification`/`Client::on_notification` - see `src/client.rs`. The marker type
// is suffixed `Action` to avoid colliding with the imported `NotifyPeriodicEventStream` message
// type.
ocpp_2_1_send_action!(
    NotifyPeriodicEventStreamAction,
    NotifyPeriodicEventStream,
    "NotifyPeriodicEventStream",
    send_notify_periodic_event_stream,
    on_notify_periodic_event_stream
);
ocpp_2_1_action!(
    NotifyPriorityCharging,
    NotifyPriorityChargingRequest,
    NotifyPriorityChargingResponse,
    "NotifyPriorityCharging",
    send_notify_priority_charging,
    on_notify_priority_charging,
    wait_for_notify_priority_charging
);
ocpp_2_1_action!(
    NotifySettlement,
    NotifySettlementRequest,
    NotifySettlementResponse,
    "NotifySettlement",
    send_notify_settlement,
    on_notify_settlement,
    wait_for_notify_settlement
);
ocpp_2_1_action!(
    NotifyWebPaymentStarted,
    NotifyWebPaymentStartedRequest,
    NotifyWebPaymentStartedResponse,
    "NotifyWebPaymentStarted",
    send_notify_web_payment_started,
    on_notify_web_payment_started,
    wait_for_notify_web_payment_started
);
ocpp_2_1_action!(
    OpenPeriodicEventStream,
    OpenPeriodicEventStreamRequest,
    OpenPeriodicEventStreamResponse,
    "OpenPeriodicEventStream",
    send_open_periodic_event_stream,
    on_open_periodic_event_stream,
    wait_for_open_periodic_event_stream
);
ocpp_2_1_action!(
    PublishFirmware,
    PublishFirmwareRequest,
    PublishFirmwareResponse,
    "PublishFirmware",
    send_publish_firmware,
    on_publish_firmware,
    wait_for_publish_firmware
);
ocpp_2_1_action!(
    PublishFirmwareStatusNotification,
    PublishFirmwareStatusNotificationRequest,
    PublishFirmwareStatusNotificationResponse,
    "PublishFirmwareStatusNotification",
    send_publish_firmware_status_notification,
    on_publish_firmware_status_notification,
    wait_for_publish_firmware_status_notification
);
ocpp_2_1_action!(
    PullDynamicScheduleUpdate,
    PullDynamicScheduleUpdateRequest,
    PullDynamicScheduleUpdateResponse,
    "PullDynamicScheduleUpdate",
    send_pull_dynamic_schedule_update,
    on_pull_dynamic_schedule_update,
    wait_for_pull_dynamic_schedule_update
);
ocpp_2_1_action!(
    ReportChargingProfiles,
    ReportChargingProfilesRequest,
    ReportChargingProfilesResponse,
    "ReportChargingProfiles",
    send_report_charging_profiles,
    on_report_charging_profiles,
    wait_for_report_charging_profiles
);
ocpp_2_1_action!(
    ReportDERControl,
    ReportDERControlRequest,
    ReportDERControlResponse,
    "ReportDERControl",
    send_report_der_control,
    on_report_der_control,
    wait_for_report_der_control
);
ocpp_2_1_action!(
    RequestBatterySwap,
    RequestBatterySwapRequest,
    RequestBatterySwapResponse,
    "RequestBatterySwap",
    send_request_battery_swap,
    on_request_battery_swap,
    wait_for_request_battery_swap
);
ocpp_2_1_action!(
    RequestStartTransaction,
    RequestStartTransactionRequest,
    RequestStartTransactionResponse,
    "RequestStartTransaction",
    send_request_start_transaction,
    on_request_start_transaction,
    wait_for_request_start_transaction
);
ocpp_2_1_action!(
    RequestStopTransaction,
    RequestStopTransactionRequest,
    RequestStopTransactionResponse,
    "RequestStopTransaction",
    send_request_stop_transaction,
    on_request_stop_transaction,
    wait_for_request_stop_transaction
);
ocpp_2_1_action!(
    ReservationStatusUpdate,
    ReservationStatusUpdateRequest,
    ReservationStatusUpdateResponse,
    "ReservationStatusUpdate",
    send_reservation_status_update,
    on_reservation_status_update,
    wait_for_reservation_status_update
);
ocpp_2_1_action!(
    ReserveNow,
    ReserveNowRequest,
    ReserveNowResponse,
    "ReserveNow",
    send_reserve_now,
    on_reserve_now,
    wait_for_reserve_now
);
ocpp_2_1_action!(
    Reset,
    ResetRequest,
    ResetResponse,
    "Reset",
    send_reset,
    on_reset,
    wait_for_reset
);
ocpp_2_1_action!(
    SecurityEventNotification,
    SecurityEventNotificationRequest,
    SecurityEventNotificationResponse,
    "SecurityEventNotification",
    send_security_event_notification,
    on_security_event_notification,
    wait_for_security_event_notification
);
ocpp_2_1_action!(
    SendLocalList,
    SendLocalListRequest,
    SendLocalListResponse,
    "SendLocalList",
    send_send_local_list,
    on_send_local_list,
    wait_for_send_local_list
);
ocpp_2_1_action!(
    SetChargingProfile,
    SetChargingProfileRequest,
    SetChargingProfileResponse,
    "SetChargingProfile",
    send_set_charging_profile,
    on_set_charging_profile,
    wait_for_set_charging_profile
);
ocpp_2_1_action!(
    SetDefaultTariff,
    SetDefaultTariffRequest,
    SetDefaultTariffResponse,
    "SetDefaultTariff",
    send_set_default_tariff,
    on_set_default_tariff,
    wait_for_set_default_tariff
);
ocpp_2_1_action!(
    SetMonitoringBase,
    SetMonitoringBaseRequest,
    SetMonitoringBaseResponse,
    "SetMonitoringBase",
    send_set_monitoring_base,
    on_set_monitoring_base,
    wait_for_set_monitoring_base
);
ocpp_2_1_action!(
    SetMonitoringLevel,
    SetMonitoringLevelRequest,
    SetMonitoringLevelResponse,
    "SetMonitoringLevel",
    send_set_monitoring_level,
    on_set_monitoring_level,
    wait_for_set_monitoring_level
);
ocpp_2_1_action!(
    SetNetworkProfile,
    SetNetworkProfileRequest,
    SetNetworkProfileResponse,
    "SetNetworkProfile",
    send_set_network_profile,
    on_set_network_profile,
    wait_for_set_network_profile
);
ocpp_2_1_action!(
    SetVariableMonitoring,
    SetVariableMonitoringRequest,
    SetVariableMonitoringResponse,
    "SetVariableMonitoring",
    send_set_variable_monitoring,
    on_set_variable_monitoring,
    wait_for_set_variable_monitoring
);
ocpp_2_1_action!(
    SetVariables,
    SetVariablesRequest,
    SetVariablesResponse,
    "SetVariables",
    send_set_variables,
    on_set_variables,
    wait_for_set_variables
);
ocpp_2_1_action!(
    SignCertificate,
    SignCertificateRequest,
    SignCertificateResponse,
    "SignCertificate",
    send_sign_certificate,
    on_sign_certificate,
    wait_for_sign_certificate
);
ocpp_2_1_action!(
    StatusNotification,
    StatusNotificationRequest,
    StatusNotificationResponse,
    "StatusNotification",
    send_status_notification,
    on_status_notification,
    wait_for_status_notification
);
ocpp_2_1_action!(
    TransactionEvent,
    TransactionEventRequest,
    TransactionEventResponse,
    "TransactionEvent",
    send_transaction_event,
    on_transaction_event,
    wait_for_transaction_event
);
ocpp_2_1_action!(
    UnlockConnector,
    UnlockConnectorRequest,
    UnlockConnectorResponse,
    "UnlockConnector",
    send_unlock_connector,
    on_unlock_connector,
    wait_for_unlock_connector
);
ocpp_2_1_action!(
    UnpublishFirmware,
    UnpublishFirmwareRequest,
    UnpublishFirmwareResponse,
    "UnpublishFirmware",
    send_unpublish_firmware,
    on_unpublish_firmware,
    wait_for_unpublish_firmware
);
ocpp_2_1_action!(
    UpdateFirmware,
    UpdateFirmwareRequest,
    UpdateFirmwareResponse,
    "UpdateFirmware",
    send_update_firmware,
    on_update_firmware,
    wait_for_update_firmware
);
ocpp_2_1_action!(
    UsePriorityCharging,
    UsePriorityChargingRequest,
    UsePriorityChargingResponse,
    "UsePriorityCharging",
    send_use_priority_charging,
    on_use_priority_charging,
    wait_for_use_priority_charging
);
ocpp_2_1_action!(
    VatNumberValidation,
    VatNumberValidationRequest,
    VatNumberValidationResponse,
    "VatNumberValidation",
    send_vat_number_validation,
    on_vat_number_validation,
    wait_for_vat_number_validation
);
