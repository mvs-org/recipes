//! Service and ServiceFactory implementation. Specialized wrapper over substrate service.

use runtime::{self, opaque::Block, RuntimeApi};
use sc_client_api::{ExecutorProvider, RemoteBackend};
use sc_executor::native_executor_instance;
pub use sc_executor::NativeExecutor;
use sc_service::{error::Error as ServiceError, Configuration, PartialComponents, TaskManager};
use sp_api::TransactionFor;
use sp_consensus::import_queue::BasicQueue;
use sp_inherents::InherentDataProviders;
use std::{sync::Arc, time::Duration};
use std::thread;
use sp_core::{U256, H256};
use crate::rpc::{ethash_rpc, EtheminerCmd, error::{Error as EthError}};
use crate::types::{Work, WorkSeal};
use crate::pow;
use sp_api::ProvideRuntimeApi;
use sc_consensus_pow::{MiningWorker, MiningMetadata, MiningBuild};
use sc_consensus_pow::{PowAlgorithm};
use sp_runtime::traits::{Block as BlockT, Header as HeaderT, UniqueSaturatedInto};
use parking_lot::Mutex;
use futures::prelude::*;
use ethash::{self, SeedHashCompute};
use parity_scale_codec::{Decode, Encode};
use ethereum_types::{self, U256 as EU256, H256 as EH256};
use lazy_static::lazy_static;

// Our native executor instance.
native_executor_instance!(
	pub Executor,
	runtime::api::dispatch,
	runtime::native_version,
);

type FullClient = sc_service::TFullClient<Block, RuntimeApi, Executor>;
type FullBackend = sc_service::TFullBackend<Block>;
type FullSelectChain = sc_consensus::LongestChain<FullBackend, Block>;

pub fn build_inherent_data_providers() -> Result<InherentDataProviders, ServiceError> {
	let providers = InherentDataProviders::new();

	providers
		.register_provider(sp_timestamp::InherentDataProvider)
		.map_err(Into::into)
		.map_err(sp_consensus::error::Error::InherentData)?;

	Ok(providers)
}

lazy_static! {
	static ref ETHASH_ALG: pow::MinimalEthashAlgorithm = pow::MinimalEthashAlgorithm::new();
}

/// Returns most parts of a service. Not enough to run a full chain,
/// But enough to perform chain operations like purge-chain
#[allow(clippy::type_complexity)]
pub fn new_partial(
	config: &Configuration,
) -> Result<
	PartialComponents<
		FullClient,
		FullBackend,
		FullSelectChain,
		BasicQueue<Block, TransactionFor<FullClient, Block>>,
		sc_transaction_pool::FullPool<Block, FullClient>,
		sc_consensus_pow::PowBlockImport<
			Block,
			Arc<FullClient>,
			FullClient,
			FullSelectChain,
			pow::MinimalEthashAlgorithm,
			impl sp_consensus::CanAuthorWith<Block>,
		>,
	>,
	ServiceError,
> {
	let inherent_data_providers = build_inherent_data_providers()?;

	let (client, backend, keystore_container, task_manager) =
		sc_service::new_full_parts::<Block, RuntimeApi, Executor>(&config)?;
	let client = Arc::new(client);

	let select_chain = sc_consensus::LongestChain::new(backend.clone());

	let transaction_pool = sc_transaction_pool::BasicPool::new_full(
		config.transaction_pool.clone(),
		config.role.is_authority().into(),
		config.prometheus_registry(),
		task_manager.spawn_handle(),
		client.clone(),
	);

	let can_author_with = sp_consensus::CanAuthorWithNativeVersion::new(client.executor().clone());
	
	let pow_block_import = sc_consensus_pow::PowBlockImport::new(
		client.clone(),
		client.clone(),
		ETHASH_ALG.clone(),
		0, // check inherents starting at block 0
		select_chain.clone(),
		inherent_data_providers.clone(),
		can_author_with,
	);

	let import_queue = sc_consensus_pow::import_queue(
		Box::new(pow_block_import.clone()),
		None,
		ETHASH_ALG.clone(),
		inherent_data_providers.clone(),
		&task_manager.spawn_handle(),
		config.prometheus_registry(),
	)?;

	Ok(PartialComponents {
		client,
		backend,
		import_queue,
		keystore_container,
		task_manager,
		transaction_pool,
		select_chain,
		inherent_data_providers,
		other: pow_block_import,
	})
}

