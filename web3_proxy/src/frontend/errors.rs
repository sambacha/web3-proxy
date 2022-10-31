//! Utlities for logging errors for admins and displaying errors to users.

use crate::{app::UserKeyData, jsonrpc::JsonRpcForwardedResponse};
use axum::{
    headers,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use derive_more::From;
use http::header::InvalidHeaderValue;
use ipnet::AddrParseError;
use redis_rate_limiter::redis::RedisError;
use reqwest::header::ToStrError;
use sea_orm::DbErr;
use std::{error::Error, net::IpAddr};
use tokio::time::Instant;
use tracing::{instrument, trace, warn};

// TODO: take "IntoResponse" instead of Response?
pub type FrontendResult = Result<Response, FrontendErrorResponse>;

// TODO:
#[derive(Debug, From)]
pub enum FrontendErrorResponse {
    Anyhow(anyhow::Error),
    Box(Box<dyn Error>),
    Database(DbErr),
    HeadersError(headers::Error),
    HeaderToString(ToStrError),
    InvalidHeaderValue(InvalidHeaderValue),
    IpAddrParse(AddrParseError),
    NotFound,
    RateLimitedUser(UserKeyData, Option<Instant>),
    RateLimitedIp(IpAddr, Option<Instant>),
    Redis(RedisError),
    Response(Response),
    /// simple way to return an error message to the user and an anyhow to our logs
    StatusCode(StatusCode, String, anyhow::Error),
    UlidDecodeError(ulid::DecodeError),
    UnknownKey,
}

impl IntoResponse for FrontendErrorResponse {
    #[instrument(level = "trace")]
    fn into_response(self) -> Response {
        // TODO: include the request id in these so that users can give us something that will point to logs
        let (status_code, response) = match self {
            Self::Anyhow(err) => {
                warn!(?err, "anyhow");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    JsonRpcForwardedResponse::from_string(
                        // TODO: is it safe to expose all of our anyhow strings?
                        err.to_string(),
                        Some(StatusCode::INTERNAL_SERVER_ERROR.as_u16().into()),
                        None,
                    ),
                )
            }
            Self::Box(err) => {
                warn!(?err, "boxed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    JsonRpcForwardedResponse::from_str(
                        // TODO: make this better. maybe include the error type?
                        "boxed error!",
                        Some(StatusCode::INTERNAL_SERVER_ERROR.as_u16().into()),
                        None,
                    ),
                )
            }
            Self::Database(err) => {
                warn!(?err, "database");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    JsonRpcForwardedResponse::from_str(
                        "database error!",
                        Some(StatusCode::INTERNAL_SERVER_ERROR.as_u16().into()),
                        None,
                    ),
                )
            }
            Self::HeadersError(err) => {
                warn!(?err, "HeadersError");
                (
                    StatusCode::BAD_REQUEST,
                    JsonRpcForwardedResponse::from_str(
                        &format!("{}", err),
                        Some(StatusCode::BAD_REQUEST.as_u16().into()),
                        None,
                    ),
                )
            }
            Self::IpAddrParse(err) => {
                warn!(?err, "IpAddrParse");
                (
                    StatusCode::BAD_REQUEST,
                    JsonRpcForwardedResponse::from_str(
                        &format!("{}", err),
                        Some(StatusCode::BAD_REQUEST.as_u16().into()),
                        None,
                    ),
                )
            }
            Self::InvalidHeaderValue(err) => {
                warn!(?err, "InvalidHeaderValue");
                (
                    StatusCode::BAD_REQUEST,
                    JsonRpcForwardedResponse::from_str(
                        &format!("{}", err),
                        Some(StatusCode::BAD_REQUEST.as_u16().into()),
                        None,
                    ),
                )
            }
            Self::NotFound => {
                // TODO: emit a stat?
                // TODO: instead of an error, show a normal html page for 404
                (
                    StatusCode::NOT_FOUND,
                    JsonRpcForwardedResponse::from_str(
                        "not found!",
                        Some(StatusCode::NOT_FOUND.as_u16().into()),
                        None,
                    ),
                )
            }
            Self::RateLimitedIp(ip, _retry_at) => {
                // TODO: emit a stat
                // TODO: include retry_at in the error
                // TODO: if retry_at is None, give an unauthorized status code?
                (
                    StatusCode::TOO_MANY_REQUESTS,
                    JsonRpcForwardedResponse::from_string(
                        format!("too many requests from ip {}!", ip),
                        Some(StatusCode::TOO_MANY_REQUESTS.as_u16().into()),
                        None,
                    ),
                )
            }
            // TODO: this should actually by the id of the key. multiple users might control one key
            Self::RateLimitedUser(user_data, _retry_at) => {
                // TODO: emit a stat
                // TODO: include retry_at in the error
                (
                    StatusCode::TOO_MANY_REQUESTS,
                    JsonRpcForwardedResponse::from_string(
                        // TODO: better error
                        format!("too many requests from {:?}!", user_data),
                        Some(StatusCode::TOO_MANY_REQUESTS.as_u16().into()),
                        None,
                    ),
                )
            }
            Self::Redis(err) => {
                warn!(?err, "redis");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    JsonRpcForwardedResponse::from_str(
                        "redis error!",
                        Some(StatusCode::INTERNAL_SERVER_ERROR.as_u16().into()),
                        None,
                    ),
                )
            }
            Self::Response(r) => {
                debug_assert_ne!(r.status(), StatusCode::OK);
                return r;
            }
            Self::StatusCode(status_code, err_msg, err) => {
                // TODO: warn is way too loud. different status codes should get different error levels. 500s should warn. 400s should stat
                trace!(?status_code, ?err_msg, ?err);
                (
                    status_code,
                    JsonRpcForwardedResponse::from_str(
                        &err_msg,
                        Some(status_code.as_u16().into()),
                        None,
                    ),
                )
            }
            Self::HeaderToString(err) => {
                trace!(?err, "HeaderToString");
                (
                    StatusCode::BAD_REQUEST,
                    JsonRpcForwardedResponse::from_str(
                        &format!("{}", err),
                        Some(StatusCode::BAD_REQUEST.as_u16().into()),
                        None,
                    ),
                )
            }
            Self::UlidDecodeError(err) => {
                trace!(?err, "UlidDecodeError");
                (
                    StatusCode::BAD_REQUEST,
                    JsonRpcForwardedResponse::from_str(
                        &format!("{}", err),
                        Some(StatusCode::BAD_REQUEST.as_u16().into()),
                        None,
                    ),
                )
            }
            // TODO: stat?
            Self::UnknownKey => (
                StatusCode::UNAUTHORIZED,
                JsonRpcForwardedResponse::from_str(
                    "unknown api key!",
                    Some(StatusCode::UNAUTHORIZED.as_u16().into()),
                    None,
                ),
            ),
        };

        (status_code, Json(response)).into_response()
    }
}

#[instrument(level = "trace")]
pub async fn handler_404() -> Response {
    FrontendErrorResponse::NotFound.into_response()
}
