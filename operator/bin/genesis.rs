use alloy_sol_types::SolValue;
use clap::Parser;
use ibc_client_tendermint::types::ConsensusState;
use ibc_core_commitment_types::commitment::CommitmentRoot;
use ibc_core_host_types::identifiers::ChainId;
use sp1_ics07_tendermint_operator::{util::TendermintRPCClient, TENDERMINT_ELF};
use sp1_ics07_tendermint_shared::types::sp1_ics07_tendermint::{
    ClientState, ConsensusState as SolConsensusState, Height, TrustThreshold,
};
use sp1_sdk::{utils::setup_logger, HashableKey, MockProver, Prover};
use std::{env, path::PathBuf, str::FromStr};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct GenesisArgs {
    /// Trusted block.
    #[clap(long)]
    trusted_block: Option<u64>,
    /// Genesis path.
    #[clap(long, default_value = "../contracts/script")]
    genesis_path: String,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SP1ICS07TendermintGenesis {
    // The encoded trusted client state.
    trusted_client_state: String,
    // The encoded trusted consensus state.
    trusted_consensus_state: String,
    vkey: String,
}

/// Fetches the trusted header hash for the given block height. Defaults to the latest block height.
/// Example:
/// ```sh
/// RUST_LOG=info TENDERMINT_RPC_URL="https://rpc.celestia-mocha.com/" cargo run --bin genesis --release
/// ```
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    setup_logger();

    let args = GenesisArgs::parse();

    let tendermint_rpc_client = TendermintRPCClient::default();
    let tendermint_prover = MockProver::new();
    let (_, vk) = tendermint_prover.setup(TENDERMINT_ELF);

    let latest_height = tendermint_rpc_client
        .get_latest_commit()
        .await?
        .result
        .signed_header
        .header
        .height
        .into();
    if args.trusted_block.is_none() {
        log::info!("Latest block height: {}", latest_height);
    }
    let trusted_height = args.trusted_block.unwrap_or(latest_height);

    let trusted_light_block = tendermint_rpc_client
        .get_light_block(trusted_height)
        .await
        .unwrap();
    let chain_id = ChainId::from_str(trusted_light_block.signed_header.header.chain_id.as_str())?;

    let trusted_client_state = ClientState {
        chain_id: chain_id.to_string(),
        trust_level: TrustThreshold {
            numerator: 1,
            denominator: 3,
        },
        latest_height: Height {
            revision_number: chain_id.revision_number(),
            revision_height: trusted_height,
        },
        is_frozen: false,
        // 2 weeks in nanoseconds
        trusting_period: 14 * 24 * 60 * 60 * 1_000_000_000,
        unbonding_period: 14 * 24 * 60 * 60 * 1_000_000_000,
    };
    let trusted_consensus_state = ConsensusState {
        timestamp: trusted_light_block.signed_header.header.time,
        root: CommitmentRoot::from_bytes(
            trusted_light_block.signed_header.header.app_hash.as_bytes(),
        ),
        next_validators_hash: trusted_light_block
            .signed_header
            .header
            .next_validators_hash,
    };

    let genesis = SP1ICS07TendermintGenesis {
        trusted_consensus_state: hex::encode(
            SolConsensusState::from(trusted_consensus_state).abi_encode(),
        ),
        trusted_client_state: hex::encode(trusted_client_state.abi_encode()),
        vkey: vk.bytes32(),
    };

    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(args.genesis_path);
    std::fs::write(
        fixture_path.join("genesis.json"),
        serde_json::to_string_pretty(&genesis).unwrap(),
    )
    .unwrap();

    Ok(())
}
