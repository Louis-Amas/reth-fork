//! Example of how to use additional rpc namespaces in the reth CLI
//!
//! Run with
//!
//! ```not_rust
//! cargo run -p additional-rpc-namespace-in-cli -- node --http --ws --enable-ext
//! ```
//!
//! This installs an additional RPC method `stateSubscriberExt_transactionCount` that can be queried via [cast](https://github.com/foundry-rs/foundry)
//!
//! ```sh
//! cast rpc stateSubscriberExt_transactionCount
//! ```But

use alloy_primitives::Address;
use clap::Parser;
use jsonrpsee::{
    core::{async_trait, SubscriptionResult},
    proc_macros::rpc,
    server::PendingSubscriptionSink,
};
use reth::{
    cli::{
        components::{RethNodeComponents, RethRpcComponents},
        config::RethRpcConfig,
        ext::{RethCliExt, RethNodeCommandConfig},
        Cli,
    },
    primitives::{B256, U256},
    providers::{CanonStateNotification, CanonStateSubscriptions},
    revm::{
        db::StorageWithOriginalValues,
        primitives::{AccountInfo, HashMap},
    },
};
use serde::{Deserialize, Serialize};

use futures::StreamExt;

fn main() {
    Cli::<MyRethCliExt>::parse().run().unwrap();
}

/// The type that tells the reth CLI what extensions to use
struct MyRethCliExt;

impl RethCliExt for MyRethCliExt {
    /// This tellLet's s the reth CLI to install the `stateSubscriber` rpc namespace via
    /// `RethCliStateSubscriberExt`
    type Node = RethCliStateSubscriberExt;
}

/// Our custom cli args extension that adds one flag to reth default CLI.
#[derive(Debug, Clone, Copy, Default, clap::Args)]
struct RethCliStateSubscriberExt {
    /// CLI flag to enable the stateSubscriber extension namespace
    #[clap(long)]
    pub enable_ext: bool,
}

impl RethNodeCommandConfig for RethCliStateSubscriberExt {
    // This is the entrypoint for the CLI to extend the RPC server with custom rpc namespaces.
    fn extend_rpc_modules<Conf, Reth>(
        &mut self,
        _config: &Conf,
        _components: &Reth,
        rpc_components: RethRpcComponents<'_, Reth>,
    ) -> eyre::Result<()>
    where
        Conf: RethRpcConfig,
        Reth: RethNodeComponents,
    {
        if !self.enable_ext {
            return Ok(())
        }

        let provider = rpc_components.registry.provider().clone();

        let ext = StateSubscriberExt { provider };
        rpc_components.modules.merge_configured(ext.into_rpc())?;

        println!("stateSubscriber extension enabled");
        Ok(())
    }
}

/// trait interface for a custom rpc namespace: `stateSubscriber`
///
/// This defines an additional namespace where all methods are configured as trait functions.
#[cfg_attr(not(test), rpc(server, namespace = "stateSubscriberExt"))]
#[cfg_attr(test, rpc(server, client, namespace = "stateSubscriberExt"))]
#[async_trait]
pub trait StateSubscriberExtApi {
    #[subscription(name = "stateSubscription", unsubscribe = "unsub", item = String)]
    async fn state_subscription(&self) -> SubscriptionResult;
}

/// The type that implements the `stateSubscriber` rpc namespace trait
pub struct StateSubscriberExt<Provider> {
    provider: Provider,
}

#[serde(rename_all = "camelCase")]
#[derive(Serialize, Deserialize)]
pub struct BlockSubscriptionResult {
    pub hash: B256,
    pub parent_hash: B256,
    pub number: u64,
}

#[serde(rename_all = "camelCase")]
#[derive(Serialize, Deserialize)]
pub struct AccountInfoResult {
    pub balance: U256,
    pub nonce: u64,
}
#[serde(rename_all = "camelCase")]
#[derive(Serialize, Deserialize)]

pub struct AccountStateResult {
    pub info: Option<AccountInfoResult>,
    pub storage: StorageWithOriginalValues,
}

#[serde(rename_all = "camelCase")]
#[derive(Serialize, Deserialize)]
pub struct BlockAndState {
    pub block: BlockSubscriptionResult,
    pub accounts_states: HashMap<Address, AccountStateResult>,
}

#[async_trait]
impl<Provider> StateSubscriberExtApiServer for StateSubscriberExt<Provider>
where
    Provider: CanonStateSubscriptions + 'static,
{
    async fn state_subscription(
        &self,
        subscription: PendingSubscriptionSink,
    ) -> SubscriptionResult {
        let _ =
            self.provider.canonical_state_stream().map(
                |update: CanonStateNotification| match update {
                    CanonStateNotification::Commit { new } => {
                        if let Some(latest_block) = new.blocks().last_key_value() {
                            let mut response = BlockAndState {
                                block: BlockSubscriptionResult {
                                    hash: latest_block.1.hash(),
                                    parent_hash: latest_block.1.parent_hash,
                                    number: latest_block.1.number,
                                },
                                accounts_states: HashMap::new(),
                            };
                            for (address, account) in new.state().state().state() {
                                let mut info: Option<AccountInfoResult> = None;
                                if let Some(_info) = &account.info {
                                    info = Some(AccountInfoResult {
                                        balance: _info.balance,
                                        nonce: _info.nonce,
                                    });
                                }
                                response.accounts_states.insert(
                                    *address,
                                    AccountStateResult {
                                        info,
                                        storage: StorageWithOriginalValues::new(),
                                    },
                                );
                            }

                            let json_response = serde_json::to_string(&response).unwrap();
                            subscription.inner.send(json_response).await;
                        }
                    }
                    CanonStateNotification::Reorg { old, new } => {}
                },
            );
        return Ok(())
    }
}

#[cfg(test)]
mod tests {
    // use super::*;
    // use jsonrpsee::{http_client::HttpClientBuilder, server::ServerBuilder};
    // use reth_transaction_pool::noop::NoopTransactionPool;
    //
    // #[tokio::test(flavor = "multi_thread")]
    // async fn test_call_transaction_count_http() {
    //     let server_addr = start_server().await;
    //     let uri = format!("http://{}", server_addr);
    //     let client = HttpClientBuilder::default().build(&uri).unwrap();
    //     let count = StateSubscriberExtApiClient::transaction_count(&client).await.unwrap();
    //     assert_eq!(count, 0);
    // }
    //
    // async fn start_server() -> std::net::SocketAddr {
    //     let server = ServerBuilder::default().build("127.0.0.1:0").await.unwrap();
    //     let addr = server.local_addr().unwrap();
    //     let pool = NoopTransactionPool::default();
    //     let api = StateSubscriberExt { pool };
    //     let server_handle = server.start(api.into_rpc());
    //
    //     tokio::spawn(server_handle.stopped());
    //
    //     addr
    // }
}