/// Builds a new service for a full client.
pub fn new_full(config: Configuration) -> Result<TaskManager, ServiceError> {
	
	let sc_service::PartialComponents {
		client,
		backend,
		mut task_manager,
		import_queue,
		keystore_container,
		select_chain,
		transaction_pool,
		inherent_data_providers,
		other: pow_block_import,
	} = new_partial(&config)?;

	let (network, network_status_sinks, system_rpc_tx, network_starter) =
		sc_service::build_network(sc_service::BuildNetworkParams {
			config: &config,
			client: client.clone(),
			transaction_pool: transaction_pool.clone(),
			spawn_handle: task_manager.spawn_handle(),
			import_queue,
			on_demand: None,
			block_announce_validator_builder: None,
		})?;

	if config.offchain_worker.enabled {
		sc_service::build_offchain_workers(
			&config,
			backend.clone(),
			task_manager.spawn_handle(),
			client.clone(),
			network.clone(),
		);
	}

	let is_authority = config.role.is_authority();
	let prometheus_registry = config.prometheus_registry().cloned();

	// Channel for the rpc handler to communicate with the authorship task.
	let (command_sink, commands_stream) = futures::channel::mpsc::channel(1000);

	let rpc_extensions_builder = {
		let client = client.clone();
		let pool = transaction_pool.clone();
		Box::new(move |deny_unsafe, _| {
			let deps = crate::rpc::FullDeps {
				client: client.clone(),
				pool: pool.clone(),
				deny_unsafe,
				command_sink: command_sink.clone(),
			};

			crate::rpc::create_full(deps)
		})
	};

	sc_service::spawn_tasks(sc_service::SpawnTasksParams {
		network: network.clone(),
		client: client.clone(),
		keystore: keystore_container.sync_keystore(),
		task_manager: &mut task_manager,
		transaction_pool: transaction_pool.clone(),
		rpc_extensions_builder,
		on_demand: None,
		remote_blockchain: None,
		backend,
		network_status_sinks,
		system_rpc_tx,
		config,
	})?;

	if is_authority {
		let proposer = sc_basic_authorship::ProposerFactory::new(
			task_manager.spawn_handle(),
			client.clone(),
			transaction_pool.clone(),
			prometheus_registry.as_ref(),
		);

		let can_author_with =
			sp_consensus::CanAuthorWithNativeVersion::new(client.executor().clone());

		// Parameter details:
		//   https://substrate.dev/rustdocs/v3.0.0/sc_consensus_pow/fn.start_mining_worker.html
		// Also refer to kulupu config:
		//   https://github.com/kulupu/kulupu/blob/master/src/service.rs
		let (_worker, worker_task) = sc_consensus_pow::start_mining_worker(
			Box::new(pow_block_import),
			client.clone(),
			select_chain,
			ETHASH_ALG.clone(),
			proposer,
			network.clone(),
			None,
			inherent_data_providers,
			// time to wait for a new block before starting to mine a new one
			Duration::from_secs(10),
			// how long to take to actually build the block (i.e. executing extrinsics)
			Duration::from_secs(10),
			can_author_with,
		);

		task_manager
			.spawn_essential_handle()
			.spawn_blocking("pow", worker_task);
		
		// Start Mining
		task_manager
			.spawn_essential_handle()
			.spawn_blocking("mining", run_mining_svc(_worker.clone(), commands_stream));

	}

	network_starter.start_network();
	Ok(task_manager)
}

