// Copyright 2015-2020 Parity Technologies (UK) Ltd.
// This file is part of OpenEthereum.

// OpenEthereum is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// OpenEthereum is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with OpenEthereum.  If not, see <http://www.gnu.org/licenses/>.

//! RPC Error codes and error objects

use std::fmt;

use jsonrpc_core::{Error, ErrorCode, Result as RpcResult, Value};
//use rlp::DecoderError;

mod codes {
    // NOTE [ToDr] Codes from [-32099, -32000]
    pub const UNSUPPORTED_REQUEST: i64 = -32000;
    pub const NO_WORK: i64 = -32001;
    pub const NO_AUTHOR: i64 = -32002;
    pub const NO_NEW_WORK: i64 = -32003;
    pub const NO_WORK_REQUIRED: i64 = -32004;
    pub const CANNOT_SUBMIT_WORK: i64 = -32005;
    pub const UNKNOWN_ERROR: i64 = -32009;
    pub const TRANSACTION_ERROR: i64 = -32010;
    pub const EXECUTION_ERROR: i64 = -32015;
    pub const EXCEPTION_ERROR: i64 = -32016;
    pub const DATABASE_ERROR: i64 = -32017;
    #[cfg(any(test, feature = "accounts"))]
    pub const ACCOUNT_LOCKED: i64 = -32020;
    #[cfg(any(test, feature = "accounts"))]
    pub const PASSWORD_INVALID: i64 = -32021;
    pub const ACCOUNT_ERROR: i64 = -32023;
    pub const REQUEST_REJECTED: i64 = -32040;
    pub const REQUEST_REJECTED_LIMIT: i64 = -32041;
    pub const REQUEST_NOT_FOUND: i64 = -32042;
    pub const ENCRYPTION_ERROR: i64 = -32055;
    #[cfg(any(test, feature = "accounts"))]
    pub const ENCODING_ERROR: i64 = -32058;
    pub const FETCH_ERROR: i64 = -32060;
    pub const NO_PEERS: i64 = -32066;
    pub const DEPRECATED: i64 = -32070;
    pub const EXPERIMENTAL_RPC: i64 = -32071;
    pub const CANNOT_RESTART: i64 = -32080;
}

pub fn unimplemented(details: Option<String>) -> Error {
    Error {
        code: ErrorCode::ServerError(codes::UNSUPPORTED_REQUEST),
        message: "This request is not implemented yet. Please create an issue on Github repo."
            .into(),
        data: details.map(Value::String),
    }
}

pub fn unsupported<T: Into<String>>(msg: T, details: Option<T>) -> Error {
    Error {
        code: ErrorCode::ServerError(codes::UNSUPPORTED_REQUEST),
        message: msg.into(),
        data: details.map(Into::into).map(Value::String),
    }
}

pub fn request_not_found() -> Error {
    Error {
        code: ErrorCode::ServerError(codes::REQUEST_NOT_FOUND),
        message: "Request not found.".into(),
        data: None,
    }
}

pub fn request_rejected() -> Error {
    Error {
        code: ErrorCode::ServerError(codes::REQUEST_REJECTED),
        message: "Request has been rejected.".into(),
        data: None,
    }
}

pub fn request_rejected_limit() -> Error {
    Error {
        code: ErrorCode::ServerError(codes::REQUEST_REJECTED_LIMIT),
        message: "Request has been rejected because of queue limit.".into(),
        data: None,
    }
}


/// Internal error signifying a logic error in code.
/// Should not be used when function can just fail
/// because of invalid parameters or incomplete node state.
pub fn internal<T: fmt::Debug>(error: &str, data: T) -> Error {
    Error {
        code: ErrorCode::InternalError,
        message: format!("Internal error occurred: {}", error),
        data: Some(Value::String(format!("{:?}", data))),
    }
}

pub fn invalid_params<T: fmt::Debug>(param: &str, details: T) -> Error {
    Error {
        code: ErrorCode::InvalidParams,
        message: format!("Couldn't parse parameters: {}", param),
        data: Some(Value::String(format!("{:?}", details))),
    }
}

pub fn execution<T: fmt::Debug>(data: T) -> Error {
    Error {
        code: ErrorCode::ServerError(codes::EXECUTION_ERROR),
        message: "Transaction execution error.".into(),
        data: Some(Value::String(format!("{:?}", data))),
    }
}

pub fn state_pruned() -> Error {
    Error {
		code: ErrorCode::ServerError(codes::UNSUPPORTED_REQUEST),
		message: "This request is not supported because your node is running with state pruning. Run with --pruning=archive.".into(),
		data: None,
	}
}

pub fn state_corrupt() -> Error {
    internal("State corrupt", "")
}

pub fn exceptional<T: fmt::Display>(data: T) -> Error {
    Error {
        code: ErrorCode::ServerError(codes::EXCEPTION_ERROR),
        message: "The execution failed due to an exception.".into(),
        data: Some(Value::String(data.to_string())),
    }
}

pub fn no_work() -> Error {
    Error {
        code: ErrorCode::ServerError(codes::NO_WORK),
        message: "Still syncing.".into(),
        data: None,
    }
}

pub fn no_new_work() -> Error {
    Error {
        code: ErrorCode::ServerError(codes::NO_NEW_WORK),
        message: "Work has not changed.".into(),
        data: None,
    }
}

pub fn no_author() -> Error {
    Error {
        code: ErrorCode::ServerError(codes::NO_AUTHOR),
        message: "Author not configured. Run Parity with --author to configure.".into(),
        data: None,
    }
}

pub fn no_work_required() -> Error {
    Error {
        code: ErrorCode::ServerError(codes::NO_WORK_REQUIRED),
        message: "External work is only required for Proof of Work engines.".into(),
        data: None,
    }
}

pub fn cannot_submit_work() -> Error {
    Error {
        code: ErrorCode::ServerError(codes::CANNOT_SUBMIT_WORK),
        message: "Cannot submit work.".into(),
        data: None,
    }
}

pub fn unavailable_block(no_ancient_block: bool, by_hash: bool) -> Error {
    if no_ancient_block {
        Error {
			code: ErrorCode::ServerError(codes::UNSUPPORTED_REQUEST),
			message: "Looks like you disabled ancient block download, unfortunately the information you're \
			trying to fetch doesn't exist in the db and is probably in the ancient blocks.".into(),
			data: None,
		}
    } else if by_hash {
        Error {
			code: ErrorCode::ServerError(codes::UNSUPPORTED_REQUEST),
			message: "Block information is incomplete while ancient block sync is still in progress, before \
					it's finished we can't determine the existence of requested item.".into(),
			data: None,
		}
    } else {
        Error {
			code: ErrorCode::ServerError(codes::UNSUPPORTED_REQUEST),
			message: "Requested block number is in a range that is not available yet, because the ancient block sync is still in progress.".into(),
			data: None,
		}
    }
}

// pub fn rlp(error: DecoderError) -> Error {
//     Error {
//         code: ErrorCode::InvalidParams,
//         message: "Invalid RLP.".into(),
//         data: Some(Value::String(format!("{:?}", error))),
//     }
// }

/// returns an error for when require_canonical was specified in RPC for EIP-1898
pub fn invalid_input() -> Error {
    Error {
        // UNSUPPORTED_REQUEST shares the same error code for EIP-1898
        code: ErrorCode::ServerError(codes::UNSUPPORTED_REQUEST),
        message: "Invalid input".into(),
        data: None,
    }
}
