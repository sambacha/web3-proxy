use axum::extract::Path;
use axum::response::Response;
use axum::{http::StatusCode, response::IntoResponse, Extension, Json};
use axum_client_ip::ClientIp;
use std::sync::Arc;
use uuid::Uuid;

use super::errors::anyhow_error_into_response;
use super::rate_limit::RateLimitResult;
use crate::stats::{Protocol, ProxyRequestLabels};
use crate::{app::Web3ProxyApp, jsonrpc::JsonRpcRequestEnum};

pub async fn public_proxy_web3_rpc(
    Json(payload): Json<JsonRpcRequestEnum>,
    Extension(app): Extension<Arc<Web3ProxyApp>>,
    ClientIp(ip): ClientIp,
) -> Response {
    let _ip = match app.rate_limit_by_ip(ip).await {
        Ok(x) => match x.try_into_response().await {
            Ok(RateLimitResult::AllowedIp(x)) => x,
            Err(err_response) => return err_response,
            _ => unimplemented!(),
        },
        Err(err) => return anyhow_error_into_response(None, None, err).into_response(),
    };

    let user_id = 0;
    let protocol = Protocol::HTTP;

    match &payload {
        JsonRpcRequestEnum::Batch(batch) => {
            // TODO: use inc_by if possible?
            for single in batch {
                let rpc_method = single.method.clone();

                let _count = app
                    .stats
                    .proxy_requests
                    .get_or_create(&ProxyRequestLabels {
                        protocol: protocol.clone(),
                        rpc_method,
                        user_id,
                    })
                    .inc();
            }
        }
        JsonRpcRequestEnum::Single(single) => {
            let rpc_method = single.method.clone();

            let _count = app
                .stats
                .proxy_requests
                .get_or_create(&ProxyRequestLabels {
                    protocol,
                    rpc_method,
                    user_id,
                })
                .inc();
        }
    };

    match app.proxy_web3_rpc(payload, user_id).await {
        Ok(response) => (StatusCode::OK, Json(&response)).into_response(),
        Err(err) => anyhow_error_into_response(None, None, err).into_response(),
    }
}

pub async fn user_proxy_web3_rpc(
    Json(payload): Json<JsonRpcRequestEnum>,
    Extension(app): Extension<Arc<Web3ProxyApp>>,
    Path(user_key): Path<Uuid>,
) -> Response {
    let user_id = match app.rate_limit_by_key(user_key).await {
        Ok(x) => match x.try_into_response().await {
            Ok(RateLimitResult::AllowedUser(x)) => x,
            Err(err_response) => return err_response,
            _ => unimplemented!(),
        },
        Err(err) => return anyhow_error_into_response(None, None, err).into_response(),
    };

    match app.proxy_web3_rpc(payload, user_id).await {
        Ok(response) => (StatusCode::OK, Json(&response)).into_response(),
        Err(err) => anyhow_error_into_response(None, None, err),
    }
}
