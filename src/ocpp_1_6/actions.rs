use crate::action::Action;
use crate::error::ClientError;
use crate::ocpp_1_6::OCPP1_6Client;
use crate::ocpp_1_6::error::OCPP1_6Error;
use rust_ocpp::v1_6::messages::authorize::{AuthorizeRequest, AuthorizeResponse};
use rust_ocpp::v1_6::messages::boot_notification::{
    BootNotificationRequest, BootNotificationResponse,
};
use rust_ocpp::v1_6::messages::cancel_reservation::{
    CancelReservationRequest, CancelReservationResponse,
};
use rust_ocpp::v1_6::messages::change_availability::{
    ChangeAvailabilityRequest, ChangeAvailabilityResponse,
};
use rust_ocpp::v1_6::messages::change_configuration::{
    ChangeConfigurationRequest, ChangeConfigurationResponse,
};
use rust_ocpp::v1_6::messages::clear_cache::{ClearCacheRequest, ClearCacheResponse};
use rust_ocpp::v1_6::messages::clear_charging_profile::{
    ClearChargingProfileRequest, ClearChargingProfileResponse,
};
use rust_ocpp::v1_6::messages::data_transfer::{DataTransferRequest, DataTransferResponse};
use rust_ocpp::v1_6::messages::diagnostics_status_notification::{
    DiagnosticsStatusNotificationRequest, DiagnosticsStatusNotificationResponse,
};
use rust_ocpp::v1_6::messages::firmware_status_notification::{
    FirmwareStatusNotificationRequest, FirmwareStatusNotificationResponse,
};
use rust_ocpp::v1_6::messages::get_composite_schedule::{
    GetCompositeScheduleRequest, GetCompositeScheduleResponse,
};
use rust_ocpp::v1_6::messages::get_configuration::{
    GetConfigurationRequest, GetConfigurationResponse,
};
use rust_ocpp::v1_6::messages::get_diagnostics::{GetDiagnosticsRequest, GetDiagnosticsResponse};
use rust_ocpp::v1_6::messages::get_local_list_version::{
    GetLocalListVersionRequest, GetLocalListVersionResponse,
};
use rust_ocpp::v1_6::messages::heart_beat::{HeartbeatRequest, HeartbeatResponse};
use rust_ocpp::v1_6::messages::meter_values::{MeterValuesRequest, MeterValuesResponse};
use rust_ocpp::v1_6::messages::remote_start_transaction::{
    RemoteStartTransactionRequest, RemoteStartTransactionResponse,
};
use rust_ocpp::v1_6::messages::remote_stop_transaction::{
    RemoteStopTransactionRequest, RemoteStopTransactionResponse,
};
use rust_ocpp::v1_6::messages::reserve_now::{ReserveNowRequest, ReserveNowResponse};
use rust_ocpp::v1_6::messages::reset::{ResetRequest, ResetResponse};
use rust_ocpp::v1_6::messages::send_local_list::{SendLocalListRequest, SendLocalListResponse};
use rust_ocpp::v1_6::messages::set_charging_profile::{
    SetChargingProfileRequest, SetChargingProfileResponse,
};
use rust_ocpp::v1_6::messages::start_transaction::{
    StartTransactionRequest, StartTransactionResponse,
};
use rust_ocpp::v1_6::messages::status_notification::{
    StatusNotificationRequest, StatusNotificationResponse,
};
use rust_ocpp::v1_6::messages::stop_transaction::{
    StopTransactionRequest, StopTransactionResponse,
};
use rust_ocpp::v1_6::messages::trigger_message::{TriggerMessageRequest, TriggerMessageResponse};
use rust_ocpp::v1_6::messages::unlock_connector::{
    UnlockConnectorRequest, UnlockConnectorResponse,
};
use rust_ocpp::v1_6::messages::update_firmware::{UpdateFirmwareRequest, UpdateFirmwareResponse};
use std::future::Future;

