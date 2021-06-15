use parity_scale_codec::{Decode, Encode};
use sc_consensus_pow::{Error, PowAlgorithm};

use sp_api::ProvideRuntimeApi;
use sp_consensus_pow::{DifficultyApi, Seal as RawSeal};
use sp_blockchain::HeaderBackend;
use ethereum_types::{self, U256 as EU256, H256 as EH256};
use sp_core::{U256, H256};
use sp_runtime::generic::BlockId;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT, UniqueSaturatedInto};
use std::{cmp, sync::Arc, time::{SystemTime, UNIX_EPOCH}};
use ethash::{self, quick_get_difficulty, slow_hash_block_number, EthashManager};
use crate::types::{WorkSeal};
use crate::rpc::{error::{Error as EthError}};

/// A minimal PoW algorithm that uses Sha3 hashing.
/// Difficulty is fixed at 1_000_000
#[derive(Clone)]
pub struct MinimalEthashAlgorithm {
	pow: Arc<EthashManager>,
}

impl MinimalEthashAlgorithm {
	pub fn new() -> Self {
		use tempdir::TempDir;

		let tempdir = TempDir::new("").unwrap();
		Self { pow: Arc::new(EthashManager::new(tempdir.path(), None, u64::max_value())), }
	}

	fn verify_seal(&self, seal: &WorkSeal) -> Result<(), EthError> {
		let mut tmp:[u8; 32] = seal.pow_hash.into();
		let pre_hash = EH256::from(tmp);
		tmp = seal.mix_digest.into();
		let mix_digest = EH256::from(tmp);

        let result = self.pow.compute_light(
            seal.header_nr,
            &pre_hash.0,
            seal.nonce,
        );
        let mix = EH256(result.mix_hash);
        let difficulty = ethash::boundary_to_difficulty(&EH256(result.value));
        // println!("******miner", "num: {num}, seed: {seed}, h: {h}, non: {non}, mix: {mix}, res: {res}",
		// 	   num = seal.header_nr,
		// 	   seed = EH256(slow_hash_block_number(seal.header_nr)),
		// 	   h = pre_hash,
		// 	   non = seal.nonce,
		// 	   mix = EH256(result.mix_hash),
		// 	   res = EH256(result.value));

        if mix != mix_digest {
            return Err(EthError::MismatchedH256SealElement);
        }

		// tmp = self.difficulty(seal.pow_hash.into()).unwrap().into();
		// let header_dif = EU256::from(tmp);
        // if difficulty < header_dif {
        //     return Err(EthError::InvalidProofOfWork);
        // }

		// println!("******miner verified ok");
        Ok(())
    }
}

// Here we implement the general PowAlgorithm trait for our concrete EthashAlgorithm
impl<B: BlockT<Hash = H256>> PowAlgorithm<B> for MinimalEthashAlgorithm {
	type Difficulty = U256;

	fn difficulty(&self, _parent: B::Hash) -> Result<Self::Difficulty, Error<B>> {
		// Fixed difficulty hardcoded here
		Ok(U256::from(1_000_000))
	}

	fn calc_difficulty(&self, _parent: B::Hash) -> Result<Self::Difficulty, Error<B>> {
		// Fixed difficulty hardcoded here
		Ok(U256::from(1_000_000))
	}

	fn verify(
		&self,
		_parent: &BlockId<B>,
		pre_hash: &H256,
		_pre_digest: Option<&[u8]>,
		seal: &RawSeal,
		difficulty: Self::Difficulty,
	) -> Result<bool, Error<B>> {
		// Try to construct a seal object by decoding the raw seal given
		let seal = match WorkSeal::decode(&mut &seal[..]) {
			Ok(seal) => seal,
			Err(_) => return Ok(false),
		};

		match self.verify_seal(&seal) {
			Ok(_) => {},
			Err(_) => return Ok(false),
		};

		Ok(true)
	}
}

/// A complete PoW Algorithm that uses Sha3 hashing.
/// Needs a reference to the client so it can grab the difficulty from the runtime.
pub struct EthashAlgorithm<C> {
	client: Arc<C>,
	pow: Arc<EthashManager>,
	minimum_difficulty: U256,
	difficulty_bound_divisor: U256,
	difficulty_increment_divisor: u64,
	duration_limit: u64,
}

impl<C> EthashAlgorithm<C> {
	pub fn new(client: Arc<C>) -> Self {
		use tempdir::TempDir;

		let tempdir = TempDir::new("").unwrap();
		Self { 
			client, 
			pow: Arc::new(EthashManager::new(tempdir.path(), None, u64::max_value())), 
			minimum_difficulty: U256::from(1_000_000),
			difficulty_bound_divisor: U256::from(2048),
            difficulty_increment_divisor: 10,
			duration_limit: 13,
		}
	}

