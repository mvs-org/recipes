use parity_scale_codec::{Decode, Encode};
use sc_consensus_pow::{Error, PowAlgorithm};

use sp_api::ProvideRuntimeApi;
use sp_consensus_pow::{DifficultyApi, Seal as RawSeal};
use sp_blockchain::HeaderBackend;
use ethereum_types::{self, U256 as EU256, H256 as EH256};
use sp_core::{U256, H256};
use sp_runtime::generic::BlockId;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT, UniqueSaturatedInto};
use std::sync::Arc;
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
	min_difficulty: U256,
}

impl<C> EthashAlgorithm<C> {
	pub fn new(client: Arc<C>) -> Self {
		use tempdir::TempDir;

		let tempdir = TempDir::new("").unwrap();
		Self { client, pow: Arc::new(EthashManager::new(tempdir.path(), None, u64::max_value())), min_difficulty: U256::from(1_000_000)}
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

	fn difficulty(&self, parent: B::Hash) -> Result<Self::Difficulty, Error<B>> {
		let parent_id = BlockId::<B>::hash(parent);
		let parent_header = self.client.header(parent_id)
				.expect("header get error")
				.expect("there should be header");

		let seal = match sc_consensus_pow::fetch_seal::<B>(
				parent_header.digest().logs.last(),
				parent,
			) {
			Ok(seal) => seal,
			Err(err) => {
				let number = parent_header.number();
				let nr :u64 = UniqueSaturatedInto::<u64>::unique_saturated_into(*number);
				if nr == 0 { //:NOTICE: genesis block doesn't have seal
					return Ok(self.min_difficulty);
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

		// parent header difficulty
		Ok(seal.difficulty)
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