/// Declares one OCPP 1.6 action: a zero-sized marker type implementing [`Action`], plus
/// `send_x`/`on_x`/`wait_for_x` convenience methods on [`OCPP1_6Client`] that just call the
/// generic `Client::call`/`Client::on`/`Client::wait_for` for it. Adding a new action is one
/// macro invocation instead of three hand-written method bodies.
macro_rules! ocpp_1_6_action {
    ($name:ident, $req:ty, $res:ty, $action:literal, $send:ident, $on:ident, $wait_for:ident) => {
        #[doc = concat!("Marker type for the `", $action, "` action.")]
        pub struct $name;

        impl Action for $name {
            const NAME: &'static str = $action;
            type Request = $req;
            type Response = $res;
        }

        impl OCPP1_6Client {
            pub async fn $send(&self, request: $req) -> Result<$res, ClientError<OCPP1_6Error>> {
                self.call::<$name>(request).await
            }

            pub async fn $on<F, FF>(&self, callback: F)
            where
                F: FnMut($req, Self) -> FF + Send + Sync + 'static,
                FF: Future<Output = Result<$res, OCPP1_6Error>> + Send,
            {
                self.on::<$name, F, FF>(callback).await
            }

            #[cfg(feature = "test")]
            pub async fn $wait_for<F, FF>(
                &self,
                callback: F,
            ) -> Result<$req, ClientError<OCPP1_6Error>>
            where
                F: FnMut($req, Self) -> FF + Send + Sync + 'static,
                FF: Future<Output = Result<$res, OCPP1_6Error>> + Send,
            {
                self.wait_for::<$name, F, FF>(callback).await
            }
        }
    };
}

