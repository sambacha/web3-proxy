//! Compute Units based on median request latencies and sizes.
//! Designed to match Alchemy's system.
//! I'm sure there will be memes about us copying, but the user experience of consistency makes a lot of sense to me.
//! TODO? pricing based on latency and bytes and
//! TODO: rate limit on compute units
//! TODO: pricing on compute units
//! TODO: script that queries influx and calculates observed relative costs

use migration::sea_orm::prelude::Decimal;
use std::str::FromStr;
use tracing::warn;

pub struct ComputeUnit(Decimal);

impl ComputeUnit {
    /// costs can vary widely depending on method and chain
    pub fn new(method: &str, chain_id: u64, response_bytes: u64) -> Self {
        // TODO: this works, but this is fragile. think of a better way to check the method is a subscription
        if method.ends_with(')') {
            return Self::subscription_response(response_bytes);
        }

        let cu = match (chain_id, method) {
            (1101, "zkevm_batchNumber") => 0,
            (1101, "zkevm_batchNumberByBlockNumber") => 0,
            (1101, "zkevm_consolidatedBlockNumber") => 0,
            (1101, "zkevm_getBatchByNumber") => 0,
            (1101, "zkevm_getBroadcastURI") => 0,
            (1101, "zkevm_isBlockConsolidated") => 0,
            (1101, "zkevm_isBlockVirtualized") => 0,
            (1101, "zkevm_verifiedBatchNumber") => 0,
            (1101, "zkevm_virtualBatchNumber") => 0,
            (137, "bor_getAuthor") => 10,
            (137, "bor_getCurrentProposer") => 10,
            (137, "bor_getCurrentValidators") => 10,
            (137, "bor_getRootHash") => 10,
            (137, "bor_getSignersAtHash") => 10,
            (_, "debug_traceBlockByHash") => 497,
            (_, "debug_traceBlockByNumber") => 497,
            (_, "debug_traceCall") => 309,
            (_, "debug_traceTransaction") => 309,
            (_, "erigon_forks") => 24,
            (_, "erigon_getHeaderByHash") => 24,
            (_, "erigon_getHeaderByNumber") => 24,
            (_, "erigon_getLogsByHash") => 24,
            (_, "erigon_issuance") => 24,
            (_, "eth_accounts") => 10,
            (_, "eth_blockNumber") => 10,
            (_, "eth_call") => 26,
            (_, "eth_chainId") => 0,
            (_, "eth_createAccessList") => 10,
            (_, "eth_estimateGas") => 87,
            (_, "eth_estimateUserOperationGas") => 500,
            (_, "eth_feeHistory") => 10,
            (_, "eth_gasPrice") => 19,
            (_, "eth_getBalance") => 19,
            (_, "eth_getBlockByHash") => 21,
            (_, "eth_getBlockByNumber") => 16,
            (_, "eth_getBlockReceipts") => 500,
            (_, "eth_getBlockTransactionCountByHash") => 20,
            (_, "eth_getBlockTransactionCountByNumber") => 20,
            (_, "eth_getCode") => 19,
            (_, "eth_getFilterChanges") => 20,
            (_, "eth_getFilterLogs") => 75,
            (_, "eth_getLogs") => 75,
            (_, "eth_getProof") => 21,
            (_, "eth_getStorageAt") => 17,
            (_, "eth_getTransactionByBlockHashAndIndex") => 15,
            (_, "eth_getTransactionByBlockNumberAndIndex") => 15,
            (_, "eth_getTransactionByHash") => 17,
            (_, "eth_getTransactionCount") => 26,
            (_, "eth_getTransactionReceipt") => 15,
            (_, "eth_getUncleByBlockHashAndIndex") => 15,
            (_, "eth_getUncleByBlockNumberAndIndex") => 15,
            (_, "eth_getUncleCountByBlockHash") => 15,
            (_, "eth_getUncleCountByBlockNumber") => 15,
            (_, "eth_getUserOperationByHash") => 17,
            (_, "eth_getUserOperationReceipt") => 15,
            (_, "eth_maxPriorityFeePerGas") => 10,
            (_, "eth_newBlockFilter") => 20,
            (_, "eth_newFilter") => 20,
            (_, "eth_newPendingTransactionFilter") => 20,
            (_, "eth_pollSubscriptions") => {
                return Self::unimplemented();
            }
            (_, "eth_protocolVersion") => 0,
            (_, "eth_sendRawTransaction") => 250,
            (_, "eth_sendUserOperation") => 1000,
            (_, "eth_subscribe") => 10,
            (_, "eth_supportedEntryPoints") => 5,
            (_, "eth_syncing") => 0,
            (_, "eth_uninstallFilter") => 10,
            (_, "eth_unsubscribe") => 10,
            (_, "net_listening") => 0,
            (_, "net_version") => 0,
            (_, "test") => 0,
            (_, "trace_block") => 24,
            (_, "trace_call") => 75,
            (_, "trace_filter") => 75,
            (_, "trace_get") => 17,
            (_, "trace_rawTransaction") => 75,
            (_, "trace_replayBlockTransactions") => 2983,
            (_, "trace_replayTransaction") => 2983,
            (_, "trace_transaction") => 26,
            (_, "web3_clientVersion") => 15,
            (_, "web3_sha3") => 15,
            (_, method) => {
                warn!("unknown method {}", method);
                return Self::unimplemented();
            }
        };

        let cu = Decimal::from(cu);

        Self(cu)
    }

    /// notifications and subscription responses cost per-byte
    pub fn subscription_response<D: Into<Decimal>>(num_bytes: D) -> Self {
        let cu = num_bytes.into() * Decimal::new(4, 2);

        Self(cu)
    }

    /// requesting an unimplemented function costs 2 CU
    pub fn unimplemented() -> Self {
        Self(2.into())
    }

    /// Compute cost per request
    /// All methods cost the same
    /// The number of bytes are based on input, and output bytes
    pub fn cost(&self, archive_request: bool, cache_hit: bool, usd_per_cu: Decimal) -> Decimal {
        // TODO: server errors are free. need to split server and user errors

        let mut cost = self.0 * usd_per_cu;

        if archive_request {
            cost *= Decimal::from_str("2.5").unwrap();
        }

        // cache hits get a 25% discount
        if cache_hit {
            cost *= Decimal::from_str("0.75").unwrap()
        }

        cost
    }
}