/// Builds a new service for a light client.
pub fn new_light(config: Configuration) -> Result<TaskManager, ServiceError> {
	let (client, backend, keystore_container, mut task_manager, on_demand) =
		sc_service::new_light_parts::<Block, RuntimeApi, Executor>(&config)?;

	let transaction_pool = Arc::new(sc_transaction_pool::BasicPool::new_light(
		config.transaction_pool.clone(),
		config.prometheus_registry(),
		task_manager.spawn_handle(),
		client.clone(),
		on_demand.clone(),
	));

	let select_chain = sc_consensus::LongestChain::new(backend.clone());
	let inherent_data_providers = build_inherent_data_providers()?;
	// FixMe #375
	let _can_author_with = sp_consensus::CanAuthorWithNativeVersion::new(client.executor().clone());

	let pow_block_import = sc_consensus_pow::PowBlockImport::new(
		client.clone(),
		client.clone(),
		ETHASH_ALG.clone(),
		0, // check inherents starting at block 0
		select_chain,
		inherent_data_providers.clone(),
		// FixMe #375
		sp_consensus::AlwaysCanAuthor,
	);

	let import_queue = sc_consensus_pow::import_queue(
		Box::new(pow_block_import),
		None,
		ETHASH_ALG.clone(),
		inherent_data_providers,
		&task_manager.spawn_handle(),
		config.prometheus_registry(),
	)?;

	let (network, network_status_sinks, system_rpc_tx, network_starter) =
		sc_service::build_network(sc_service::BuildNetworkParams {
			config: &config,
			client: client.clone(),
			transaction_pool: transaction_pool.clone(),
			spawn_handle: task_manager.spawn_handle(),
			import_queue,
			on_demand: Some(on_demand.clone()),
			block_announce_validator_builder: None,
		})?;

	sc_service::spawn_tasks(sc_service::SpawnTasksParams {
		remote_blockchain: Some(backend.remote_blockchain()),
		transaction_pool,
		task_manager: &mut task_manager,
		on_demand: Some(on_demand),
		rpc_extensions_builder: Box::new(|_, _| ()),
		config,
		client,
		keystore: keystore_container.sync_keystore(),
		backend,
		network,
		network_status_sinks,
		system_rpc_tx,
	})?;

	network_starter.start_network();

	Ok(task_manager)
}

pub async fn run_mining_svc<B, Algorithm, C, CS>(
	worker : Arc<Mutex<MiningWorker<B, Algorithm, C>>>,
	mut commands_stream: CS,
)
	where 
	B: BlockT,
	Algorithm: PowAlgorithm<B, Difficulty = U256>,
	C: sp_api::ProvideRuntimeApi<B>,
	CS: Stream<Item=EtheminerCmd<<B as BlockT>::Hash>> + Unpin + 'static,
{
	let seed_compute = SeedHashCompute::default();

	while let Some(command) = commands_stream.next().await {
		match command {
			EtheminerCmd::GetWork { mut sender } => {
				let metadata = worker.lock().metadata();
				if let Some(metadata) = metadata {
					let nr :u64 = UniqueSaturatedInto::<u64>::unique_saturated_into(metadata.number);
					let pow_hash:U256 = U256::from(metadata.pre_hash.as_ref());
					let seed_hash:U256 = seed_compute.hash_block_number(nr).into();
					let tmp:[u8; 32] = metadata.difficulty.into();
					let tt = ethash::difficulty_to_boundary(&ethereum_types::U256::from(tmp));
					let target:U256 = U256::from(tt.as_ref());

					let ret = Ok(Work { 
						pow_hash, 
						seed_hash,
						target, 
						number: Some(nr),
					 });

					ethash_rpc::send_result(&mut sender, ret)
					// ethash_rpc::send_result(&mut sender, future.await)
				} else {
					ethash_rpc::send_result(&mut sender, Err(EthError::NoWork))
				}
			}
			EtheminerCmd::SubmitWork {  nonce, pow_hash, mix_digest, mut sender } => {
				let mut worker = worker.lock();
				let metadata = worker.metadata();
				if let Some(metadata) = metadata {
					let header_nr :u64 = UniqueSaturatedInto::<u64>::unique_saturated_into(metadata.number);
					let seal = WorkSeal{nonce, pow_hash, mix_digest, header_nr};
					worker.submit(seal.encode());
					ethash_rpc::send_result(&mut sender, Ok(true))
				} else {
					ethash_rpc::send_result(&mut sender, Err(EthError::NoMetaData))
				}

						
			}
			EtheminerCmd::SubmitHashrate { hash, mut sender } => {
				
			}
		}
	}
}
