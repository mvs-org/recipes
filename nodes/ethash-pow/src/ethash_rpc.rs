
use jsonrpc_core::Result;
use jsonrpc_core::Error;
use jsonrpc_derive::rpc;

use futures::{
	channel::{mpsc, oneshot},
	TryFutureExt,
	FutureExt,
	SinkExt
};

// use futures::channel::mpsc::Sender;
// use sc_consensus_pow::{MiningWorker, MiningMetadata, MiningBuild};
// use sc_consensus_pow::{Error, PowAlgorithm};
// use sp_runtime::traits::{Block as BlockT, Header as HeaderT};
// use sp_api::ProvideRuntimeApi;
// use parking_lot::Mutex;
use runtime::{self, opaque::Block, RuntimeApi};
use std::sync::Arc;
use ethereum_types::{H160, H256, H64, U256, U64};
use crate::types::work::{Work};
use crate::helpers::{errors};

/// Future's type for jsonrpc
type FutureResult<T> = Box<dyn jsonrpc_core::futures::Future<Item = T, Error = Error> + Send>;
/// sender passed to the authorship task to report errors or successes.
pub type Sender<T> = Option<oneshot::Sender<std::result::Result<T, String>>>;

/// Message sent to the background authorship task, usually by RPC.
pub enum EtheminerCmd<Hash> {
	GetWork {
		/// sender to report errors/success to the rpc.
		sender: Sender<Work>,
	},
	/// Tells the engine to finalize the block with the supplied hash
	SubmitWork {
		/// hash of the block
		hash: Hash,
		/// sender to report errors/success to the rpc.
		sender: Sender<()>,
	}
}

#[rpc(server)]
pub trait EthashRpc {
	#[rpc(name = "eth_getWork")]
    fn eth_getWork(&self, _: Option<u64>) -> FutureResult<Work>;

	#[rpc(name = "eth_submitWork")]
	fn eth_submitWork(&self, _: H64, _: H256, _: H256) -> Result<bool>;

	#[rpc(name = "eth_hashrate")]
    fn eth_hashrate(&self) -> Result<U256>;

	#[rpc(name = "eth_submitHashrate")]
	fn eth_submitHashrate(&self, _: U256, _: H256) -> Result<bool>;
}

/// A struct that implements the `EthashRpc`
pub struct EthashData<C, Hash> {
	client: Arc<C>,
	command_sink: Sender<EtheminerCmd<Hash>>,
}

impl<C, Hash> EthashData<C, Hash> {
	/// Create new `EthashData` instance with the given reference to the client.
	pub fn new(client: Arc<C>, command_sink: Sender<EtheminerCmd<Hash>>) -> Self {
		Self {
			client,
			command_sink,
		}
	}
}

impl<C: Send + Sync + 'static, Hash: Send + 'static> EthashRpc for EthashData<C, Hash> {
	fn eth_getWork(&self, no_new_work_timeout: Option<u64>) -> FutureResult<Work> {
		let mut sink = self.command_sink.clone();
		let future = async move {
			let (sender, receiver) = oneshot::channel();
			let command = EtheminerCmd::GetWork {
				sender: Some(sender),
			};
			sink.send(command).await?;
			receiver.await?
		}.boxed();

		Box::new(future.map_err(Error::from).compat())
	}

	fn eth_submitWork(&self, _: H64, _: H256, _: H256) -> Result<bool> {
		Ok(true)
	}

	fn eth_hashrate(&self) -> Result<U256> {
		Err(errors::unimplemented(None))
	}

	fn eth_submitHashrate(&self, _: U256, _: H256) -> Result<bool> {
		Ok(true)
	}
}
