// Copyright (c) Zefchain Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Conversions from types declared in [`linera-sdk`] to types generated by [`wit-bindgen`].

use linera_base::{
    crypto::CryptoHash,
    data_types::{Amount, BlockHeight, Resources, SendMessageRequest, Timestamp},
    identifiers::{
        Account, ApplicationId, BlobId, BytecodeId, ChainId, ChannelName, Destination, MessageId,
        Owner, StreamName,
    },
};

use super::wit::contract_system_api as wit_system_api;

impl From<CryptoHash> for wit_system_api::CryptoHash {
    fn from(crypto_hash: CryptoHash) -> Self {
        let parts = <[u64; 4]>::from(crypto_hash);

        wit_system_api::CryptoHash {
            part1: parts[0],
            part2: parts[1],
            part3: parts[2],
            part4: parts[3],
        }
    }
}

impl From<ChainId> for wit_system_api::CryptoHash {
    fn from(chain_id: ChainId) -> Self {
        chain_id.0.into()
    }
}

impl From<Owner> for wit_system_api::Owner {
    fn from(owner: Owner) -> Self {
        wit_system_api::Owner {
            inner0: owner.0.into(),
        }
    }
}

impl From<BlobId> for wit_system_api::BlobId {
    fn from(blob_id: BlobId) -> Self {
        wit_system_api::BlobId {
            inner0: blob_id.0.into(),
        }
    }
}

impl From<Amount> for wit_system_api::Amount {
    fn from(host: Amount) -> Self {
        wit_system_api::Amount {
            inner0: (host.lower_half(), host.upper_half()),
        }
    }
}

impl From<Account> for wit_system_api::Account {
    fn from(account: Account) -> Self {
        wit_system_api::Account {
            chain_id: account.chain_id.into(),
            owner: account.owner.map(|owner| owner.into()),
        }
    }
}

impl From<BlockHeight> for wit_system_api::BlockHeight {
    fn from(block_height: BlockHeight) -> Self {
        wit_system_api::BlockHeight {
            inner0: block_height.0,
        }
    }
}

impl From<ChainId> for wit_system_api::ChainId {
    fn from(chain_id: ChainId) -> Self {
        wit_system_api::ChainId {
            inner0: chain_id.0.into(),
        }
    }
}

impl From<ApplicationId> for wit_system_api::ApplicationId {
    fn from(application_id: ApplicationId) -> Self {
        wit_system_api::ApplicationId {
            bytecode_id: application_id.bytecode_id.into(),
            creation: application_id.creation.into(),
        }
    }
}

impl From<BytecodeId> for wit_system_api::BytecodeId {
    fn from(bytecode_id: BytecodeId) -> Self {
        wit_system_api::BytecodeId {
            message_id: bytecode_id.message_id.into(),
        }
    }
}

impl From<MessageId> for wit_system_api::MessageId {
    fn from(message_id: MessageId) -> Self {
        wit_system_api::MessageId {
            chain_id: message_id.chain_id.into(),
            height: message_id.height.into(),
            index: message_id.index,
        }
    }
}

impl From<Timestamp> for wit_system_api::Timestamp {
    fn from(timestamp: Timestamp) -> Self {
        Self {
            inner0: timestamp.micros(),
        }
    }
}

impl From<SendMessageRequest<Vec<u8>>> for wit_system_api::SendMessageRequest {
    fn from(message: SendMessageRequest<Vec<u8>>) -> Self {
        Self {
            destination: message.destination.into(),
            authenticated: message.authenticated,
            is_tracked: message.is_tracked,
            grant: message.grant.into(),
            message: message.message,
        }
    }
}

impl From<Destination> for wit_system_api::Destination {
    fn from(destination: Destination) -> Self {
        match destination {
            Destination::Recipient(chain_id) => {
                wit_system_api::Destination::Recipient(chain_id.into())
            }
            Destination::Subscribers(subscription) => {
                wit_system_api::Destination::Subscribers(subscription.into())
            }
        }
    }
}

impl From<ChannelName> for wit_system_api::ChannelName {
    fn from(name: ChannelName) -> Self {
        wit_system_api::ChannelName {
            inner0: name.into_bytes(),
        }
    }
}

impl From<StreamName> for wit_system_api::StreamName {
    fn from(name: StreamName) -> Self {
        wit_system_api::StreamName {
            inner0: name.into_bytes(),
        }
    }
}

impl From<Resources> for wit_system_api::Resources {
    fn from(resources: Resources) -> Self {
        wit_system_api::Resources {
            fuel: resources.fuel,
            read_operations: resources.read_operations,
            write_operations: resources.write_operations,
            bytes_to_read: resources.bytes_to_read,
            bytes_to_write: resources.bytes_to_write,
            messages: resources.messages,
            message_size: resources.message_size,
            storage_size_delta: resources.storage_size_delta,
        }
    }
}

impl From<log::Level> for wit_system_api::LogLevel {
    fn from(level: log::Level) -> Self {
        match level {
            log::Level::Trace => wit_system_api::LogLevel::Trace,
            log::Level::Debug => wit_system_api::LogLevel::Debug,
            log::Level::Info => wit_system_api::LogLevel::Info,
            log::Level::Warn => wit_system_api::LogLevel::Warn,
            log::Level::Error => wit_system_api::LogLevel::Error,
        }
    }
}
