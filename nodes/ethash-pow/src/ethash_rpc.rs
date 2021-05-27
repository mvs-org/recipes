use jsonrpc_core::Result;
use jsonrpc_derive::rpc;
use runtime::{self, opaque::Block, RuntimeApi};
use futures::channel::mpsc::Sender;
use sc_consensus_pow::{MiningWorker, MiningMetadata, MiningBuild};
use sc_consensus_pow::{Error, PowAlgorithm};
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};
use sp_api::ProvideRuntimeApi;
use std::sync::Arc;
use parking_lot::Mutex;

/// Message sent to the background authorship task, usually by RPC.
pub enum EtheminerCmd<Hash> {

	GetWork {
		/// specify the parent hash of the about-to-created block
		parent_hash: Option<Hash>,
		/// sender to report errors/success to the rpc.
		sender: Sender<()>,
	},
	/// Tells the engine to finalize the block with the supplied hash
	SubmitWork {
		/// hash of the block
		hash: Hash,
		/// sender to report errors/success to the rpc.
		sender: Sender<()>,
	}
}

#[rpc]
pub trait EthashRpc {
	#[rpc(name = "eth_getWork")]
	fn eth_getWork(&self) -> Result<u64>;

	#[rpc(name = "eth_submitWork")]
	fn eth_submitWork(&self, val: u64) -> Result<u64>;

	#[rpc(name = "eth_submitHashrate")]
	fn eth_submitHashrate(&self, val: u64) -> Result<u64>;
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
	fn eth_getWork(&self) -> Result<u64> {
		Ok(0)
	}

	fn eth_submitWork(&self, val: u64) -> Result<u64> {
		Ok(0)
	}

	fn eth_submitHashrate(&self, val: u64) -> Result<u64> {
		Ok(0)
	}
}
