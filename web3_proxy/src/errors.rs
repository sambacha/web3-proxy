//! Utlities for logging errors for admins and displaying errors to users.

use crate::frontend::authorization::Authorization;
use crate::jsonrpc::{JsonRpcErrorData, JsonRpcForwardedResponse};
use crate::response_cache::JsonRpcResponseEnum;
use crate::rpcs::provider::EthersHttpProvider;
use axum::extract::ws::Message;
use axum::{
    headers,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use derive_more::{Display, Error, From};
use ethers::prelude::ContractError;
use ethers::types::{H256, U64};
use http::header::InvalidHeaderValue;
use http::uri::InvalidUri;
use ipnet::AddrParseError;
use migration::sea_orm::DbErr;
use redis_rate_limiter::redis::RedisError;
use redis_rate_limiter::RedisPoolError;
use reqwest::header::ToStrError;
use rust_decimal::Error as DecimalError;
use serde::Serialize;
use serde_json::value::RawValue;
use siwe::VerificationError;
use std::sync::Arc;
use std::{borrow::Cow, net::IpAddr};
use tokio::{sync::AcquireError, task::JoinError, time::Instant};
use tracing::{debug, error, trace, warn};

pub type Web3ProxyResult<T> = Result<T, Web3ProxyError>;
// TODO: take "IntoResponse" instead of Response?
pub type Web3ProxyResponse = Web3ProxyResult<Response>;

impl From<Web3ProxyError> for Web3ProxyResult<()> {
    fn from(value: Web3ProxyError) -> Self {
        Err(value)
    }
}

// TODO: replace all String with `Cow<'static, str>`
#[derive(Debug, Display, Error, From)]
pub enum Web3ProxyError {
    Abi(ethers::abi::Error),
    #[error(ignore)]
    #[from(ignore)]
    AccessDenied(Cow<'static, str>),
    #[error(ignore)]
    Anyhow(anyhow::Error),
    Arc(Arc<Self>),
    #[error(ignore)]
    #[from(ignore)]
    BadRequest(Cow<'static, str>),
    #[error(ignore)]
    #[from(ignore)]
    BadResponse(Cow<'static, str>),
    BadRouting,
    Contract(ContractError<EthersHttpProvider>),
    Database(DbErr),
    Decimal(DecimalError),
    EthersHttpClient(ethers::prelude::HttpClientError),
    EthersProvider(ethers::prelude::ProviderError),
    EthersWsClient(ethers::prelude::WsClientError),
    FlumeRecv(flume::RecvError),
    GasEstimateNotU256,
    HdrRecord(hdrhistogram::errors::RecordError),
    Headers(headers::Error),
    HeaderToString(ToStrError),
    HttpUri(InvalidUri),
    Hyper(hyper::Error),
    InfluxDb2Request(influxdb2::RequestError),
    #[display(fmt = "{} > {}", min, max)]
    #[from(ignore)]
    InvalidBlockBounds {
        min: u64,
        max: u64,
    },
    InvalidHeaderValue(InvalidHeaderValue),
    InvalidEip,
    InvalidInviteCode,
    Io(std::io::Error),
    UnknownReferralCode,
    InvalidReferer,
    InvalidSignatureLength,
    InvalidUserTier,
    InvalidUserAgent,
    InvalidUserKey,
    IpAddrParse(AddrParseError),
    #[error(ignore)]
    #[from(ignore)]
    IpNotAllowed(IpAddr),
    JoinError(JoinError),
    #[display(fmt = "{:?}", _0)]
    #[error(ignore)]
    JsonRpcErrorData(JsonRpcErrorData),
    #[display(fmt = "{:?}", _0)]
    #[error(ignore)]
    MsgPackEncode(rmp_serde::encode::Error),
    NoBlockNumberOrHash,
    NoBlocksKnown,
    NoConsensusHeadBlock,
    NoDatabase,
    NoHandleReady,
    NoServersSynced,
    #[display(fmt = "{}/{}", num_known, min_head_rpcs)]
    #[from(ignore)]
    NotEnoughRpcs {
        num_known: usize,
        min_head_rpcs: usize,
    },
    #[display(fmt = "{}/{}", available, needed)]
    #[from(ignore)]
    NotEnoughSoftLimit {
        available: u32,
        needed: u32,
    },
    NotFound,
    #[error(ignore)]
    #[from(ignore)]
    NotImplemented(Cow<'static, str>),
    NoVolatileRedisDatabase,
    OriginRequired,
    #[error(ignore)]
    #[from(ignore)]
    OriginNotAllowed(headers::Origin),
    #[display(fmt = "{:?}", _0)]
    #[error(ignore)]
    ParseBytesError(Option<ethers::types::ParseBytesError>),
    ParseMsgError(siwe::ParseError),
    ParseAddressError,
    #[display(fmt = "{:?}, {:?}", _0, _1)]
    RateLimited(Authorization, Option<Instant>),
    Redis(RedisError),
    RedisDeadpool(RedisPoolError),
    RefererRequired,
    #[display(fmt = "{:?}", _0)]
    #[error(ignore)]
    #[from(ignore)]
    RefererNotAllowed(headers::Referer),
    SemaphoreAcquireError(AcquireError),
    SendAppStatError(flume::SendError<crate::stats::AppStat>),
    SerdeJson(serde_json::Error),
    SiweVerification(VerificationError),
    /// simple way to return an error message to the user and an anyhow to our logs
    #[display(fmt = "{}, {}, {:?}", _0, _1, _2)]
    StatusCode(StatusCode, Cow<'static, str>, Option<anyhow::Error>),
    /// TODO: what should be attached to the timout?
    #[display(fmt = "{:?}", _0)]
    #[error(ignore)]
    Timeout(Option<tokio::time::error::Elapsed>),
    UlidDecode(ulid::DecodeError),
    #[error(ignore)]
    UnknownBlockHash(H256),
    #[display(fmt = "known: {known}, unknown: {unknown}")]
    #[error(ignore)]
    UnknownBlockNumber {
        known: U64,
        unknown: U64,
    },
    UnknownKey,
    UserAgentRequired,
    #[error(ignore)]
    UserAgentNotAllowed(headers::UserAgent),
    UserIdZero,
    PaymentRequired,
    WatchRecvError(tokio::sync::watch::error::RecvError),
    WatchSendError,
    WebsocketOnly,
    #[display(fmt = "{:?}, {}", _0, _1)]
    #[error(ignore)]
    WithContext(Option<Box<Web3ProxyError>>, Cow<'static, str>),
}

impl Web3ProxyError {
    pub fn as_response_parts<R: Serialize>(&self) -> (StatusCode, JsonRpcResponseEnum<R>) {
        // TODO: include a unique request id in the data
        let (code, err): (StatusCode, JsonRpcErrorData) = match self {
            Self::Abi(err) => {
                warn!(?err, "abi error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    JsonRpcErrorData {
                        message: err.to_string().into(),
                        code: StatusCode::INTERNAL_SERVER_ERROR.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::AccessDenied(msg) => {
                // TODO: attach something to this trace. probably don't include much in the message though. don't want to leak creds by accident
                trace!(%msg, "access denied");
                (
                    StatusCode::FORBIDDEN,
                    JsonRpcErrorData {
                        message: format!("FORBIDDEN: {}", msg).into(),
                        code: StatusCode::FORBIDDEN.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::Anyhow(err) => {
                warn!(?err, "anyhow");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    JsonRpcErrorData {
                        // TODO: is it safe to expose all of our anyhow strings?
                        message: "INTERNAL SERVER ERROR".into(),
                        code: StatusCode::INTERNAL_SERVER_ERROR.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::Arc(err) => {
                // recurse
                return err.as_response_parts();
            }
            Self::BadRequest(err) => {
                trace!(?err, "BAD_REQUEST");
                (
                    StatusCode::BAD_REQUEST,
                    JsonRpcErrorData {
                        message: format!("bad request: {}", err).into(),
                        code: StatusCode::BAD_REQUEST.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::BadResponse(err) => {
                // TODO: think about this one more. ankr gives us this because ethers fails to parse responses without an id
                debug!(?err, "BAD_RESPONSE");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    JsonRpcErrorData {
                        message: format!("bad response: {}", err).into(),
                        code: StatusCode::INTERNAL_SERVER_ERROR.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::BadRouting => {
                error!("BadRouting");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    JsonRpcErrorData {
                        message: "bad routing".into(),
                        code: StatusCode::INTERNAL_SERVER_ERROR.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::Contract(err) => {
                warn!("Contract Error: {:#?}", err);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    JsonRpcErrorData {
                        message: format!("contract error: {}", err).into(),
                        code: StatusCode::INTERNAL_SERVER_ERROR.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::Database(err) => {
                error!("database err={:#?}", err);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    JsonRpcErrorData {
                        message: "database error!".into(),
                        code: StatusCode::INTERNAL_SERVER_ERROR.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::Decimal(err) => {
                debug!("Decimal Error: {:#?}", err);
                (
                    StatusCode::BAD_REQUEST,
                    JsonRpcErrorData {
                        message: format!("decimal error: {}", err).into(),
                        code: StatusCode::BAD_REQUEST.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::EthersHttpClient(err) => {
                if let Ok(err) = JsonRpcErrorData::try_from(err) {
                    trace!(?err, "EthersHttpClient jsonrpc error");
                    (StatusCode::OK, err)
                } else {
                    warn!(?err, "EthersHttpClient");
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        JsonRpcErrorData {
                            message: "ethers http client error".into(),
                            code: StatusCode::INTERNAL_SERVER_ERROR.as_u16().into(),
                            data: None,
                        },
                    )
                }
            }
            Self::EthersProvider(err) => {
                if let Ok(err) = JsonRpcErrorData::try_from(err) {
                    trace!(?err, "EthersProvider jsonrpc error");
                    (StatusCode::OK, err)
                } else {
                    warn!(?err, "EthersProvider");
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        JsonRpcErrorData {
                            message: "ethers provider error".into(),
                            code: StatusCode::INTERNAL_SERVER_ERROR.as_u16().into(),
                            data: None,
                        },
                    )
                }
            }
            Self::EthersWsClient(err) => {
                if let Ok(err) = JsonRpcErrorData::try_from(err) {
                    trace!(?err, "EthersWsClient jsonrpc error");
                    (StatusCode::OK, err)
                } else {
                    warn!(?err, "EthersWsClient");
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        JsonRpcErrorData {
                            message: "ethers ws client error".into(),
                            code: StatusCode::INTERNAL_SERVER_ERROR.as_u16().into(),
                            data: None,
                        },
                    )
                }
            }
            Self::FlumeRecv(err) => {
                warn!(?err, "FlumeRecvError");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    JsonRpcErrorData {
                        message: "flume recv error!".into(),
                        code: StatusCode::INTERNAL_SERVER_ERROR.as_u16().into(),
                        data: None,
                    },
                )
            }
            // Self::JsonRpcForwardedError(x) => (StatusCode::OK, x),
            Self::GasEstimateNotU256 => {
                trace!("GasEstimateNotU256");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    JsonRpcErrorData {
                        message: "gas estimate result is not an U256".into(),
                        code: StatusCode::INTERNAL_SERVER_ERROR.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::HdrRecord(err) => {
                warn!(?err, "HdrRecord");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    JsonRpcErrorData {
                        message: err.to_string().into(),
                        code: StatusCode::INTERNAL_SERVER_ERROR.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::Headers(err) => {
                trace!(?err, "HeadersError");
                (
                    StatusCode::BAD_REQUEST,
                    JsonRpcErrorData {
                        message: err.to_string().into(),
                        code: StatusCode::BAD_REQUEST.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::HeaderToString(err) => {
                trace!(?err, "HeaderToString");
                (
                    StatusCode::BAD_REQUEST,
                    JsonRpcErrorData {
                        message: err.to_string().into(),
                        code: StatusCode::BAD_REQUEST.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::HttpUri(err) => {
                trace!(?err, "HttpUri");
                (
                    StatusCode::BAD_REQUEST,
                    JsonRpcErrorData {
                        message: err.to_string().into(),
                        code: StatusCode::BAD_REQUEST.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::Hyper(err) => {
                warn!(?err, "hyper");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    JsonRpcErrorData {
                        // TODO: is it safe to expose these error strings?
                        message: err.to_string().into(),
                        code: StatusCode::INTERNAL_SERVER_ERROR.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::InfluxDb2Request(err) => {
                // TODO: attach a request id to the message and to this error so that if people report problems, we can dig in sentry to find out more
                error!(?err, "influxdb2");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    JsonRpcErrorData {
                        message: "influxdb2 error!".into(),
                        code: StatusCode::INTERNAL_SERVER_ERROR.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::InvalidBlockBounds { min, max } => {
                trace!(%min, %max, "InvalidBlockBounds");
                (
                    StatusCode::BAD_REQUEST,
                    JsonRpcErrorData {
                        message: format!(
                            "Invalid blocks bounds requested. min ({}) > max ({})",
                            min, max
                        )
                        .into(),
                        code: StatusCode::BAD_REQUEST.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::IpAddrParse(err) => {
                debug!(?err, "IpAddrParse");
                (
                    StatusCode::BAD_REQUEST,
                    JsonRpcErrorData {
                        message: err.to_string().into(),
                        code: StatusCode::BAD_REQUEST.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::IpNotAllowed(ip) => {
                trace!(?ip, "IpNotAllowed");
                (
                    StatusCode::FORBIDDEN,
                    JsonRpcErrorData {
                        message: format!("IP ({}) is not allowed!", ip).into(),
                        code: StatusCode::FORBIDDEN.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::InvalidHeaderValue(err) => {
                trace!(?err, "InvalidHeaderValue");
                (
                    StatusCode::BAD_REQUEST,
                    JsonRpcErrorData {
                        message: err.to_string().into(),
                        code: StatusCode::BAD_REQUEST.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::InvalidEip => {
                trace!("InvalidEip");
                (
                    StatusCode::BAD_REQUEST,
                    JsonRpcErrorData {
                        message: "invalid message eip given".into(),
                        code: StatusCode::BAD_REQUEST.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::InvalidInviteCode => {
                trace!("InvalidInviteCode");
                (
                    StatusCode::UNAUTHORIZED,
                    JsonRpcErrorData {
                        message: "invalid invite code".into(),
                        code: StatusCode::UNAUTHORIZED.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::Io(err) => {
                warn!(?err, "std io");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    JsonRpcErrorData {
                        // TODO: is it safe to expose our io error strings?
                        message: err.to_string().into(),
                        code: StatusCode::INTERNAL_SERVER_ERROR.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::InvalidReferer => {
                trace!("InvalidReferer");
                (
                    StatusCode::BAD_REQUEST,
                    JsonRpcErrorData {
                        message: "invalid referer!".into(),
                        code: StatusCode::BAD_REQUEST.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::InvalidSignatureLength => {
                trace!("InvalidSignatureLength");
                (
                    StatusCode::BAD_REQUEST,
                    JsonRpcErrorData {
                        message: "invalid signature length".into(),
                        code: StatusCode::BAD_REQUEST.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::InvalidUserAgent => {
                trace!("InvalidUserAgent");
                (
                    StatusCode::FORBIDDEN,
                    JsonRpcErrorData {
                        message: "invalid user agent!".into(),
                        code: StatusCode::FORBIDDEN.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::InvalidUserKey => {
                trace!("InvalidUserKey");
                (
                    StatusCode::BAD_REQUEST,
                    JsonRpcErrorData {
                        message: "UserKey was not a ULID or UUID".into(),
                        code: StatusCode::BAD_REQUEST.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::InvalidUserTier => {
                warn!("InvalidUserTier");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    JsonRpcErrorData {
                        message: "UserTier is not valid!".into(),
                        code: StatusCode::BAD_REQUEST.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::JoinError(err) => {
                let code = if err.is_cancelled() {
                    trace!(?err, "JoinError. likely shutting down");
                    StatusCode::BAD_GATEWAY
                } else {
                    warn!(?err, "JoinError");
                    StatusCode::INTERNAL_SERVER_ERROR
                };

                (
                    code,
                    JsonRpcErrorData {
                        // TODO: different messages of cancelled or not?
                        message: "Unable to complete request".into(),
                        code: code.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::JsonRpcErrorData(jsonrpc_error_data) => {
                // TODO: do this without clone? the Arc needed it though
                (StatusCode::OK, jsonrpc_error_data.clone())
            }
            Self::MsgPackEncode(err) => {
                warn!(?err, "MsgPackEncode");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    JsonRpcErrorData {
                        message: format!("msgpack encode error: {}", err).into(),
                        code: StatusCode::INTERNAL_SERVER_ERROR.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::NoBlockNumberOrHash => {
                warn!("NoBlockNumberOrHash");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    JsonRpcErrorData {
                        message: "Blocks here must have a number or hash".into(),
                        code: StatusCode::INTERNAL_SERVER_ERROR.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::NoBlocksKnown => {
                error!("NoBlocksKnown");
                (
                    StatusCode::BAD_GATEWAY,
                    JsonRpcErrorData {
                        message: "no blocks known".into(),
                        code: StatusCode::BAD_GATEWAY.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::NoConsensusHeadBlock => {
                error!("NoConsensusHeadBlock");
                (
                    StatusCode::BAD_GATEWAY,
                    JsonRpcErrorData {
                        message: "no consensus head block".into(),
                        code: StatusCode::BAD_GATEWAY.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::NoDatabase => {
                error!("no database configured");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    JsonRpcErrorData {
                        message: "no database configured!".into(),
                        code: StatusCode::INTERNAL_SERVER_ERROR.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::NoHandleReady => {
                error!("NoHandleReady");
                (
                    StatusCode::BAD_GATEWAY,
                    JsonRpcErrorData {
                        message: "unable to retry for request handle".into(),
                        code: StatusCode::BAD_GATEWAY.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::NoVolatileRedisDatabase => {
                error!("no volatile redis database configured");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    JsonRpcErrorData {
                        message: "no volatile redis database configured!".into(),
                        code: StatusCode::INTERNAL_SERVER_ERROR.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::NoServersSynced => {
                warn!("NoServersSynced");
                (
                    StatusCode::BAD_GATEWAY,
                    JsonRpcErrorData {
                        message: "no servers synced".into(),
                        code: StatusCode::BAD_GATEWAY.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::NotEnoughRpcs {
                num_known,
                min_head_rpcs,
            } => {
                error!("NotEnoughRpcs {}/{}", num_known, min_head_rpcs);
                (
                    StatusCode::BAD_GATEWAY,
                    JsonRpcErrorData {
                        message: format!(
                            "not enough rpcs connected {}/{}",
                            num_known, min_head_rpcs
                        )
                        .into(),
                        code: StatusCode::BAD_GATEWAY.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::NotEnoughSoftLimit { available, needed } => {
                error!("NotEnoughSoftLimit {}/{}", available, needed);
                (
                    StatusCode::BAD_GATEWAY,
                    JsonRpcErrorData {
                        message: format!(
                            "not enough soft limit available {}/{}",
                            available, needed
                        )
                        .into(),
                        code: StatusCode::BAD_GATEWAY.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::NotFound => {
                // TODO: emit a stat?
                // TODO: instead of an error, show a normal html page for 404?
                (
                    StatusCode::NOT_FOUND,
                    JsonRpcErrorData {
                        message: "not found!".into(),
                        code: StatusCode::NOT_FOUND.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::NotImplemented(msg) => {
                warn!("NotImplemented: {}", msg);
                (
                    StatusCode::NOT_IMPLEMENTED,
                    JsonRpcErrorData {
                        message: format!(
                            "{} is not yet implemented. contact us if you need this",
                            msg
                        )
                        .into(),
                        code: StatusCode::NOT_IMPLEMENTED.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::OriginRequired => {
                trace!("OriginRequired");
                (
                    StatusCode::BAD_REQUEST,
                    JsonRpcErrorData {
                        message: "Origin required".into(),
                        code: StatusCode::BAD_REQUEST.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::OriginNotAllowed(origin) => {
                trace!("OriginNotAllowed origin={}", origin);
                (
                    StatusCode::FORBIDDEN,
                    JsonRpcErrorData {
                        message: format!("Origin ({}) is not allowed!", origin).into(),
                        code: StatusCode::FORBIDDEN.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::ParseBytesError(err) => {
                trace!("ParseBytesError err={:#?}", err);
                (
                    StatusCode::BAD_REQUEST,
                    JsonRpcErrorData {
                        message: "parse bytes error!".into(),
                        code: StatusCode::BAD_REQUEST.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::ParseMsgError(err) => {
                trace!("ParseMsgError err={:#?}", err);
                (
                    StatusCode::BAD_REQUEST,
                    JsonRpcErrorData {
                        message: "parse message error!".into(),
                        code: StatusCode::BAD_REQUEST.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::ParseAddressError => {
                trace!("ParseAddressError");
                (
                    StatusCode::BAD_REQUEST,
                    JsonRpcErrorData {
                        message: "unable to parse address".into(),
                        code: StatusCode::BAD_REQUEST.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::PaymentRequired => {
                trace!("PaymentRequiredError");
                (
                    StatusCode::PAYMENT_REQUIRED,
                    JsonRpcErrorData {
                        message: "Payment is required to activate premium".into(),
                        code: StatusCode::PAYMENT_REQUIRED.as_u16().into(),
                        data: None,
                    },
                )
            }
            // TODO: this should actually by the id of the key. multiple users might control one key
            Self::RateLimited(authorization, retry_at) => {
                // TODO: emit a stat

                let retry_msg = if let Some(retry_at) = retry_at {
                    let retry_in = retry_at.duration_since(Instant::now()).as_secs();

                    format!(" Retry in {} seconds", retry_in)
                } else {
                    "".to_string()
                };

                // create a string with either the IP or the rpc_key_id
                let msg = if authorization.checks.rpc_secret_key_id.is_none() {
                    format!("too many requests from {}.{}", authorization.ip, retry_msg)
                } else {
                    format!(
                        "too many requests from rpc key #{}.{}",
                        authorization.checks.rpc_secret_key_id.unwrap(),
                        retry_msg,
                    )
                };

                (
                    StatusCode::TOO_MANY_REQUESTS,
                    JsonRpcErrorData {
                        message: msg.into(),
                        code: StatusCode::TOO_MANY_REQUESTS.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::Redis(err) => {
                warn!(?err, "redis");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    JsonRpcErrorData {
                        message: "redis error!".into(),
                        code: StatusCode::INTERNAL_SERVER_ERROR.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::RedisDeadpool(err) => {
                error!(?err, "redis deadpool");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    JsonRpcErrorData {
                        // TODO: is it safe to expose our io error strings?
                        message: err.to_string().into(),
                        code: StatusCode::INTERNAL_SERVER_ERROR.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::RefererRequired => {
                trace!("referer required");
                (
                    StatusCode::BAD_REQUEST,
                    JsonRpcErrorData {
                        message: "Referer required".into(),
                        code: StatusCode::BAD_REQUEST.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::RefererNotAllowed(referer) => {
                trace!(?referer, "referer not allowed");
                (
                    StatusCode::FORBIDDEN,
                    JsonRpcErrorData {
                        message: format!("Referer ({:?}) is not allowed", referer).into(),
                        code: StatusCode::FORBIDDEN.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::SemaphoreAcquireError(err) => {
                error!(?err, "semaphore acquire");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    JsonRpcErrorData {
                        // TODO: is it safe to expose all of our anyhow strings?
                        message: "semaphore acquire error".into(),
                        code: StatusCode::INTERNAL_SERVER_ERROR.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::SendAppStatError(err) => {
                error!(?err, "SendAppStatError");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    JsonRpcErrorData {
                        message: "error stat_sender sending response_stat".into(),
                        code: StatusCode::INTERNAL_SERVER_ERROR.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::SerdeJson(err) => {
                trace!(?err, "serde json");
                (
                    StatusCode::BAD_REQUEST,
                    JsonRpcErrorData {
                        message: format!("de/serialization error! {}", err).into(),
                        code: StatusCode::BAD_REQUEST.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::SiweVerification(err) => {
                trace!(?err, "Siwe Verification");
                (
                    StatusCode::UNAUTHORIZED,
                    JsonRpcErrorData {
                        message: format!("siwe verification error: {}", err.to_string()).into(),
                        code: StatusCode::UNAUTHORIZED.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::StatusCode(status_code, err_msg, err) => {
                // different status codes should get different error levels. 500s should warn. 400s should stat
                let code = status_code.as_u16();
                if (500..600).contains(&code) {
                    warn!(%err_msg, ?err, "server error {}", code);
                } else {
                    trace!(%err_msg, ?err, "user error {}", code);
                }

                (
                    *status_code,
                    JsonRpcErrorData {
                        message: err_msg.clone(),
                        code: code.into(),
                        data: None,
                    },
                )
            }
            Self::Timeout(x) => (
                StatusCode::REQUEST_TIMEOUT,
                JsonRpcErrorData {
                    message: format!("request timed out: {:?}", x).into(),
                    code: StatusCode::REQUEST_TIMEOUT.as_u16().into(),
                    // TODO: include the actual id!
                    data: None,
                },
            ),
            Self::UlidDecode(err) => {
                // trace!(?err, "UlidDecodeError");
                (
                    StatusCode::BAD_REQUEST,
                    JsonRpcErrorData {
                        message: format!("{}", err).into(),
                        code: StatusCode::BAD_REQUEST.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::UnknownBlockHash(hash) => {
                debug!(%hash, "UnknownBlockHash");
                (
                    StatusCode::OK,
                    JsonRpcErrorData {
                        message: format!("block {} not found", hash).into(),
                        code: -32000,
                        data: None,
                    },
                )
            }
            Self::UnknownBlockNumber { known, unknown } => {
                debug!(%known, %unknown, "UnknownBlockNumber");
                (
                    StatusCode::OK,
                    JsonRpcErrorData {
                        message: format!("block #{} not found. best known is #{}", unknown, known)
                            .into(),
                        code: -32000,
                        data: None,
                    },
                )
            }
            Self::UnknownKey => (
                StatusCode::UNAUTHORIZED,
                JsonRpcErrorData {
                    message: "unknown api key!".into(),
                    code: StatusCode::UNAUTHORIZED.as_u16().into(),
                    data: None,
                },
            ),
            Self::UnknownReferralCode => {
                trace!("UnknownReferralCode");
                (
                    StatusCode::UNAUTHORIZED,
                    JsonRpcErrorData {
                        message: "invalid referral code".into(),
                        code: StatusCode::UNAUTHORIZED.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::UserAgentRequired => {
                trace!("UserAgentRequired");
                (
                    StatusCode::UNAUTHORIZED,
                    JsonRpcErrorData {
                        message: "User agent required".into(),
                        code: StatusCode::UNAUTHORIZED.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::UserAgentNotAllowed(ua) => {
                trace!("UserAgentNotAllowed ua={}", ua);
                (
                    StatusCode::FORBIDDEN,
                    JsonRpcErrorData {
                        message: format!("User agent ({}) is not allowed!", ua).into(),
                        code: StatusCode::FORBIDDEN.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::UserIdZero => {
                warn!("UserIdZero");
                // TODO: this might actually be an application error and not a BAD_REQUEST
                (
                    StatusCode::BAD_REQUEST,
                    JsonRpcErrorData {
                        message: "user ids should always be non-zero".into(),
                        code: StatusCode::BAD_REQUEST.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::WatchRecvError(err) => {
                error!("WatchRecvError err={:#?}", err);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    JsonRpcErrorData {
                        message: "watch recv error!".into(),
                        code: StatusCode::INTERNAL_SERVER_ERROR.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::WatchSendError => {
                error!("WatchSendError");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    JsonRpcErrorData {
                        message: "watch send error!".into(),
                        code: StatusCode::INTERNAL_SERVER_ERROR.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::WebsocketOnly => {
                trace!("WebsocketOnly. redirect_public_url not set");
                (
                    StatusCode::BAD_REQUEST,
                    JsonRpcErrorData {
                        message: "only websockets work here".into(),
                        code: StatusCode::BAD_REQUEST.as_u16().into(),
                        data: None,
                    },
                )
            }
            Self::WithContext(err, msg) => match err {
                Some(err) => {
                    warn!(?err, %msg, "error w/ context");
                    return err.as_response_parts();
                }
                None => {
                    warn!(%msg, "error w/ context");
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        JsonRpcErrorData {
                            message: msg.clone(),
                            code: StatusCode::INTERNAL_SERVER_ERROR.as_u16().into(),
                            data: None,
                        },
                    )
                }
            },
        };

        (code, JsonRpcResponseEnum::from(err))
    }

    #[inline]
    pub fn into_response_with_id(self, id: Option<Box<RawValue>>) -> Response {
        let (status_code, response_data) = self.as_response_parts();

        let id = id.unwrap_or_default();

        let response = JsonRpcForwardedResponse::from_response_data(response_data, id);

        (status_code, Json(response)).into_response()
    }
}

impl From<ethers::types::ParseBytesError> for Web3ProxyError {
    fn from(err: ethers::types::ParseBytesError) -> Self {
        Self::ParseBytesError(Some(err))
    }
}

impl From<tokio::time::error::Elapsed> for Web3ProxyError {
    fn from(err: tokio::time::error::Elapsed) -> Self {
        Self::Timeout(Some(err))
    }
}

impl IntoResponse for Web3ProxyError {
    #[inline]
    fn into_response(self) -> Response {
        self.into_response_with_id(Default::default())
    }
}

pub trait Web3ProxyErrorContext<T> {
    fn web3_context<S: Into<Cow<'static, str>>>(self, msg: S) -> Result<T, Web3ProxyError>;
}

impl<T> Web3ProxyErrorContext<T> for Option<T> {
    fn web3_context<S: Into<Cow<'static, str>>>(self, msg: S) -> Result<T, Web3ProxyError> {
        self.ok_or(Web3ProxyError::WithContext(None, msg.into()))
    }
}

impl<T, E> Web3ProxyErrorContext<T> for Result<T, E>
where
    E: Into<Web3ProxyError>,
{
    fn web3_context<S: Into<Cow<'static, str>>>(self, msg: S) -> Result<T, Web3ProxyError> {
        self.map_err(|err| Web3ProxyError::WithContext(Some(Box::new(err.into())), msg.into()))
    }
}

impl Web3ProxyError {
    pub fn into_message(self, id: Option<Box<RawValue>>) -> Message {
        let (_, err) = self.as_response_parts();

        let id = id.unwrap_or_default();

        let err = JsonRpcForwardedResponse::from_response_data(err, id);

        let msg = serde_json::to_string(&err).expect("errors should always serialize to json");

        // TODO: what about a binary message?
        Message::Text(msg)
    }
}
