use std::collections::HashMap;
use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;

use itertools::Itertools;
use parking_lot::RwLock;
use serde::Deserialize;

const RPC_PORT: u16 = 3030;
const CRAWL_INTERVAL: u64 = 2;

#[derive(Deserialize, Debug)]
struct NodeInfoResponse {
    jsonrpc: String,
    id: String,
    result: NodeInfo,
}

#[derive(Deserialize, Debug)]
struct NodeInfo {
    is_miner: bool,
    is_syncing: bool,
    version: String,
}

#[derive(Deserialize, Debug)]
struct PeerInfoResponse {
    jsonrpc: String,
    id: String,
    result: PeerInfo,
}

#[derive(Deserialize, Debug)]
struct PeerInfo {
    peers: Vec<SocketAddr>,
}

struct Node {
    is_miner: bool,
    is_syncing: bool,
    version: String,
    peers: HashSet<SocketAddr>,
}

#[tokio::main]
async fn main() {
    let initial_node: SocketAddr = "50.18.246.201:4131".parse().unwrap();

    let nodes: Arc<RwLock<HashMap<SocketAddr, Option<Node>>>> =
        Arc::new(RwLock::new(HashMap::new()));
    nodes.write().insert(initial_node, None);

    let data_info =
        r#"{"jsonrpc": "2.0", "id":"documentation", "method": "getnodeinfo", "params": []}"#;
    let data_peers =
        r#"{"jsonrpc": "2.0", "id":"documentation", "method": "getpeerinfo", "params": []}"#;

    loop {
        println!("NODE COUNT: {}", nodes.read().len());
        println!(
            "MINER COUNT: {}",
            nodes
                .read()
                .values()
                .filter(|node| if let Some(node) = node {
                    node.is_miner
                } else {
                    false
                })
                .count()
        );

        println!(
            "VERSION COUNTS: {:#?}",
            nodes
                .read()
                .values()
                .filter(|node| node.is_some())
                .map(|node| &node.as_ref().unwrap().version)
                .counts()
        );

        // Copy the addresses for this iteration.
        let addrs: HashSet<SocketAddr> = nodes.read().keys().cloned().collect();

        for addr in addrs {
            let nodes_clone = nodes.clone();
            tokio::task::spawn(async move {
                let mut rpc = addr;
                rpc.set_port(RPC_PORT);

                let client = reqwest::Client::new();
                let node_info_res = client
                    .post(format!("http://{}", rpc.clone()))
                    .timeout(std::time::Duration::from_secs(5))
                    .body(data_info)
                    .header("content-type", "application/json")
                    .send()
                    .await;

                let node_info_response = match node_info_res {
                    Err(_e) => return,
                    Ok(res) => match res.json::<NodeInfoResponse>().await {
                        Ok(res) => res,
                        Err(_e) => return,
                    },
                };

                // Query the node for peers.
                let client = reqwest::Client::new();
                let peer_info_res = client
                    .post(format!("http://{}", rpc.clone()))
                    .timeout(std::time::Duration::from_secs(5))
                    .body(data_peers)
                    .header("content-type", "application/json")
                    .send()
                    .await;

                let peer_info_response = match peer_info_res {
                    Err(_e) => return,
                    Ok(res) => match res.json::<PeerInfoResponse>().await {
                        Ok(res) => res,
                        Err(_e) => return,
                    },
                };

                let is_miner = node_info_response.result.is_miner;
                let is_syncing = node_info_response.result.is_syncing;
                let version = node_info_response.result.version;

                // Update the list of peers for this node.
                let result_peers = peer_info_response.result.peers.iter();
                let new_peers = result_peers.clone().cloned().collect();

                let node = Node {
                    is_miner,
                    is_syncing,
                    version,
                    peers: new_peers,
                };

                nodes_clone.write().insert(addr, Some(node));

                // Insert new address to be queried in the next loop.
                for addr in result_peers {
                    let mut nodes_clone_g = nodes_clone.write();

                    if !nodes_clone_g.contains_key(addr) {
                        nodes_clone_g.insert(*addr, None);
                    }
                }
            });
        }

        // Sleep between crawls.
        tokio::time::sleep(std::time::Duration::from_secs(CRAWL_INTERVAL)).await;
    }
}