ocpp_1_6_action!(
    Authorize,
    AuthorizeRequest,
    AuthorizeResponse,
    "Authorize",
    send_authorize,
    on_authorize,
    wait_for_authorize
);
ocpp_1_6_action!(
    BootNotification,
    BootNotificationRequest,
    BootNotificationResponse,
    "BootNotification",
    send_boot_notification,
    on_boot_notification,
    wait_for_boot_notification
);
ocpp_1_6_action!(
    CancelReservation,
    CancelReservationRequest,
    CancelReservationResponse,
    "CancelReservation",
    send_cancel_reservation,
    on_cancel_reservation,
    wait_for_cancel_reservation
);
ocpp_1_6_action!(
    ChangeAvailability,
    ChangeAvailabilityRequest,
    ChangeAvailabilityResponse,
    "ChangeAvailability",
    send_change_availability,
    on_change_availability,
    wait_for_change_availability
);
ocpp_1_6_action!(
    ChangeConfiguration,
    ChangeConfigurationRequest,
    ChangeConfigurationResponse,
    "ChangeConfiguration",
    send_change_configuration,
    on_change_configuration,
    wait_for_change_configuration
);
ocpp_1_6_action!(
    ClearCache,
    ClearCacheRequest,
    ClearCacheResponse,
    "ClearCache",
    send_clear_cache,
    on_clear_cache,
    wait_for_clear_cache
);
ocpp_1_6_action!(
    ClearChargingProfile,
    ClearChargingProfileRequest,
    ClearChargingProfileResponse,
    "ClearChargingProfile",
    send_clear_charging_profile,
    on_clear_charging_profile,
    wait_for_clear_charging_profile
);
ocpp_1_6_action!(
    DataTransfer,
    DataTransferRequest,
    DataTransferResponse,
    "DataTransfer",
    send_data_transfer,
    on_data_transfer,
    wait_for_data_transfer
);
ocpp_1_6_action!(
    DiagnosticsStatusNotification,
    DiagnosticsStatusNotificationRequest,
    DiagnosticsStatusNotificationResponse,
    "DiagnosticsStatusNotification",
    send_diagnostics_status_notification,
    on_diagnostics_status_notification,
    wait_for_diagnostics_status_notification
);
ocpp_1_6_action!(
    FirmwareStatusNotification,
    FirmwareStatusNotificationRequest,
    FirmwareStatusNotificationResponse,
    "FirmwareStatusNotification",
    send_firmware_status_notification,
    on_firmware_status_notification,
    wait_for_firmware_status_notification
);
ocpp_1_6_action!(
    GetCompositeSchedule,
    GetCompositeScheduleRequest,
    GetCompositeScheduleResponse,
    "GetCompositeSchedule",
    send_get_composite_schedule,
    on_get_composite_schedule,
    wait_for_get_composite_schedule
);
ocpp_1_6_action!(
    GetConfiguration,
    GetConfigurationRequest,
    GetConfigurationResponse,
    "GetConfiguration",
    send_get_configuration,
    on_get_configuration,
    wait_for_get_configuration
);
ocpp_1_6_action!(
    GetDiagnostics,
    GetDiagnosticsRequest,
    GetDiagnosticsResponse,
    "GetDiagnostics",
    send_get_diagnostics,
    on_get_diagnostics,
    wait_for_get_diagnostics
);
ocpp_1_6_action!(
    GetLocalListVersion,
    GetLocalListVersionRequest,
    GetLocalListVersionResponse,
    "GetLocalListVersion",
    send_get_local_list_version,
    on_get_local_list_version,
    wait_for_get_local_list_version
);
ocpp_1_6_action!(
    Heartbeat,
    HeartbeatRequest,
    HeartbeatResponse,
    "Heartbeat",
    send_heartbeat,
    on_heartbeat,
    wait_for_heartbeat
);
ocpp_1_6_action!(
    MeterValues,
    MeterValuesRequest,
    MeterValuesResponse,
    "MeterValues",
    send_meter_values,
    on_meter_values,
    wait_for_meter_values
);
ocpp_1_6_action!(
    RemoteStartTransaction,
    RemoteStartTransactionRequest,
    RemoteStartTransactionResponse,
    "RemoteStartTransaction",
    send_remote_start_transaction,
    on_remote_start_transaction,
    wait_for_remote_start_transaction
);
ocpp_1_6_action!(
    RemoteStopTransaction,
    RemoteStopTransactionRequest,
    RemoteStopTransactionResponse,
    "RemoteStopTransaction",
    send_remote_stop_transaction,
    on_remote_stop_transaction,
    wait_for_remote_stop_transaction
);
ocpp_1_6_action!(
    ReserveNow,
    ReserveNowRequest,
    ReserveNowResponse,
    "ReserveNow",
    send_reserve_now,
    on_reserve_now,
    wait_for_reserve_now
);
ocpp_1_6_action!(
    Reset,
    ResetRequest,
    ResetResponse,
    "Reset",
    send_reset,
    on_reset,
    wait_for_reset
);
ocpp_1_6_action!(
    SendLocalList,
    SendLocalListRequest,
    SendLocalListResponse,
    "SendLocalList",
    send_send_local_list,
    on_send_local_list,
    wait_for_send_local_list
);
ocpp_1_6_action!(
    SetChargingProfile,
    SetChargingProfileRequest,
    SetChargingProfileResponse,
    "SetChargingProfile",
    send_set_charging_profile,
    on_set_charging_profile,
    wait_for_set_charging_profile
);
ocpp_1_6_action!(
    StartTransaction,
    StartTransactionRequest,
    StartTransactionResponse,
    "StartTransaction",
    send_start_transaction,
    on_start_transaction,
    wait_for_start_transaction
);
ocpp_1_6_action!(
    StatusNotification,
    StatusNotificationRequest,
    StatusNotificationResponse,
    "StatusNotification",
    send_status_notification,
    on_status_notification,
    wait_for_status_notification
);
ocpp_1_6_action!(
    StopTransaction,
    StopTransactionRequest,
    StopTransactionResponse,
    "StopTransaction",
    send_stop_transaction,
    on_stop_transaction,
    wait_for_stop_transaction
);
ocpp_1_6_action!(
    TriggerMessage,
    TriggerMessageRequest,
    TriggerMessageResponse,
    "TriggerMessage",
    send_trigger_message,
    on_trigger_message,
    wait_for_trigger_message
);
ocpp_1_6_action!(
    UnlockConnector,
    UnlockConnectorRequest,
    UnlockConnectorResponse,
    "UnlockConnector",
    send_unlock_connector,
    on_unlock_connector,
    wait_for_unlock_connector
);
ocpp_1_6_action!(
    UpdateFirmware,
    UpdateFirmwareRequest,
    UpdateFirmwareResponse,
    "UpdateFirmware",
    send_update_firmware,
    on_update_firmware,
    wait_for_update_firmware
);
