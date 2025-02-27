use clap::Parser;
use serde::Deserialize;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{pubkey::Pubkey, signature::Keypair, signer::Signer, system_instruction, transaction::Transaction};
use yellowstone_grpc_client::{ClientTlsConfig, GeyserGrpcClient};
use std::{fs, str::FromStr};
use {
    futures::{sink::SinkExt, stream::StreamExt},
    yellowstone_grpc_proto::prelude::{
        subscribe_update::UpdateOneof, SubscribeRequest
    },
};

#[derive(Deserialize)]
struct Config {
    senders: Vec<String>,
    receivers: Vec<String>,
    amounts: Vec<u64>,
}

#[derive(Debug, Clone, Parser)]
#[clap(author, version, about)]
struct Args {
    #[clap(short, long)]
    endpoint: String,

    #[clap(long)]
    x_token: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let solana_client = RpcClient::new(String::from(""));
    let config: Config = serde_yaml::from_str(&fs::read_to_string("config.yaml").expect("Failed to read config.yaml")).expect("Invalid config format");

    let args = Args::parse();

    let client = GeyserGrpcClient::build_from_shared(args.endpoint).unwrap()
        .x_token(args.x_token).unwrap()
        .tls_config(ClientTlsConfig::new().with_native_roots()).unwrap()
        .connect()
        .await;
    let (mut subscribe_tx, mut stream) = client.expect("initialised client").subscribe().await.unwrap();

    subscribe_tx
        .send(SubscribeRequest {
            ..Default::default()
        })
        .await?;

    while let Some(message) = stream.next().await {
        match message?.update_oneof {
            Some(UpdateOneof::Block(block)) => {
                println!("New block detected: {}", block.block_height.unwrap().block_height);

                send_transactions(&solana_client, &config).await?;
            }
            _ => {},
        }
    }


    Ok(())
}

async fn send_transactions(client: &RpcClient, config: &Config) -> anyhow::Result<()> {
    for ((sender_private_key, receiver_address), amount) in
        config.senders.iter().zip(&config.receivers).zip(&config.amounts)
    {
        let sender_keypair = Keypair::from_base58_string(sender_private_key);
        let receiver_pubkey = Pubkey::from_str(receiver_address)
            .expect("Invalid receiver wallet address");

        let lamports = *amount;

        let transfer_instruction = system_instruction::transfer(
            &sender_keypair.pubkey(),
            &receiver_pubkey,
            lamports,
        );

        let transaction = Transaction::new_signed_with_payer(
            &[transfer_instruction],
            Some(&sender_keypair.pubkey()),
            &[&sender_keypair],
            solana_sdk::hash::Hash::new_unique(),
        );
        
        match client.send_and_confirm_transaction(&transaction).await {
            Ok(sig) => println!("Tx sent: {}", sig.to_string()),
            Err(e) => eprintln!("Failed to send tx: {}", e)
        };


        println!(
            "Sending {} SOL from {} to {}...",
            lamports as f64 / 1_000_000_000.0, // Convert lamports to SOL
            sender_keypair.pubkey(),
            receiver_pubkey
        );
    }

    Ok(())
}