	fn verify_seal(&self, seal: &WorkSeal) -> Result<(), EthError> {
		let mut tmp:[u8; 32] = seal.pow_hash.into();
		let pre_hash = EH256::from(tmp);
		tmp = seal.mix_digest.into();
		let mix_digest = EH256::from(tmp);

        let result = self.pow.compute_light(
            seal.header_nr,
            &pre_hash.0,
            seal.nonce,
        );
        let mix = EH256(result.mix_hash);
        let difficulty = ethash::boundary_to_difficulty(&EH256(result.value));
        // println!("******miner", "num: {num}, seed: {seed}, h: {h}, non: {non}, mix: {mix}, res: {res}",
		// 	   num = seal.header_nr,
		// 	   seed = EH256(slow_hash_block_number(seal.header_nr)),
		// 	   h = pre_hash,
		// 	   non = seal.nonce,
		// 	   mix = EH256(result.mix_hash),
		// 	   res = EH256(result.value));

        if mix != mix_digest {
            return Err(EthError::MismatchedH256SealElement);
        }

		// tmp = self.difficulty(seal.pow_hash.into()).unwrap().into();
		// let header_dif = EU256::from(tmp);
        // if difficulty < header_dif {
        //     return Err(EthError::InvalidProofOfWork);
        // }

		// println!("******miner verified ok");
        Ok(())
    }
}

// Manually implement clone. Deriving doesn't work because
// it'll derive impl<C: Clone> Clone for EthashAlgorithm<C>. But C in practice isn't Clone.
impl<C> Clone for EthashAlgorithm<C> {
	fn clone(&self) -> Self {
		Self::new(self.client.clone())
	}
}

// Here we implement the general PowAlgorithm trait for our concrete EthashAlgorithm
impl<B: BlockT<Hash = H256>, C> PowAlgorithm<B> for EthashAlgorithm<C>
where
	C: HeaderBackend<B> + ProvideRuntimeApi<B>,
{
	type Difficulty = U256;

	fn difficulty(&self, hash: B::Hash) -> Result<Self::Difficulty, Error<B>> {
		let header = match self.client.header(BlockId::<B>::hash(hash)) {
			Ok(header) => match header {
				Some(header) => header,
				None => {
					return Err(sc_consensus_pow::Error::Other(format!("there should be header")));
				},
			},
			Err(err) => {
				return Err(sc_consensus_pow::Error::Other(format!("{:?}", err)));
			},
		};

		let seal = match sc_consensus_pow::fetch_seal::<B>(
				header.digest().logs.last(),
				hash,
			) {
			Ok(seal) => seal,
			Err(err) => {
				let nr :u64 = UniqueSaturatedInto::<u64>::unique_saturated_into(*header.number());
				if nr == 0 { //:NOTICE: use minimum_difficulty in genesis block 
					return Ok(self.minimum_difficulty);
				} else {
					return Err(sc_consensus_pow::Error::Other(format!("{:?}", err)));
				}
			},
		};
		let seal = match WorkSeal::decode(&mut &seal[..]) {
			Ok(seal) => seal,
			Err(err) => {
				return Err(sc_consensus_pow::Error::Other(format!("{:?}", err)));
			},
		};

		// header difficulty
		Ok(seal.difficulty)
	}

	fn calc_difficulty(&self, parent: B::Hash) -> Result<Self::Difficulty, Error<B>> {
		let parent_header = match self.client.header(BlockId::<B>::hash(parent)) {
			Ok(header) => match header {
				Some(header) => header,
				None => {
					//:NOTICE: This should be the genesis header, use minimum_difficulty
					return Ok(self.minimum_difficulty);
				},
			},
			Err(err) => {
				return Err(sc_consensus_pow::Error::Other(format!("{:?}", err)));
			},
		};

		let seal = match sc_consensus_pow::fetch_seal::<B>(
				parent_header.digest().logs.last(),
				parent,
			) {
			Ok(seal) => seal,
			Err(err) => {
				let nr :u64 = UniqueSaturatedInto::<u64>::unique_saturated_into(*parent_header.number());
				if nr == 0 { //:NOTICE: use minimum_difficulty in genesis block 
					return Ok(self.minimum_difficulty);
				} else {
					return Err(sc_consensus_pow::Error::Other(format!("{:?}", err)));
				}
			},
		};
		let parent_seal = match WorkSeal::decode(&mut &seal[..]) {
			Ok(seal) => seal,
			Err(err) => {
				return Err(sc_consensus_pow::Error::Other(format!("{:?}", err)));
			},
		};

		let min_difficulty = self.minimum_difficulty;
		let difficulty_bound_divisor = self.difficulty_bound_divisor;
		let duration_limit = self.duration_limit;
		let now :u64 = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();

        let mut target = if now >= parent_seal.timestamp + duration_limit {
			parent_seal.difficulty - (parent_seal.difficulty / difficulty_bound_divisor)
		} else {
			parent_seal.difficulty + (parent_seal.difficulty / difficulty_bound_divisor)
		};
		target = cmp::max(min_difficulty, target);
		
		// parent header difficulty
		Ok(target)
	}

	fn verify(
		&self,
		_parent: &BlockId<B>,
		pre_hash: &H256,
		_pre_digest: Option<&[u8]>,
		seal: &RawSeal,
		difficulty: Self::Difficulty,
	) -> Result<bool, Error<B>> {
		// Try to construct a seal object by decoding the raw seal given
		let seal = match WorkSeal::decode(&mut &seal[..]) {
			Ok(seal) => seal,
			Err(_) => return Ok(false),
		};

		match self.verify_seal(&seal) {
			Ok(_) => {},
			Err(_) => return Ok(false),
		};

		Ok(true)
	}
}
